use anyhow::{Context, Result};
use std::sync::Arc;
use wasmtime::{Engine, Module};

/// Ahead-of-Time (AOT) compiler that compiles raw .wasm bytecode into native Cranelift machine code (.cwasm).
/// By precompiling modules during deployment, cold start requests never pay the JIT compilation penalty.
pub struct AotCompiler;

impl AotCompiler {
    /// Compiles standard WebAssembly bytecode into native machine code (.cwasm).
    pub fn compile(engine: &Engine, wasm_bytes: &[u8]) -> Result<Vec<u8>> {
        engine
            .precompile_module(wasm_bytes)
            .context("Failed to AOT compile Wasm bytecode to native Cranelift machine code")
    }

    /// Fast-path deserialization of precompiled machine code into a live in-memory Module.
    /// This operation is virtually instantaneous (~50-100 microseconds).
    ///
    /// # Safety
    /// Caller must ensure `cwasm_bytes` were produced by the exact same version and configuration
    /// of Wasmtime.
    pub unsafe fn deserialize(engine: &Engine, cwasm_bytes: &[u8]) -> Result<Arc<Module>> {
        let module = Module::deserialize(engine, cwasm_bytes)
            .context("Failed to deserialize native cwasm machine code")?;
        Ok(Arc::new(module))
    }

    /// Compiles raw wasm bytes directly into a live in-memory Module.
    pub fn compile_to_module(engine: &Engine, wasm_bytes: &[u8]) -> Result<Arc<Module>> {
        let module = Module::new(engine, wasm_bytes)
            .context("Failed to compile Wasm bytecode into Module")?;
        Ok(Arc::new(module))
    }
}
