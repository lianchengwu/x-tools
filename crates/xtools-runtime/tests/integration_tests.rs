use std::path::PathBuf;
use xtools_protocol::*;
use xtools_runtime::PluginLoader;

fn get_dist_plugins_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dist/plugins")
}

#[test]
fn test_plugin_scanner_discovers_all_plugins() {
    let loader = PluginLoader::new();
    let plugins_dir = get_dist_plugins_dir();
    let discovered = loader.scan_dir(&plugins_dir);

    assert_eq!(discovered.len(), 3, "Expected 3 plugins, found {:?}", discovered);

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
}

#[test]
fn test_time_plugin_lifecycle_and_events() {
    let loader = PluginLoader::new();
    let path = get_dist_plugins_dir().join("time.wasm");
    let mut instance = loader.load_instance(&path).expect("Failed to load time.wasm");

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
    let loader = PluginLoader::new();
    let path = get_dist_plugins_dir().join("json.wasm");
    let mut instance = loader.load_instance(&path).expect("Failed to load json.wasm");

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
    let temp_dir = std::env::temp_dir().join(format!("xtools-test-{}", std::process::id()));
    let loader = PluginLoader::new().with_storage_root(temp_dir.clone());
    let path = get_dist_plugins_dir().join("trans.wasm");
    let mut instance = loader.load_instance(&path).expect("Failed to load trans.wasm");

    // 1. Initial view render
    let view = instance.render().expect("Failed to render trans view");
    assert!(matches!(view.root, UiNode::Container { .. }));

    // 2. Set Baidu config
    let _ = instance.handle_event(&UiEvent::InputChanged {
        id: "cfg_baidu_appid".to_string(),
        value: "test_appid_123".to_string(),
    }).expect("Failed to input appid");

    let _ = instance.handle_event(&UiEvent::InputChanged {
        id: "cfg_baidu_key".to_string(),
        value: "test_key_456".to_string(),
    }).expect("Failed to input key");

    // 3. Save config
    let save_resp = instance.handle_event(&UiEvent::Click {
        id: "btn_save_config".to_string(),
    }).expect("Failed to save config");
    assert!(matches!(save_resp, UiResponse::ShowToast(..)));

    // Verify storage file created by host capability
    let config_file = temp_dir.join("xtools.trans").join("config.json");
    assert!(config_file.exists(), "Expected config file to be written by host storage API");
    let content = std::fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("test_appid_123"));
    assert!(content.contains("test_key_456"));

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
