use serde::{Deserialize, Serialize};
use serde_json::Value;
use xtools_sdk::JsonTreeNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonIssue {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl JsonIssue {
    pub fn display(&self) -> String {
        format!("第 {} 行第 {} 列：{}", self.line, self.column, self.message)
    }
}

pub fn empty_input(text: &str) -> bool {
    text.trim().is_empty()
}

pub fn format_json(input: &str) -> Result<String, JsonIssue> {
    let value = parse(input)?;
    serde_json::to_string_pretty(&value).map_err(from_error)
}

pub fn minify_json(input: &str) -> Result<String, JsonIssue> {
    let value = parse(input)?;
    serde_json::to_string(&value).map_err(from_error)
}

pub fn validate_json(input: &str) -> Result<(), JsonIssue> {
    parse(input).map(|_| ())
}

pub fn parse(input: &str) -> Result<Value, JsonIssue> {
    serde_json::from_str(input).map_err(from_error)
}

/// Unescape stringified / escaped JSON text.
///
/// Handles:
/// 1. Double-quoted JSON strings: `"{\"a\": 1}"` -> `{"a": 1}`
/// 2. Escaped JSON without outer quotes: `{\"a\": 1}` -> `{"a": 1}`
/// 3. Backslash escapes: `\"`, `\\`, `\n`, `\t`, `\/`, `\r`, `\uXXXX`
///
/// If the unescaped result is valid JSON, formats it nicely; otherwise returns the unescaped text.
pub fn unescape_json(input: &str) -> Result<String, JsonIssue> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Case 1: The entire input is a valid double-quoted JSON string literal
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        if let Ok(parsed_str) = serde_json::from_str::<String>(trimmed) {
            if let Ok(json_val) = serde_json::from_str::<Value>(&parsed_str) {
                return serde_json::to_string_pretty(&json_val).map_err(from_error);
            } else {
                return Ok(parsed_str);
            }
        }
    }

    // Case 2: Escaped JSON not wrapped in quotes (e.g. `{\"a\": 1}`)
    let unescaped = unescape_raw_string(trimmed);

    // If unescaped string is valid JSON, format it nicely
    if let Ok(json_val) = serde_json::from_str::<Value>(&unescaped) {
        return serde_json::to_string_pretty(&json_val).map_err(from_error);
    }

    // Case 3: Try wrapping in quotes and parsing as JSON string literal
    let wrapped = format!("\"{}\"", trimmed);
    if let Ok(parsed_str) = serde_json::from_str::<String>(&wrapped) {
        if let Ok(json_val) = serde_json::from_str::<Value>(&parsed_str) {
            return serde_json::to_string_pretty(&json_val).map_err(from_error);
        }
    }

    Ok(unescaped)
}

fn unescape_raw_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0c'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let mut hex = String::with_capacity(4);
                    for _ in 0..4 {
                        if let Some(&hc) = chars.peek() {
                            if hc.is_ascii_hexdigit() {
                                hex.push(hc);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4 {
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                                continue;
                            }
                        }
                    }
                    out.push('\\');
                    out.push('u');
                    out.push_str(&hex);
                }
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }

    out
}

fn from_error(err: serde_json::Error) -> JsonIssue {
    let full = err.to_string();
    let message = full
        .split(" at line ")
        .next()
        .unwrap_or(&full)
        .trim()
        .to_string();
    JsonIssue {
        line: err.line(),
        column: err.column(),
        message,
    }
}

// -----------------------------------------------------------------------------
// JSON Tree Folding Model
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
    CloseBrace,
    CloseBracket,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Object => "object",
            NodeType::Array => "array",
            NodeType::String => "string",
            NodeType::Number => "number",
            NodeType::Boolean => "boolean",
            NodeType::Null => "null",
            NodeType::CloseBrace | NodeType::CloseBracket => "close",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTreeNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub depth: usize,
    pub key_text: String,
    pub node_type: String,
    pub value_text: String,
    pub summary_text: String,
    pub is_expandable: bool,
    pub is_expanded: bool,
    pub has_comma: bool,
    pub children: Vec<usize>,
    pub close_node_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonTree {
    pub nodes: Vec<RawTreeNode>,
}

