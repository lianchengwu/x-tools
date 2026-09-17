pub mod codec_ops;
pub mod id_gen;

use codec_ops::{CODEC_KINDS, CodecKind, decode, empty_input, encode};
use id_gen::{GeneratorConfig, ID_KINDS, IdKind, generate};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecPlugin {
    pub input: String,
    pub output: String,
    pub kind_index: usize,
    pub error: Option<String>,
    pub status: String,
    #[serde(default)]
    pub gen_kind_index: usize,
    #[serde(default = "default_gen_length")]
    pub gen_length: usize,
    #[serde(default = "default_gen_count")]
    pub gen_count: usize,
    #[serde(default = "default_true")]
    pub gen_uppercase: bool,
    #[serde(default = "default_true")]
    pub gen_lowercase: bool,
    #[serde(default = "default_true")]
    pub gen_digits: bool,
    #[serde(default = "default_true")]
    pub gen_symbols: bool,
    #[serde(default)]
    pub gen_exclude_ambiguous: bool,
}

fn default_gen_length() -> usize {
    16
}

fn default_gen_count() -> usize {
    1
}

fn default_true() -> bool {
    true
}

impl CodecPlugin {
    pub fn kind(&self) -> CodecKind {
        CodecKind::from_index(self.kind_index)
    }

    pub fn gen_kind(&self) -> IdKind {
        IdKind::from_index(self.gen_kind_index)
    }

    pub fn gen_config(&self) -> GeneratorConfig {
        GeneratorConfig {
            kind: self.gen_kind(),
            length: self.gen_length,
            count: self.gen_count,
            uppercase: self.gen_uppercase,
            lowercase: self.gen_lowercase,
            digits: self.gen_digits,
            symbols: self.gen_symbols,
            exclude_ambiguous: self.gen_exclude_ambiguous,
        }
    }

    pub fn perform_generate(&mut self, count_override: Option<usize>) -> Result<UiResponse, String> {
        let mut cfg = self.gen_config();
        if let Some(cnt) = count_override {
            cfg.count = cnt;
        }
        self.output = generate(&cfg);
        self.error = None;
        let count_str = if cfg.count > 1 {
            format!(" × {}", cfg.count)
        } else {
            String::new()
        };
        self.status = format!("已生成{count_str} · {}", cfg.kind.label());
        Ok(UiResponse::UpdateView(self.render()))
    }
}

