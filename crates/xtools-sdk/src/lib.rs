pub mod builder;
pub mod host;
pub mod macros;

pub use builder::*;
pub use host::*;
pub use xtools_protocol::*;

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
