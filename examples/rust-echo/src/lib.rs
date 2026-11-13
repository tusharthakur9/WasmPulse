// WasmPulse Rust Guest Microservice
// Targets wasm32-wasip1 with full standard library support

use serde::{Deserialize, Serialize};

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_log(level: i32, ptr: i32, len: i32);
    fn host_get_input_len() -> i32;
    fn host_read_input(dest_ptr: i32, max_len: i32) -> i32;
    fn host_write_output(src_ptr: i32, len: i32);
    fn host_kv_set(k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32) -> i32;
    fn host_kv_get(k_ptr: i32, k_len: i32, v_out_ptr: i32, max_v_len: i32) -> i32;
}

fn log(level: i32, msg: &str) {
    unsafe {
        host_log(level, msg.as_ptr() as i32, msg.len() as i32);
    }
}

#[derive(Deserialize)]
struct RequestPayload {
    name: Option<String>,
    #[allow(dead_code)]
    operation: Option<String>,
}

#[derive(Serialize)]
struct ResponsePayload {
    status: String,
    greeting: String,
    kv_counter: u64,
    runtime: String,
}

#[no_mangle]
pub extern "C" fn _start() -> i32 {
    log(2, "Rust guest execution started in Wasm sandbox");

    // 1. Read input payload
    let input_len = unsafe { host_get_input_len() };
    let mut input_bytes = vec![0u8; input_len as usize];
    if input_len > 0 {
        unsafe {
            host_read_input(input_bytes.as_mut_ptr() as i32, input_len);
        }
    }

    // 2. Parse JSON or fallback
    let name = if let Ok(parsed) = serde_json::from_slice::<RequestPayload>(&input_bytes) {
        parsed.name.unwrap_or_else(|| "Anonymous".into())
    } else {
        "Anonymous".into()
    };

    // 3. Interact with isolated tenant KV store: read and increment counter
    let key = "visit_counter";
    let mut counter_buf = [0u8; 16];
    let read_len = unsafe {
        host_kv_get(
            key.as_ptr() as i32,
            key.len() as i32,
            counter_buf.as_mut_ptr() as i32,
            16,
        )
    };

    let current_counter: u64 = if read_len > 0 {
        let s = std::str::from_utf8(&counter_buf[..read_len as usize]).unwrap_or("0");
        s.parse::<u64>().unwrap_or(0)
    } else {
        0
    };

    let new_counter = current_counter + 1;
    let new_counter_str = format!("{}", new_counter);
    unsafe {
        host_kv_set(
            key.as_ptr() as i32,
            key.len() as i32,
            new_counter_str.as_ptr() as i32,
            new_counter_str.len() as i32,
        );
    }

    // 4. Construct JSON response
    let response = ResponsePayload {
        status: "success".into(),
        greeting: format!("Hello, {}! Served from WasmPulse sandbox with sub-millisecond cold start.", name),
        kv_counter: new_counter,
        runtime: "WasmPulse/v0.1.0".into(),
    };

    if let Ok(json_bytes) = serde_json::to_vec(&response) {
        unsafe {
            host_write_output(json_bytes.as_ptr() as i32, json_bytes.len() as i32);
        }
    }

    log(2, "Execution completed successfully");
    0
}
