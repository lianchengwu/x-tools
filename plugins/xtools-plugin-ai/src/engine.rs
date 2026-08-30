use serde::{Deserialize, Serialize};
use xtools_sdk::host;
use xtools_sdk::{ChatMessage, ChatRole, HttpRequest};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
const REQUEST_TIMEOUT_MS: u64 = 120_000;

/// 一个服务商（OpenAI 兼容接口）：名称 + 地址 + 密钥 + 该服务商下的模型列表
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
}

impl ProviderConfig {
    pub fn is_usable(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty()
    }
}

/// AI 服务配置：多服务商、多模型，外加当前选中的服务商与模型。
/// base_url / api_key / model 为旧版单服务商字段，仅用于读取旧配置迁移，不再写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub selected_provider_id: String,
    #[serde(default)]
    pub selected_model: String,
    #[serde(default, skip_serializing)]
    pub base_url: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default, skip_serializing)]
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            selected_provider_id: String::new(),
            selected_model: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
        }
    }
}

impl AiConfig {
    /// 迁移旧版单服务商配置并校正失效的选中项。
    /// 返回 true 表示结构发生了变化（调用方可选择写回存储）。
    pub fn normalize(&mut self) -> bool {
        let mut changed = false;

        if self.providers.is_empty()
            && (!self.base_url.trim().is_empty()
                || !self.api_key.trim().is_empty()
                || !self.model.trim().is_empty())
        {
            let id = format!("p{}", host::now_millis());
            let model = self.model.trim().to_string();
            self.providers.push(ProviderConfig {
                id: id.clone(),
                name: "默认".to_string(),
                base_url: self.base_url.trim().to_string(),
                api_key: self.api_key.trim().to_string(),
                models: if model.is_empty() {
                    Vec::new()
                } else {
                    vec![model.clone()]
                },
            });
            self.selected_provider_id = id;
            self.selected_model = model;
            self.base_url.clear();
            self.api_key.clear();
            self.model.clear();
            changed = true;
        }

        // 选中的服务商不存在时回落到第一个
        if self.selected_provider_id.is_empty()
            || !self.providers.iter().any(|p| p.id == self.selected_provider_id)
        {
            self.selected_provider_id = self
                .providers
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_default();
            self.selected_model.clear();
            changed = true;
        }

        // 选中的模型不在当前服务商列表中时回落到第一个
        if let Some(provider) = self.selected_provider() {
            if self.selected_model.is_empty()
                || !provider.models.iter().any(|m| *m == self.selected_model)
            {
                self.selected_model = provider.models.first().cloned().unwrap_or_default();
                changed = true;
            }
        }

        changed
    }

    pub fn selected_provider(&self) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|p| p.id == self.selected_provider_id)
    }

    pub fn is_configured(&self) -> bool {
        !self.selected_model.trim().is_empty()
            && self.selected_provider().is_some_and(ProviderConfig::is_usable)
    }

    /// 全部可选模型，形如「服务商 / 模型」。返回 (provider_id, model, 展示名)。
    pub fn model_options(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for provider in &self.providers {
            for model in &provider.models {
                let display = if provider.name.trim().is_empty() {
                    model.clone()
                } else {
                    format!("{} / {}", provider.name.trim(), model)
                };
                out.push((provider.id.clone(), model.clone(), display));
            }
        }
        out
    }

    pub fn selected_index(&self) -> usize {
        self.model_options()
            .iter()
            .position(|(pid, m, _)| pid == &self.selected_provider_id && m == &self.selected_model)
            .unwrap_or(0)
    }

    pub fn select_by_index(&mut self, index: usize) {
        if let Some((pid, model, _)) = self.model_options().get(index) {
            self.selected_provider_id = pid.clone();
            self.selected_model = model.clone();
        }
    }
}

/// 由 base_url 拼出 chat/completions 完整接口地址。
/// 用户可能只填根路径（如 https://api.deepseek.com/v1），也可能直接粘贴完整
/// 接口地址，后者不应重复追加。
fn chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