impl JsonTree {
    pub fn from_value(value: &Value) -> Self {
        let mut nodes = Vec::new();
        let _ = build_tree_recursive(value, None, 0, String::new(), false, &mut nodes);
        Self { nodes }
    }

    pub fn toggle(&mut self, node_id: usize) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            if node.is_expandable {
                node.is_expanded = !node.is_expanded;
            }
        }
    }

    pub fn expand_all(&mut self) {
        for node in &mut self.nodes {
            if node.is_expandable {
                node.is_expanded = true;
            }
        }
    }

    pub fn collapse_all(&mut self) {
        for node in &mut self.nodes {
            if node.is_expandable {
                node.is_expanded = false;
            }
        }
    }

    pub fn fold_level(&mut self, max_depth: usize) {
        for node in &mut self.nodes {
            if node.is_expandable {
                node.is_expanded = node.depth < max_depth;
            }
        }
    }

    /// Compute visible items for Slint model based on expanded state
    pub fn visible_nodes(&self) -> Vec<JsonTreeNode> {
        let mut raw_visible = Vec::new();
        if self.nodes.is_empty() {
            return Vec::new();
        }

        self.collect_visible(0, &mut raw_visible);

        raw_visible
            .into_iter()
            .map(|r| JsonTreeNode {
                id: r.id,
                parent: r.parent,
                depth: r.depth,
                key: r.key_text,
                value_preview: r.value_text,
                node_type: r.node_type,
                summary_text: r.summary_text,
                is_leaf: !r.is_expandable,
                collapsed: !r.is_expanded,
                has_comma: r.has_comma,
                line_start: r.id,
                line_end: r.id,
            })
            .collect()
    }

    fn collect_visible(&self, node_id: usize, visible: &mut Vec<RawTreeNode>) {
        if node_id >= self.nodes.len() {
            return;
        }
        let node = &self.nodes[node_id];
        visible.push(node.clone());

        if node.is_expandable && node.is_expanded {
            for &child_id in &node.children {
                self.collect_visible(child_id, visible);
            }
            if let Some(close_id) = node.close_node_id {
                if close_id < self.nodes.len() {
                    visible.push(self.nodes[close_id].clone());
                }
            }
        }
    }
}

