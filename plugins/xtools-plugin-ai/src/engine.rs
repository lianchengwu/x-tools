use serde::{Deserialize, Serialize};
use xtools_sdk::host;
use xtools_sdk::{ChatMessage, ChatRole, HttpRequest};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
const REQUEST_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            api_key: String::new(),
            model: default_model(),
        }
    }
}

impl AiConfig {
    pub fn is_configured(&self) -> bool {
        !self.base_url.trim().is_empty()
            && !self.api_key.trim().is_empty()
            && !self.model.trim().is_empty()
    }
}

pub fn chat_completion(config: &AiConfig, history: &[ChatMessage]) -> Result<String, String> {
    if history.is_empty() || history.last().map(|m| m.role) != Some(ChatRole::User) {
        return Err("请输入要发送的问题".to_string());
    }
    if !config.is_configured() {
        return Err("请先在托盘菜单「设置」中配置接口地址、API Key 与模型名".to_string());
    }

    let base = config.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/chat/completions");

    let body = ChatRequest {
        model: config.model.trim(),
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
        .with_header("Authorization", format!("Bearer {}", config.api_key.trim()))
        .with_timeout_ms(REQUEST_TIMEOUT_MS);

    let resp = host::http_request(req).map_err(|e| format!("AI 请求失败: {e}"))?;
    if !resp.is_success() {
        return Err(format!("AI 接口返回 HTTP 错误码: {}", resp.status));
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

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert!(!config.is_configured());

        let configured = AiConfig {
            api_key: "sk-test".to_string(),
            ..AiConfig::default()
        };
        assert!(configured.is_configured());
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
        let config = AiConfig {
            base_url: "https://api.example.com/v1/".to_string(),
            api_key: "sk-test".to_string(),
            model: "test-model".to_string(),
        };
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
            model: config.model.trim(),
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
