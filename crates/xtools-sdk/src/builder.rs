use xtools_protocol::*;

/// Create a vertical container (column)
pub fn column(children: Vec<UiNode>) -> UiNode {
    UiNode::Container {
        direction: Direction::Vertical,
        spacing: 8.0,
        padding: Padding::all(0.0),
        align: Alignment::Start,
        fill_width: true,
        fill_height: false,
        children,
    }
}

/// Create a horizontal container (row)
pub fn row(children: Vec<UiNode>) -> UiNode {
    UiNode::Container {
        direction: Direction::Horizontal,
        spacing: 8.0,
        padding: Padding::all(0.0),
        align: Alignment::Center,
        fill_width: true,
        fill_height: false,
        children,
    }
}

/// Create a visual card
pub fn card(children: Vec<UiNode>) -> UiNode {
    UiNode::Card {
        title: None,
        padding: Padding::all(12.0),
        children,
    }
}

/// Create a card with title
pub fn card_with_title(title: impl Into<String>, children: Vec<UiNode>) -> UiNode {
    UiNode::Card {
        title: Some(title.into()),
        padding: Padding::all(12.0),
        children,
    }
}

/// Create a standard label
pub fn label(text: impl Into<String>) -> UiNode {
    UiNode::Label {
        text: text.into(),
        variant: LabelVariant::Default,
        wrap: false,
        monospace: false,
    }
}

/// Create a title label
pub fn title_label(text: impl Into<String>) -> UiNode {
    UiNode::Label {
        text: text.into(),
        variant: LabelVariant::Title,
        wrap: false,
        monospace: false,
    }
}

/// Create a secondary/muted label
pub fn secondary_label(text: impl Into<String>) -> UiNode {
    UiNode::Label {
        text: text.into(),
        variant: LabelVariant::Secondary,
        wrap: true,
        monospace: false,
    }
}

/// Create an error label
pub fn error_label(text: impl Into<String>) -> UiNode {
    UiNode::Label {
        text: text.into(),
        variant: LabelVariant::Error,
        wrap: true,
        monospace: false,
    }
}

/// Create a single-line text input
pub fn text_input(id: impl Into<String>, value: impl Into<String>) -> UiNode {
    UiNode::TextInput {
        id: id.into(),
        label: None,
        value: value.into(),
        placeholder: String::new(),
        multiline: false,
        readonly: false,
        rows: None,
        on_change: false,
        monospace: false,
    }
}

/// Create a multi-line text area
pub fn text_area(id: impl Into<String>, value: impl Into<String>, rows: u32) -> UiNode {
    UiNode::TextInput {
        id: id.into(),
        label: None,
        value: value.into(),
        placeholder: String::new(),
        multiline: true,
        readonly: false,
        rows: Some(rows),
        on_change: false,
        monospace: false,
    }
}

/// Create a button
pub fn button(id: impl Into<String>, label: impl Into<String>) -> UiNode {
    UiNode::Button {
        id: id.into(),
        label: label.into(),
        variant: ButtonVariant::Secondary,
        icon: None,
        disabled: false,
        tooltip: None,
    }
}

/// Create a primary button
pub fn primary_button(id: impl Into<String>, label: impl Into<String>) -> UiNode {
    UiNode::Button {
        id: id.into(),
        label: label.into(),
        variant: ButtonVariant::Primary,
        icon: None,
        disabled: false,
        tooltip: None,
    }
}

/// Create a dropdown selector
pub fn select(
    id: impl Into<String>,
    options: Vec<SelectOption>,
    selected_index: usize,
) -> UiNode {
    UiNode::Select {
        id: id.into(),
        label: None,
        options,
        selected_index,
    }
}

/// Create a code editor / viewer
pub fn code_editor(
    id: impl Into<String>,
    value: impl Into<String>,
    language: impl Into<String>,
) -> UiNode {
    UiNode::CodeEditor {
        id: id.into(),
        value: value.into(),
        language: language.into(),
        readonly: false,
        height: None,
        line_numbers: true,
        wrap: false,
    }
}

/// Create a JSON tree viewer
pub fn json_tree_viewer(id: impl Into<String>, nodes: Vec<JsonTreeNode>) -> UiNode {
    UiNode::JsonTreeViewer {
        id: id.into(),
        nodes,
    }
}

/// Create a divider
pub fn divider() -> UiNode {
    UiNode::Divider
}

/// Create a spacer
pub fn spacer() -> UiNode {
    UiNode::Spacer { size: None }
}

/// Create fixed spacer
pub fn fixed_spacer(size: f32) -> UiNode {
    UiNode::Spacer { size: Some(size) }
}

/// Create a badge
pub fn badge(text: impl Into<String>, variant: BadgeVariant) -> UiNode {
    UiNode::Badge {
        text: text.into(),
        variant,
    }
}
