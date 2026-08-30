use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique identifier of the plugin, e.g. "xtools.time"
    pub id: String,
    /// Display name shown in UI and titlebar, e.g. "时间戳转换"
    pub name: String,
    /// Semantic version of the plugin, e.g. "0.1.0"
    pub version: String,
    /// Short summary of what the plugin does
    #[serde(default)]
    pub description: String,
    /// Author / Organization
    #[serde(default)]
    pub author: String,
    /// Text or symbol mark drawn on the floating orbital ball (e.g. "clock", "{}", "文")
    #[serde(default = "default_mark")]
    pub mark: String,
    /// Optional SVG icon content for high-res rendering or custom styling
    #[serde(default)]
    pub icon_svg: Option<String>,
    /// Window dimensions and capabilities
    #[serde(default)]
    pub window: WindowConfig,
    /// Permissions requested by the plugin
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

fn default_mark() -> String {
    "•".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Default window width in logical pixels
    pub width: u32,
    /// Default window height in logical pixels
    pub height: u32,
    /// Whether user can resize the window
    pub resizable: bool,
    /// Custom window title if different from manifest name
    pub title: Option<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 480,
            height: 380,
            resizable: true,
            title: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum Permission {
    /// Ability to read and write system clipboard
    Clipboard,
    /// Ability to make outbound HTTP requests to specific domains or wildcard
    Http(Vec<String>),
    /// Ability to read and persist key-value storage in plugin-scoped storage
    Storage,
    /// Periodic timer tick events
    Timer { interval_ms: u32 },
}
