pub mod builder;
pub mod host;
pub mod macros;

pub use builder::*;
pub use host::*;
pub use xtools_protocol::*;

/// 把序列化后的 JSON 打包为 (ptr, len) 返回给宿主；序列化失败返回 0。
///
/// `into_boxed_slice()` 会收缩 `Vec` 的多余容量，保证 capacity == len：
/// 宿主侧 `_xtools_dealloc` 以 `Vec::from_raw_parts(ptr, len, len)` 重建后释放，
/// 若 capacity > len，释放布局会小于分配布局，属于未定义行为。
pub fn pack_json_to_host<T: serde::Serialize + ?Sized>(value: &T) -> u64 {
    match serde_json::to_vec(value) {
        Ok(bytes) => {
            let boxed = bytes.into_boxed_slice();
            let len = boxed.len() as u32;
            let ptr = Box::into_raw(boxed) as *mut u8;
            pack_ptr_len(ptr as u32, len)
        }
        Err(_) => 0,
    }
}

pub trait XPlugin: Sized + 'static {
    /// Return the static metadata and manifest of this plugin.
    fn manifest() -> PluginManifest;

    /// Initialize the plugin instance.
    fn init() -> Result<Self, String>;

    /// Render the current UI state.
    fn render(&self) -> UiView;

    /// Process a UI event or lifecycle event, returning the updated state or response.
    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String>;
}