impl XPlugin for CodecPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.codec".to_string(),
            name: "编码解码".to_string(),
            version: "0.8.3".to_string(),
            description: "Unicode、UTF-8、URL、Hex、Base64 编解码与随机数、密码、UUIDv7、NanoID 等 ID 生成".to_string(),
            author: "xtools".to_string(),
            mark: "码".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 725,
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
            gen_kind_index: 0,
            gen_length: 16,
            gen_count: 1,
            gen_uppercase: true,
            gen_lowercase: true,
            gen_digits: true,
            gen_symbols: true,
            gen_exclude_ambiguous: false,
        })
    }

    fn render(&self) -> UiView {
        let kind = self.kind();
        let kind_options: Vec<SelectOption> = CODEC_KINDS
            .iter()
            .enumerate()
            .map(|(i, k)| SelectOption::new(i.to_string(), k.label()))
            .collect();

        if kind == CodecKind::Generator {
            let gen_kind = self.gen_kind();
            let gen_kind_options: Vec<SelectOption> = ID_KINDS
                .iter()
                .enumerate()
                .map(|(i, k)| SelectOption::new(i.to_string(), k.label()))
                .collect();

            let mut children = Vec::new();

            children.push(row(vec![
                label("模式 (Mode)"),
                spacer(),
                select("select_kind", kind_options, self.kind_index),
            ]));

            let mut config_row = vec![
                label("ID类型:"),
                select("select_gen_kind", gen_kind_options, self.gen_kind_index),
            ];
            if gen_kind.supports_custom_length() {
                config_row.push(spacer());
                config_row.push(label("长度:"));
                config_row.push(UiNode::TextInput {
                    id: "input_gen_length".to_string(),
                    label: None,
                    value: self.gen_length.to_string(),
                    placeholder: "长度".to_string(),
                    multiline: false,
                    readonly: false,
                    rows: Some(1),
                    on_change: true,
                    monospace: true,
                });
            }
            config_row.push(spacer());
            config_row.push(label("数量:"));
            config_row.push(UiNode::TextInput {
                id: "input_gen_count".to_string(),
                label: None,
                value: self.gen_count.to_string(),
                placeholder: "数量".to_string(),
                multiline: false,
                readonly: false,
                rows: Some(1),
                on_change: true,
                monospace: true,
            });
            children.push(row(config_row));

            if gen_kind == IdKind::Password {
                children.push(row(vec![
                    UiNode::Switch {
                        id: "switch_upper".to_string(),
                        label: "大写 (A-Z)".to_string(),
                        checked: self.gen_uppercase,
                    },
                    UiNode::Switch {
                        id: "switch_lower".to_string(),
                        label: "小写 (a-z)".to_string(),
                        checked: self.gen_lowercase,
                    },
                    UiNode::Switch {
                        id: "switch_digits".to_string(),
                        label: "数字 (0-9)".to_string(),
                        checked: self.gen_digits,
                    },
                    UiNode::Switch {
                        id: "switch_symbols".to_string(),
                        label: "符号 (!@#$)".to_string(),
                        checked: self.gen_symbols,
                    },
                    UiNode::Switch {
                        id: "switch_ambiguous".to_string(),
                        label: "排除混淆".to_string(),
                        checked: self.gen_exclude_ambiguous,
                    },
                ]));
            }

            children.push(row(vec![
                primary_button("btn_encode", "⚡ 立即生成"),
                button("btn_gen_5", "生成 5 个"),
                button("btn_gen_10", "生成 10 个"),
                spacer(),
            ]));
            children.push(label("生成结果 (Output) - 点击单项直接复制"));
            children.push(UiNode::TextInput {
                id: "input_target".to_string(),
                label: None,
                value: self.output.clone(),
                placeholder: "点击上方生成按钮生成…".to_string(),
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
            children.push(row(bottom));

            return UiView::new(column(children));
        }

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
                "btn_gen_5" => self.perform_generate(Some(5)),
                "btn_gen_10" => self.perform_generate(Some(10)),
                "btn_swap" => {
                    if self.kind() != CodecKind::Generator {
                        std::mem::swap(&mut self.input, &mut self.output);
                    }
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
                            message: "已复制生成结果".to_string(),
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
            UiEvent::InputChanged { id, value } => match id.as_str() {
                "input_source" => {
                    self.input = value;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "input_gen_length" => {
                    if let Ok(len) = value.trim().parse::<usize>() {
                        self.gen_length = len.clamp(1, 256);
                        self.perform_generate(None)
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                "input_gen_count" => {
                    if let Ok(cnt) = value.trim().parse::<usize>() {
                        self.gen_count = cnt.clamp(1, 50);
                        self.perform_generate(None)
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::SelectChanged { id, index, .. } => match id.as_str() {
                "select_kind" => {
                    self.kind_index = index;
                    self.error = None;
                    self.status = self.kind().hint().to_string();
                    if self.kind() == CodecKind::Generator && self.output.is_empty() {
                        return self.perform_generate(None);
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "select_gen_kind" => {
                    self.gen_kind_index = index;
                    self.gen_length = self.gen_kind().default_length();
                    self.perform_generate(None)
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::ToggleChanged { id, checked } => match id.as_str() {
                "switch_upper" => {
                    self.gen_uppercase = checked;
                    self.perform_generate(None)
                }
                "switch_lower" => {
                    self.gen_lowercase = checked;
                    self.perform_generate(None)
                }
                "switch_digits" => {
                    self.gen_digits = checked;
                    self.perform_generate(None)
                }
                "switch_symbols" => {
                    self.gen_symbols = checked;
                    self.perform_generate(None)
                }
                "switch_ambiguous" => {
                    self.gen_exclude_ambiguous = checked;
                    self.perform_generate(None)
                }
                _ => Ok(UiResponse::NoChange),
            },
            _ => Ok(UiResponse::NoChange),
        }
    }
}

impl CodecPlugin {
    fn convert(&mut self, encoding: bool) -> Result<UiResponse, String> {
        let kind = self.kind();
        if kind == CodecKind::Generator {
            return self.perform_generate(if encoding { None } else { Some(5) });
        }
        if empty_input(&self.input) {
            self.error = Some("先输入要转换的文字。".to_string());
            return Ok(UiResponse::UpdateView(self.render()));
        }
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

    #[test]
    fn generator_select_and_generate_password() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_kind".into(),
                index: 6,
                value: "6".into(),
            })
            .unwrap();
        assert_eq!(plugin.kind(), CodecKind::Generator);
        assert_eq!(plugin.gen_kind(), IdKind::Password);
        assert!(!plugin.output.is_empty());
        assert_eq!(plugin.output.len(), 16);

        // Custom length
        plugin
            .handle_event(UiEvent::InputChanged {
                id: "input_gen_length".into(),
                value: "24".into(),
            })
            .unwrap();
        assert_eq!(plugin.gen_length, 24);
        assert_eq!(plugin.output.len(), 24);

        // Render contains switches and generator controls
        let view = serde_json::to_string(&plugin.render()).unwrap();
        assert!(view.contains("select_gen_kind"));
        assert!(view.contains("switch_upper"));
        assert!(view.contains("switch_symbols"));
    }

    #[test]
    fn generator_random_number_custom_length() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin.kind_index = 6;
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_gen_kind".into(),
                index: 1,
                value: "1".into(),
            })
            .unwrap();
        assert_eq!(plugin.gen_kind(), IdKind::RandomNumber);
        assert_eq!(plugin.gen_length, 6);
        assert_eq!(plugin.output.len(), 6);
        assert!(plugin.output.chars().all(|c| c.is_ascii_digit()));

        // Set length to 10
        plugin
            .handle_event(UiEvent::InputChanged {
                id: "input_gen_length".into(),
                value: "10".into(),
            })
            .unwrap();
        assert_eq!(plugin.output.len(), 10);
        assert!(plugin.output.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn generator_uuidv7_and_nanoid_and_snowflake() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin.kind_index = 6;

        // UUIDv7 (index 2)
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_gen_kind".into(),
                index: 2,
                value: "2".into(),
            })
            .unwrap();
        assert_eq!(plugin.gen_kind(), IdKind::UuidV7);
        assert_eq!(plugin.output.len(), 36);
        assert_eq!(plugin.output.chars().nth(14).unwrap(), '7');

        // NanoID (index 3)
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_gen_kind".into(),
                index: 3,
                value: "3".into(),
            })
            .unwrap();
        assert_eq!(plugin.gen_kind(), IdKind::NanoId);
        assert_eq!(plugin.output.len(), 21);

        // Snowflake ID (index 4)
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_gen_kind".into(),
                index: 4,
                value: "4".into(),
            })
            .unwrap();
        assert_eq!(plugin.gen_kind(), IdKind::Snowflake);
        assert!(plugin.output.parse::<u64>().is_ok());
    }

    #[test]
    fn generator_batch_count() {
        let mut plugin = CodecPlugin::init().unwrap();
        plugin.kind_index = 6;
        plugin.gen_kind_index = 2; // UUIDv7
        plugin
            .handle_event(UiEvent::Click {
                id: "btn_gen_5".into(),
            })
            .unwrap();
        let lines: Vec<&str> = plugin.output.lines().collect();
        assert_eq!(lines.len(), 5);
        for line in lines {
            assert_eq!(line.len(), 36);
        }
    }
}
