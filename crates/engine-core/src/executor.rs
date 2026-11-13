use crate::attestation::{AuditLedger, ExecutionReceipt};
use crate::config::EngineConfig;
use crate::host_api::register_host_api;
use crate::sandboxing::TenantContext;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wasmtime::{Engine, Linker, Module, Store, Trap};

/// Result metrics and payload returned from a serverless sandbox invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Cold start latency in microseconds (sandbox initialization + instantiation).
    pub cold_start_us: u128,
    /// Execution run latency in microseconds.
    pub execution_us: u128,
    /// Total wall-clock duration in microseconds.
    pub total_us: u128,
    /// Exact CPU fuel consumed by guest instructions.
    pub fuel_consumed: u64,
    /// Exit code returned by guest entrypoint.
    pub exit_code: i32,
    /// Raw output bytes written by the guest function.
    pub output: Vec<u8>,
    /// Logs emitted by the function via `host_log`.
    pub logs: Vec<String>,
    /// Cryptographic Attestation Receipt for verifiable execution.
    pub receipt: Option<ExecutionReceipt>,
}

/// The core multi-tenant serverless execution engine.
pub struct WasmExecutor {
    engine: Engine,
    linker: Linker<TenantContext>,
    config: EngineConfig,
    tenant_stores: Arc<Mutex<HashMap<String, Arc<Mutex<HashMap<String, Vec<u8>>>>>>>,
    ledger: Arc<AuditLedger>,
    _epoch_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WasmExecutor {
    /// Creates a new executor instance using the given configuration and pre-built engine.
    pub fn new(engine: Engine, config: EngineConfig) -> Result<Self> {
        let mut linker = Linker::new(&engine);
        register_host_api(&mut linker).context("Failed to register host capabilities")?;

        // Start background epoch-interruption ticker to enforce timeouts
        let engine_clone = engine.clone();
        let tick_ms = config.epoch_tick_interval_ms;
        let epoch_handle = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Some(handle.spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
                loop {
                    interval.tick().await;
                    engine_clone.increment_epoch();
                }
            }))
        } else {
            std::thread::Builder::new()
                .name("wasm-epoch-ticker".into())
                .spawn(move || loop {
                    std::thread::sleep(Duration::from_millis(tick_ms));
                    engine_clone.increment_epoch();
                })
                .ok();
            None
        };

        Ok(Self {
            engine,
            linker,
            config,
            tenant_stores: Arc::new(Mutex::new(HashMap::new())),
            ledger: Arc::new(AuditLedger::default()),
            _epoch_handle: epoch_handle,
        })
    }

    /// Access the cryptographic Merkle audit ledger.
    pub fn ledger(&self) -> Arc<AuditLedger> {
        self.ledger.clone()
    }

    /// Retrieves or initializes a tenant-isolated Key-Value store.
    pub fn get_or_create_tenant_kv(&self, tenant_id: &str) -> Arc<Mutex<HashMap<String, Vec<u8>>>> {
        let mut map = self.tenant_stores.lock().unwrap();
        map.entry(tenant_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(HashMap::new())))
            .clone()
    }

    /// Executes a sandboxed Wasm function with sub-millisecond cold start tracking.
    pub fn execute(
        &self,
        module: &Module,
        tenant_id: &str,
        function_name: &str,
        input_data: Vec<u8>,
        custom_fuel_limit: Option<u64>,
        custom_timeout_ms: Option<u64>,
    ) -> Result<ExecutionResult> {
        let t_zero = Instant::now();

        let fuel_limit = custom_fuel_limit.unwrap_or(self.config.default_fuel_limit);
        let timeout_ms = custom_timeout_ms.unwrap_or(self.config.default_timeout_ms);
        let tenant_kv = self.get_or_create_tenant_kv(tenant_id);

        let context = TenantContext::new(
            tenant_id,
            function_name,
            fuel_limit,
            input_data.clone(),
            tenant_kv,
        );

        // 1. INSTANTIATION PHASE (The Cold Start Hot Path)
        let mut store = Store::new(&self.engine, context);
        store.set_fuel(fuel_limit).context("Failed to allocate initial fuel")?;

        // Compute epoch deadline (e.g. 500ms timeout / 1ms epoch tick = 500 ticks)
        let epoch_ticks = std::cmp::max(1, timeout_ms / self.config.epoch_tick_interval_ms);
        store.set_epoch_deadline(epoch_ticks);

        // Instant instantiation from virtual memory pool
        let instance = self.linker.instantiate(&mut store, module)
            .context("Failed to instantiate sandboxed module")?;

        let t_instantiated = Instant::now();
        let cold_start_us = t_instantiated.duration_since(t_zero).as_micros();

        // 2. INVOCATION PHASE
        // Standard convention: guest exports `_start` or `entrypoint` or `handle_request`
        let entrypoint = instance
            .get_typed_func::<(), i32>(&mut store, "_start")
            .or_else(|_| instance.get_typed_func::<(), i32>(&mut store, "entrypoint"))
            .or_else(|_| instance.get_typed_func::<(), i32>(&mut store, "handle_request"));

        let run_func = match entrypoint {
            Ok(func) => func,
            Err(_) => {
                bail!("Wasm module does not export a recognized entrypoint (_start, entrypoint, or handle_request)");
            }
        };

        let t_call_start = Instant::now();
        let call_res = run_func.call(&mut store, ());
        let t_call_end = Instant::now();
        let execution_us = t_call_end.duration_since(t_call_start).as_micros();
        let total_us = t_call_end.duration_since(t_zero).as_micros();

        // 3. RESOURCE AUDIT & TEARDOWN
        let remaining_fuel = store.get_fuel().unwrap_or(0);
        let fuel_consumed = fuel_limit.saturating_sub(remaining_fuel);

        match call_res {
            Ok(exit_code) => {
                let context = store.into_data();
                let receipt = self.ledger.record(
                    tenant_id,
                    function_name,
                    &input_data,
                    &context.output_buffer,
                    fuel_consumed,
                    exit_code,
                    cold_start_us,
                    execution_us,
                    total_us,
                );

                Ok(ExecutionResult {
                    cold_start_us,
                    execution_us,
                    total_us,
                    fuel_consumed,
                    exit_code,
                    output: context.output_buffer,
                    logs: context.logs,
                    receipt: Some(receipt),
                })
            }
            Err(err) => {
                if let Some(trap) = err.downcast_ref::<Trap>() {
                    match trap {
                        Trap::OutOfFuel => {
                            bail!("Function execution terminated: CPU Fuel quota ({} instructions) exhausted", fuel_limit);
                        }
                        Trap::Interrupt => {
                            bail!("Function execution terminated: Wall-clock timeout ({} ms) exceeded", timeout_ms);
                        }
                        Trap::MemoryOutOfBounds => {
                            bail!("Function execution terminated: Memory boundary violation");
                        }
                        _ => bail!("Sandbox execution trapped: {}", trap),
                    }
                }
                bail!("Execution error: {:?}", err);
            }
        }
    }
}
