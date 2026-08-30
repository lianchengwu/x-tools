use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use arboard::Clipboard;
use parking_lot::Mutex;
use wasmtime::{Caller, Linker};
use xtools_protocol::*;

use crate::error::RuntimeError;

/// Host runtime context passed into the Wasmtime Store for each plugin instance.
pub struct HostContext {
    pub plugin_id: String,
    /// 插件键值存储的根目录（SQLite 库位于其下 storage.db）
    pub storage_root: PathBuf,
    pub clipboard: Arc<Mutex<Option<Clipboard>>>,
    pub http_client: ureq::Agent,
}

/// 构建宿主 HTTP 客户端。
///
/// `http_status_as_error(false)`：4xx/5xx 作为正常响应透传给插件（保留真实状态码与
/// 响应体），由插件自行判断 `is_success()`；否则 ureq 会丢弃错误响应，插件只能看到
/// 无意义的 502。
fn build_http_agent(global_timeout: std::time::Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(global_timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

impl HostContext {
    pub fn new(plugin_id: String, storage_root: PathBuf) -> Self {
        let clipboard = match Clipboard::new() {
            Ok(cb) => Arc::new(Mutex::new(Some(cb))),
            Err(e) => {
                log::warn!("Failed to initialize clipboard backend: {e}");
                Arc::new(Mutex::new(None))
            }
        };

        let http_client = build_http_agent(std::time::Duration::from_secs(10));

        Self {
            plugin_id,
            storage_root,
            clipboard,
            http_client,
        }
    }
}

/// Helper function to allocate memory inside WASM and copy host bytes into it.
fn alloc_and_write(
    mut caller: &mut Caller<'_, HostContext>,
    bytes: &[u8],
) -> Result<(u32, u32), wasmtime::Error> {
    let alloc_func = caller
        .get_export(EXPORT_ALLOC)
        .and_then(|ext| ext.into_func())
        .ok_or_else(|| wasmtime::Error::msg("Missing _xtools_alloc export"))?;

    let alloc_typed = alloc_func.typed::<u32, u32>(&caller)?;
    let len = bytes.len() as u32;
    let ptr = alloc_typed.call(&mut caller, len)?;

    let memory = caller
        .get_export("memory")
        .and_then(|ext| ext.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("Missing memory export"))?;

    memory.write(&mut caller, ptr as usize, bytes)?;
    Ok((ptr, len))
}

/// Register all host capability imports into the Wasmtime Linker.
pub fn register_host_functions(linker: &mut Linker<HostContext>) -> Result<(), RuntimeError> {
    // 1. host_log
    linker.func_wrap(
        HOST_MODULE,
        HOST_LOG,
        |mut caller: Caller<'_, HostContext>, level: u32, ptr: u32, len: u32| {
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(m) => m,
                None => return,
            };

            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_ok() {
                let msg = String::from_utf8_lossy(&buf);
                match level {
                    0 => log::trace!("[plugin:{}] {}", caller.data().plugin_id, msg),
                    1 => log::debug!("[plugin:{}] {}", caller.data().plugin_id, msg),
                    2 => log::info!("[plugin:{}] {}", caller.data().plugin_id, msg),
                    3 => log::warn!("[plugin:{}] {}", caller.data().plugin_id, msg),
                    _ => log::error!("[plugin:{}] {}", caller.data().plugin_id, msg),
                }
            }
        },
    )?;

    // 2. host_clipboard_read
    linker.func_wrap(
        HOST_MODULE,
        HOST_CLIPBOARD_READ,
        |mut caller: Caller<'_, HostContext>, out_ptr_ptr: u32, out_len_ptr: u32| -> i32 {
            let text = {
                let mut guard = caller.data().clipboard.lock();
                guard
                    .as_mut()
                    .and_then(|cb| cb.get_text().ok())
                    .unwrap_or_default()
            };

            let bytes = text.into_bytes();
            match alloc_and_write(&mut caller, &bytes) {
                Ok((ptr, len)) => {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return -1,
                    };
                    let _ = memory.write(&mut caller, out_ptr_ptr as usize, &ptr.to_le_bytes());
                    let _ = memory.write(&mut caller, out_len_ptr as usize, &len.to_le_bytes());
                    0
                }
                Err(e) => {
                    log::error!("Clipboard read alloc failed: {e}");
                    -1
                }
            }
        },
    )?;

    // 3. host_clipboard_write
    linker.func_wrap(
        HOST_MODULE,
        HOST_CLIPBOARD_WRITE,
        |mut caller: Caller<'_, HostContext>, ptr: u32, len: u32| -> i32 {
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(m) => m,
                None => return -1,
            };

            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_err() {
                return -1;
            }

            let text = match String::from_utf8(buf) {
                Ok(s) => s,
                Err(_) => return -2,
            };

            let mut guard = caller.data().clipboard.lock();
            if let Some(cb) = guard.as_mut() {
                if let Err(e) = cb.set_text(text) {
                    log::warn!("Clipboard write failed: {e}");
                    return -3;
                }
            }
            0
        },
    )?;

    // 4. host_http_request
    linker.func_wrap(
        HOST_MODULE,
        HOST_HTTP_REQUEST,
        |mut caller: Caller<'_, HostContext>,
         req_ptr: u32,
         req_len: u32,
         res_ptr_ptr: u32,
         res_len_ptr: u32|
         -> i32 {
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(m) => m,
                None => return -1,
            };

            let mut req_buf = vec![0u8; req_len as usize];
            if memory.read(&caller, req_ptr as usize, &mut req_buf).is_err() {
                return -1;
            }

            let req: HttpRequest = match serde_json::from_slice(&req_buf) {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Failed to parse HttpRequest from WASM: {e}");
                    return -2;
                }
            };

            // 插件可通过 HttpRequest.timeout_ms 请求更长的超时（如 LLM 长回答），
            // 未指定时沿用宿主默认的 10s 全局超时。
            let client = match req.timeout_ms {
                Some(ms) if ms > 0 => {
                    crate::host_env::build_http_agent(std::time::Duration::from_millis(ms))
                }
                _ => caller.data().http_client.clone(),
            };
            let method = req.method.to_uppercase();
            let http_res = match method.as_str() {
                "GET" => {
                    let mut r = client.get(&req.url);
                    for (k, v) in &req.headers {
                        r = r.header(k, v);
                    }
                    r.call()
                }
                "DELETE" => {
                    let mut r = client.delete(&req.url);
                    for (k, v) in &req.headers {
                        r = r.header(k, v);
                    }
                    r.call()
                }
                "PUT" => {
                    let mut r = client.put(&req.url);
                    for (k, v) in &req.headers {
                        r = r.header(k, v);
                    }
                    if let Some(body) = req.body {
                        r.send(body.as_slice())
                    } else {
                        r.send_empty()
                    }
                }
                _ => {
                    let mut r = client.post(&req.url);
                    for (k, v) in &req.headers {
                        r = r.header(k, v);
                    }
                    if let Some(body) = req.body {
                        r.send(body.as_slice())
                    } else {
                        r.send_empty()
                    }
                }
            };
            let response = match http_res {
                Ok(mut resp) => {
                    let status = resp.status().as_u16();
                    let mut headers = Vec::new();
                    for (k, v) in resp.headers() {
                        if let Ok(v_str) = v.to_str() {
                            headers.push((k.as_str().to_string(), v_str.to_string()));
                        }
                    }
                    let body = resp.body_mut().read_to_vec().unwrap_or_default();
                    HttpResponse {
                        status,
                        headers,
                        body,
                    }
                }
                Err(e) => {
                    log::warn!("HTTP request error: {e}");
                    HttpResponse {
                        status: 502,
                        headers: Vec::new(),
                        body: format!("HTTP error: {e}").into_bytes(),
                    }
                }
            };

            let resp_bytes = match serde_json::to_vec(&response) {
                Ok(b) => b,
                Err(_) => return -3,
            };

            match alloc_and_write(&mut caller, &resp_bytes) {
                Ok((ptr, len)) => {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return -4,
                    };
                    let _ = memory.write(&mut caller, res_ptr_ptr as usize, &ptr.to_le_bytes());
                    let _ = memory.write(&mut caller, res_len_ptr as usize, &len.to_le_bytes());
                    0
                }
                Err(e) => {
                    log::error!("HTTP response alloc failed: {e}");
                    -5
                }
            }
        },
    )?;

    // 5. host_storage_get
    linker.func_wrap(
        HOST_MODULE,
        HOST_STORAGE_GET,
        |mut caller: Caller<'_, HostContext>,
         key_ptr: u32,
         key_len: u32,
         out_ptr_ptr: u32,
         out_len_ptr: u32|
         -> i32 {
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(m) => m,
                None => return -1,
            };

            let mut key_buf = vec![0u8; key_len as usize];
            if memory.read(&caller, key_ptr as usize, &mut key_buf).is_err() {
                return -1;
            }

            let key = match String::from_utf8(key_buf) {
                Ok(k) => k,
                Err(_) => return -2,
            };

            let root = caller.data().storage_root.clone();
            let bytes = crate::storage::read_from(&root, &caller.data().plugin_id, &key)
                .unwrap_or_default();

            if bytes.is_empty() {
                let _ = memory.write(&mut caller, out_ptr_ptr as usize, &0u32.to_le_bytes());
                let _ = memory.write(&mut caller, out_len_ptr as usize, &0u32.to_le_bytes());
                return 0;
            }

            match alloc_and_write(&mut caller, &bytes) {
                Ok((ptr, len)) => {
                    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                        Some(m) => m,
                        None => return -3,
                    };
                    let _ = memory.write(&mut caller, out_ptr_ptr as usize, &ptr.to_le_bytes());
                    let _ = memory.write(&mut caller, out_len_ptr as usize, &len.to_le_bytes());
                    0
                }
                Err(_) => -4,
            }
        },
    )?;

    // 6. host_storage_set
    linker.func_wrap(
        HOST_MODULE,
        HOST_STORAGE_SET,
        |mut caller: Caller<'_, HostContext>,
         key_ptr: u32,
         key_len: u32,
         val_ptr: u32,
         val_len: u32|
         -> i32 {
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(m) => m,
                None => return -1,
            };

            let mut key_buf = vec![0u8; key_len as usize];
            let mut val_buf = vec![0u8; val_len as usize];
            if memory.read(&caller, key_ptr as usize, &mut key_buf).is_err()
                || memory.read(&caller, val_ptr as usize, &mut val_buf).is_err()
            {
                return -1;
            }

            let key = match String::from_utf8(key_buf) {
                Ok(k) => k,
                Err(_) => return -2,
            };

            let root = caller.data().storage_root.clone();
            let plugin_id = caller.data().plugin_id.clone();
            if let Err(e) = crate::storage::write_to(&root, &plugin_id, &key, &val_buf) {
                log::error!("Failed to write storage {plugin_id}/{key}: {e}");
                return -4;
            }

            0
        },
    )?;

    // 7. host_now_millis
    linker.func_wrap(
        HOST_MODULE,
        HOST_NOW_MILLIS,
        |_caller: Caller<'_, HostContext>| -> i64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        },
    )?;

    Ok(())
}