pub fn chat_completion(config: &AiConfig, history: &[ChatMessage]) -> Result<String, String> {
    if history.is_empty() || history.last().map(|m| m.role) != Some(ChatRole::User) {
        return Err("请输入要发送的问题".to_string());
    }
    let Some(provider) = config.selected_provider() else {
        return Err("请先在托盘菜单「设置」中添加服务商与模型".to_string());
    };
    if !provider.is_usable() || config.selected_model.trim().is_empty() {
        return Err("请先在托盘菜单「设置」中补全接口地址、API Key 与模型".to_string());
    }

    let url = chat_url(&provider.base_url);

    let body = ChatRequest {
        model: config.selected_model.trim(),
        messages: history
            .iter()
            .map(|m| WireMessage {
                role: match m.role {
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                },
                content: m.content.as_str(),
            })
            .collect(),
        stream: false,
    };
    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| format!("序列化请求失败: {e}"))?;

    let req = HttpRequest::post(url, body_bytes)
        .with_header("Content-Type", "application/json")
        .with_header("Authorization", format!("Bearer {}", provider.api_key.trim()))
        .with_timeout_ms(REQUEST_TIMEOUT_MS);

    let resp = host::http_request(req).map_err(|e| format!("AI 请求失败: {e}"))?;
    if !resp.is_success() {
        // 网络层失败时宿主返回 502 + "HTTP error: ..."，接口报错时返回原始响应体；
        // 优先透出接口 JSON 错误信息，否则截取响应体片段帮助定位（超时、鉴权失败等）。
        let body_text = resp.text().unwrap_or_default();
        let detail = serde_json::from_str::<ChatResponse>(&body_text)
            .ok()
            .and_then(|parsed| parsed.error)
            .and_then(|err| err.message)
            .filter(|msg| !msg.trim().is_empty())
            .unwrap_or_else(|| truncate_for_display(&body_text, 200));
        return Err(format!("AI 接口返回 HTTP {}: {}", resp.status, detail));
    }

    let body_text = resp.text().map_err(|e| format!("解析响应失败: {e}"))?;
    let parsed: ChatResponse =
        serde_json::from_str(&body_text).map_err(|e| format!("解析 AI JSON 响应失败: {e}"))?;

    parsed.into_text()
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
}

/// 截取错误响应体片段用于展示，避免长 HTML/JSON 刷屏
fn truncate_for_display(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    error: Option<ApiErrorBody>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatChoiceMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    message: Option<String>,
}

