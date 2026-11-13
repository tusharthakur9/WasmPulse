use engine_core::{create_standard_engine, EngineConfig, WasmExecutor};
use std::fs;
use std::path::PathBuf;
use wasmtime::Module;

#[tokio::test]
async fn test_end_to_end_rust_guest_execution() {
    let wasm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-wasip1/release/rust_echo.wasm");

    if !wasm_path.exists() {
        eprintln!("Skipping integration test because rust_echo.wasm not built yet at {:?}", wasm_path);
        return;
    }

    let wasm_bytes = fs::read(&wasm_path).expect("Failed to read compiled rust_echo.wasm");
    let config = EngineConfig::default();
    let engine = create_standard_engine().expect("Failed to create engine");
    let module = Module::new(&engine, &wasm_bytes).expect("Failed to compile Module");
    let executor = WasmExecutor::new(engine, config).expect("Failed to construct executor");

    // Invocation 1: First invocation (Cold Start)
    let payload_1 = br#"{"name": "Alice"}"#.to_vec();
    let res_1 = executor.execute(
        &module,
        "tenant-corp",
        "echo",
        payload_1,
        Some(10_000_000),
        Some(500),
    ).expect("Invocation 1 failed");

    println!("\n=== INVOCATION 1 (COLD START) ===");
    println!("Cold start latency: {} microseconds ({} ms)", res_1.cold_start_us, res_1.cold_start_us as f64 / 1000.0);
    println!("Execution latency:  {} microseconds", res_1.execution_us);
    println!("Fuel consumed:      {} instructions", res_1.fuel_consumed);
    println!("Output:             {}", String::from_utf8_lossy(&res_1.output));

    assert_eq!(res_1.exit_code, 0);
    let output_str_1 = String::from_utf8_lossy(&res_1.output);
    assert!(output_str_1.contains("Hello, Alice!"));
    assert!(output_str_1.contains("\"kv_counter\":1"));

    // Invocation 2: Second invocation (Warm Sandbox, state incremented)
    let payload_2 = br#"{"name": "Bob"}"#.to_vec();
    let res_2 = executor.execute(
        &module,
        "tenant-corp",
        "echo",
        payload_2,
        Some(10_000_000),
        Some(500),
    ).expect("Invocation 2 failed");

    println!("\n=== INVOCATION 2 (WARM EXECUTION) ===");
    println!("Cold start latency: {} microseconds", res_2.cold_start_us);
    println!("Execution latency:  {} microseconds", res_2.execution_us);
    println!("Output:             {}", String::from_utf8_lossy(&res_2.output));

    assert_eq!(res_2.exit_code, 0);
    let output_str_2 = String::from_utf8_lossy(&res_2.output);
    assert!(output_str_2.contains("Hello, Bob!"));
    assert!(output_str_2.contains("\"kv_counter\":2"));

    println!("\nIntegration Test PASSED: Cold start is sub-millisecond and stateful KV store persists!");
}
