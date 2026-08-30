//! AI 对话的后台执行：从插件存储读取配置与历史、构建 OpenAI 兼容请求、
//! 以 SSE 流式接收增量。请求在独立线程运行，不阻塞 UI；支持中途取消。

use std::io::BufRead;
use std::io::BufReader;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use xtools_protocol::ChatMessage;
use xtools_protocol::ChatRole;

use crate::ai_config::plugins_root;
use crate::ai_config::AiConfigFile;
use xtools_runtime::storage;

/// 全局超时覆盖整个请求-响应（含流式读取）周期
const STREAM_TIMEOUT_SECS: u64 = 300;

/// 一次流式请求所需的参数（从插件存储解析得到）
#[derive(Debug, Clone)]
pub struct AiChatParams {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// (role, content)，已含刚入列的用户消息
    pub messages: Vec<(String, String)>,
}

/// 请求结果：Completed = 完整回答；Aborted = 用户停止（保留部分内容）；Failed = 失败
#[derive(Debug, Clone)]
pub enum AiOutcome {
    Completed(String),
    Aborted(String),
    Failed(String),
}

/// 由 base_url 拼出 chat/completions 完整接口地址。
/// 用户可能只填根路径（如 https://api.deepseek.com/v1），也可能直接粘贴完整
/// 接口地址，后者不应重复追加。
pub fn chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

/// 从插件存储（SQLite）读取当前配置与「当前激活会话」的对话历史，组装请求参数。
/// 必须在插件完成「阶段 A」（用户消息入列并持久化）之后调用。
pub fn load_chat_context(plugin_id: &str) -> Result<AiChatParams, String> {
    load_chat_context_from(&plugins_root(), plugin_id)
}

pub fn load_chat_context_from(root: &std::path::Path, plugin_id: &str) -> Result<AiChatParams, String> {
    let config_bytes = storage::read_from(root, plugin_id, "config.json")
        .ok_or("尚未找到 AI 配置，请到「设置」中添加服务商")?;
    let mut config: AiConfigFile =
        serde_json::from_slice(&config_bytes).map_err(|e| format!("解析 AI 配置失败: {e}"))?;
    config.normalize();
    let provider = config
        .selected_provider()
        .ok_or("尚未配置 AI 服务商，请到「设置」中添加")?;
    if provider.base_url.trim().is_empty()
        || provider.api_key.trim().is_empty()
        || config.selected_model.trim().is_empty()
    {
        return Err("AI 服务商配置不完整（接口地址 / API Key / 模型）".to_string());
    }

    let messages = read_active_session_messages(root, plugin_id);

    Ok(AiChatParams {
        base_url: provider.base_url.clone(),
        api_key: provider.api_key.clone(),
        model: config.selected_model.clone(),
        messages,
    })
}

/// 读取当前激活会话的消息列表（与插件写入的 sessions.json 结构一致）
fn read_active_session_messages(root: &std::path::Path, plugin_id: &str) -> Vec<(String, String)> {
    #[derive(serde::Deserialize)]
    struct PersistedSession {
        #[serde(default)]
        id: String,
        #[serde(default)]
        messages: Vec<ChatMessage>,
    }
    #[derive(serde::Deserialize)]
    struct PersistedSessions {
        #[serde(default)]
        sessions: Vec<PersistedSession>,
        #[serde(default)]
        active_id: String,
    }

    let Some(bytes) = storage::read_from(root, plugin_id, "sessions.json") else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<PersistedSessions>(&bytes) else {
        return Vec::new();
    };
    let messages = file
        .sessions
        .iter()
        .find(|s| s.id == file.active_id)
        .or_else(|| file.sessions.first())
        .map(|s| s.messages.clone())
        .unwrap_or_default();

    messages
        .into_iter()
        .map(|m| {
            (
                match m.role {
                    ChatRole::User => "user".to_string(),
                    ChatRole::Assistant => "assistant".to_string(),
                },
                m.content,
            )
        })
        .collect()
}

