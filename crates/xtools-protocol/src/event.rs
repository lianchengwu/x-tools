use serde::{Deserialize, Serialize};
use crate::ui::{Toast, UiView};

/// Events emitted by the Native UI Runner and passed into the WASM plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UiEvent {
    /// Button click event
    Click { id: String },
    /// Input value changed event
    InputChanged { id: String, value: String },
    /// Dropdown selection changed event
    SelectChanged { id: String, index: usize, value: String },
    /// Switch toggle state changed event
    ToggleChanged { id: String, checked: bool },
    /// Tab switched event
    TabChanged { id: String, index: usize, tab_id: String },
    /// JSON Tree node expand/collapse toggle event
    JsonTreeToggle { id: String, node_id: usize },
    /// 异步请求完成回调：宿主后台线程执行完 AI 请求后回填给插件。
    /// error 非 None 表示请求失败（插件应回滚用户消息）；aborted 表示用户主动停止，
    /// content 为已生成的部分内容。
    AssistantDone {
        #[serde(default)]
        content: String,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        aborted: bool,
    },
    /// Periodic timer tick event (for clock updates, etc.)
    TimerTick,
    /// Window opened or gained focus
    Activated,
    /// Window closed or lost focus
    Deactivated,
    /// Custom extension event
    Custom { id: String, payload: String },
}

/// Response returned by the WASM plugin after processing an event or lifecycle hook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum UiResponse {
    /// Update the entire UI with a new view
    UpdateView(UiView),
    /// No visual changes needed
    NoChange,
    /// Display a toast notification without changing the entire view
    ShowToast(Toast),
    /// Request host to copy text to clipboard
    CopyToClipboard(String),
    /// Request the window to close
    CloseWindow,
}

impl From<UiView> for UiResponse {
    fn from(view: UiView) -> Self {
        Self::UpdateView(view)
    }
}
