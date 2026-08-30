//! 独立设置窗口：从托盘菜单「设置」打开。
//! 集中管理百度翻译与 AI 服务（多服务商/多模型）的配置，
//! 写入各插件的存储文件，插件下次打开时生效。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;
use xtools_ui::boot::{capture_target_desktop, init_input_method_env, take_activation_token};
use xtools_ui::instance::{claim_instance, raise_instance};
use xtools_ui::slint_chrome::{WindowDragState, setup_raise_timer};

use crate::ai_config::{
    load_ai_config, new_provider_id, save_ai_config, save_baidu_config, AiConfigFile,
    AiProviderEntry,
};
use crate::window_prefs;

slint::include_modules!();

const INSTANCE_NAME: &str = "xtools-settings";

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

/// 自动保存成功后短暂显示「✓ 已自动保存」
fn flash_autosave(ui_weak: slint::Weak<RunnerWindow>) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.set_settings_autosave_visible(true);

        let ui_reset = ui_weak.clone();
        slint::Timer::single_shot(Duration::from_millis(1600), move || {
            if let Some(u) = ui_reset.upgrade() {
                u.set_settings_autosave_visible(false);
            }
        });
    }
}

/// 把配置状态同步到服务商列表 UI
fn rebuild_ai_providers(ui: &RunnerWindow, config: &AiConfigFile) {
    let items: Vec<AiProviderItem> = config
        .providers
        .iter()
        .map(|p| AiProviderItem {
            id: p.id.clone().into(),
            name: p.name.clone().into(),
            base_url: p.base_url.clone().into(),
            api_key: p.api_key.clone().into(),
            models: slint::ModelRc::new(slint::VecModel::from(
                p.models
                    .iter()
                    .map(|m| m.clone().into())
                    .collect::<Vec<slint::SharedString>>(),
            )),
        })
        .collect();
    ui.set_ai_providers(slint::ModelRc::new(slint::VecModel::from(items)));
}

