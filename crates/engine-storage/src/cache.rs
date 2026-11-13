use anyhow::{bail, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::Module;

/// Metadata for a deployed serverless function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetadata {
    pub tenant_id: String,
    pub name: String,
    pub bytecode_size_bytes: usize,
    pub created_at_timestamp: u64,
    pub invocation_count: u64,
}

struct InternalFunctionRecord {
    pub metadata: FunctionMetadata,
    pub module: Arc<Module>,
    pub invocation_counter: AtomicU64,
}

/// Thread-safe in-memory registry and fast LRU-style cache for active Wasm modules.
pub struct ModuleRegistry {
    records: DashMap<String, Arc<InternalFunctionRecord>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            records: DashMap::new(),
        }
    }

    fn make_key(tenant_id: &str, function_name: &str) -> String {
        format!("{}/{}", tenant_id, function_name)
    }

    /// Registers or updates a function in the in-memory cache.
    pub fn register(
        &self,
        tenant_id: &str,
        function_name: &str,
        bytecode_size_bytes: usize,
        module: Arc<Module>,
    ) {
        let key = Self::make_key(tenant_id, function_name);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let metadata = FunctionMetadata {
            tenant_id: tenant_id.to_string(),
            name: function_name.to_string(),
            bytecode_size_bytes,
            created_at_timestamp: now,
            invocation_count: 0,
        };

        let record = Arc::new(InternalFunctionRecord {
            metadata,
            module,
            invocation_counter: AtomicU64::new(0),
        });

        self.records.insert(key, record);
    }

    /// Retrieves an in-memory Module reference for execution.
    pub fn get(&self, tenant_id: &str, function_name: &str) -> Option<Arc<Module>> {
        let key = Self::make_key(tenant_id, function_name);
        if let Some(record) = self.records.get(&key) {
            record.invocation_counter.fetch_add(1, Ordering::Relaxed);
            Some(record.module.clone())
        } else {
            None
        }
    }

    /// Lists all deployed functions with their current metrics.
    pub fn list(&self) -> Vec<FunctionMetadata> {
        self.records
            .iter()
            .map(|entry| {
                let mut meta = entry.value().metadata.clone();
                meta.invocation_count = entry.value().invocation_counter.load(Ordering::Relaxed);
                meta
            })
            .collect()
    }

    /// Removes a function from the registry.
    pub fn remove(&self, tenant_id: &str, function_name: &str) -> Result<()> {
        let key = Self::make_key(tenant_id, function_name);
        if self.records.remove(&key).is_some() {
            Ok(())
        } else {
            bail!("Function '{}' not found for tenant '{}'", function_name, tenant_id);
        }
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}