/// 执行流式对话请求（阻塞，应在后台线程调用）。
/// 每收到一个增量就以「当前完整文本」回调 on_delta；结束时返回结果。
pub fn stream_chat(
    params: &AiChatParams,
    cancel: &AtomicBool,
    on_delta: &dyn Fn(String),
) -> AiOutcome {
    let url = chat_url(&params.base_url);
    let body = serde_json::json!({
        "model": params.model.trim(),
        "messages": params
            .messages
            .iter()
            .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
            .collect::<Vec<_>>(),
        "stream": true,
    });

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(STREAM_TIMEOUT_SECS)))
        .http_status_as_error(false)
        .build()
        .into();

    let result = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", params.api_key.trim()))
        .header("Accept", "text/event-stream")
        .send_json(&body);

    let mut response = match result {
        Ok(resp) => resp,
        Err(e) => return AiOutcome::Failed(format!("AI 请求失败: {e}")),
    };

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        return AiOutcome::Failed(format!(
            "AI 接口返回 HTTP {}: {}",
            status,
            error_detail(&body_text)
        ));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    if content_type.contains("text/event-stream") {
        stream_sse(response, cancel, on_delta)
    } else {
        // 服务商不支持流式：整体读取普通 JSON 响应
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        match parse_chat_content(&body_text) {
            Some(content) => {
                on_delta(content.clone());
                AiOutcome::Completed(content)
            }
            None => AiOutcome::Failed(format!(
                "AI 接口返回 HTTP {status}: {}",
                error_detail(&body_text)
            )),
        }
    }
}

fn stream_sse(
    mut response: ureq::http::Response<ureq::Body>,
    cancel: &AtomicBool,
    on_delta: &dyn Fn(String),
) -> AiOutcome {
    let mut acc = String::new();
    let reader = BufReader::new(response.body_mut().as_reader());
    for line in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return AiOutcome::Aborted(acc);
        }
        let line = match line {
            Ok(l) => l,
            Err(e) => return AiOutcome::Failed(format!("读取流式响应失败: {e}")),
        };
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload == "[DONE]" {
            break;
        }
        if payload.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        if let Some(delta) = value["choices"][0]["delta"]["content"].as_str() {
            acc.push_str(delta);
            on_delta(acc.clone());
        }
    }
    if acc.is_empty() {
        AiOutcome::Failed("AI 未返回回答内容".to_string())
    } else {
        AiOutcome::Completed(acc)
    }
}

/// 从 OpenAI 兼容的非流式响应中取回答内容
fn parse_chat_content(body_text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body_text).ok()?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

/// 优先透出接口 JSON 错误信息，否则截取响应体片段（超时、鉴权失败等）
fn error_detail(body_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .map(str::to_string)
                .or_else(|| v["message"].as_str().map(str::to_string))
        })
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| truncate(body_text, 200))
}

fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn spawn_stub(
        respond: impl FnOnce(&mut std::net::TcpStream) + Send + 'static,
    ) -> (u16, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        use std::io::Read as _;
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf: Vec<u8> = Vec::new();
            let mut tmp = [0u8; 8192];
            loop {
                let n = stream.read(&mut tmp).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                    let mut content_length = 0usize;
                    for l in headers.lines() {
                        if let Some(v) = l.strip_prefix("content-length:") {
                            content_length = v.trim().parse().unwrap_or(0);
                        }
                    }
                    if buf.len() >= pos + 4 + content_length {
                        break;
                    }
                }
            }
            let raw = String::from_utf8_lossy(&buf).to_string();
            use std::io::Write;
            let mut stream = stream;
            respond(&mut stream);
            let _ = stream.flush();
            raw
        });
        (port, handle)
    }

    fn sse_response(chunks: Vec<String>) -> impl FnOnce(&mut std::net::TcpStream) {
        move |stream: &mut std::net::TcpStream| {
            use std::io::Write;
            let head =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(head.as_bytes());
            for c in chunks {
                // chunked 编码：每段一个 chunk
                let payload = format!("{c}\n\n");
                let _ = write!(stream, "{:x}\r\n", payload.len());
                let _ = stream.write_all(payload.as_bytes());
                let _ = stream.write_all(b"\r\n");
                let _ = stream.flush();
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            let _ = stream.write_all(b"0\r\n\r\n");
        }
    }

    fn params(port: u16) -> AiChatParams {
        AiChatParams {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            api_key: "sk-test".into(),
            model: "test-model".into(),
            messages: vec![("user".into(), "你好".into())],
        }
    }

    #[test]
    fn test_load_chat_context_reads_active_session_from_db() {
        let root = std::env::temp_dir().join(format!(
            "xtools-ai-ctx-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let ai = "xtools.ai";

        // 配置（与插件共享同一 SQLite 键值）
        storage::write_to(
            &root,
            ai,
            "config.json",
            r#"{"providers":[{"id":"p1","name":"Stub","base_url":"http://127.0.0.1:9/v1","api_key":"sk-stub","models":["m1","m2"]}],"selected_provider_id":"p1","selected_model":"m2"}"#
                .as_bytes(),
        )
        .unwrap();

        // 两个会话，激活的是第二个 —— 请求必须携带它的消息，而不是另一个会话的
        let sessions_json = r#"{"sessions":[
                {"id":"s1","title":"旧会话","messages":[{"role":"User","content":"上一条会话的问题"}]},
                {"id":"s2","title":"当前","messages":[{"role":"User","content":"你好"},{"role":"Assistant","content":"你好！"},{"role":"User","content":"刚输入的问题"}]}
            ],"active_id":"s2"}"#;
        storage::write_to(&root, ai, "sessions.json", sessions_json.as_bytes()).unwrap();

        let ctx = load_chat_context_from(&root, ai).expect("load context");
        assert_eq!(ctx.model, "m2");
        assert_eq!(ctx.api_key, "sk-stub");
        // 请求消息 = 激活会话「s2」的完整历史（以刚输入的用户消息结尾）
        assert_eq!(ctx.messages.len(), 3, "{:?}", ctx.messages);
        assert_eq!(ctx.messages[2], ("user".to_string(), "刚输入的问题".to_string()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_chat_url_join() {
        assert_eq!(
            chat_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://x.example.com/v1/chat/completions/"),
            "https://x.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_stream_chat_sse_deltas_and_request_shape() {
        let (port, server) = spawn_stub(
            sse_response(vec![
                r#"data: {"choices":[{"delta":{"content":"你"}}]}"#.to_string(),
                r#"data: {"choices":[{"delta":{"content":"好"}}]}"#.to_string(),
                "data: [DONE]".to_string(),
            ]),
        );

        let deltas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let deltas_cb = deltas.clone();
        let cancel = AtomicBool::new(false);
        let outcome = stream_chat(&params(port), &cancel, &move |full: String| {
            deltas_cb.lock().unwrap().push(full);
        });

        let raw = server.join().unwrap();
        println!("captured request:\n{raw}");

        // 请求体：stream=true、model、messages、鉴权头
        assert!(raw.starts_with("POST /v1/chat/completions HTTP/1.1"), "{raw}");
        let body = raw.split("\r\n\r\n").nth(1).unwrap();
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["model"], "test-model");
        assert_eq!(json["stream"], true);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "你好");
        assert!(raw.to_lowercase().contains("authorization: bearer sk-test"));
        assert!(raw.to_lowercase().contains("accept: text/event-stream"));

        // 增量回调：逐字累积的完整文本
        let d = deltas.lock().unwrap();
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!(d[0], "你");
        assert_eq!(d[1], "你好");

        match outcome {
            AiOutcome::Completed(text) => assert_eq!(text, "你好"),
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn test_stream_chat_non_stream_fallback() {
        let (port, server) = spawn_stub(|stream| {
            use std::io::Write;
            let body = r#"{"choices":[{"message":{"role":"assistant","content":"整体回答"}}]}"#;
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            );
        });
        let seen = Arc::new(std::sync::Mutex::new(0));
        let seen_cb = seen.clone();
        let outcome = stream_chat(
            &params(port),
            &AtomicBool::new(false),
            &move |_full: String| {
                *seen_cb.lock().unwrap() += 1;
            },
        );
        let _ = server.join();
        assert_eq!(*seen.lock().unwrap(), 1);
        assert!(matches!(outcome, AiOutcome::Completed(ref t) if t == "整体回答"), "{outcome:?}");
    }

    #[test]
    fn test_stream_chat_http_error_surfaces_body() {
        let (port, server) = spawn_stub(|stream| {
            use std::io::Write;
            let body = r#"{"error":{"message":"Invalid API key"}}"#;
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            );
        });
        let outcome = stream_chat(&params(port), &AtomicBool::new(false), &|_| {});
        let _ = server.join();
        match outcome {
            AiOutcome::Failed(msg) => {
                assert!(msg.contains("401"), "{msg}");
                assert!(msg.contains("Invalid API key"), "{msg}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn test_stream_chat_cancel_returns_partial() {
        let (port, server) = spawn_stub(
            sse_response(vec![
                r#"data: {"choices":[{"delta":{"content":"部分"}}]}"#.to_string(),
                r#"data: {"choices":[{"delta":{"content":"内容"}}]}"#.to_string(),
                "data: [DONE]".to_string(),
            ]),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_cb = cancel.clone();
        let first = Arc::new(std::sync::Mutex::new(true));
        let first_cb = first.clone();
        let outcome = stream_chat(&params(port), &cancel, &move |full: String| {
            // 收到第一个增量后立即请求取消
            if *first_cb.lock().unwrap() {
                *first_cb.lock().unwrap() = false;
                cancel_cb.store(true, Ordering::Relaxed);
            }
            let _ = full;
        });
        let _ = server.join();
        assert!(matches!(outcome, AiOutcome::Aborted(ref t) if t == "部分"), "{outcome:?}");
        let _ = cancel;
    }
}