/// 关闭窗口时冲刷全部未落盘修改
fn save_all_configs(ui: &RunnerWindow, ai_config: &AiConfigFile) {
    if let Err(e) = save_baidu_config(&ui.get_settings_baidu_appid(), &ui.get_settings_baidu_key())
    {
        log::warn!("关闭时保存百度翻译配置失败: {e}");
    }
    if let Err(e) = save_ai_config(ai_config) {
        log::warn!("关闭时保存 AI 配置失败: {e}");
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

    let baidu = crate::ai_config::load_baidu_config();
    let ai_state: Rc<RefCell<AiConfigFile>> =
        Rc::new(RefCell::new(load_ai_config()));
    let model_drafts: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));

    let ui = RunnerWindow::new()?;
    ui.set_plugin_kind("settings".into());
    ui.set_window_title("xtools 设置".into());
    ui.set_window_icon("⚙".into());
    ui.set_resizable(false);
    ui.set_show_expand_button(false);

    ui.set_settings_baidu_appid(baidu.baidu_appid.into());
    ui.set_settings_baidu_key(baidu.baidu_key.into());
    let opacity = window_prefs::load().normalized_opacity();
    ui.set_window_opacity(opacity);
    ui.set_settings_opacity_percent((opacity * 100.0).round());
    rebuild_ai_providers(&ui, &ai_state.borrow());

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

    // 自动保存：任何修改后 600ms 防抖写入（关闭窗口前再强制冲刷一次）
    let save_timer = Rc::new(RefCell::new(slint::Timer::default()));
    let schedule_save = {
        let ui_w = ui.as_weak();
        let ai_state = ai_state.clone();
        let save_timer = save_timer.clone();
        move || {
            let ui_ref = ui_w.clone();
            let ai_state = ai_state.clone();
            save_timer.borrow().start(
                slint::TimerMode::SingleShot,
                Duration::from_millis(600),
                move || {
                    if let Some(u) = ui_ref.upgrade() {
                        let ai_snapshot = ai_state.borrow().clone();
                        let result = save_baidu_config(
                            &u.get_settings_baidu_appid(),
                            &u.get_settings_baidu_key(),
                        )
                        .and_then(|()| save_ai_config(&ai_snapshot));
                        match result {
                            Ok(()) => flash_autosave(ui_ref.clone()),
                            Err(e) => show_toast(ui_ref.clone(), &e, false),
                        }
                    }
                },
            );
        }
    };

    // Window close handler（退出前冲刷尚未落盘的自动保存）
    {
        let ui_w_close = ui.as_weak();
        let ai_state = ai_state.clone();
        ui.on_close_clicked(move || {
            if let Some(u) = ui_w_close.upgrade() {
                save_all_configs(&u, &ai_state.borrow());
                let _ = u.hide();
            }
            std::process::exit(0);
        });
    }

    // 百度翻译输入（双向绑定属性，防抖统一保存）
    {
        let schedule_save = schedule_save.clone();
        ui.on_input_changed(move |_id, _value| {
            schedule_save();
        });
    }

    // AI 服务商：字段编辑（仅更新状态，不重建列表，避免打断输入）
    {
        let ai_state = ai_state.clone();
        let schedule_save = schedule_save.clone();
        ui.on_ai_provider_field_changed(move |id, field, value| {
            let value = value.trim().to_string();
            let mut cfg = ai_state.borrow_mut();
            if let Some(p) = cfg.providers.iter_mut().find(|p| p.id == id.as_str()) {
                match field.as_str() {
                    "name" => p.name = value,
                    "base_url" => p.base_url = value,
                    "api_key" => p.api_key = value,
                    _ => {}
                }
                drop(cfg);
                schedule_save();
            }
        });
    }

    // AI 服务商：添加 / 删除
    {
        let ui_w = ui.as_weak();
        let ai_state = ai_state.clone();
        ui.on_ai_provider_added(move || {
            let Some(u) = ui_w.upgrade() else {
                return;
            };
            {
                let mut cfg = ai_state.borrow_mut();
                let index = cfg.providers.len() + 1;
                cfg.providers.push(AiProviderEntry {
                    id: new_provider_id(),
                    name: format!("服务商 {index}"),
                    ..Default::default()
                });
            }
            let snapshot = ai_state.borrow().clone();
            rebuild_ai_providers(&u, &snapshot);
            if let Err(e) = save_ai_config(&snapshot) {
                show_toast(u.as_weak(), &e, false);
            }
        });
    }
    {
        let ui_w = ui.as_weak();
        let ai_state = ai_state.clone();
        ui.on_ai_provider_removed(move |id| {
            let Some(u) = ui_w.upgrade() else {
                return;
            };
            {
                let mut cfg = ai_state.borrow_mut();
                cfg.providers.retain(|p| p.id != id.as_str());
                cfg.normalize();
            }
            let snapshot = ai_state.borrow().clone();
            rebuild_ai_providers(&u, &snapshot);
            if let Err(e) = save_ai_config(&snapshot) {
                show_toast(u.as_weak(), &e, false);
            }
        });
    }

    // AI 模型：草稿、添加、删除
    {
        let drafts = model_drafts.clone();
        ui.on_ai_new_model_changed(move |id, value| {
            drafts
                .borrow_mut()
                .insert(id.to_string(), value.to_string());
        });
    }
    {
        let ui_w = ui.as_weak();
        let ai_state = ai_state.clone();
        let drafts = model_drafts.clone();
        ui.on_ai_model_added(move |id| {
            let Some(u) = ui_w.upgrade() else {
                return;
            };
            let draft = drafts.borrow_mut().remove(id.as_str()).unwrap_or_default();
            let model = draft.trim().to_string();
            if model.is_empty() {
                return;
            }
            {
                let mut cfg = ai_state.borrow_mut();
                if let Some(p) = cfg.providers.iter_mut().find(|p| p.id == id.as_str()) {
                    if !p.models.iter().any(|m| m == &model) {
                        p.models.push(model);
                    }
                }
                cfg.normalize();
            }
            let snapshot = ai_state.borrow().clone();
            rebuild_ai_providers(&u, &snapshot);
            if let Err(e) = save_ai_config(&snapshot) {
                show_toast(u.as_weak(), &e, false);
            }
        });
    }
    {
        let ui_w = ui.as_weak();
        let ai_state = ai_state.clone();
        ui.on_ai_model_removed(move |id, model| {
            let Some(u) = ui_w.upgrade() else {
                return;
            };
            {
                let mut cfg = ai_state.borrow_mut();
                if let Some(p) = cfg.providers.iter_mut().find(|p| p.id == id.as_str()) {
                    p.models.retain(|m| m != model.as_str());
                }
                cfg.normalize();
            }
            let snapshot = ai_state.borrow().clone();
            rebuild_ai_providers(&u, &snapshot);
            if let Err(e) = save_ai_config(&snapshot) {
                show_toast(u.as_weak(), &e, false);
            }
        });
    }

    // 窗口透明度：Slider 拖动即时生效并持久化
    {
        let ui_w = ui.as_weak();
        ui.on_opacity_changed(move |percent| {
            let opacity = (percent as f32 / 100.0).clamp(
                window_prefs::MIN_OPACITY,
                window_prefs::DEFAULT_OPACITY,
            );
            if let Some(u) = ui_w.upgrade() {
                u.set_window_opacity(opacity);
            }
            if let Err(e) = window_prefs::save_opacity(opacity) {
                log::warn!("保存窗口透明度失败: {e}");
            }
        });
    }

    let _raise_timer = setup_raise_timer(lock, ui.as_weak());

    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_config::TransConfigFile;

    #[test]
    fn test_config_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("xtools-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 百度：保存时保留已有 engine_index
        let baidu_path = dir.join("trans.json");
        crate::ai_config::save_json_test_helper(
            &baidu_path,
            &TransConfigFile {
                engine_index: 1,
                baidu_appid: String::new(),
                baidu_key: String::new(),
            },
        )
        .unwrap();
        let mut baidu: TransConfigFile = crate::ai_config::load_json_test_helper(&baidu_path);
        baidu.baidu_appid = "appid-1".into();
        baidu.baidu_key = "key-1".into();
        crate::ai_config::save_json_test_helper(&baidu_path, &baidu).unwrap();
        let reloaded: TransConfigFile = crate::ai_config::load_json_test_helper(&baidu_path);
        assert_eq!(reloaded.engine_index, 1);
        assert_eq!(reloaded.baidu_appid, "appid-1");
        assert_eq!(reloaded.baidu_key, "key-1");

        // AI：多服务商写读回环，序列化不包含旧字段
        let ai_path = dir.join("ai.json");
        let mut config = AiConfigFile::default();
        config.providers.push(AiProviderEntry {
            id: "p1".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: "sk-1".into(),
            models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
        });
        config.selected_provider_id = "p1".into();
        config.selected_model = "deepseek-chat".into();
        crate::ai_config::save_json_test_helper(&ai_path, &config).unwrap();
        let loaded: AiConfigFile = crate::ai_config::load_json_test_helper(&ai_path);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].models.len(), 2);
        assert_eq!(loaded.selected_model, "deepseek-chat");
        let text = std::fs::read_to_string(&ai_path).unwrap();
        // 旧版顶层字段 skip_serializing，不再写文件（providers 内的 base_url 合法存在）
        assert!(!text.contains("\"model\":"), "{text}");
        assert_eq!(loaded.base_url, "");
        assert_eq!(loaded.api_key, "");
        assert_eq!(loaded.model, "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_normalize_fixes_dangling_selection() {
        let mut config = AiConfigFile::default();
        config.providers.push(AiProviderEntry {
            id: "p1".into(),
            name: "A".into(),
            base_url: "https://a.example.com".into(),
            api_key: "sk-a".into(),
            models: vec!["m1".into()],
        });
        config.providers.push(AiProviderEntry {
            id: "p2".into(),
            name: "B".into(),
            base_url: "https://b.example.com".into(),
            api_key: "sk-b".into(),
            models: vec!["m3".into()],
        });
        config.selected_provider_id = "p2".into();
        config.selected_model = "m3".into();
        assert!(!config.normalize());

        // 删除选中服务商后回落到第一个
        config.providers.retain(|p| p.id != "p2");
        assert!(config.normalize());
        assert_eq!(config.selected_provider_id, "p1");
        assert_eq!(config.selected_model, "m1");
    }
}