fn build_tree_recursive(
    value: &Value,
    parent: Option<usize>,
    depth: usize,
    key_text: String,
    has_comma: bool,
    nodes: &mut Vec<RawTreeNode>,
) -> usize {
    let id = nodes.len();
    match value {
        Value::Object(map) => {
            let is_empty = map.is_empty();
            let summary_text = format!("{{ {} 项 }}", map.len());
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::Object.as_str().to_string(),
                value_text: if is_empty { "{}".into() } else { "{".into() },
                summary_text,
                is_expandable: !is_empty,
                is_expanded: true,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);

            if !is_empty {
                let mut child_ids = Vec::with_capacity(map.len());
                let total = map.len();
                for (idx, (k, v)) in map.iter().enumerate() {
                    let child_comma = idx + 1 < total;
                    let k_display = format!("\"{}\": ", k);
                    let child_id =
                        build_tree_recursive(v, Some(id), depth + 1, k_display, child_comma, nodes);
                    child_ids.push(child_id);
                }

                // Add closing node
                let close_id = nodes.len();
                let close_node = RawTreeNode {
                    id: close_id,
                    parent: Some(id),
                    depth,
                    key_text: String::new(),
                    node_type: NodeType::CloseBrace.as_str().to_string(),
                    value_text: "}".into(),
                    summary_text: String::new(),
                    is_expandable: false,
                    is_expanded: false,
                    has_comma,
                    children: Vec::new(),
                    close_node_id: None,
                };
                nodes.push(close_node);

                nodes[id].children = child_ids;
                nodes[id].close_node_id = Some(close_id);
            }

            id
        }
        Value::Array(arr) => {
            let is_empty = arr.is_empty();
            let summary_text = format!("[ {} 项 ]", arr.len());
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::Array.as_str().to_string(),
                value_text: if is_empty { "[]".into() } else { "[".into() },
                summary_text,
                is_expandable: !is_empty,
                is_expanded: true,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);

            if !is_empty {
                let mut child_ids = Vec::with_capacity(arr.len());
                let total = arr.len();
                for (idx, v) in arr.iter().enumerate() {
                    let child_comma = idx + 1 < total;
                    let k_display = format!("[{}]: ", idx);
                    let child_id =
                        build_tree_recursive(v, Some(id), depth + 1, k_display, child_comma, nodes);
                    child_ids.push(child_id);
                }

                let close_id = nodes.len();
                let close_node = RawTreeNode {
                    id: close_id,
                    parent: Some(id),
                    depth,
                    key_text: String::new(),
                    node_type: NodeType::CloseBracket.as_str().to_string(),
                    value_text: "]".into(),
                    summary_text: String::new(),
                    is_expandable: false,
                    is_expanded: false,
                    has_comma,
                    children: Vec::new(),
                    close_node_id: None,
                };
                nodes.push(close_node);

                nodes[id].children = child_ids;
                nodes[id].close_node_id = Some(close_id);
            }

            id
        }
        Value::String(s) => {
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::String.as_str().to_string(),
                value_text: format!("\"{}\"", s),
                summary_text: String::new(),
                is_expandable: false,
                is_expanded: false,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);
            id
        }
        Value::Number(n) => {
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::Number.as_str().to_string(),
                value_text: n.to_string(),
                summary_text: String::new(),
                is_expandable: false,
                is_expanded: false,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);
            id
        }
        Value::Bool(b) => {
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::Boolean.as_str().to_string(),
                value_text: b.to_string(),
                summary_text: String::new(),
                is_expandable: false,
                is_expanded: false,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);
            id
        }
        Value::Null => {
            let node = RawTreeNode {
                id,
                parent,
                depth,
                key_text,
                node_type: NodeType::Null.as_str().to_string(),
                value_text: "null".into(),
                summary_text: String::new(),
                is_expandable: false,
                is_expanded: false,
                has_comma,
                children: Vec::new(),
                close_node_id: None,
            };
            nodes.push(node);
            id
        }
    }
}

pub fn build_json_tree(value: &Value) -> Vec<JsonTreeNode> {
    let tree = JsonTree::from_value(value);
    tree.visible_nodes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_object() {
        let out = format_json("{\"a\":1}").unwrap();
        assert_eq!(out, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn minifies_object() {
        let out = minify_json("{\n  \"a\": 1\n}").unwrap();
        assert_eq!(out, "{\"a\":1}");
    }

    #[test]
    fn parses_nested() {
        let val = parse("{\"arr\":[1,2,true,null,\"str\"]}").unwrap();
        assert!(val.is_object());
    }

    #[test]
    fn reports_error_line_col() {
        let err = parse("{\n  \"a\": \n}").unwrap_err();
        assert_eq!(err.line, 3);
        assert_eq!(err.column, 1);
    }

    #[test]
    fn test_unescape_double_quoted() {
        let input = r#""{\"name\": \"xtools\", \"num\": 42}""#;
        let out = unescape_json(input).unwrap();
        assert!(out.contains("\"name\": \"xtools\""));
    }

    #[test]
    fn test_unescape_unquoted_slashes() {
        let input = r#"{\"a\": 1, \"b\": \"hello\nworld\"}"#;
        let out = unescape_json(input).unwrap();
        assert!(out.contains("\"a\": 1"));
    }

    #[test]
    fn test_json_tree_building_and_folding() {
        let val = parse("{\"obj\": {\"nested\": 1}, \"arr\": [10, 20]}").unwrap();
        let mut tree = JsonTree::from_value(&val);
        assert_eq!(tree.nodes.len(), 9);

        let visible = tree.visible_nodes();
        assert_eq!(visible.len(), 9);

        // Collapse root obj
        tree.toggle(0);
        let visible_collapsed = tree.visible_nodes();
        assert_eq!(visible_collapsed.len(), 1);

        // Expand all
        tree.expand_all();
        assert_eq!(tree.visible_nodes().len(), 9);

        // Fold level 1
        tree.fold_level(1);
        let visible_lvl1 = tree.visible_nodes();
        assert!(visible_lvl1.len() < 8);
    }
}
