pub mod engine;

use std::sync::atomic::{AtomicU64, Ordering};

use engine::AiConfig;
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

/// 单个会话的历史上限（条数），防止无限增长
const MAX_HISTORY: usize = 100;
/// 会话总数上限，超出时淘汰最旧的会话
const MAX_SESSIONS: usize = 20;
const SESSIONS_KEY: &str = "sessions.json";

/// 一个会话：标题 + 消息列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiPlugin {
    /// 全部会话（多会话管理），持久化到插件存储
    pub sessions: Vec<ChatSession>,
    /// 当前激活的会话 id
    pub active_session_id: String,
    /// 底部输入框草稿（全局，不随会话切换）
    pub draft: String,
    pub config: AiConfig,
    pub pending: bool,
    pub error: Option<String>,
    pub status: String,
}

/// 会话持久化快照
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedSessions {
    #[serde(default)]
    sessions: Vec<ChatSession>,
    #[serde(default)]
    active_id: String,
}

impl AiPlugin {
    /// 毫秒时间戳 + 进程内计数器，保证同一毫秒内创建的会话 id 也不重复
    fn new_session_id() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = host::now_millis().max(0) as u64;
        let n = millis
            .wrapping_mul(1000)
            .wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
        format!("s{n}")
    }

    fn fresh_session() -> ChatSession {
        ChatSession {
            id: Self::new_session_id(),
            title: "新会话".to_string(),
            messages: Vec::new(),
        }
    }

    /// 当前激活会话的可变引用（保证始终存在）
    fn active_mut(&mut self) -> &mut ChatSession {
        if !self.sessions.iter().any(|s| s.id == self.active_session_id) {
            if let Some(first) = self.sessions.first() {
                self.active_session_id = first.id.clone();
            } else {
                let s = Self::fresh_session();
                self.active_session_id = s.id.clone();
                self.sessions.push(s);
            }
        }
        self.sessions
            .iter_mut()
            .find(|s| s.id == self.active_session_id)
            .expect("active session ensured")
    }

    fn active(&self) -> &ChatSession {
        self.sessions
            .iter()
            .find(|s| s.id == self.active_session_id)
            .or_else(|| self.sessions.first())
            .expect("at least one session always exists")
    }

    /// 会话下拉选项：(标签, id)。标签形如「标题 (条数)」
    fn session_options(&self) -> Vec<(String, String)> {
        self.sessions
            .iter()
            .map(|s| (format!("{} ({})", s.title, s.messages.len()), s.id.clone()))
            .collect()
    }

    fn active_session_index(&self) -> usize {
        self.session_options()
            .iter()
            .position(|(_, id)| *id == self.active_session_id)
            .unwrap_or(0)
    }

    fn select_session_by_index(&mut self, index: usize) {
        if let Some((_, id)) = self.session_options().get(index) {
            self.active_session_id = id.clone();
        }
    }

    /// 会话标题为空或仍为默认值时，用首条用户消息自动命名
    fn ensure_session_title(session: &mut ChatSession) {
        let default_like = session.title.is_empty() || session.title == "新会话";
        if default_like {
            if let Some(m) = session.messages.iter().find(|m| m.role == ChatRole::User) {
                let title: String = m.content.chars().take(16).collect();
                session.title = title;
            }
        }
    }

    /// 将全部会话写入插件存储，供下次打开恢复
    fn persist(&self) {
        if let Ok(bytes) = serde_json::to_vec(&PersistedSessions {
            sessions: self.sessions.clone(),
            active_id: self.active_session_id.clone(),
        }) {
            let _ = host::storage_set(SESSIONS_KEY, &bytes);
        }
    }

    /// 读取全部会话
    fn load_persisted() -> PersistedSessions {
        host::storage_get(SESSIONS_KEY)
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<PersistedSessions>(&bytes).ok())
            .unwrap_or_default()
    }

    /// 历史超限时从最旧开始裁剪
    fn trim_history(messages: &mut Vec<ChatMessage>) {
        while messages.len() > MAX_HISTORY {
            messages.remove(0);
        }
    }

    /// 请求失败/无内容时回滚当前会话末尾的用户消息到草稿，便于修改后重试
    fn rollback_last_user_message(&mut self) {
        if let Some(m) = self.active_mut().messages.pop() {
            if m.role == ChatRole::User {
                self.draft = m.content;
            }
        }
    }
}

