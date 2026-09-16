pub mod engine;

use engine::{
    SOURCE_LANGS, TARGET_LANGS, TransConfig, clean_trans_text, swap_state, translate,
};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransPlugin {
    pub source_text: String,
    pub target_text: String,
    pub src_lang_idx: usize,
    pub dst_lang_idx: usize,
    pub config: TransConfig,
    pub pending: bool,
    pub error: Option<String>,
    pub status: String,
}

impl XPlugin for TransPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.trans".to_string(),
            name: "智能翻译".to_string(),
            version: "0.8.2".to_string(),
            description: "支持 MyMemory (免密钥) 与百度翻译 API 的即时划词与多语言翻译工具".to_string(),
            author: "xtools".to_string(),
            mark: "译".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 725,
                height: 600,
                resizable: true,
                title: Some("智能翻译".to_string()),
            },
            permissions: vec![
                Permission::Clipboard,
                Permission::Http(vec!["*".to_string()]),
                Permission::Storage,
            ],
        }
    }

    fn init() -> Result<Self, String> {
        let mut config = TransConfig::default();
        if let Ok(Some(bytes)) = host::storage_get("config.json") {
            if let Ok(loaded) = serde_json::from_slice::<TransConfig>(&bytes) {
                config = loaded;
            }
        }

        let status = if config.engine_index == 1 {
            "引擎：百度翻译".to_string()
        } else {
            "引擎：MyMemory (免密钥)".to_string()
        };

        let mut source_text = String::new();
        let mut target_text = String::new();
        let mut error = None;

        // 尝试自动获取划词（Primary Selection），过滤符号只保留英文字符后直接翻译
        if let Ok(primary) = host::clipboard_read_primary() {
            let cleaned = clean_trans_text(&primary);
            if !cleaned.is_empty() {
                source_text = cleaned;
                if config.engine_index == 1
                    && (config.baidu_appid.trim().is_empty()
                        || config.baidu_key.trim().is_empty())
                {
                    error = Some(
                        "请先在托盘菜单「设置」中配置百度翻译 AppID 与密钥。".to_string(),
                    );
                } else if let Ok(res) = translate(
                    &source_text,
                    0,
                    0,
                    &config,
                ) {
                    target_text = res;
                }
            }
        }

        Ok(Self {
            source_text,
            target_text,
            src_lang_idx: 0,
            dst_lang_idx: 0,
            config,
            pending: false,
            error,
            status,
        })
    }

    fn render(&self) -> UiView {
        let src_options: Vec<SelectOption> = SOURCE_LANGS
            .iter()
            .enumerate()
            .map(|(i, (_, label))| SelectOption::new(i.to_string(), *label))
            .collect();

        let dst_options: Vec<SelectOption> = TARGET_LANGS
            .iter()
            .enumerate()
            .map(|(i, (_, label))| SelectOption::new(i.to_string(), *label))
            .collect();

        let engine_options = vec![
            SelectOption::new("0", "MyMemory (免密钥)"),
            SelectOption::new("1", "百度翻译"),
        ];

        let mut children = Vec::new();

        // 1. Top Bar: Section Title + Engine Selector
        children.push(row(vec![
            label("原文 (Source)"),
            spacer(),
            select("select_engine", engine_options, self.config.engine_index),
        ]));

        // 2. Source Text Input
        children.push(text_area(
            "input_source",
            &self.source_text,
            5,
        ));

        // 3. Middle Action Bar: Language selectors + Swap button + Translate button
        let translate_btn_label = if self.pending {
            "翻译中…"
        } else {
            "翻译"
        };

        let lang_bar = row(vec![
            select("select_src_lang", src_options, self.src_lang_idx),
            button("btn_swap_lang", "⇄"),
            select("select_dst_lang", dst_options, self.dst_lang_idx),
            spacer(),
            button("btn_auto_translate", "🔤 划词翻译"),
            primary_button("btn_translate", translate_btn_label),
        ]);
        children.push(lang_bar);

        // 4. Target Text Label & Output Area
        children.push(label("译文 (Translation)"));
        children.push(text_area(
            "input_target",
            &self.target_text,
            5,
        ));

        // 5. Bottom Bar: Status / Error Note + Action buttons
        let mut bottom_items = Vec::new();
        if let Some(err) = &self.error {
            bottom_items.push(error_label(err));
        } else {
            bottom_items.push(secondary_label(&self.status));
        }
        bottom_items.push(spacer());
        bottom_items.push(button("btn_clear", "🗑 清空"));
        bottom_items.push(button("btn_copy", "📋 复制"));
        children.push(row(bottom_items));

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } => match id.as_str() {
                "btn_auto_translate" => {
                    self.error = None;
                    match host::clipboard_read_primary() {
                        Ok(primary) => {
                            let cleaned = clean_trans_text(&primary);
                            if cleaned.is_empty() {
                                self.error = Some("未检测到有效的划词内容。".to_string());
                                return Ok(UiResponse::UpdateView(self.render()));
                            }
                            self.source_text = cleaned;
                            if self.config.engine_index == 1
                                && (self.config.baidu_appid.trim().is_empty()
                                    || self.config.baidu_key.trim().is_empty())
                            {
                                self.error = Some(
                                    "请先在托盘菜单「设置」中配置百度翻译 AppID 与密钥。".to_string(),
                                );
                                return Ok(UiResponse::UpdateView(self.render()));
                            }

                            match translate(
                                &self.source_text,
                                self.src_lang_idx,
                                self.dst_lang_idx,
                                &self.config,
                            ) {
                                Ok(res) => {
                                    self.target_text = res;
                                    self.error = None;
                                }
                                Err(e) => {
                                    self.error = Some(e);
                                }
                            }
                            Ok(UiResponse::UpdateView(self.render()))
                        }
                        Err(e) => {
                            self.error = Some(format!("读取划词失败: {e}"));
                            Ok(UiResponse::UpdateView(self.render()))
                        }
                    }
                }
                "btn_translate" => {
                    self.error = None;
                    if self.source_text.trim().is_empty() {
                        self.error = Some("先输入要翻译的文字。".to_string());
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    // 翻译时若包含下划线或横杠等符号，清洗掉符号只留英文字符
                    if self.source_text.contains('_') || self.source_text.contains('-') {
                        let cleaned = clean_trans_text(&self.source_text);
                        if !cleaned.is_empty() {
                            self.source_text = cleaned;
                        }
                    }

                    if self.config.engine_index == 1
                        && (self.config.baidu_appid.trim().is_empty()
                            || self.config.baidu_key.trim().is_empty())
                    {
                        self.error = Some(
                            "请先在托盘菜单「设置」中配置百度翻译 AppID 与密钥。".to_string(),
                        );
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    match translate(
                        &self.source_text,
                        self.src_lang_idx,
                        self.dst_lang_idx,
                        &self.config,
                    ) {
                        Ok(res) => {
                            self.target_text = res;
                            self.error = None;
                        }
                        Err(e) => {
                            self.error = Some(e);
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_swap_lang" => {
                    let (new_src, new_dst, new_in, new_out) = swap_state(
                        self.src_lang_idx,
                        self.dst_lang_idx,
                        self.source_text.clone(),
                        self.target_text.clone(),
                    );
                    self.src_lang_idx = new_src;
                    self.dst_lang_idx = new_dst;
                    self.source_text = new_in;
                    self.target_text = new_out;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_clear" => {
                    self.source_text.clear();
                    self.target_text.clear();
                    self.error = None;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_copy" => {
                    let trimmed = self.target_text.trim();
                    if !trimmed.is_empty() {
                        let _ = host::clipboard_write(trimmed);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制译文".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::ShowToast(Toast {
                            message: "译文为空，无需复制".to_string(),
                            level: ToastLevel::Warning,
                            duration_ms: 1500,
                        }))
                    }
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::InputChanged { id, value } => match id.as_str() {
                "input_source" => {
                    self.source_text = value;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::SelectChanged { id, index, .. } => match id.as_str() {
                "select_src_lang" => {
                    self.src_lang_idx = index;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "select_dst_lang" => {
                    self.dst_lang_idx = index;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "select_engine" => {
                    self.config.engine_index = index;
                    if let Ok(bytes) = serde_json::to_vec(&self.config) {
                        let _ = host::storage_set("config.json", &bytes);
                    }
                    self.status = if index == 1 {
                        "引擎：百度翻译".to_string()
                    } else {
                        "引擎：MyMemory (免密钥)".to_string()
                    };
                    Ok(UiResponse::UpdateView(self.render()))
                }
                _ => Ok(UiResponse::NoChange),
            },
            _ => Ok(UiResponse::NoChange),
        }
    }
}

export_plugin!(TransPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trans_plugin_init_and_render() {
        let plugin = TransPlugin::init().unwrap();
        let view = plugin.render();
        assert!(matches!(view.root, UiNode::Container { .. }));
        assert_eq!(plugin.src_lang_idx, 0);
        assert_eq!(plugin.dst_lang_idx, 0);
    }

    #[test]
    fn test_trans_plugin_swap_and_clear() {
        let mut plugin = TransPlugin::init().unwrap();
        plugin.src_lang_idx = 1; // zh-CN
        plugin.dst_lang_idx = 1; // en
        plugin.source_text = "你好".to_string();
        plugin.target_text = "Hello".to_string();

        let resp = plugin.handle_event(UiEvent::Click { id: "btn_swap_lang".to_string() }).unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        assert_eq!(plugin.src_lang_idx, 2); // en in SOURCE_LANGS
        assert_eq!(plugin.dst_lang_idx, 0); // zh-CN in TARGET_LANGS
        assert_eq!(plugin.source_text, "Hello");
        assert_eq!(plugin.target_text, "你好");

        plugin.handle_event(UiEvent::Click { id: "btn_clear".to_string() }).unwrap();
        assert!(plugin.source_text.is_empty());
        assert!(plugin.target_text.is_empty());
    }

    #[test]
    fn test_trans_plugin_baidu_missing_credentials_error() {
        let mut plugin = TransPlugin::init().unwrap();
        assert!(plugin.config.baidu_appid.is_empty());

        // 切到百度引擎（无密钥），直接翻译时给出指向托盘设置的提示
        plugin.handle_event(UiEvent::SelectChanged {
            id: "select_engine".to_string(),
            index: 1,
            value: "1".to_string(),
        })
        .unwrap();
        plugin.source_text = "你好".to_string();

        plugin.handle_event(UiEvent::Click { id: "btn_translate".to_string() }).unwrap();
        let err = plugin.error.as_deref().unwrap();
        assert!(err.contains("托盘") && err.contains("设置"));
    }

    #[test]
    fn test_trans_plugin_clean_symbols_on_translate() {
        let mut plugin = TransPlugin::init().unwrap();
        plugin.source_text = "hello_world-test".to_string();

        // btn_translate should clean underscores and hyphens to "hello world test"
        let _ = plugin.handle_event(UiEvent::Click { id: "btn_translate".to_string() });
        assert_eq!(plugin.source_text, "hello world test");
    }

    #[test]
    fn test_trans_plugin_auto_translate_empty_selection() {
        let mut plugin = TransPlugin::init().unwrap();
        // In test environment, clipboard_read_primary returns empty
        let _ = plugin.handle_event(UiEvent::Click { id: "btn_auto_translate".to_string() });
        assert!(plugin.error.is_some());
}
}
