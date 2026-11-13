use engine_core::WasmExecutor;
use engine_storage::ModuleRegistry;
use std::sync::Arc;
use wasmtime::Engine;

/// Shared global application state for the Axum HTTP Gateway.
#[derive(Clone)]
pub struct AppState {
    pub executor: Arc<WasmExecutor>,
    pub registry: Arc<ModuleRegistry>,
    pub engine: Engine,
}

impl AppState {
    pub fn new(executor: WasmExecutor, registry: ModuleRegistry, engine: Engine) -> Self {
        Self {
            executor: Arc::new(executor),
            registry: Arc::new(registry),
            engine,
        }
    }
}