impl XPlugin for AiPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.ai".to_string(),
            name: "AI 问答".to_string(),
            version: "0.5.0".to_string(),
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
        // 迁移旧版单服务商配置；结构变化时写回，保证设置窗口读到一致格式
        if config.normalize() {
            if let Ok(bytes) = serde_json::to_vec(&config) {
                let _ = host::storage_set("config.json", &bytes);
            }
        }

        // 恢复会话（旧版单会话 history.json 自动迁移）
        let saved = Self::load_persisted();
        let mut sessions = saved.sessions;
        for s in &mut sessions {
            Self::trim_history(&mut s.messages);
        }
        if sessions.is_empty() {
            sessions.push(Self::fresh_session());
        }
        let active_session_id = if sessions.iter().any(|s| s.id == saved.active_id) {
            saved.active_id
        } else {
            sessions[0].id.clone()
        };

        // 打开工具即自动把剪贴板内容填入输入框（优先于已保存草稿），是否发送由用户手动点击决定
        let clipboard = host::clipboard_read().unwrap_or_default();
        let has_history = sessions
            .iter()
            .any(|s| !s.messages.is_empty());
        let (draft, status) = if !clipboard.trim().is_empty() {
            (
                clipboard,
                "AI 就绪：已自动填入剪贴板内容，点击「发送」提问".to_string(),
            )
        } else if has_history {
            (String::new(), "AI 就绪：已恢复上次会话，可继续追问".to_string())
        } else {
            (String::new(), "AI 就绪：剪贴板为空，请输入问题".to_string())
        };
        let status = match config.selected_provider() {
            Some(_p) if !config.selected_model.is_empty() => {
                format!("{status}；当前模型：{}", config.selected_model)
            }
            _ => format!("{status}；尚未配置模型，请在「设置」中添加"),
        };

        Ok(Self {
            sessions,
            active_session_id,
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
            button("btn_new_session", "➕"),
            button("btn_copy", "📋"),
            button("btn_clear", "🗑"),
        ]));

        // 2. Chat Conversation View（当前会话）
        children.push(chat_viewer("chat_messages", self.active().messages.clone()));

        // 3. Session / Model Selectors（AI 窗口顶栏与输入框底部展示与切换）
        let session_opts: Vec<SelectOption> = self
            .session_options()
            .into_iter()
            .map(|(label, id)| SelectOption::new(id, label))
            .collect();
        children.push(select(
            "select_session",
            session_opts,
            self.active_session_index(),
        ));

        let options: Vec<SelectOption> = self
            .config
            .model_options()
            .into_iter()
            .map(|(_, _, display)| SelectOption::new(display.clone(), display))
            .collect();
        children.push(select("select_model", options, self.config.selected_index()));

        // 4. Status / Error Note
        if let Some(err) = &self.error {
            children.push(error_label(err));
        } else {
            children.push(secondary_label(&self.status));
        }

        // 5. Input Bar: draft input + send button
        children.push(row(vec![
            text_area("input_draft", &self.draft, 3),
            primary_button("btn_send", if self.pending { "回答中…" } else { "发送" }),
        ]));

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } => match id.as_str() {
                // 阶段 A：校验并立即入列用户消息（不做网络请求，窗口不阻塞）。
                // 实际的 HTTP 请求由宿主后台线程执行，完成后以 AssistantDone 回填。
                "btn_send" => {
                    self.error = None;
                    let text = self.draft.trim().to_string();
                    if text.is_empty() {
                        self.error = Some("请输入要发送的问题。".to_string());
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    if !self.config.is_configured() {
                        self.error = Some(
                            "请先在托盘菜单「设置」中添加服务商、API Key 与模型。".to_string(),
                        );
                        return Ok(UiResponse::UpdateView(self.render()));
                    }

                    let session = self.active_mut();
                    session.messages.push(ChatMessage {
                        role: ChatRole::User,
                        content: text,
                    });
                    Self::trim_history(&mut session.messages);
                    Self::ensure_session_title(session);
                    self.draft.clear();
                    self.pending = true;
                    self.status = "正在请求 AI…".to_string();
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                // 新建会话：创建并切换到空会话
                "btn_new_session" => {
                    let session = Self::fresh_session();
                    self.active_session_id = session.id.clone();
                    self.sessions.push(session);
                    // 超限时淘汰最旧的会话（不淘汰当前）
                    while self.sessions.len() > MAX_SESSIONS {
                        let oldest = self
                            .sessions
                            .iter()
                            .map(|s| s.id.clone())
                            .find(|id| *id != self.active_session_id);
                        match oldest {
                            Some(id) => self.sessions.retain(|s| s.id != id),
                            None => break,
                        }
                    }
                    self.pending = false;
                    self.error = None;
                    self.status = "AI 就绪：已开始新会话".to_string();
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                // 删除当前会话；仅剩一个时清空重置
                "btn_clear" => {
                    let was_last = self.sessions.len() == 1;
                    if was_last {
                        let fresh = Self::fresh_session();
                        self.active_session_id = fresh.id.clone();
                        self.sessions = vec![fresh];
                        self.status = "AI 就绪：已开始新会话".to_string();
                    } else {
                        let removed = self.active_session_id.clone();
                        self.sessions.retain(|s| s.id != removed);
                        self.active_session_id = self.sessions[0].id.clone();
                        self.status = format!(
                            "已删除会话，已切换到「{}」",
                            self.active().title
                        );
                    }
                    self.pending = false;
                    self.draft.clear();
                    self.error = None;
                    self.persist();
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "btn_copy" => {
                    let last_answer = self
                        .active()
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
            UiEvent::SelectChanged { id, index, .. } if id == "select_session" => {
                self.select_session_by_index(index as usize);
                self.pending = false;
                self.error = None;
                self.status = format!("已切换会话：{}", self.active().title);
                self.persist();
                Ok(UiResponse::UpdateView(self.render()))
            }
            UiEvent::SelectChanged { id, index, .. } if id == "select_model" => {
                self.config.select_by_index(index as usize);
                if let Ok(bytes) = serde_json::to_vec(&self.config) {
                    let _ = host::storage_set("config.json", &bytes);
                }
                self.error = None;
                self.status = match self.config.selected_provider() {
                    Some(p) if !self.config.selected_model.is_empty() => format!(
                        "已切换模型：{} / {}",
                        p.name.trim(),
                        self.config.selected_model
                    ),
                    _ => "未选择模型".to_string(),
                };
                Ok(UiResponse::UpdateView(self.render()))
            }
            // 宿主后台请求完成：追加回答 / 失败回滚 / 中止保留部分内容
            UiEvent::AssistantDone { content, error, aborted } => {
                self.pending = false;
                // 过期保护：当前会话必须以用户消息结尾（请求期间切换/清空会话等场景直接丢弃）
                if self.active().messages.last().map(|m| m.role) != Some(ChatRole::User) {
                    return Ok(UiResponse::UpdateView(self.render()));
                }
                let text = content.trim().to_string();
                match error {
                    Some(e) => {
                        self.rollback_last_user_message();
                        self.error = Some(format!("{e}（已保留输入，可再次点击「发送」重试）"));
                    }
                    None if text.is_empty() => {
                        self.rollback_last_user_message();
                        self.error = Some(if aborted {
                            "已停止生成。".to_string()
                        } else {
                            "AI 未返回回答内容".to_string()
                        });
                    }
                    None => {
                        let session = self.active_mut();
                        session.messages.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: text,
                        });
                        self.error = None;
                        self.status = if aborted {
                            "已停止生成，已保留部分回答".to_string()
                        } else {
                            "AI 已回答，可继续追问".to_string()
                        };
                    }
                }
                self.persist();
                Ok(UiResponse::UpdateView(self.render()))
            }
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
        // 初始只有一个空会话
        assert_eq!(plugin.sessions.len(), 1);
        assert!(plugin.active().messages.is_empty());
        assert!(plugin.status.contains("剪贴板为空"));
    }

    #[test]
    fn test_persisted_sessions_roundtrip() {
        let saved = PersistedSessions {
            sessions: vec![ChatSession {
                id: "s1".into(),
                title: "会话一".into(),
                messages: vec![ChatMessage {
                    role: ChatRole::User,
                    content: "你好".to_string(),
                }],
            }],
            active_id: "s1".into(),
        };
        let bytes = serde_json::to_vec(&saved).unwrap();
        let loaded: PersistedSessions = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.active_id, "s1");
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
        assert!(plugin.active().messages.is_empty());
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
        assert!(plugin.active().messages.is_empty());
    }

    fn configured_plugin() -> AiPlugin {
        let mut plugin = AiPlugin::init().unwrap();
        plugin.config.providers.push(engine::ProviderConfig {
            id: "p1".into(),
            name: "A".into(),
            base_url: "https://a.example.com/v1".into(),
            api_key: "sk-a".into(),
            models: vec!["m1".into()],
        });
        plugin.config.normalize();
        plugin
    }

    #[test]
    fn test_send_phase_a_appends_user_message_immediately() {
        let mut plugin = configured_plugin();
        plugin.draft = "你好".into();
        let resp = plugin
            .handle_event(UiEvent::Click { id: "btn_send".into() })
            .unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        // 用户消息立即入列，草稿清空，进入等待状态，不产生错误
        assert_eq!(plugin.active().messages.len(), 1);
        assert_eq!(plugin.active().messages[0].content, "你好");
        // 首条用户消息自动命名为会话标题
        assert_eq!(plugin.active().title, "你好");
        assert!(plugin.draft.is_empty());
        assert!(plugin.pending);
        assert!(plugin.error.is_none());
    }

    #[test]
    fn test_assistant_done_success_appends_answer() {
        let mut plugin = configured_plugin();
        plugin.draft = "你好".into();
        plugin.handle_event(UiEvent::Click { id: "btn_send".into() }).unwrap();
        plugin
            .handle_event(UiEvent::AssistantDone {
                content: "答案".into(),
                error: None,
                aborted: false,
            })
            .unwrap();
        assert_eq!(plugin.active().messages.len(), 2);
        assert_eq!(plugin.active().messages[1].content, "答案");
        assert!(!plugin.pending);
        assert!(plugin.status.contains("已回答"));
        assert!(plugin.error.is_none());
    }

    #[test]
    fn test_assistant_done_error_rolls_back_to_draft() {
        let mut plugin = configured_plugin();
        plugin.draft = "你好".into();
        plugin.handle_event(UiEvent::Click { id: "btn_send".into() }).unwrap();
        plugin
            .handle_event(UiEvent::AssistantDone {
                content: String::new(),
                error: Some("AI 接口返回 HTTP 401: bad key".into()),
                aborted: false,
            })
            .unwrap();
        assert!(plugin.active().messages.is_empty());
        assert_eq!(plugin.draft, "你好");
        assert!(plugin.error.as_deref().unwrap().contains("401"));
        assert!(!plugin.pending);
    }

    #[test]
    fn test_assistant_done_abort_keeps_partial() {
        let mut plugin = configured_plugin();
        plugin.draft = "写首诗".into();
        plugin.handle_event(UiEvent::Click { id: "btn_send".into() }).unwrap();
        plugin
            .handle_event(UiEvent::AssistantDone {
                content: "春天来了".into(),
                error: None,
                aborted: true,
            })
            .unwrap();
        assert_eq!(plugin.active().messages.len(), 2);
        assert_eq!(plugin.active().messages[1].content, "春天来了");
        assert!(plugin.status.contains("停止"));
    }

    #[test]
    fn test_assistant_done_stale_is_ignored() {
        let mut plugin = configured_plugin();
        plugin.draft = "你好".into();
        plugin.handle_event(UiEvent::Click { id: "btn_send".into() }).unwrap();
        // 请求期间用户清空了对话，迟到的结果应被丢弃
        plugin.handle_event(UiEvent::Click { id: "btn_clear".into() }).unwrap();
        plugin
            .handle_event(UiEvent::AssistantDone {
                content: "迟到的回答".into(),
                error: None,
                aborted: false,
            })
            .unwrap();
        assert!(plugin.active().messages.is_empty());
        assert!(!plugin.pending);
        assert!(plugin.error.is_none());
    }

    #[test]
    fn test_session_lifecycle_create_switch_delete() {
        let mut plugin = configured_plugin();
        // 新建会话
        plugin.handle_event(UiEvent::Click { id: "btn_new_session".into() }).unwrap();
        assert_eq!(plugin.sessions.len(), 2);
        assert!(plugin.active().messages.is_empty());
        assert_eq!(plugin.active().title, "新会话");

        // 在新会话发送，首条消息自动命名
        plugin.draft = "帮我写排序".into();
        plugin.handle_event(UiEvent::Click { id: "btn_send".into() }).unwrap();
        assert_eq!(plugin.active().title, "帮我写排序");
        plugin
            .handle_event(UiEvent::AssistantDone {
                content: "好的".into(),
                error: None,
                aborted: false,
            })
            .unwrap();
        assert_eq!(plugin.active().messages.len(), 2);

        // 切回第一个（空）会话
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_session".into(),
                index: 0,
                value: "0".into(),
            })
            .unwrap();
        assert!(plugin.active().messages.is_empty());

        // 再切回来，消息还在
        plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_session".into(),
                index: 1,
                value: "1".into(),
            })
            .unwrap();
        assert_eq!(plugin.active().messages.len(), 2);

        // 删除当前会话（还有另一个），自动切换到剩余会话
        plugin.handle_event(UiEvent::Click { id: "btn_clear".into() }).unwrap();
        assert_eq!(plugin.sessions.len(), 1);
        assert!(plugin.active().messages.is_empty());

        // 删除最后一个会话 → 重置为全新空会话（始终保留一个）
        plugin.handle_event(UiEvent::Click { id: "btn_clear".into() }).unwrap();
        assert_eq!(plugin.sessions.len(), 1);
        assert!(plugin.active().messages.is_empty());
        assert_eq!(plugin.active().title, "新会话");
    }

    #[test]
    fn test_select_model_event_updates_selection_and_render() {
        let mut plugin = AiPlugin::init().unwrap();
        plugin.config.providers.push(engine::ProviderConfig {
            id: "p1".into(),
            name: "A".into(),
            base_url: "https://a.example.com/v1".into(),
            api_key: "sk-a".into(),
            models: vec!["m1".into(), "m2".into()],
        });
        plugin.config.providers.push(engine::ProviderConfig {
            id: "p2".into(),
            name: "B".into(),
            base_url: "https://b.example.com/v1".into(),
            api_key: "sk-b".into(),
            models: vec!["m3".into()],
        });
        plugin.config.normalize();

        // 初始选中第一个
        assert_eq!(plugin.config.selected_index(), 0);

        // 切换到 B / m3（索引 2）
        let resp = plugin
            .handle_event(UiEvent::SelectChanged {
                id: "select_model".to_string(),
                index: 2,
                value: "2".to_string(),
            })
            .unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        assert_eq!(plugin.config.selected_provider_id, "p2");
        assert_eq!(plugin.config.selected_model, "m3");
        assert!(plugin.status.contains("B / m3"), "{}", plugin.status);

        // 渲染中包含 select_model 节点且选中索引正确
        let view = plugin.render();
        assert!(find_select_node(&view.root));
    }

    fn find_select_node(node: &UiNode) -> bool {
        match node {
            UiNode::Select { id, selected_index, .. } => {
                id == "select_model" && *selected_index == 2
            }
            UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
                children.iter().any(find_select_node)
            }
            _ => false,
        }
    }

    #[test]
    fn test_ai_plugin_clear_and_copy() {
        let mut plugin = AiPlugin::init().unwrap();

        // 无回答时复制给出提示
        let resp = plugin.handle_event(UiEvent::Click { id: "btn_copy".to_string() }).unwrap();
        assert!(matches!(resp, UiResponse::ShowToast(t) if t.level == ToastLevel::Warning));

        plugin.active_mut().messages = vec![
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
        assert!(plugin.active().messages.is_empty());
        assert!(plugin.draft.is_empty());
        assert!(plugin.status.contains("新会话"));
    }
}
