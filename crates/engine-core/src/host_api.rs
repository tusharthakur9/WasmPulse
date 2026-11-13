use crate::sandboxing::TenantContext;
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmtime::{Caller, Linker};

/// Registers both custom host APIs and standard WASI Preview 1 stubs into the Linker.
pub fn register_host_api(linker: &mut Linker<TenantContext>) -> Result<()> {
    // ---------------------------------------------------------
    // 1. Custom High-Performance Capabilities ("env" module)
    // ---------------------------------------------------------
    register_custom_env_api(linker)?;

    // ---------------------------------------------------------
    // 2. High-Density WASI Preview 1 Subsystem ("wasi_snapshot_preview1")
    // ---------------------------------------------------------
    register_wasi_preview1(linker)?;

    Ok(())
}

fn register_custom_env_api(linker: &mut Linker<TenantContext>) -> Result<()> {
    // Structured guest logging: host_log(level: i32, ptr: i32, len: i32)
    linker.func_wrap(
        "env",
        "host_log",
        |mut caller: Caller<'_, TenantContext>, level: i32, ptr: i32, len: i32| {
            if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let data = mem.data(&caller);
                let start = ptr as usize;
                let end = start + (len as usize);
                if end <= data.len() {
                    let msg = String::from_utf8_lossy(&data[start..end]).to_string();
                    let prefix = match level {
                        1 => "[DEBUG]",
                        2 => "[INFO]",
                        3 => "[WARN]",
                        4 => "[ERROR]",
                        _ => "[LOG]",
                    };
                    let formatted = format!("{} [Tenant: {}] {}", prefix, caller.data().tenant_id, msg);
                    caller.data_mut().logs.push(formatted);
                }
            }
        },
    )?;

    // Get input payload length: host_get_input_len() -> i32
    linker.func_wrap("env", "host_get_input_len", |caller: Caller<'_, TenantContext>| -> i32 {
        caller.data().input_buffer.len() as i32
    })?;

    // Read input payload into guest memory: host_read_input(dest_ptr: i32, max_len: i32) -> i32
    linker.func_wrap(
        "env",
        "host_read_input",
        |mut caller: Caller<'_, TenantContext>, dest_ptr: i32, max_len: i32| -> i32 {
            let input_bytes = caller.data().input_buffer.clone();
            let copy_len = std::cmp::min(input_bytes.len(), max_len as usize);
            if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let start = dest_ptr as usize;
                let data = mem.data_mut(&mut caller);
                if start + copy_len <= data.len() {
                    data[start..start + copy_len].copy_from_slice(&input_bytes[..copy_len]);
                    return copy_len as i32;
                }
            }
            -1
        },
    )?;

    // Write response payload from guest memory: host_write_output(src_ptr: i32, len: i32)
    linker.func_wrap(
        "env",
        "host_write_output",
        |mut caller: Caller<'_, TenantContext>, src_ptr: i32, len: i32| {
            if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let data = mem.data(&caller);
                let start = src_ptr as usize;
                let end = start + (len as usize);
                if end <= data.len() {
                    let out_slice = data[start..end].to_vec();
                    caller.data_mut().output_buffer = out_slice;
                }
            }
        },
    )?;

    // Tenant isolated KV store SET: host_kv_set(k_ptr, k_len, v_ptr, v_len) -> i32
    linker.func_wrap(
        "env",
        "host_kv_set",
        |mut caller: Caller<'_, TenantContext>, k_ptr: i32, k_len: i32, v_ptr: i32, v_len: i32| -> i32 {
            let (key, val) = {
                if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                    let data = mem.data(&caller);
                    let k_start = k_ptr as usize;
                    let k_end = k_start + (k_len as usize);
                    let v_start = v_ptr as usize;
                    let v_end = v_start + (v_len as usize);
                    if k_end <= data.len() && v_end <= data.len() {
                        let k = String::from_utf8_lossy(&data[k_start..k_end]).to_string();
                        let v = data[v_start..v_end].to_vec();
                        (Some(k), Some(v))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            };

            let mut success = false;
            if let (Some(k), Some(v)) = (key, val) {
                if let Ok(mut map) = caller.data().kv_store.lock() {
                    map.insert(k, v);
                    success = true;
                }
            }
            if success { 0 } else { -1 }
        },
    )?;

    // Tenant isolated KV store GET: host_kv_get(k_ptr, k_len, v_out_ptr, max_v_len) -> i32
    linker.func_wrap(
        "env",
        "host_kv_get",
        |mut caller: Caller<'_, TenantContext>, k_ptr: i32, k_len: i32, v_out_ptr: i32, max_v_len: i32| -> i32 {
            let key = {
                if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                    let data = mem.data(&caller);
                    let k_start = k_ptr as usize;
                    let k_end = k_start + (k_len as usize);
                    if k_end <= data.len() {
                        Some(String::from_utf8_lossy(&data[k_start..k_end]).to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let val_opt = if let Some(k) = key {
                caller.data().kv_store.lock().ok().and_then(|map| map.get(&k).cloned())
            } else {
                None
            };

            if let Some(val) = val_opt {
                let copy_len = std::cmp::min(val.len(), max_v_len as usize);
                if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                    let start = v_out_ptr as usize;
                    let data = mem.data_mut(&mut caller);
                    if start + copy_len <= data.len() {
                        data[start..start + copy_len].copy_from_slice(&val[..copy_len]);
                        return copy_len as i32;
                    }
                }
            }
            -1
        },
    )?;

    Ok(())
}

