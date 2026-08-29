/// Compile-time menu entries. No plugin directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ToolId {
    Time,
    Json,
    Trans,
}

impl ToolId {
    /// Left-to-right fan order: Time, Json, Trans.
    pub const ALL: [ToolId; 3] = [ToolId::Time, ToolId::Json, ToolId::Trans];

    /// On-disk mark. Clock is drawn as a glyph in the host; others are pango text.
    pub fn mark(self) -> &'static str {
        match self {
            ToolId::Time => "clock",
            ToolId::Json => "{}",
            ToolId::Trans => "文",
        }
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            ToolId::Time => "xtools-time",
            ToolId::Json => "xtools-json",
            ToolId::Trans => "xtools-trans",
        }
    }

    pub fn instance_name(self) -> &'static str {
        match self {
            ToolId::Time => TIME_INSTANCE,
            ToolId::Json => JSON_INSTANCE,
            ToolId::Trans => TRANS_INSTANCE,
        }
    }
}

/// Abstract single-instance name for the host process (`\0` + this).
pub const HOST_INSTANCE: &str = "xtools-host";

/// Abstract single-instance name for the timestamp tool.
pub const TIME_INSTANCE: &str = "xtools-time";

/// Abstract single-instance name for the JSON tool.
pub const JSON_INSTANCE: &str = "xtools-json";

/// Abstract single-instance name for the translate tool.
pub const TRANS_INSTANCE: &str = "xtools-trans";
