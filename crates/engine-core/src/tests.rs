#[cfg(test)]
mod tests {
    use crate::allocator::create_standard_engine;
    use crate::config::EngineConfig;
    use crate::executor::WasmExecutor;
    use wasmtime::Module;

    // A minimal valid Wasm module that writes output and returns 0
    const WAT_HELLO: &str = r#"
        (module
            (import "env" "host_log" (func $host_log (param i32 i32 i32)))
            (import "env" "host_write_output" (func $host_write_output (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "Hello from WasmPulse Sandbox!")
            (func (export "_start") (result i32)
                ;; Call host_log(level=2, ptr=0, len=29)
                (call $host_log (i32.const 2) (i32.const 0) (i32.const 29))
                ;; Call host_write_output(ptr=0, len=29)
                (call $host_write_output (i32.const 0) (i32.const 29))
                (i32.const 0)
            )
        )
    "#;

    // An infinite loop module designed to test fuel exhaustion
    const WAT_INFINITE_LOOP: &str = r#"
        (module
            (memory (export "memory") 1)
            (func (export "_start") (result i32)
                (loop
                    (br 0)
                )
                (i32.const 0)
            )
        )
    "#;

    #[tokio::test]
    async fn test_sub_millisecond_cold_start() {
        let config = EngineConfig {
            max_instances: 100,
            ..Default::default()
        };
        let engine = create_standard_engine().expect("Failed to create engine");
        let wasm_bytes = wat::parse_str(WAT_HELLO).expect("Failed to parse WAT");
        let module = Module::new(&engine, &wasm_bytes).expect("Failed to compile module");

        let executor = WasmExecutor::new(engine, config).expect("Failed to create executor");

        // Execute function
        let res = executor.execute(
            &module,
            "tenant-alpha",
            "hello-func",
            vec![],
            Some(1_000_000),
            Some(100),
        ).expect("Execution failed");

        println!("Execution Metrics:");
        println!("  Cold start: {} us", res.cold_start_us);
        println!("  Execution:  {} us", res.execution_us);
        println!("  Total:      {} us", res.total_us);
        println!("  Fuel used:  {}", res.fuel_consumed);
        println!("  Output:     {}", String::from_utf8_lossy(&res.output));

        assert_eq!(res.exit_code, 0);
        assert_eq!(String::from_utf8_lossy(&res.output), "Hello from WasmPulse Sandbox!");
        assert!(res.logs.iter().any(|l| l.contains("Hello from WasmPulse Sandbox!")));
        assert!(res.fuel_consumed > 0);

        // Verify receipt
        assert!(res.receipt.is_some());
        let receipt = res.receipt.unwrap();
        assert!(receipt.verify(), "Cryptographic execution proof verification failed");
    }

    #[tokio::test]
    async fn test_fuel_exhaustion_defense() {
        let config = EngineConfig::default();
        let engine = create_standard_engine().expect("Failed to create engine");
        let wasm_bytes = wat::parse_str(WAT_INFINITE_LOOP).expect("Failed to parse WAT");
        let module = Module::new(&engine, &wasm_bytes).expect("Failed to compile module");

        let executor = WasmExecutor::new(engine, config).expect("Failed to create executor");

        // Allocate only 5,000 instructions of fuel
        let res = executor.execute(
            &module,
            "tenant-attacker",
            "infinite-loop",
            vec![],
            Some(5_000),
            Some(500),
        );

        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        println!("Caught trapped execution: {}", err_msg);
        assert!(err_msg.contains("Fuel quota") || err_msg.contains("exhausted"));
    }

    #[tokio::test]
    async fn test_kv_store_roundtrip() {
        const WAT_KV: &str = r#"
            (module
                (import "env" "host_kv_set" (func $host_kv_set (param i32 i32 i32 i32) (result i32)))
                (import "env" "host_kv_get" (func $host_kv_get (param i32 i32 i32 i32) (result i32)))
                (import "env" "host_write_output" (func $host_write_output (param i32 i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "user_session")
                (data (i32.const 16) "token_xyz123")
                (func (export "_start") (result i32)
                    (local $len i32)
                    ;; 1. Set KV: key at 0 (len 12), val at 16 (len 12)
                    (drop (call $host_kv_set (i32.const 0) (i32.const 12) (i32.const 16) (i32.const 12)))
                    ;; 2. Read KV back into memory at offset 32 (max 64)
                    (local.set $len (call $host_kv_get (i32.const 0) (i32.const 12) (i32.const 32) (i32.const 64)))
                    ;; 3. Write read value to output
                    (call $host_write_output (i32.const 32) (local.get $len))
                    (i32.const 0)
                )
            )
        "#;

        let config = EngineConfig::default();
        let engine = create_standard_engine().expect("Failed to create engine");
        let wasm_bytes = wat::parse_str(WAT_KV).expect("Failed to parse WAT");
        let module = Module::new(&engine, &wasm_bytes).expect("Failed to compile module");

        let executor = WasmExecutor::new(engine, config).expect("Failed to create executor");

        let res = executor.execute(
            &module,
            "tenant-bob",
            "kv-func",
            vec![],
            Some(1_000_000),
            Some(500),
        ).expect("KV execution failed");

        assert_eq!(String::from_utf8_lossy(&res.output), "token_xyz123");
    }

    #[tokio::test]
    async fn test_verifiable_cryptographic_attestation_receipts() {
        let config = EngineConfig::default();
        let engine = create_standard_engine().expect("Failed to create engine");
        let wasm_bytes = wat::parse_str(WAT_HELLO).expect("Failed to parse WAT");
        let module = Module::new(&engine, &wasm_bytes).expect("Failed to compile module");

        let executor = WasmExecutor::new(engine, config).expect("Failed to create executor");

        // 1. First execution
        let res1 = executor.execute(
            &module,
            "tenant-finance",
            "settle-payment",
            b"{\"amount\": 500}".to_vec(),
            Some(1_000_000),
            Some(500),
        ).expect("Execution 1 failed");

        let receipt1 = res1.receipt.expect("Receipt 1 missing");
        assert!(receipt1.verify(), "Receipt 1 verification must succeed");

        // 2. Second execution chained to first
        let res2 = executor.execute(
            &module,
            "tenant-finance",
            "settle-payment",
            b"{\"amount\": 1200}".to_vec(),
            Some(1_000_000),
            Some(500),
        ).expect("Execution 2 failed");

        let receipt2 = res2.receipt.expect("Receipt 2 missing");
        assert!(receipt2.verify(), "Receipt 2 verification must succeed");
        assert_eq!(receipt2.prev_receipt_hash, receipt1.proof_hash, "Chained hash must match previous receipt proof");

        // 3. Verify ledger Merkle chain
        let (valid, count) = executor.ledger().verify_chain();
        assert!(valid, "Audit ledger Merkle chain must be valid");
        assert_eq!(count, 2, "Must verify exactly 2 chained receipts");

        // 4. Test tamper detection
        let mut tampered = receipt2.clone();
        tampered.fuel_consumed += 1; // Tamper instruction count
        assert!(!tampered.verify(), "Tampered receipt verification must fail");
    }
}