impl ChatResponse {
    fn into_text(self) -> Result<String, String> {
        if let Some(err) = &self.error {
            if let Some(msg) = &err.message {
                if !msg.trim().is_empty() {
                    return Err(format!("AI 接口错误: {msg}"));
                }
            }
        }
        for choice in self.choices {
            if let Some(message) = choice.message {
                if let Some(content) = message.content {
                    if !content.trim().is_empty() {
                        return Ok(content);
                    }
                }
            }
        }
        if let Some(msg) = &self.message {
            if !msg.trim().is_empty() {
                return Err(format!("AI 接口错误: {msg}"));
            }
        }
        Err("AI 未返回回答内容".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, name: &str, models: &[&str]) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            name: name.to_string(),
            base_url: format!("https://api.{name}.example.com/v1"),
            api_key: format!("sk-{name}"),
            models: models.iter().map(|m| m.to_string()).collect(),
        }
    }

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert!(!config.is_configured());
        assert!(config.model_options().is_empty());

        let mut config = AiConfig::default();
        config.providers.push(provider("p1", "DeepSeek", &["deepseek-chat"]));
        config.normalize();
        assert!(config.is_configured());
        assert_eq!(config.selected_model, "deepseek-chat");
    }

    #[test]
    fn test_legacy_config_migration() {
        let raw = r#"{"base_url": "https://api.old.com/v1", "api_key": "sk-old", "model": "gpt-4o-mini"}"#;
        let mut config: AiConfig = serde_json::from_str(raw).unwrap();
        assert!(config.normalize(), "首次迁移应发生变化");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].name, "默认");
        assert_eq!(config.providers[0].models, vec!["gpt-4o-mini"]);
        assert_eq!(config.selected_model, "gpt-4o-mini");
        assert!(config.is_configured());
        assert!(!config.normalize(), "重复迁移应无变化");

        // 迁移后序列化不再包含旧字段
        let bytes = serde_json::to_vec(&config).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("base_url\": \"https://api.old.com"), "{text}");
    }

    #[test]
    fn test_normalize_fixes_dangling_selection() {
        let mut config = AiConfig::default();
        config.providers.push(provider("p1", "A", &["m1", "m2"]));
        config.providers.push(provider("p2", "B", &["m3"]));
        config.selected_provider_id = "pX".into();
        config.selected_model = "nope".into();
        config.normalize();
        assert_eq!(config.selected_provider_id, "p1");
        assert_eq!(config.selected_model, "m1");

        // 删除选中模型后回落
        config.providers[0].models.clear();
        assert!(config.normalize());
        assert_eq!(config.selected_model, "");
        assert!(!config.is_configured());
    }

    #[test]
    fn test_model_selection() {
        let mut config = AiConfig::default();
        config.providers.push(provider("p1", "A", &["m1"]));
        config.providers.push(provider("p2", "B", &["m2", "m3"]));

        assert_eq!(config.model_options().len(), 3);
        assert_eq!(config.model_options()[2].2, "B / m3");

        config.select_by_index(2);
        assert_eq!(config.selected_provider_id, "p2");
        assert_eq!(config.selected_model, "m3");
        assert_eq!(config.selected_index(), 2);

        config.select_by_index(0);
        assert_eq!(config.selected_provider_id, "p1");
        assert_eq!(config.selected_model, "m1");

        // 越界不动
        config.select_by_index(99);
        assert_eq!(config.selected_model, "m1");
    }

    #[test]
    fn test_chat_completion_validates_input() {
        let config = AiConfig::default();
        let empty: [ChatMessage; 0] = [];
        assert_eq!(
            chat_completion(&config, &empty).unwrap_err(),
            "请输入要发送的问题"
        );
        let no_user = [ChatMessage {
            role: ChatRole::Assistant,
            content: "你好".to_string(),
        }];
        assert_eq!(
            chat_completion(&config, &no_user).unwrap_err(),
            "请输入要发送的问题"
        );
        let user_only = [ChatMessage {
            role: ChatRole::User,
            content: "你好".to_string(),
        }];
        assert!(chat_completion(&config, &user_only).unwrap_err().contains("托盘"));

        // 服务商存在但信息不全
        let mut half = AiConfig::default();
        half.providers.push(ProviderConfig {
            id: "p1".into(),
            name: "A".into(),
            base_url: "".into(),
            api_key: "sk".into(),
            models: vec!["m".into()],
        });
        half.normalize();
        assert!(chat_completion(&half, &user_only).unwrap_err().contains("补全"));
    }

    #[test]
    fn test_chat_url_join() {
        assert_eq!(
            chat_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        // 末尾斜杠
        assert_eq!(
            chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        // 直接粘贴完整接口地址：不重复追加
        assert_eq!(
            chat_url("https://x.example.com/v1/chat/completions"),
            "https://x.example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://x.example.com/v1/chat/completions/"),
            "https://x.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_truncate_for_display() {
        assert_eq!(truncate_for_display("  short  ", 10), "short");
        let long = "x".repeat(300);
        let cut = truncate_for_display(&long, 200);
        assert_eq!(cut.chars().count(), 201); // 200 字符 + 省略号
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn test_deserialize_openai_response() {
        let raw = r#"{
            "id": "chatcmpl-1",
            "choices": [
                {"index": 0, "message": {"role": "assistant", "content": "你好！"}}
            ]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.into_text().unwrap(), "你好！");
    }

    #[test]
    fn test_deserialize_error_response() {
        let raw = r#"{"error": {"message": "Incorrect API key", "code": "invalid_api_key"}}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert!(parsed.into_text().unwrap_err().contains("Incorrect API key"));

        let raw2 = r#"{"message": "rate limited"}"#;
        let parsed2: ChatResponse = serde_json::from_str(raw2).unwrap();
        assert!(parsed2.into_text().unwrap_err().contains("rate limited"));
    }

    #[test]
    fn test_deserialize_empty_choices() {
        let raw = r#"{"choices": []}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.into_text().unwrap_err(), "AI 未返回回答内容");
    }

    #[test]
    fn test_request_body_shape() {
        let mut config = AiConfig::default();
        config.providers.push(provider("p1", "test", &["test-model", "unused"]));
        config.normalize();
        config.select_by_index(0);
        let history = vec![
            ChatMessage {
                role: ChatRole::User,
                content: "你好".to_string(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "你好！".to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: "继续".to_string(),
            },
        ];
        let body = ChatRequest {
            model: config.selected_model.trim(),
            messages: history
                .iter()
                .map(|m| WireMessage {
                    role: match m.role {
                        ChatRole::User => "user",
                        ChatRole::Assistant => "assistant",
                    },
                    content: m.content.as_str(),
                })
                .collect(),
            stream: false,
        };
        let json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(json["model"], "test-model");
        assert_eq!(json["stream"], false);
        assert_eq!(json["messages"].as_array().unwrap().len(), 3);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "你好");
        assert_eq!(json["messages"][1]["role"], "assistant");
        assert_eq!(json["messages"][2]["content"], "继续");
    }
}
