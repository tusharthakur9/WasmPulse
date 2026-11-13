use criterion::{black_box, criterion_group, criterion_main, Criterion};
use engine_core::{create_standard_engine, EngineConfig, WasmExecutor};
use wasmtime::Module;

const WAT_BENCH: &str = r#"
    (module
        (import "env" "host_write_output" (func $host_write_output (param i32 i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "bench_ok")
        (func (export "_start") (result i32)
            (call $host_write_output (i32.const 0) (i32.const 8))
            (i32.const 0)
        )
    )
"#;

fn bench_cold_starts(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = EngineConfig::default();
    let engine = create_standard_engine().unwrap();
    let wasm_bytes = wat::parse_str(WAT_BENCH).unwrap();
    let module = Module::new(&engine, &wasm_bytes).unwrap();
    let executor = WasmExecutor::new(engine, config).unwrap();

    let mut group = c.benchmark_group("wasm_serverless");
    group.sample_size(50);

    group.bench_function("cold_start_instantiation_and_exec", |b| {
        b.to_async(&rt).iter(|| async {
            let res = executor.execute(
                &module,
                black_box("tenant-bench"),
                black_box("bench-func"),
                black_box(vec![]),
                Some(1_000_000),
                Some(500),
            ).unwrap();
            black_box(res);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_cold_starts);
criterion_main!(benches);
