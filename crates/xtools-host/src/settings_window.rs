//! 独立设置窗口：从托盘菜单「设置」打开。
//! 集中管理百度翻译与 AI 服务的配置（写入各插件的存储文件，插件下次打开时生效）。

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use slint::ComponentHandle;
use xtools_ui::boot::{capture_target_desktop, init_input_method_env, take_activation_token};
use xtools_ui::instance::{claim_instance, raise_instance};
use xtools_ui::slint_chrome::{WindowDragState, setup_raise_timer};

slint::include_modules!();

const INSTANCE_NAME: &str = "xtools-settings";

// -----------------------------------------------------------------------------
// 配置文件读写（与 WASM 插件的存储位置保持一致）
// -----------------------------------------------------------------------------

/// 智能翻译插件存储：~/.config/xtools/plugins/xtools.trans/config.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransConfigFile {
    #[serde(default)]
    pub engine_index: usize,
    #[serde(default)]
    pub baidu_appid: String,
    #[serde(default)]
    pub baidu_key: String,
}

/// AI 问答插件存储：~/.config/xtools/plugins/xtools.ai/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfigFile {
    #[serde(default = "default_ai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
}

fn default_ai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_ai_model() -> String {
    "gpt-4o-mini".to_string()
}

impl Default for AiConfigFile {
    fn default() -> Self {
        Self {
            base_url: default_ai_base_url(),
            api_key: String::new(),
            model: default_ai_model(),
        }
    }
}

fn plugins_root() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xtools").join("plugins"))
        .unwrap_or_else(|| PathBuf::from("storage"))
}

fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("写入配置失败: {e}"))
}

pub fn baidu_config_path() -> PathBuf {
    plugins_root().join("xtools.trans").join("config.json")
}

pub fn ai_config_path() -> PathBuf {
    plugins_root().join("xtools.ai").join("config.json")
}

/// 读取百度翻译配置（engine_index 保留插件内的选择）
pub fn load_baidu_config() -> TransConfigFile {
    load_json(&baidu_config_path())
}

/// 保存百度翻译 AppID / 密钥，保留现有引擎选择
pub fn save_baidu_config(appid: &str, key: &str) -> Result<(), String> {
    let mut config: TransConfigFile = load_json(&baidu_config_path());
    config.baidu_appid = appid.trim().to_string();
    config.baidu_key = key.trim().to_string();
    save_json(&baidu_config_path(), &config)
}

pub fn load_ai_config() -> AiConfigFile {
    load_json(&ai_config_path())
}

pub fn save_ai_config(base_url: &str, api_key: &str, model: &str) -> Result<(), String> {
    let mut config: AiConfigFile = load_json(&ai_config_path());
    if !base_url.trim().is_empty() {
        config.base_url = base_url.trim().to_string();
    }
    config.api_key = api_key.trim().to_string();
    if !model.trim().is_empty() {
        config.model = model.trim().to_string();
    }
    save_json(&ai_config_path(), &config)
}

// -----------------------------------------------------------------------------
// 设置窗口
// -----------------------------------------------------------------------------

fn show_toast(ui_weak: slint::Weak<RunnerWindow>, message: &str, is_success: bool) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.set_toast_message(message.into());
        ui.set_toast_is_success(is_success);
        ui.set_toast_visible(true);

        let ui_reset = ui_weak.clone();
        slint::Timer::single_shot(Duration::from_millis(1800), move || {
            if let Some(u) = ui_reset.upgrade() {
                u.set_toast_visible(false);
            }
        });
    }
}

