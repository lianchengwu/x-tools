pub mod engine;

use engine::{AiConfig, chat_completion};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

/// 对话历史持久化上限（条数），防止无限增长
const MAX_HISTORY: usize = 100;
const HISTORY_KEY: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlugin {
    /// 完整对话历史（多轮上下文），持久化到插件存储
    pub messages: Vec<ChatMessage>,
    /// 底部输入框草稿
    pub draft: String,
    pub config: AiConfig,
    pub pending: bool,
    pub error: Option<String>,
    pub status: String,
}

/// 持久化的会话快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedChat {
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default)]
    draft: String,
}

impl AiPlugin {
    /// 将当前对话与草稿写入插件存储，供下次打开恢复
    fn persist(&self) {
        if let Ok(bytes) = serde_json::to_vec(&PersistedChat {
            messages: self.messages.clone(),
            draft: self.draft.clone(),
        }) {
            let _ = host::storage_set(HISTORY_KEY, &bytes);
        }
    }

    fn load_persisted() -> PersistedChat {
        host::storage_get(HISTORY_KEY)
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<PersistedChat>(&bytes).ok())
            .unwrap_or_default()
    }

    /// 历史超限时从最旧开始裁剪
    fn trim_history(messages: &mut Vec<ChatMessage>) {
        while messages.len() > MAX_HISTORY {
            messages.remove(0);
        }
    }
}

