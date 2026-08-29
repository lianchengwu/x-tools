pub mod json_ops;

use json_ops::{
    empty_input, format_json, minify_json, parse, unescape_json, validate_json, JsonTree,
};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonPlugin {
    pub text: String,
    pub error: Option<String>,
    pub note: Option<String>,
    pub active_tab: usize,
    pub tree: Option<JsonTree>,
    pub can_copy: bool,
}

impl XPlugin for JsonPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.json".to_string(),
            name: "JSON 格式化与校验".to_string(),
            version: "0.4.0".to_string(),
            description: "JSON 格式化、压缩、去转义、语法校验与树形折叠工具".to_string(),
            author: "xtools".to_string(),
            mark: "{}".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 580,
                height: 600,
                resizable: true,
                title: Some("JSON 格式化与校验".to_string()),
            },
            permissions: vec![Permission::Clipboard],
        }
    }

    fn init() -> Result<Self, String> {
        let initial_text = "{\n  \"hello\": \"xtools\",\n  \"wasm\": true,\n  \"version\": [0, 4, 0]\n}".to_string();
        let tree = parse(&initial_text).ok().map(|val| JsonTree::from_value(&val));

        Ok(Self {
            text: initial_text,
            error: None,
            note: None,
            active_tab: 0,
            tree,
            can_copy: true,
        })
    }

    fn render(&self) -> UiView {
        let mut children = Vec::new();

        // 1. Toolbar actions row
        let toolbar = row(vec![
            primary_button("btn_format", "✨ 格式化"),
            button("btn_minify", "⚡ 压缩"),
            button("btn_unescape", "🔓 去转义"),
            button("btn_validate", "🔍 校验"),
            button("btn_clear", "🗑 清空"),
            spacer(),
            button("btn_copy", "📋 复制"),
        ]);
        children.push(toolbar);

        // 2. Tabs: Code View vs Tree View
        let tree_nodes = self
            .tree
            .as_ref()
            .map(|t| t.visible_nodes())
            .unwrap_or_default();

        let tabs = UiNode::Tabs {
            id: "json_tabs".to_string(),
            active_index: self.active_tab,
            tabs: vec![
                TabItem {
                    id: "tab_code".to_string(),
                    label: "📝 源码编辑".to_string(),
                    content: Box::new(UiNode::TextInput {
                        id: "json_code".to_string(),
                        label: None,
                        value: self.text.clone(),
                        placeholder: "请在此输入或粘贴 JSON 文本...".to_string(),
                        multiline: true,
                        readonly: false,
                        rows: Some(15),
                        on_change: true,
                        monospace: true,
                    }),
                },
                TabItem {
                    id: "tab_tree".to_string(),
                    label: "🌲 树形折叠".to_string(),
                    content: Box::new(column(vec![
                        row(vec![
                            button("btn_expand_all", "全部展开"),
                            button("btn_collapse_all", "全部折叠"),
                            button("btn_fold_level_2", "折叠至2层"),
                        ]),
                        json_tree_viewer("json_tree", tree_nodes),
                    ])),
                },
            ],
        };
        children.push(tabs);

        // 3. Error or Status Banner
        if let Some(err) = &self.error {
            children.push(error_label(err));
        } else if let Some(note) = &self.note {
            children.push(secondary_label(note));
        } else if !empty_input(&self.text) {
            let char_count = self.text.chars().count();
            let line_count = self.text.lines().count();
            let status_msg = format!("字符数: {char_count} | 行数: {line_count}");
            children.push(secondary_label(status_msg));
        }

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } => match id.as_str() {
                "btn_format" => {
                    if empty_input(&self.text) {
                        self.error = Some("这一栏是空的\n先粘贴一段 JSON。".to_string());
                        self.note = None;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match format_json(&self.text) {
                        Ok(formatted) => {
                            self.text = formatted;
                            self.error = None;
                            self.note = Some("已格式化".to_string());
                            self.can_copy = true;
                            if let Ok(val) = parse(&self.text) {
                                self.tree = Some(JsonTree::from_value(&val));
                            }
                        }
                        Err(issue) => {
                            self.error = Some(issue.display());
                            self.note = None;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_minify" => {
                    if empty_input(&self.text) {
                        self.error = Some("这一栏是空的\n先粘贴一段 JSON。".to_string());
                        self.note = None;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match minify_json(&self.text) {
                        Ok(minified) => {
                            self.text = minified;
                            self.error = None;
                            self.note = Some("已压缩".to_string());
                            self.can_copy = true;
                            if let Ok(val) = parse(&self.text) {
                                self.tree = Some(JsonTree::from_value(&val));
                            }
                        }
                        Err(issue) => {
                            self.error = Some(issue.display());
                            self.note = None;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_unescape" => {
                    if empty_input(&self.text) {
                        self.error = Some("这一栏是空的\n先粘贴一段含转义字符的文本。".to_string());
                        self.note = None;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match unescape_json(&self.text) {
                        Ok(unescaped) => {
                            self.text = unescaped;
                            self.error = None;
                            self.note = Some("已去转义".to_string());
                            self.can_copy = !empty_input(&self.text);
                            if let Ok(val) = parse(&self.text) {
                                self.tree = Some(JsonTree::from_value(&val));
                            }
                        }
                        Err(issue) => {
                            self.error = Some(issue.display());
                            self.note = None;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_validate" => {
                    if empty_input(&self.text) {
                        self.error = Some("这一栏是空的\n先粘贴一段 JSON。".to_string());
                        self.note = None;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match validate_json(&self.text) {
                        Ok(()) => {
                            self.error = None;
                            self.note = Some("JSON 有效".to_string());
                            if let Ok(val) = parse(&self.text) {
                                self.tree = Some(JsonTree::from_value(&val));
                            }
                        }
                        Err(issue) => {
                            self.error = Some(issue.display());
                            self.note = None;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_clear" => {
                    self.text.clear();
                    self.error = None;
                    self.note = None;
                    self.tree = None;
                    self.can_copy = false;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_copy" => {
                    if !empty_input(&self.text) {
                        let _ = host::clipboard_write(&self.text);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制 JSON 内容".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                "btn_expand_all" => {
                    if let Some(tree) = &mut self.tree {
                        tree.expand_all();
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_collapse_all" => {
                    if let Some(tree) = &mut self.tree {
                        tree.collapse_all();
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_fold_level_2" => {
                    if let Some(tree) = &mut self.tree {
                        tree.fold_level(2);
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::InputChanged { id, value } => {
                if id == "json_code" {
                    self.text = value;
                    self.can_copy = !empty_input(&self.text);
                    if empty_input(&self.text) {
                        self.error = None;
                        self.note = None;
                        self.tree = None;
                    } else {
                        match parse(&self.text) {
                            Ok(val) => {
                                self.error = None;
                                self.tree = Some(JsonTree::from_value(&val));
                            }
                            Err(issue) => {
                                self.error = Some(issue.display());
                                self.note = None;
                            }
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            },
            UiEvent::TabChanged { id, index, .. } => {
                if id == "json_tabs" {
                    self.active_tab = index;
                    if index == 1 {
                        if empty_input(&self.text) {
                            self.error = Some("当前为空，请先输入 JSON 内容".to_string());
                            self.note = None;
                        } else {
                            match parse(&self.text) {
                                Ok(val) => {
                                    self.tree = Some(JsonTree::from_value(&val));
                                    self.error = None;
                                }
                                Err(issue) => {
                                    self.error = Some(format!("无法解析为 JSON 进行树形折叠：{}", issue.display()));
                                    self.note = None;
                                }
                            }
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            },
            UiEvent::JsonTreeToggle { id, node_id } => {
                if id == "json_tree" {
                    if let Some(tree) = &mut self.tree {
                        tree.toggle(node_id);
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            },
            _ => Ok(UiResponse::NoChange),
        }
    }
}

export_plugin!(JsonPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_plugin_format_and_minify() {
        let mut plugin = JsonPlugin::init().unwrap();
        plugin.text = "{\"a\":1,\"b\":[2,3]}".to_string();

        let resp = plugin.handle_event(UiEvent::Click { id: "btn_format".to_string() }).unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        assert!(plugin.text.contains('\n'));
        assert_eq!(plugin.note.as_deref(), Some("已格式化"));

        let resp_mini = plugin.handle_event(UiEvent::Click { id: "btn_minify".to_string() }).unwrap();
        assert!(matches!(resp_mini, UiResponse::UpdateView(_)));
        assert_eq!(plugin.text, "{\"a\":1,\"b\":[2,3]}");
        assert_eq!(plugin.note.as_deref(), Some("已压缩"));
    }

    #[test]
    fn test_json_plugin_validate() {
        let mut plugin = JsonPlugin::init().unwrap();
        plugin.text = "{\"valid\": true}".to_string();
        plugin.handle_event(UiEvent::Click { id: "btn_validate".to_string() }).unwrap();
        assert_eq!(plugin.note.as_deref(), Some("JSON 有效"));
        assert!(plugin.error.is_none());

        plugin.text = "{\"invalid\": }".to_string();
        plugin.handle_event(UiEvent::Click { id: "btn_validate".to_string() }).unwrap();
        assert!(plugin.error.is_some());
    }

    #[test]
    fn test_json_plugin_unescape() {
        let mut plugin = JsonPlugin::init().unwrap();
        plugin.text = r#""{\"nested\": \"value\"}""#.to_string();
        plugin.handle_event(UiEvent::Click { id: "btn_unescape".to_string() }).unwrap();
        assert_eq!(plugin.note.as_deref(), Some("已去转义"));
        assert!(plugin.text.contains("\"nested\": \"value\""));
    }

    #[test]
    fn test_json_plugin_tree_toggle() {
        let mut plugin = JsonPlugin::init().unwrap();
        plugin.text = "{\"root\": {\"child\": 42}}".to_string();
        plugin.handle_event(UiEvent::TabChanged {
            id: "json_tabs".to_string(),
            index: 1,
            tab_id: "tab_tree".to_string(),
        }).unwrap();
        assert!(plugin.tree.is_some());

        let visible_before = plugin.tree.as_ref().unwrap().visible_nodes().len();
        plugin.handle_event(UiEvent::JsonTreeToggle {
            id: "json_tree".to_string(),
            node_id: 0,
        }).unwrap();
        let visible_after = plugin.tree.as_ref().unwrap().visible_nodes().len();
        assert!(visible_after < visible_before);
    }
}
