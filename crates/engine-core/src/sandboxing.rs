use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Context state passed into the Wasm Store for each isolated sandbox invocation.
pub struct TenantContext {
    /// Identifier for the tenant (for multi-tenant isolation and billing).
    pub tenant_id: String,
    /// Name of the function being executed.
    pub function_name: String,
    /// Maximum fuel allocated for this execution.
    pub fuel_limit: u64,
    /// Captures standard out / log messages generated inside the sandbox.
    pub logs: Vec<String>,
    /// Raw HTTP request / binary input payload provided to the guest.
    pub input_buffer: Vec<u8>,
    /// Buffer where the guest function writes its response payload.
    pub output_buffer: Vec<u8>,
    /// Simple isolated in-memory Key-Value store per tenant.
    pub kv_store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl TenantContext {
    pub fn new(
        tenant_id: impl Into<String>,
        function_name: impl Into<String>,
        fuel_limit: u64,
        input_data: Vec<u8>,
        kv_store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            function_name: function_name.into(),
            fuel_limit,
            logs: Vec::new(),
            input_buffer: input_data,
            output_buffer: Vec::new(),
            kv_store,
        }
    }
}