impl XPlugin for AiPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.ai".to_string(),
            name: "AI 问答".to_string(),
            version: "0.4.0".to_string(),
            description: "基于 OpenAI 兼容接口的多轮 AI 对话工具，打开时自动填入剪贴板内容".to_string(),
            author: "xtools".to_string(),
            mark: "智".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 560,
                height: 640,
                resizable: true,
                title: Some("AI 问答".to_string()),
            },
            permissions: vec![
                Permission::Clipboard,
                Permission::Http(vec!["*".to_string()]),
                Permission::Storage,
            ],
        }
    }

    fn init() -> Result<Self, String> {
        let mut config = AiConfig::default();
        if let Ok(Some(bytes)) = host::storage_get("config.json") {
            if let Ok(loaded) = serde_json::from_slice::<AiConfig>(&bytes) {
                config = loaded;
            }
        }

        // 恢复上次对话与草稿
        let saved = Self::load_persisted();
        let mut messages = saved.messages;
        Self::trim_history(&mut messages);

        // 打开工具即自动把剪贴板内容填入输入框（优先于已保存草稿），是否发送由用户手动点击决定
        let clipboard = host::clipboard_read().unwrap_or_default();
        let (draft, status) = if !clipboard.trim().is_empty() {
            (
                clipboard,
                "AI 就绪：已自动填入剪贴板内容，点击「发送」提问".to_string(),
            )
        } else if !messages.is_empty() {
            (saved.draft, "AI 就绪：已恢复上次对话，可继续追问".to_string())
        } else {
            (saved.draft, "AI 就绪：剪贴板为空，请输入问题".to_string())
        };

        Ok(Self {
            messages,
            draft,
            config,
            pending: false,
            error: None,
            status,
        })
    }

    fn render(&self) -> UiView {
        let mut children = Vec::new();

        // 1. Top Bar: Section Title + Copy / Clear
        children.push(row(vec![
            label("AI 对话"),
            spacer(),
            button("btn_copy", "📋"),
            button("btn_clear", "🗑"),
        ]));

        // 2. Chat Conversation View
        children.push(chat_viewer("chat_messages", self.messages.clone()));

        // 3. Status / Error Note
        if let Some(err) = &self.error {
            children.push(error_label(err));
        } else {
            children.push(secondary_label(&self.status));
        }

        // 4. Input Bar: draft input + send button
        children.push(row(vec![
            text_area("input_draft", &self.draft, 3),
            primary_button("btn_send", if self.pending { "回答中…" } else { "🚀 发送" }),
        ]));

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } => match id.as_str() {
                "btn_send" => {
                    self.error = None;
                    let text = self.draft.trim().to_string();
                    if text.is_empty() {
                        self.error = Some("请输入要发送的问题。".to_string());
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    if !self.config.is_configured() {
                        self.error = Some(
                            "请先在托盘菜单「设置」中配置 AI 接口地址、API Key 与模型名。"
                                .to_string(),
                        );
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    // 先入列用户消息并清空草稿，失败时回滚以便重试
                    self.messages.push(ChatMessage {
                        role: ChatRole::User,
                        content: text.clone(),
                    });
                    self.draft.clear();
                    Self::trim_history(&mut self.messages);

                    match chat_completion(&self.config, &self.messages) {
                        Ok(res) => {
                            self.messages.push(ChatMessage {
                                role: ChatRole::Assistant,
                                content: res,
                            });
                            self.status = "AI 已回答，可继续追问".to_string();
                            self.error = None;
                        }
                        Err(e) => {
                            self.messages.pop();
                            self.draft = text;
                            self.error = Some(e);
                        }
                    }
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_clear" => {
                    self.messages.clear();
                    self.draft.clear();
                    self.error = None;
                    self.status = "AI 就绪：已开始新对话".to_string();
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_copy" => {
                    let last_answer = self
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == ChatRole::Assistant)
                        .map(|m| m.content.trim().to_string());
                    match last_answer {
                        Some(answer) if !answer.is_empty() => {
                            let _ = host::clipboard_write(&answer);
                            Ok(UiResponse::ShowToast(Toast {
                                message: "已复制最新回答".to_string(),
                                level: ToastLevel::Success,
                                duration_ms: 1500,
                            }))
                        }
                        _ => Ok(UiResponse::ShowToast(Toast {
                            message: "还没有可复制的回答".to_string(),
                            level: ToastLevel::Warning,
                            duration_ms: 1500,
                        })),
                    }
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::InputChanged { id, value } => match id.as_str() {
                "input_draft" => {
                    self.draft = value;
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                _ => Ok(UiResponse::NoChange),
            },
            _ => Ok(UiResponse::NoChange),
        }
    }
}

export_plugin!(AiPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_plugin_init_and_render() {
        let plugin = AiPlugin::init().unwrap();
        let view = plugin.render();
        assert!(matches!(view.root, UiNode::Container { .. }));
        // 原生测试环境下剪贴板与存储均为空，不应自动填入内容
        assert!(plugin.draft.is_empty());
        assert!(plugin.messages.is_empty());
        assert!(plugin.status.contains("剪贴板为空"));
    }

    #[test]
    fn test_persisted_chat_roundtrip() {
        let chat = PersistedChat {
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "你好".to_string(),
            }],
            draft: "未发送的问题".to_string(),
        };
        let bytes = serde_json::to_vec(&chat).unwrap();
        let loaded: PersistedChat = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.draft, "未发送的问题");

        // 空存储 / 空对象都能安全回退
        let empty: PersistedChat = serde_json::from_slice(b"{}").unwrap();
        assert!(empty.messages.is_empty());
        assert!(empty.draft.is_empty());
    }

    #[test]
    fn test_trim_history() {
        let mut messages: Vec<ChatMessage> = (0..MAX_HISTORY + 10)
            .map(|i| ChatMessage {
                role: if i % 2 == 0 { ChatRole::User } else { ChatRole::Assistant },
                content: i.to_string(),
            })
            .collect();
        AiPlugin::trim_history(&mut messages);
        assert_eq!(messages.len(), MAX_HISTORY);
        // 最旧的 10 条被裁掉
        assert_eq!(messages[0].content, "10");
        assert_eq!(messages.last().unwrap().content, "109");
    }

    #[test]
    fn test_ai_plugin_send_validations() {
        let mut plugin = AiPlugin::init().unwrap();

        // 空草稿拦截
        let resp = plugin
            .handle_event(UiEvent::Click { id: "btn_send".to_string() })
            .unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        assert!(plugin.error.as_deref().unwrap().contains("请输入"));

        // 未配置密钥时给出指向托盘设置的提示，不产生消息
        plugin.handle_event(UiEvent::InputChanged {
            id: "input_draft".to_string(),
            value: "你好".to_string(),
        })
        .unwrap();
        plugin.handle_event(UiEvent::Click { id: "btn_send".to_string() }).unwrap();
        let err = plugin.error.as_deref().unwrap();
        assert!(err.contains("托盘") && err.contains("设置"));
        assert!(plugin.messages.is_empty());
    }

    #[test]
    fn test_ai_plugin_unconfigured_message() {
        let mut plugin = AiPlugin::init().unwrap();
        assert!(!plugin.config.is_configured());

        plugin.draft = "你好".to_string();
        plugin.handle_event(UiEvent::Click { id: "btn_send".to_string() }).unwrap();
        assert!(plugin.error.as_deref().unwrap().contains("托盘"));
        // 发送被拦截，草稿保留
        assert_eq!(plugin.draft, "你好");
        assert!(plugin.messages.is_empty());
    }

    #[test]
    fn test_ai_plugin_clear_and_copy() {
        let mut plugin = AiPlugin::init().unwrap();

        // 无回答时复制给出提示
        let resp = plugin.handle_event(UiEvent::Click { id: "btn_copy".to_string() }).unwrap();
        assert!(matches!(resp, UiResponse::ShowToast(t) if t.level == ToastLevel::Warning));

        plugin.messages = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "你好".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "你好！有什么可以帮你？".to_string(),
            },
        ];

        // 复制最新一条回答（原生测试下 clipboard_write 为空实现，仅验证走通）
        let resp = plugin.handle_event(UiEvent::Click { id: "btn_copy".to_string() }).unwrap();
        assert!(matches!(resp, UiResponse::ShowToast(t) if t.level == ToastLevel::Success));

        plugin.handle_event(UiEvent::Click { id: "btn_clear".to_string() }).unwrap();
        assert!(plugin.messages.is_empty());
        assert!(plugin.draft.is_empty());
        assert!(plugin.status.contains("新对话"));
    }
}
