use std::path::PathBuf;
use xtools_protocol::*;
use xtools_runtime::PluginLoader;

/// Resolve the built artifact of a plugin. Prefers the portable `dist/plugins`
/// layout (short names), then falls back to cargo's wasm32 release output
/// (crate names). Returns None on a fresh checkout before any plugin was built.
fn plugin_artifact(name: &str) -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let candidates = [
        root.join("dist/plugins").join(format!("{name}.wasm")),
        root.join("target/wasm32-unknown-unknown/release")
            .join(format!("xtools_plugin_{name}.wasm")),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn require_plugin_artifact(test: &str, name: &str) -> Option<PathBuf> {
    match plugin_artifact(name) {
        Some(path) => Some(path),
        None => {
            eprintln!(
                "skipping {test}: no built {name} plugin artifact found; \
                 build it first with `cargo build --target wasm32-unknown-unknown --release -p xtools-plugin-{name}`"
            );
            None
        }
    }
}

#[test]
fn test_plugin_scanner_discovers_all_plugins() {
    let Some(plugins_dir) = require_plugin_artifact("test_plugin_scanner_discovers_all_plugins", "time")
        .map(|p| p.parent().unwrap().to_path_buf())
    else {
        return;
    };
    let loader = PluginLoader::new();
    let discovered = loader.scan_dir(&plugins_dir);

    assert_eq!(discovered.len(), 4, "Expected 4 plugins, found {:?}", discovered);

    let ids: Vec<_> = discovered.iter().map(|p| p.manifest.id.as_str()).collect();
    assert!(ids.contains(&"xtools.time"));
    assert!(ids.contains(&"xtools.json"));
    assert!(ids.contains(&"xtools.trans"));

    let time_p = discovered.iter().find(|p| p.manifest.id == "xtools.time").unwrap();
    assert_eq!(time_p.manifest.name, "时间戳转换");
    assert_eq!(time_p.manifest.mark, "clock");

    let json_p = discovered.iter().find(|p| p.manifest.id == "xtools.json").unwrap();
    assert_eq!(json_p.manifest.name, "JSON 格式化与校验");
    assert_eq!(json_p.manifest.mark, "{}");

    let trans_p = discovered.iter().find(|p| p.manifest.id == "xtools.trans").unwrap();
    assert_eq!(trans_p.manifest.name, "智能翻译");
    assert_eq!(trans_p.manifest.mark, "文");

    let ai_p = discovered.iter().find(|p| p.manifest.id == "xtools.ai").unwrap();
    assert_eq!(ai_p.manifest.name, "AI 问答");
    assert_eq!(ai_p.manifest.mark, "智");
}

#[test]
fn test_time_plugin_lifecycle_and_events() {
    let Some(path) = require_plugin_artifact("test_time_plugin_lifecycle_and_events", "time") else {
        return;
    };
    let loader = PluginLoader::new();
    let mut instance = loader.load_instance(&path).expect("Failed to load time plugin");

    // 1. Initial render
    let view = instance.render().expect("Failed to render initial view");
    assert!(matches!(view.root, UiNode::Container { .. }));

    // 2. Simulate input of seconds: "1700000000"
    let evt = UiEvent::InputChanged {
        id: "input_seconds".to_string(),
        value: "1700000000".to_string(),
    };
    let resp = instance.handle_event(&evt).expect("Failed to handle seconds input");
    if let UiResponse::UpdateView(new_view) = resp {
        let serialized = serde_json::to_string(&new_view).unwrap();
        assert!(serialized.contains("1700000000"));
        assert!(serialized.contains("1700000000000"));
        assert!(serialized.contains("2023-11-15"));
    } else {
        panic!("Expected UpdateView response");
    }

    // 3. Test copy click event
    let copy_evt = UiEvent::Click { id: "copy_seconds".to_string() };
    let copy_resp = instance.handle_event(&copy_evt).expect("Failed to handle copy event");
    assert!(matches!(copy_resp, UiResponse::ShowToast(..)));

    // 4. Test Timezone change
    let tz_evt = UiEvent::SelectChanged {
        id: "select_tz".to_string(),
        index: 1, // UTC
        value: "1".to_string(),
    };
    let tz_resp = instance.handle_event(&tz_evt).expect("Failed to change timezone");
    assert!(matches!(tz_resp, UiResponse::UpdateView(..)));
}

#[test]
fn test_json_plugin_lifecycle_and_formatting() {
    let Some(path) = require_plugin_artifact("test_json_plugin_lifecycle_and_formatting", "json") else {
        return;
    };
    let loader = PluginLoader::new();
    let mut instance = loader.load_instance(&path).expect("Failed to load json plugin");

    // 1. Send input with unformatted JSON
    let unformatted = r#"{"b":2,"a":1}"#;
    let evt_input = UiEvent::InputChanged {
        id: "json_code".to_string(),
        value: unformatted.to_string(),
    };
    let _ = instance.handle_event(&evt_input).expect("Failed to input JSON");

    // 2. Click Format
    let evt_format = UiEvent::Click { id: "btn_format".to_string() };
    let resp_format = instance.handle_event(&evt_format).expect("Failed to format JSON");
    if let UiResponse::UpdateView(view) = resp_format {
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains(r#"{\n  \"b\": 2,\n  \"a\": 1\n}"#) || serialized.contains(r#"{\n  \"a\": 1"#));
        assert!(serialized.contains("已格式化"));
    } else {
        panic!("Expected UpdateView response");
    }

    // 3. Click Minify
    let evt_minify = UiEvent::Click { id: "btn_minify".to_string() };
    let resp_minify = instance.handle_event(&evt_minify).expect("Failed to minify JSON");
    if let UiResponse::UpdateView(view) = resp_minify {
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains(r#"{\"b\":2,\"a\":1}"#) || serialized.contains(r#"{\"a\":1,\"b\":2}"#));
        assert!(serialized.contains("已压缩"));
    } else {
        panic!("Expected UpdateView response");
    }

    // 4. Click Validate
    let evt_val = UiEvent::Click { id: "btn_validate".to_string() };
    let resp_val = instance.handle_event(&evt_val).expect("Failed to validate JSON");
    if let UiResponse::UpdateView(view) = resp_val {
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains("JSON 有效"));
    }

    // 5. Test Unescape
    let evt_unescape_in = UiEvent::InputChanged {
        id: "json_code".to_string(),
        value: r#""{\"nested\": 42}""#.to_string(),
    };
    let _ = instance.handle_event(&evt_unescape_in).expect("Failed to input escaped JSON");
    let evt_unescape = UiEvent::Click { id: "btn_unescape".to_string() };
    let resp_unescape = instance.handle_event(&evt_unescape).expect("Failed to unescape JSON");
    if let UiResponse::UpdateView(view) = resp_unescape {
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains("已去转义"));
        assert!(serialized.contains("nested") && serialized.contains("42"));
    }
    // 6. Test Invalid JSON Error Reporting
    let evt_bad = UiEvent::InputChanged {
        id: "json_code".to_string(),
        value: r#"{"unclosed": "#.to_string(),
    };
    let resp_bad = instance.handle_event(&evt_bad).expect("Failed to process invalid JSON");
    if let UiResponse::UpdateView(view) = resp_bad {
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(serialized.contains("第 1 行") || serialized.contains("Error") || serialized.contains("EOF"));
    }
}

#[test]
fn test_trans_plugin_lifecycle_and_storage() {
    let Some(path) = require_plugin_artifact("test_trans_plugin_lifecycle_and_storage", "trans") else {
        return;
    };
    let temp_dir = std::env::temp_dir().join(format!("xtools-test-{}", std::process::id()));
    let loader = PluginLoader::new().with_storage_root(temp_dir.clone());
    let mut instance = loader.load_instance(&path).expect("Failed to load trans plugin");

    // 1. Initial view render
    let view = instance.render().expect("Failed to render trans view");
    assert!(matches!(view.root, UiNode::Container { .. }));

    // 2. 切换引擎触发插件持久化（AppID/密钥配置已移至宿主设置窗口）
    let select_resp = instance.handle_event(&UiEvent::SelectChanged {
        id: "select_engine".to_string(),
        index: 1, // 百度翻译
        value: "1".to_string(),
    }).expect("Failed to select engine");
    assert!(matches!(select_resp, UiResponse::UpdateView(..)));

    // Verify storage persisted via host capability (SQLite)
    let content = xtools_runtime::storage::read_from(&temp_dir, "xtools.trans", "config.json")
        .map(|b| String::from_utf8(b).unwrap())
        .expect("Expected config to be written by host storage API");
    assert!(content.contains("engine_index"), "engine_index should be persisted: {content}");

    // 4. Test language swap
    let _ = instance.handle_event(&UiEvent::SelectChanged {
        id: "select_src_lang".to_string(),
        index: 1, // zh-CN
        value: "1".to_string(),
    }).unwrap();
    let _ = instance.handle_event(&UiEvent::SelectChanged {
        id: "select_dst_lang".to_string(),
        index: 1, // en
        value: "1".to_string(),
    }).unwrap();
    let _ = instance.handle_event(&UiEvent::InputChanged {
        id: "input_source".to_string(),
        value: "苹果".to_string(),
    }).unwrap();

    let swap_resp = instance.handle_event(&UiEvent::Click {
        id: "btn_swap_lang".to_string(),
    }).unwrap();
    assert!(matches!(swap_resp, UiResponse::UpdateView(..)));

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// -----------------------------------------------------------------------------
// AI 插件端到端验证：阶段 A（用户消息立即入列）+ 宿主 AssistantDone 回填流程
// 注意：实际 HTTP 请求由宿主后台线程执行（见 xtools-host/src/ai_runtime.rs），
// 插件侧不再发起网络请求，因此这里用 AssistantDone 事件模拟宿主回填。
// -----------------------------------------------------------------------------

fn collect_chat_messages(node: &UiNode, out: &mut Vec<(usize, String)>) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                collect_chat_messages(child, out);
            }
        }
        UiNode::Chat { messages, .. } => {
            for m in messages {
                out.push((m.role as usize, m.content.clone()));
            }
        }
        _ => {}
    }
}

