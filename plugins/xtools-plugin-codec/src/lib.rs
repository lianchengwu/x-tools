pub mod codec_ops;

use codec_ops::{CODEC_KINDS, CodecKind, decode, empty_input, encode};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecPlugin {
    pub input: String,
    pub output: String,
    pub kind_index: usize,
    pub error: Option<String>,
    pub status: String,
}

impl CodecPlugin {
    fn kind(&self) -> CodecKind {
        CodecKind::from_index(self.kind_index)
    }
}

impl XPlugin for CodecPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.codec".to_string(),
            name: "编码解码".to_string(),
            version: "0.5.0".to_string(),
            description: "Unicode、UTF-8、URL、Hex、Base64 编解码与大小写转换".to_string(),
            author: "xtools".to_string(),
            mark: "码".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 580,
                height: 600,
                resizable: true,
                title: Some("编码解码".to_string()),
            },
            permissions: vec![Permission::Clipboard],
        }
    }

    fn init() -> Result<Self, String> {
        Ok(Self {
            input: String::new(),
            output: String::new(),
            kind_index: 0,
            error: None,
            status: CodecKind::Unicode.hint().to_string(),
        })
    }

    fn render(&self) -> UiView {
        let kind = self.kind();
        let kind_options: Vec<SelectOption> = CODEC_KINDS
            .iter()
            .enumerate()
            .map(|(i, k)| SelectOption::new(i.to_string(), k.label()))
            .collect();

        let mut children = Vec::new();

        children.push(row(vec![
            label("输入 (Input)"),
            spacer(),
            select("select_kind", kind_options, self.kind_index),
        ]));

        children.push(UiNode::TextInput {
            id: "input_source".to_string(),
            label: None,
            value: self.input.clone(),
            placeholder: "输入或粘贴要转换的文本…".to_string(),
            multiline: true,
            readonly: false,
            rows: Some(8),
            on_change: true,
            monospace: true,
        });

        children.push(row(vec![
            primary_button("btn_encode", kind.encode_label()),
            button("btn_decode", kind.decode_label()),
            button("btn_swap", "⇄"),
            spacer(),
        ]));

        children.push(label("输出 (Output)"));
        children.push(UiNode::TextInput {
            id: "input_target".to_string(),
            label: None,
            value: self.output.clone(),
            placeholder: String::new(),
            multiline: true,
            readonly: true,
            rows: Some(8),
            on_change: false,
            monospace: true,
        });

        let mut bottom = Vec::new();
        if let Some(err) = &self.error {
            bottom.push(error_label(err));
        } else {
            bottom.push(secondary_label(&self.status));
        }
        bottom.push(spacer());
        bottom.push(button("btn_clear", "🗑 清空"));
        bottom.push(button("btn_copy", "📋 复制"));
        children.push(row(bottom));

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } => match id.as_str() {
                "btn_encode" => self.convert(true),
                "btn_decode" => self.convert(false),
                "btn_swap" => {
                    std::mem::swap(&mut self.input, &mut self.output);
                    self.error = None;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_clear" => {
                    self.input.clear();
                    self.output.clear();
                    self.error = None;
                    self.status = self.kind().hint().to_string();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_copy" => {
                    if !self.output.is_empty() {
                        let _ = host::clipboard_write(&self.output);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制转换结果".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::ShowToast(Toast {
                            message: "结果为空，无需复制".to_string(),
                            level: ToastLevel::Warning,
                            duration_ms: 1500,
                        }))
                    }
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::InputChanged { id, value } => {
                if id == "input_source" {
                    self.input = value;
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            }
            UiEvent::SelectChanged { id, index, .. } => {
                if id == "select_kind" {
                    self.kind_index = index;
                    self.error = None;
                    self.status = self.kind().hint().to_string();
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            }
            _ => Ok(UiResponse::NoChange),
        }
    }
}

impl CodecPlugin {
    fn convert(&mut self, encoding: bool) -> Result<UiResponse, String> {
        if empty_input(&self.input) {
            self.error = Some("先输入要转换的文字。".to_string());
            return Ok(UiResponse::UpdateView(self.render()));
        }
        let kind = self.kind();
        let result = if encoding {
            encode(kind, &self.input)
        } else {
            decode(kind, &self.input)
        };
        match result {
            Ok(out) => {
                self.output = out;
                self.error = None;
                self.status = if encoding {
                    format!("已{} · {}", kind.encode_label(), kind.label())
                } else {
                    format!("已{} · {}", kind.decode_label(), kind.label())
                };
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
        Ok(UiResponse::UpdateView(self.render()))
    }
}

export_plugin!(CodecPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_renders_unicode_default() {
        let plugin = CodecPlugin::init().unwrap();
        assert_eq!(plugin.kind_index, 0);
        let view = plugin.render();
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("Unicode"));
        assert!(json.contains("btn_encode"));
    }

    #[test]
    fn encodes_and_decodes_unicode() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin
            .handle_event(UiEvent::InputChanged {
                id: "input_source".into(),
                value: "你好".into(),
            })
            .unwrap();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_encode".into(),
            })
            .unwrap();
        assert_eq!(plugin.output, r"\u4f60\u597d");
        assert!(plugin.status.contains("编码"));

        plugin
            .handle_event(UiEvent::Click {
                id: "btn_swap".into(),
            })
            .unwrap();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_decode".into(),
            })
            .unwrap();
        assert_eq!(plugin.output, "你好");
    }

    #[test]
    fn case_buttons_upper_and_lower() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_kind".into(),
                index: 5,
                value: "5".into(),
            })
            .unwrap();
        assert_eq!(plugin.kind(), CodecKind::Case);
        plugin.input = "Hello".into();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_encode".into(),
            })
            .unwrap();
        assert_eq!(plugin.output, "HELLO");
        plugin.input = "Hello".into();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_decode".into(),
            })
            .unwrap();
        assert_eq!(plugin.output, "hello");
        let view = serde_json::to_string(&plugin.render()).unwrap();
        assert!(view.contains("大写"));
        assert!(view.contains("小写"));
    }

    #[test]
    fn empty_input_sets_error() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_encode".into(),
            })
            .unwrap();
        assert!(plugin.error.is_some());
    }

    #[test]
    fn base64_and_clear() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_kind".into(),
                index: 4,
                value: "4".into(),
            })
            .unwrap();
        plugin.input = "hello".into();
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_encode".into(),
            })
            .unwrap();
        assert_eq!(plugin.output, "aGVsbG8=");
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_clear".into(),
            })
            .unwrap();
        assert!(plugin.input.is_empty());
        assert!(plugin.output.is_empty());
        assert!(plugin.error.is_none());
    }
}
