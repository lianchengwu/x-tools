use serde::{Deserialize, Serialize};

/// Root structure of a declarative UI view returned by a WASM plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiView {
    /// Window title override
    #[serde(default)]
    pub title: Option<String>,
    /// Root layout node
    pub root: UiNode,
    /// Optional transient toast notification to display
    #[serde(default)]
    pub toast: Option<Toast>,
}

impl UiView {
    pub fn new(root: UiNode) -> Self {
        Self {
            title: None,
            root,
            toast: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_toast(mut self, toast: Toast) -> Self {
        self.toast = Some(toast);
        self
    }
}

/// A node in the declarative UI tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UiNode {
    /// Container arranging children along a vertical or horizontal axis
    Container {
        #[serde(default)]
        direction: Direction,
        #[serde(default)]
        spacing: f32,
        #[serde(default)]
        padding: Padding,
        #[serde(default)]
        align: Alignment,
        #[serde(default)]
        fill_width: bool,
        #[serde(default)]
        fill_height: bool,
        children: Vec<UiNode>,
    },
    /// Visual card / container with background, border, and optional title
    Card {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        padding: Padding,
        children: Vec<UiNode>,
    },
    /// Text label
    Label {
        text: String,
        #[serde(default)]
        variant: LabelVariant,
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        monospace: bool,
    },
    /// Text input field (single-line or multi-line)
    TextInput {
        id: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        readonly: bool,
        #[serde(default)]
        rows: Option<u32>,
        #[serde(default)]
        on_change: bool,
        #[serde(default)]
        monospace: bool,
    },
    /// Clickable action button
    Button {
        id: String,
        label: String,
        #[serde(default)]
        variant: ButtonVariant,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        disabled: bool,
        #[serde(default)]
        tooltip: Option<String>,
    },
    /// Dropdown / combobox selector
    Select {
        id: String,
        #[serde(default)]
        label: Option<String>,
        options: Vec<SelectOption>,
        #[serde(default)]
        selected_index: usize,
    },
    /// Boolean toggle switch
    Switch {
        id: String,
        label: String,
        #[serde(default)]
        checked: bool,
    },
    /// Code viewer or editor with line numbers and monospace font
    CodeEditor {
        id: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        language: String,
        #[serde(default)]
        readonly: bool,
        #[serde(default)]
        height: Option<f32>,
        #[serde(default = "default_true")]
        line_numbers: bool,
        #[serde(default)]
        wrap: bool,
    },
    /// Collapsible JSON tree viewer node list
    JsonTreeViewer {
        id: String,
        nodes: Vec<JsonTreeNode>,
    },
    /// Scrollable chat conversation view (AI assistant)
    Chat {
        id: String,
        messages: Vec<ChatMessage>,
    },
    /// Horizontal or vertical divider line
    Divider,
    /// Flexible or fixed empty spacing
    Spacer {
        #[serde(default)]
        size: Option<f32>,
    },
    /// Multi-tab container
    Tabs {
        id: String,
        #[serde(default)]
        active_index: usize,
        tabs: Vec<TabItem>,
    },
    /// Status badge chip
    Badge {
        text: String,
        #[serde(default)]
        variant: BadgeVariant,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const fn all(val: f32) -> Self {
        Self { top: val, right: val, bottom: val, left: val }
    }

    pub const fn symmetric(v: f32, h: f32) -> Self {
        Self { top: v, right: h, bottom: v, left: h }
    }

    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LabelVariant {
    #[default]
    Default,
    Title,
    Subtitle,
    Secondary,
    Muted,
    Error,
    Success,
    Warning,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ButtonVariant {
    #[default]
    Secondary,
    Primary,
    Outline,
    Danger,
    Ghost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BadgeVariant {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabItem {
    pub id: String,
    pub label: String,
    pub content: Box<UiNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonTreeNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub depth: usize,
    pub key: String,
    pub value_preview: String,
    pub node_type: String,
    #[serde(default)]
    pub summary_text: String,
    pub is_leaf: bool,
    pub collapsed: bool,
    #[serde(default)]
    pub has_comma: bool,
    pub line_start: usize,
    pub line_end: usize,
}

/// Role of a chat conversation participant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChatRole {
    #[default]
    User,
    Assistant,
}

/// A single message in a chat conversation view
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toast {
    pub message: String,
    #[serde(default)]
    pub level: ToastLevel,
    #[serde(default = "default_toast_duration")]
    pub duration_ms: u32,
}

fn default_toast_duration() -> u32 {
    2500
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ToastLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}