fn find_ai_error(node: &UiNode, out: &mut Vec<String>) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                find_ai_error(child, out);
            }
        }
        UiNode::Label { text, variant, .. } if *variant == LabelVariant::Error => {
            out.push(text.clone());
        }
        _ => {}
    }
}

fn collect_drafts(node: &UiNode, out: &mut Vec<String>) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                collect_drafts(child, out);
            }
        }
        UiNode::TextInput { id, value, .. } if id == "input_draft" => {
            out.push(value.clone());
        }
        _ => {}
    }
}

/// 每个 AI 测试用独立临时目录：libtest 并行执行时共享目录会被
/// 其它测试的 remove_dir_all 删掉正在使用的 SQLite 存储（Windows 上尤甚）。
fn load_ai_instance(test: &str) -> Option<(xtools_runtime::PluginInstance, PathBuf)> {
    let path = require_plugin_artifact("test_ai_plugin_flow", "ai")?;
    let temp_dir = std::env::temp_dir().join(format!("xtools-ai-test-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    Some((PluginLoader::new().with_storage_root(temp_dir.clone()).load_instance(&path).expect("load ai plugin"), temp_dir))
}

/// 写入多服务商配置（SQLite 键值；temp_dir 即存储根目录）
fn seed_ai_config(temp_dir: &std::path::Path, selected_model: &str) {
    let json = r#"{"providers":[{"id":"p1","name":"Stub","base_url":"http://127.0.0.1:9/v1","api_key":"sk-stub","models":["m1","m2"]}],"selected_provider_id":"p1","selected_model":"MODEL"}"#
        .replace("MODEL", selected_model);
    xtools_runtime::storage::write_to(temp_dir, "xtools.ai", "config.json", json.as_bytes())
        .unwrap();
}

#[test]
fn test_ai_send_phase_a_and_assistant_done_success() {
    let Some((mut instance, temp_dir)) = load_ai_instance("test_ai_send_phase_a_and_assistant_done_success") else {
        return;
    };
    seed_ai_config(&temp_dir, "m1");
    instance.init().expect("init ai plugin");

    // 阶段 A：发送后用户消息立即入列、草稿清空、无错误（不产生任何网络请求）
    instance.handle_event(&UiEvent::InputChanged {
        id: "input_draft".to_string(),
        value: "你好".to_string(),
    }).expect("input draft");
    let resp = instance.handle_event(&UiEvent::Click { id: "btn_send".to_string() }).unwrap();
    let UiResponse::UpdateView(view) = resp else {
        panic!("expected UpdateView after send");
    };
    let mut msgs = Vec::new();
    collect_chat_messages(&view.root, &mut msgs);
    assert_eq!(msgs.len(), 1, "phase A should only append the user message: {msgs:?}");
    assert_eq!(msgs[0].0, 0);
    assert_eq!(msgs[0].1, "你好");
    let mut drafts = Vec::new();
    collect_drafts(&view.root, &mut drafts);
    assert_eq!(drafts.first().map(String::as_str), Some(""));
    let mut errors = Vec::new();
    find_ai_error(&view.root, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");

    // 宿主回填：追加助手回答
    let resp = instance.handle_event(&UiEvent::AssistantDone {
        content: "这是回答".to_string(),
        error: None,
        aborted: false,
    }).unwrap();
    let UiResponse::UpdateView(view) = resp else {
        panic!("expected UpdateView after done");
    };
    let mut msgs = Vec::new();
    collect_chat_messages(&view.root, &mut msgs);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].0, 1);
    assert_eq!(msgs[1].1, "这是回答");

    // 会话已持久化到 SQLite（供下次打开恢复）
    let sessions = xtools_runtime::storage::read_from(&temp_dir, "xtools.ai", "sessions.json")
        .expect("sessions should be persisted");
    let sessions = String::from_utf8(sessions).unwrap();
    assert!(sessions.contains("这是回答"), "{sessions}");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_ai_assistant_done_error_rolls_back() {
    let Some((mut instance, temp_dir)) = load_ai_instance("test_ai_assistant_done_error_rolls_back") else {
        return;
    };
    seed_ai_config(&temp_dir, "m1");
    instance.init().expect("init ai plugin");

    instance.handle_event(&UiEvent::InputChanged {
        id: "input_draft".to_string(),
        value: "你好".to_string(),
    }).unwrap();
    instance.handle_event(&UiEvent::Click { id: "btn_send".to_string() }).unwrap();

    // 失败回填：用户消息回滚到草稿，展示错误
    let resp = instance.handle_event(&UiEvent::AssistantDone {
        content: String::new(),
        error: Some("AI 接口返回 HTTP 401: Invalid API key".to_string()),
        aborted: false,
    }).unwrap();
    let UiResponse::UpdateView(view) = resp else {
        panic!("expected UpdateView");
    };
    let mut msgs = Vec::new();
    collect_chat_messages(&view.root, &mut msgs);
    assert!(msgs.is_empty(), "failed send should roll back: {msgs:?}");
    let mut drafts = Vec::new();
    collect_drafts(&view.root, &mut drafts);
    assert_eq!(drafts.first().map(String::as_str), Some("你好"));
    let mut errors = Vec::new();
    find_ai_error(&view.root, &mut errors);
    assert!(errors.iter().any(|e| e.contains("401")), "{errors:?}");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_ai_assistant_done_stale_is_ignored() {
    let Some((mut instance, temp_dir)) = load_ai_instance("test_ai_assistant_done_stale_is_ignored") else {
        return;
    };
    seed_ai_config(&temp_dir, "m1");
    instance.init().expect("init ai plugin");

    instance.handle_event(&UiEvent::InputChanged {
        id: "input_draft".to_string(),
        value: "你好".to_string(),
    }).unwrap();
    instance.handle_event(&UiEvent::Click { id: "btn_send".to_string() }).unwrap();
    // 请求期间清空对话，迟到的结果应被丢弃
    instance.handle_event(&UiEvent::Click { id: "btn_clear".to_string() }).unwrap();
    instance.handle_event(&UiEvent::AssistantDone {
        content: "迟到的回答".to_string(),
        error: None,
        aborted: false,
    }).unwrap();

    let view = instance.render().unwrap();
    let mut msgs = Vec::new();
    collect_chat_messages(&view.root, &mut msgs);
    assert!(msgs.is_empty(), "stale result must be dropped: {msgs:?}");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_ai_assistant_done_abort_keeps_partial() {
    let Some((mut instance, temp_dir)) = load_ai_instance("test_ai_assistant_done_abort_keeps_partial") else {
        return;
    };
    seed_ai_config(&temp_dir, "m1");
    instance.init().expect("init ai plugin");

    instance.handle_event(&UiEvent::InputChanged {
        id: "input_draft".to_string(),
        value: "写首诗".to_string(),
    }).unwrap();
    instance.handle_event(&UiEvent::Click { id: "btn_send".to_string() }).unwrap();
    instance.handle_event(&UiEvent::AssistantDone {
        content: "春天来了".to_string(),
        error: None,
        aborted: true,
    }).unwrap();

    let view = instance.render().unwrap();
    let mut msgs = Vec::new();
    collect_chat_messages(&view.root, &mut msgs);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].1, "春天来了");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
