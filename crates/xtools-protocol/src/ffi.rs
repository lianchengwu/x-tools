//! Low-level FFI constants, memory packing, and export symbols.

pub const EXPORT_ALLOC: &str = "_xtools_alloc";
pub const EXPORT_DEALLOC: &str = "_xtools_dealloc";
pub const EXPORT_MANIFEST: &str = "xtools_plugin_manifest";
pub const EXPORT_INIT: &str = "xtools_plugin_init";
pub const EXPORT_RENDER: &str = "xtools_plugin_render";
pub const EXPORT_HANDLE_EVENT: &str = "xtools_plugin_handle_event";

pub const HOST_MODULE: &str = "xtools_host";
pub const HOST_LOG: &str = "host_log";
pub const HOST_CLIPBOARD_READ: &str = "host_clipboard_read";
pub const HOST_CLIPBOARD_WRITE: &str = "host_clipboard_write";
pub const HOST_HTTP_REQUEST: &str = "host_http_request";
pub const HOST_STORAGE_GET: &str = "host_storage_get";
pub const HOST_STORAGE_SET: &str = "host_storage_set";
pub const HOST_NOW_MILLIS: &str = "host_now_millis";

/// Pack a 32-bit pointer and 32-bit length into a single 64-bit integer for WASM function returns.
#[inline]
pub const fn pack_ptr_len(ptr: u32, len: u32) -> u64 {
    (ptr as u64) | ((len as u64) << 32)
}

/// Unpack a 64-bit integer into a 32-bit pointer and 32-bit length.
#[inline]
pub const fn unpack_ptr_len(val: u64) -> (u32, u32) {
    let ptr = (val & 0xFFFF_FFFF) as u32;
    let len = (val >> 32) as u32;
    (ptr, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_unpack() {
        let ptr = 0x1234_5678;
        let len = 0x9ABC_DEF0;
        let packed = pack_ptr_len(ptr, len);
        let (unpacked_ptr, unpacked_len) = unpack_ptr_len(packed);
        assert_eq!(ptr, unpacked_ptr);
        assert_eq!(len, unpacked_len);
    }
}
