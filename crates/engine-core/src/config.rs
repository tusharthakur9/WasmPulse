use serde::{Deserialize, Serialize};

/// Configuration parameters for the WasmPulse engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Maximum number of concurrent pooled core instances.
    pub max_instances: u32,
    /// Maximum memories per instance.
    pub max_memories: u32,
    /// Maximum memory pages (64KB per page) allocated per instance. Default: 256 (16 MB).
    pub max_memory_pages: u64,
    /// Amount of linear memory to keep resident (warm in cache) per slot in bytes. Default: 512KB.
    pub linear_memory_keep_resident_bytes: usize,
    /// Default fuel quota (CPU instruction budget) per invocation. Default: 10,000,000.
    pub default_fuel_limit: u64,
    /// Epoch tick interval in milliseconds for wall-clock execution timeouts. Default: 1ms.
    pub epoch_tick_interval_ms: u64,
    /// Wall-clock timeout in milliseconds. Default: 500ms.
    pub default_timeout_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_instances: 10_000,
            max_memories: 1,
            max_memory_pages: 256, // 16 MB
            linear_memory_keep_resident_bytes: 512 * 1024, // 512 KB
            default_fuel_limit: 10_000_000,
            epoch_tick_interval_ms: 1,
            default_timeout_ms: 500,
        }
    }
}
