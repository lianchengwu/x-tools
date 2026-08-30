use xtools_protocol::*;

#[cfg(target_arch = "wasm32")]
mod sys {
    #[link(wasm_import_module = "xtools_host")]
    unsafe extern "C" {
        pub fn host_log(level: u32, ptr: *const u8, len: u32);
        pub fn host_clipboard_read(out_ptr_ptr: *mut *mut u8, out_len_ptr: *mut u32) -> i32;
        pub fn host_clipboard_write(ptr: *const u8, len: u32) -> i32;
        pub fn host_http_request(
            req_ptr: *const u8,
            req_len: u32,
            res_ptr_ptr: *mut *mut u8,
            res_len_ptr: *mut u32,
        ) -> i32;
        pub fn host_storage_get(
            key_ptr: *const u8,
            key_len: u32,
            out_ptr_ptr: *mut *mut u8,
            out_len_ptr: *mut u32,
        ) -> i32;
        pub fn host_storage_set(
            key_ptr: *const u8,
            key_len: u32,
            val_ptr: *const u8,
            val_len: u32,
        ) -> i32;
        pub fn host_now_millis() -> i64;
    }
}

/// Log message through the host logging facility
pub fn log(level: LogLevel, message: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        sys::host_log(level as u32, message.as_ptr(), message.len() as u32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (level, message);
    }
}

pub fn log_info(message: &str) {
    log(LogLevel::Info, message);
}

pub fn log_error(message: &str) {
    log(LogLevel::Error, message);
}

/// Read text from the system clipboard
pub fn clipboard_read() -> Result<String, String> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: u32 = 0;
        let res = sys::host_clipboard_read(&mut out_ptr, &mut out_len);
        if res < 0 {
            return Err(match res {
                ERR_PERM_CLIPBOARD => "剪贴板访问被宿主拒绝：manifest 未声明 Clipboard 权限".to_string(),
                _ => format!("Failed to read clipboard (code {res})"),
            });
        }
        if out_ptr.is_null() || out_len == 0 {
            return Ok(String::new());
        }
        let slice = std::slice::from_raw_parts(out_ptr, out_len as usize);
        let s = String::from_utf8_lossy(slice).into_owned();
        // Deallocate host-allocated buffer
        let _ = Vec::from_raw_parts(out_ptr, out_len as usize, out_len as usize);
        Ok(s)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(String::new())
    }
}

/// Write text to the system clipboard
pub fn clipboard_write(text: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let res = sys::host_clipboard_write(text.as_ptr(), text.len() as u32);
        if res < 0 {
            Err(match res {
                ERR_PERM_CLIPBOARD => "剪贴板访问被宿主拒绝：manifest 未声明 Clipboard 权限".to_string(),
                _ => format!("Failed to write clipboard (code {res})"),
            })
        } else {
            Ok(())
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
        Ok(())
    }
}

/// Execute an HTTP request through host capability
pub fn http_request(req: HttpRequest) -> Result<HttpResponse, String> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let json_bytes = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: u32 = 0;
        let res = sys::host_http_request(
            json_bytes.as_ptr(),
            json_bytes.len() as u32,
            &mut out_ptr,
            &mut out_len,
        );
        if res < 0 {
            return Err(match res {
                ERR_PERM_HTTP => {
                    "HTTP 请求被宿主拒绝：目标地址不在 manifest 的 Http 白名单内".to_string()
                }
                _ => format!("Host HTTP request failed with code {res}"),
            });
        }
        if out_ptr.is_null() || out_len == 0 {
            return Err("Empty response from host".to_string());
        }
        let slice = std::slice::from_raw_parts(out_ptr, out_len as usize);
        let resp: Result<HttpResponse, _> = serde_json::from_slice(slice);
        // Free buffer
        let _ = Vec::from_raw_parts(out_ptr, out_len as usize, out_len as usize);
        resp.map_err(|e| e.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = req;
        Err("HTTP not supported on mock target".to_string())
    }
}

/// Retrieve stored value by key
pub fn storage_get(key: &str) -> Result<Option<Vec<u8>>, String> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut out_ptr: *mut u8 = std::ptr::null_mut();
        let mut out_len: u32 = 0;
        let res = sys::host_storage_get(
            key.as_ptr(),
            key.len() as u32,
            &mut out_ptr,
            &mut out_len,
        );
        if res < 0 {
            return Err(match res {
                ERR_PERM_STORAGE => "存储访问被宿主拒绝：manifest 未声明 Storage 权限".to_string(),
                _ => format!("Host storage_get failed with code {res}"),
            });
        }
        if out_ptr.is_null() || out_len == 0 {
            return Ok(None);
        }
        let vec = Vec::from_raw_parts(out_ptr, out_len as usize, out_len as usize);
        Ok(Some(vec))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        Ok(None)
    }
}

/// Save value by key
pub fn storage_set(key: &str, val: &[u8]) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let res = sys::host_storage_set(
            key.as_ptr(),
            key.len() as u32,
            val.as_ptr(),
            val.len() as u32,
        );
        if res < 0 {
            Err(match res {
                ERR_PERM_STORAGE => "存储访问被宿主拒绝：manifest 未声明 Storage 权限".to_string(),
                _ => format!("Host storage_set failed with code {res}"),
            })
        } else {
            Ok(())
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (key, val);
        Ok(())
    }
}

/// Get current timestamp in milliseconds
pub fn now_millis() -> i64 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        sys::host_now_millis()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}