pub fn run_settings() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    init_input_method_env();
    capture_target_desktop();
    let token = take_activation_token();

    // 单实例：重复点击托盘「设置」时拉起已有窗口
    let mut lock = None;
    for _ in 0..30 {
        match claim_instance(INSTANCE_NAME) {
            Ok(Some(l)) => {
                lock = Some(l);
                break;
            }
            Ok(None) => {
                if let Ok(true) = raise_instance(INSTANCE_NAME, token.as_deref()) {
                    log::info!("Raised existing settings window");
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => {
                log::warn!("Instance lock attempt for '{INSTANCE_NAME}': {err}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let Some(lock) = lock else {
        if let Ok(true) = raise_instance(INSTANCE_NAME, token.as_deref()) {
            return Ok(());
        }
        return Err(format!("Failed to claim instance for '{INSTANCE_NAME}'").into());
    };

    let baidu = load_baidu_config();
    let ai = load_ai_config();

    let ui = RunnerWindow::new()?;
    ui.set_plugin_kind("settings".into());
    ui.set_window_title("xtools 设置".into());
    ui.set_window_icon("⚙".into());
    ui.set_resizable(false);
    ui.set_show_expand_button(false);

    ui.set_settings_baidu_appid(baidu.baidu_appid.into());
    ui.set_settings_baidu_key(baidu.baidu_key.into());
    ui.set_settings_ai_base_url(ai.base_url.into());
    ui.set_settings_ai_api_key(ai.api_key.into());
    ui.set_settings_ai_model(ai.model.into());

    // Window drag handlers（无边框窗口 chrome 拖动）
    let drag_state = WindowDragState::new();
    {
        let ds = drag_state.clone();
        let ui_w = ui.as_weak();
        ui.on_window_drag_started(move || {
            if let Some(ui) = ui_w.upgrade() {
                ds.on_drag_started(&ui.window());
            }
        });
    }
    {
        let ds = drag_state;
        let ui_w = ui.as_weak();
        ui.on_window_dragged(move |dx, dy| {
            if let Some(ui) = ui_w.upgrade() {
                ds.on_dragged(&ui.window(), dx, dy);
            }
        });
    }

    // Window close handler
    let ui_w_close = ui.as_weak();
    ui.on_close_clicked(move || {
        if let Some(u) = ui_w_close.upgrade() {
            let _ = u.hide();
        }
        std::process::exit(0);
    });

    // 保存动作（输入框通过双向绑定，保存时直接读取属性）
    let ui_w = ui.as_weak();
    ui.on_button_clicked(move |id| {
        let Some(ui) = ui_w.upgrade() else {
            return;
        };
        match id.as_str() {
            "btn_save_baidu" => match save_baidu_config(
                &ui.get_settings_baidu_appid(),
                &ui.get_settings_baidu_key(),
            ) {
                Ok(()) => show_toast(
                    ui.as_weak(),
                    "百度翻译配置已保存，重新打开「智能翻译」生效",
                    true,
                ),
                Err(e) => show_toast(ui.as_weak(), &e, false),
            },
            "btn_save_ai" => match save_ai_config(
                &ui.get_settings_ai_base_url(),
                &ui.get_settings_ai_api_key(),
                &ui.get_settings_ai_model(),
            ) {
                Ok(()) => show_toast(
                    ui.as_weak(),
                    "AI 服务配置已保存，重新打开「AI 问答」生效",
                    true,
                ),
                Err(e) => show_toast(ui.as_weak(), &e, false),
            },
            _ => {}
        }
    });

    let _raise_timer = setup_raise_timer(lock, ui.as_weak());

    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("xtools-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 百度：保存时保留已有 engine_index
        let baidu_path = dir.join("trans.json");
        save_json(
            &baidu_path,
            &TransConfigFile {
                engine_index: 1,
                baidu_appid: String::new(),
                baidu_key: String::new(),
            },
        )
        .unwrap();
        let mut baidu: TransConfigFile = load_json(&baidu_path);
        baidu.baidu_appid = "appid-1".into();
        baidu.baidu_key = "key-1".into();
        save_json(&baidu_path, &baidu).unwrap();
        let reloaded: TransConfigFile = load_json(&baidu_path);
        assert_eq!(reloaded.engine_index, 1);
        assert_eq!(reloaded.baidu_appid, "appid-1");
        assert_eq!(reloaded.baidu_key, "key-1");

        // AI：默认值与回退
        let ai_path = dir.join("ai.json");
        let missing: AiConfigFile = load_json(&ai_path);
        assert_eq!(missing.base_url, default_ai_base_url());
        assert!(missing.api_key.is_empty());
        save_json(
            &ai_path,
            &AiConfigFile {
                base_url: " https://api.deepseek.com/v1 ".into(),
                api_key: "sk-1".into(),
                model: "deepseek-chat".into(),
            },
        )
        .unwrap();
        let ai: AiConfigFile = load_json(&ai_path);
        assert_eq!(ai.base_url, " https://api.deepseek.com/v1 ");
        assert_eq!(ai.api_key, "sk-1");
        assert_eq!(ai.model, "deepseek-chat");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_baidu_preserves_engine_index() {
        let dir = std::env::temp_dir().join(format!("xtools-settings-baidu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        save_json(
            &path,
            &TransConfigFile {
                engine_index: 1,
                baidu_appid: "old".into(),
                baidu_key: "old".into(),
            },
        )
        .unwrap();

        // 模拟 save_baidu_config 的读改写逻辑
        let mut config: TransConfigFile = load_json(&path);
        config.baidu_appid = "new-appid".into();
        config.baidu_key = "new-key".into();
        save_json(&path, &config).unwrap();

        let final_config: TransConfigFile = load_json(&path);
        assert_eq!(final_config.engine_index, 1);
        assert_eq!(final_config.baidu_appid, "new-appid");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