/// Registers the WASI Snapshot Preview 1 ABI to allow standard wasm32-wasip1 binaries to execute.
fn register_wasi_preview1(linker: &mut Linker<TenantContext>) -> Result<()> {
    let module = "wasi_snapshot_preview1";

    // environ_sizes_get(environc: *mut i32, environ_buf_size: *mut i32) -> errno
    linker.func_wrap(module, "environ_sizes_get", |mut caller: Caller<'_, TenantContext>, count_ptr: i32, size_ptr: i32| -> i32 {
        if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let data = mem.data_mut(&mut caller);
            let c = count_ptr as usize;
            let s = size_ptr as usize;
            if c + 4 <= data.len() && s + 4 <= data.len() {
                data[c..c + 4].copy_from_slice(&0u32.to_le_bytes());
                data[s..s + 4].copy_from_slice(&0u32.to_le_bytes());
                return 0; // SUCCESS
            }
        }
        28 // EIO
    })?;

    // environ_get(environ: *mut *mut u8, environ_buf: *mut u8) -> errno
    linker.func_wrap(module, "environ_get", |_caller: Caller<'_, TenantContext>, _env_ptr: i32, _buf_ptr: i32| -> i32 {
        0 // SUCCESS
    })?;

    // args_sizes_get(argc: *mut i32, argv_buf_size: *mut i32) -> errno
    linker.func_wrap(module, "args_sizes_get", |mut caller: Caller<'_, TenantContext>, count_ptr: i32, size_ptr: i32| -> i32 {
        if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let data = mem.data_mut(&mut caller);
            let c = count_ptr as usize;
            let s = size_ptr as usize;
            if c + 4 <= data.len() && s + 4 <= data.len() {
                data[c..c + 4].copy_from_slice(&0u32.to_le_bytes());
                data[s..s + 4].copy_from_slice(&0u32.to_le_bytes());
                return 0;
            }
        }
        28
    })?;

    // args_get(argv: *mut *mut u8, argv_buf: *mut u8) -> errno
    linker.func_wrap(module, "args_get", |_caller: Caller<'_, TenantContext>, _argv_ptr: i32, _buf_ptr: i32| -> i32 {
        0
    })?;

    // clock_time_get(id: i32, precision: i64, time: *mut u64) -> errno
    linker.func_wrap(module, "clock_time_get", |mut caller: Caller<'_, TenantContext>, _id: i32, _precision: i64, time_ptr: i32| -> i32 {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let data = mem.data_mut(&mut caller);
            let start = time_ptr as usize;
            if start + 8 <= data.len() {
                data[start..start + 8].copy_from_slice(&now_ns.to_le_bytes());
                return 0;
            }
        }
        28
    })?;

    // random_get(buf: *mut u8, buf_len: i32) -> errno
    linker.func_wrap(module, "random_get", |mut caller: Caller<'_, TenantContext>, buf_ptr: i32, buf_len: i32| -> i32 {
        if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let data = mem.data_mut(&mut caller);
            let start = buf_ptr as usize;
            let len = buf_len as usize;
            if start + len <= data.len() {
                // Simple fast deterministic seed filling
                for i in 0..len {
                    data[start + i] = ((start + i * 31 + 17) & 0xFF) as u8;
                }
                return 0;
            }
        }
        28
    })?;

    // proc_exit(rval: i32)
    linker.func_wrap(module, "proc_exit", |_caller: Caller<'_, TenantContext>, _rval: i32| {
        // Exit acknowledged
    })?;

    // fd_write(fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32) -> errno
    linker.func_wrap(module, "fd_write", |mut caller: Caller<'_, TenantContext>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
        let mut total_written = 0u32;
        let mut collected_bytes = Vec::new();

        if let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let data = mem.data(&caller);
            let mut cur_iov = iovs_ptr as usize;
            for _ in 0..iovs_len {
                if cur_iov + 8 > data.len() {
                    break;
                }
                let ptr = u32::from_le_bytes(data[cur_iov..cur_iov + 4].try_into().unwrap()) as usize;
                let len = u32::from_le_bytes(data[cur_iov + 4..cur_iov + 8].try_into().unwrap()) as usize;
                cur_iov += 8;

                if ptr + len <= data.len() {
                    collected_bytes.extend_from_slice(&data[ptr..ptr + len]);
                    total_written += len as u32;
                }
            }

            if fd == 1 || fd == 2 {
                let msg = String::from_utf8_lossy(&collected_bytes).to_string();
                let log_entry = format!("[stdout/stderr] {}", msg.trim_end());
                caller.data_mut().logs.push(log_entry);
            }

            let mem = caller.get_export("memory").and_then(|e| e.into_memory()).unwrap();
            let data_mut = mem.data_mut(&mut caller);
            let n_start = nwritten_ptr as usize;
            if n_start + 4 <= data_mut.len() {
                data_mut[n_start..n_start + 4].copy_from_slice(&total_written.to_le_bytes());
                return 0; // SUCCESS
            }
        }
        28
    })?;

    // fd_close(fd: i32) -> errno
    linker.func_wrap(module, "fd_close", |_caller: Caller<'_, TenantContext>, _fd: i32| -> i32 {
        0
    })?;

    // fd_seek(fd: i32, offset: i64, whence: i32, newoffset_ptr: i32) -> errno
    linker.func_wrap(module, "fd_seek", |_caller: Caller<'_, TenantContext>, _fd: i32, _offset: i64, _whence: i32, _new_ptr: i32| -> i32 {
        0
    })?;

    // fd_read(fd: i32, iovs: i32, iovs_len: i32, nread: i32) -> errno
    linker.func_wrap(module, "fd_read", |_caller: Caller<'_, TenantContext>, _fd: i32, _iovs: i32, _len: i32, _nread: i32| -> i32 {
        0
    })?;

    Ok(())
}
