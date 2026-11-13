use crate::config::EngineConfig;
use anyhow::{Context, Result};
use wasmtime::{Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig, Strategy};

/// Builds an optimized Wasmtime Engine using a virtual memory pooling allocator.
/// This eliminates OS-level `mmap` / `munmap` syscalls during the invocation hot path,
/// enabling sub-millisecond (< 500µs) cold start instantiations.
pub fn create_pooled_engine(config: &EngineConfig) -> Result<Engine> {
    let mut wasm_config = Config::new();

    // 1. Ahead-of-Time Cranelift compilation with maximum speed optimizations
    wasm_config.strategy(Strategy::Cranelift);
    wasm_config.cranelift_opt_level(OptLevel::Speed);

    // 2. Anti-DoS protections: Instruction-based fuel metering & wall-clock epoch timers
    wasm_config.consume_fuel(true);
    wasm_config.epoch_interruption(true);

    // 3. Configure virtual memory pooling allocator
    let mut pooling_config = PoolingAllocationConfig::default();
    
    // Set max core instances that can be concurrently live in the pool
    pooling_config.total_core_instances(config.max_instances);
    pooling_config.max_memories_per_module(config.max_memories);
    
    // Total memory cap per instance (pages * 64KB)
    pooling_config.max_memory_size((config.max_memory_pages * 64 * 1024) as usize);
    
    // Keep portions of linear memory resident in cache to eliminate page-fault spikes
    pooling_config.linear_memory_keep_resident(config.linear_memory_keep_resident_bytes);

    wasm_config.allocation_strategy(InstanceAllocationStrategy::Pooling(pooling_config));

    Engine::new(&wasm_config).context("Failed to construct pooled Wasmtime engine")
}

/// Fallback standard engine (useful for environments with strict virtual address space limits).
pub fn create_standard_engine() -> Result<Engine> {
    let mut wasm_config = Config::new();
    wasm_config.strategy(Strategy::Cranelift);
    wasm_config.cranelift_opt_level(OptLevel::Speed);
    wasm_config.consume_fuel(true);
    wasm_config.epoch_interruption(true);
    Engine::new(&wasm_config).context("Failed to construct standard Wasmtime engine")
}
