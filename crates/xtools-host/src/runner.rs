use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;
use xtools_protocol::*;
use xtools_runtime::{PluginInstance, PluginLoader};
use xtools_ui::boot::{
    capture_target_desktop, init_input_method_env, prefer_x11_for_skip_taskbar,
    take_activation_token,
};
use xtools_ui::instance::{claim_instance, raise_instance};
use xtools_ui::slint_chrome::{
    ResizeEdge, WindowDragState, WindowResizeState, copy_to_clipboard, setup_raise_timer,
};
#[cfg(unix)]
use xtools_ui::slint_chrome::{setup_auto_exit_on_focus_loss_timer, setup_skip_taskbar_timer};

slint::include_modules!();

pub fn find_plugin_wasm(arg: &str) -> Option<PathBuf> {
    let direct_path = PathBuf::from(arg);
    if direct_path.exists() && direct_path.is_file() {
        return Some(direct_path);
    }

    let mut search_dirs = Vec::new();

    // 1. Plugins relative to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_dirs.push(parent.join("plugins"));
            search_dirs.push(parent.to_path_buf());
        }
    }

    // 2. Current working directory & dev paths
    search_dirs.push(PathBuf::from("plugins"));
    search_dirs.push(PathBuf::from("wasm/dist/plugins"));
    search_dirs.push(PathBuf::from("dist/plugins"));
    search_dirs.push(PathBuf::from("wasm/target/wasm32-unknown-unknown/release"));
    search_dirs.push(PathBuf::from("target/wasm32-unknown-unknown/release"));

    // 3. User data directory
    if let Some(data) = dirs::data_dir() {
        search_dirs.push(data.join("xtools").join("plugins"));
    }

    // 4. System directories
    search_dirs.push(PathBuf::from("/usr/share/xtools/plugins"));
    search_dirs.push(PathBuf::from("/usr/local/share/xtools/plugins"));

    let clean_name = arg
        .trim_start_matches("xtools.")
        .trim_start_matches("xtools-plugin-")
        .trim_start_matches("xtools-")
        .trim_end_matches(".wasm");

    for dir in &search_dirs {
        let p1 = dir.join(format!("{arg}.wasm"));
        if p1.is_file() {
            return Some(p1);
        }
        let p2 = dir.join(format!("{clean_name}.wasm"));
        if p2.is_file() {
            return Some(p2);
        }
        let p3 = dir.join(format!("xtools_plugin_{clean_name}.wasm"));
        if p3.is_file() {
            return Some(p3);
        }
    }

    None
}

pub fn list_plugins() {
    let loader = PluginLoader::new();
    let mut search_dirs = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_dirs.push(parent.join("plugins"));
        }
    }
    search_dirs.push(PathBuf::from("plugins"));
    search_dirs.push(PathBuf::from("wasm/dist/plugins"));
    search_dirs.push(PathBuf::from("dist/plugins"));
    if let Some(data) = dirs::data_dir() {
        search_dirs.push(data.join("xtools").join("plugins"));
    }
    search_dirs.push(PathBuf::from("/usr/share/xtools/plugins"));
    search_dirs.push(PathBuf::from("/usr/local/share/xtools/plugins"));

    let mut found_count = 0;
    println!("Discovered xtools WASM plugins:");
    for dir in &search_dirs {
        if dir.exists() {
            let plugins = loader.scan_dir(dir);
            for p in plugins {
                found_count += 1;
                println!(
                    "  • {} ({}) v{} [{}] - {}",
                    p.manifest.name,
                    p.manifest.id,
                    p.manifest.version,
                    p.manifest.mark,
                    p.path.display()
                );
            }
        }
    }
    if found_count == 0 {
        println!("  (No plugins found in search paths)");
    }
}

fn show_toast(ui_weak: slint::Weak<RunnerWindow>, message: &str, is_success: bool) {
    if let Some(ui) = ui_weak.upgrade() {
        ui.set_toast_message(message.into());
        ui.set_toast_is_success(is_success);
        ui.set_toast_visible(true);

        let ui_reset = ui_weak.clone();
        slint::Timer::single_shot(Duration::from_millis(1500), move || {
            if let Some(u) = ui_reset.upgrade() {
                u.set_toast_visible(false);
            }
        });
    }
}

pub fn run_plugin(plugin_arg: &str) -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    init_input_method_env();
    capture_target_desktop();
    prefer_x11_for_skip_taskbar();
    let token = take_activation_token();

    let wasm_path = find_plugin_wasm(plugin_arg).ok_or_else(|| {
        format!("Cannot find WASM plugin for '{plugin_arg}'. Please build plugins first.")
    })?;

    log::info!("Loading WASM plugin from {:?}", wasm_path);
    let bytes = std::fs::read(&wasm_path)?;
    let engine = wasmtime::Engine::default();

    let storage_root = dirs::config_dir()
        .map(|p| p.join("xtools").join("plugins"))
        .unwrap_or_else(|| PathBuf::from("storage"));

    let mut instance = PluginInstance::load(&engine, &bytes, Some(storage_root))?;
    let manifest = instance.manifest().clone();
    log::info!(
        "Loaded plugin: {} ({}) v{}",
        manifest.name,
        manifest.id,
        manifest.version
    );

    // Determine plugin kind for UI rendering
    let plugin_kind = match manifest.id.as_str() {
        "xtools.time" => "time",
        "xtools.json" => "json",
        "xtools.trans" => "trans",
        _ => "generic",
    };

    // Single-instance handling with robust claim retries
    let instance_name = manifest.id.replace('.', "-");
    let mut lock = None;
    for _ in 0..30 {
        match claim_instance(&instance_name) {
            Ok(Some(l)) => {
                lock = Some(l);
                break;
            }
            Ok(None) => {
                if let Ok(true) = raise_instance(&instance_name, token.as_deref()) {
                    log::info!("Raised existing instance '{}'", instance_name);
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => {
                log::warn!("Instance lock attempt for '{}': {err}", instance_name);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    let Some(lock) = lock else {
        if let Ok(true) = raise_instance(&instance_name, token.as_deref()) {
            return Ok(());
        }
        return Err(format!("Failed to claim instance for '{instance_name}' or communicate with active instance").into());
    };

    instance.init()?;
    let initial_view = instance.render()?;

    let (normal_w, normal_h, expanded_w, expanded_h) = match plugin_kind {
        "json" => (580, 600, 960, 720),
        "trans" => (540, 580, 840, 700),
        "time" => (480, 400, 720, 500),
        _ => (
            manifest.window.width,
            manifest.window.height,
            (manifest.window.width as f32 * 1.5).round() as u32,
            (manifest.window.height as f32 * 1.35).round() as u32,
        ),
    };

    let ui = RunnerWindow::new()?;
    ui.set_plugin_kind(plugin_kind.into());
    ui.set_window_title(manifest.name.clone().into());
    ui.set_window_icon(manifest.mark.clone().into());
    ui.set_resizable(manifest.window.resizable);
    ui.set_show_expand_button(manifest.window.resizable);

    sync_ui_view(&ui, &initial_view, plugin_kind);

    let drag_state = WindowDragState::new();
    let resize_state = WindowResizeState::new();

    // Window drag handlers
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

    // Window resize & expand handlers
    {
        let rs = resize_state.clone();
        let ui_w = ui.as_weak();
        ui.on_window_resize_started(move |edge_code| {
            if let Some(ui) = ui_w.upgrade() {
                let edge = match edge_code {
                    1 => Some(ResizeEdge::East),
                    2 => Some(ResizeEdge::South),
                    3 => Some(ResizeEdge::SouthEast),
                    _ => None,
                };
                rs.on_resize_started(ui.window(), edge);
            }
        });
    }
    {
        let rs = resize_state.clone();
        let ui_w = ui.as_weak();
        ui.on_window_resized(move |dx, dy| {
            if let Some(ui) = ui_w.upgrade() {
                rs.on_resized(&ui.window(), dx, dy, 420, 320);
            }
        });
    }
    {
        let rs = resize_state;
        let ui_w = ui.as_weak();
        ui.on_expand_clicked(move || {
            if let Some(ui) = ui_w.upgrade() {
                let is_exp = rs.toggle_expand(&ui.window(), normal_w, normal_h, expanded_w, expanded_h);
                ui.set_is_expanded(is_exp);
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

    let plugin_cell = Rc::new(RefCell::new(instance));

    // Helper closure for event handling
    let handle_event = {
        let plugin_cell = plugin_cell.clone();
        let ui_weak = ui.as_weak();
        let p_kind = plugin_kind.to_string();

        move |event: UiEvent| {
            let mut p = plugin_cell.borrow_mut();
            match p.handle_event(&event) {
                Ok(UiResponse::UpdateView(view)) => {
                    if let Some(u) = ui_weak.upgrade() {
                        sync_ui_view(&u, &view, &p_kind);
                    }
                }
                Ok(UiResponse::ShowToast(toast)) => {
                    let is_success = toast.level != ToastLevel::Error;
                    show_toast(ui_weak.clone(), &toast.message, is_success);
                    if let Ok(view) = p.render() {
                        if let Some(u) = ui_weak.upgrade() {
                            sync_ui_view(&u, &view, &p_kind);
                        }
                    }
                }
                Ok(UiResponse::CopyToClipboard(text)) => {
                    let _ = copy_to_clipboard(&text);
                    show_toast(ui_weak.clone(), "已复制到剪贴板", true);
                }
                Ok(UiResponse::CloseWindow) => {
                    if let Some(u) = ui_weak.upgrade() {
                        let _ = u.hide();
                    }
                    std::process::exit(0);
                }
                Ok(UiResponse::NoChange) => {}
                Err(e) => {
                    log::error!("Error handling event {:?}: {e}", event);
                }
            }
        }
    };

    // Wire Time Plugin Callbacks
    if plugin_kind == "time" {
        {
            let h = handle_event.clone();
            ui.on_time_seconds_edited(move |val| {
                h(UiEvent::InputChanged {
                    id: "input_seconds".to_string(),
                    value: val.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_time_millis_edited(move |val| {
                h(UiEvent::InputChanged {
                    id: "input_millis".to_string(),
                    value: val.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_time_local_edited(move |val| {
                h(UiEvent::InputChanged {
                    id: "input_local".to_string(),
                    value: val.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_time_now_clicked(move || {
                h(UiEvent::Click {
                    id: "btn_now".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_time_copy_sec(move || {
                h(UiEvent::Click {
                    id: "copy_seconds".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_time_sec_copied(true);
                    let ui_reset = ui_w.clone();
                    slint::Timer::single_shot(Duration::from_millis(1500), move || {
                        if let Some(u) = ui_reset.upgrade() {
                            u.set_time_sec_copied(false);
                        }
                    });
                }
            });
        }
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_time_copy_ms(move || {
                h(UiEvent::Click {
                    id: "copy_millis".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_time_ms_copied(true);
                    let ui_reset = ui_w.clone();
                    slint::Timer::single_shot(Duration::from_millis(1500), move || {
                        if let Some(u) = ui_reset.upgrade() {
                            u.set_time_ms_copied(false);
                        }
                    });
                }
            });
        }
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_time_copy_local(move || {
                h(UiEvent::Click {
                    id: "copy_local".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_time_local_copied(true);
                    let ui_reset = ui_w.clone();
                    slint::Timer::single_shot(Duration::from_millis(1500), move || {
                        if let Some(u) = ui_reset.upgrade() {
                            u.set_time_local_copied(false);
                        }
                    });
                }
            });
        }
        {
            let h = handle_event.clone();
            ui.on_time_tz_changed(move |idx| {
                h(UiEvent::SelectChanged {
                    id: "select_tz".to_string(),
                    index: idx as usize,
                    value: idx.to_string(),
                });
            });
        }
    }

    // Wire JSON Plugin Callbacks
    if plugin_kind == "json" {
        {
            let h = handle_event.clone();
            ui.on_json_format(move || {
                h(UiEvent::Click {
                    id: "btn_format".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_minify(move || {
                h(UiEvent::Click {
                    id: "btn_minify".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_unescape(move || {
                h(UiEvent::Click {
                    id: "btn_unescape".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_validate(move || {
                h(UiEvent::Click {
                    id: "btn_validate".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_clear(move || {
                h(UiEvent::Click {
                    id: "btn_clear".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_json_copy(move || {
                h(UiEvent::Click {
                    id: "btn_copy".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_json_copied(true);
                    let ui_reset = ui_w.clone();
                    slint::Timer::single_shot(Duration::from_millis(1500), move || {
                        if let Some(u) = ui_reset.upgrade() {
                            u.set_json_copied(false);
                        }
                    });
                }
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_mode_switch(move |mode| {
                h(UiEvent::TabChanged {
                    id: "json_tabs".to_string(),
                    index: mode as usize,
                    tab_id: format!("tab_{mode}"),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_expand_all(move || {
                h(UiEvent::Click {
                    id: "btn_expand_all".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_collapse_all(move || {
                h(UiEvent::Click {
                    id: "btn_collapse_all".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_fold_level(move |_lvl| {
                h(UiEvent::Click {
                    id: "btn_fold_level_2".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_text_edited(move |t| {
                h(UiEvent::InputChanged {
                    id: "json_code".to_string(),
                    value: t.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_json_tree_toggle(move |node_id| {
                h(UiEvent::JsonTreeToggle {
                    id: "json_tree".to_string(),
                    node_id: node_id as usize,
                });
            });
        }
    }

    // Wire Trans Plugin Callbacks
    if plugin_kind == "trans" {
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_trans_translate(move || {
                if let Some(u) = ui_w.upgrade() {
                    u.set_trans_pending(true);
                }
                h(UiEvent::Click {
                    id: "btn_translate".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_trans_pending(false);
                }
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_swap(move || {
                h(UiEvent::Click {
                    id: "btn_swap_lang".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_clear(move || {
                h(UiEvent::Click {
                    id: "btn_clear".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            let ui_w = ui.as_weak();
            ui.on_trans_copy(move || {
                h(UiEvent::Click {
                    id: "btn_copy".to_string(),
                });
                if let Some(u) = ui_w.upgrade() {
                    u.set_trans_copied(true);
                    let ui_reset = ui_w.clone();
                    slint::Timer::single_shot(Duration::from_millis(1500), move || {
                        if let Some(u) = ui_reset.upgrade() {
                            u.set_trans_copied(false);
                        }
                    });
                }
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_toggle_settings(move || {
                h(UiEvent::Click {
                    id: "btn_toggle_settings".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_save_settings(move |appid, key| {
                h(UiEvent::InputChanged {
                    id: "cfg_baidu_appid".to_string(),
                    value: appid.to_string(),
                });
                h(UiEvent::InputChanged {
                    id: "cfg_baidu_key".to_string(),
                    value: key.to_string(),
                });
                h(UiEvent::Click {
                    id: "btn_save_config".to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_source_edited(move |t| {
                h(UiEvent::InputChanged {
                    id: "input_source".to_string(),
                    value: t.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_src_changed(move |idx| {
                h(UiEvent::SelectChanged {
                    id: "select_src_lang".to_string(),
                    index: idx as usize,
                    value: idx.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_dst_changed(move |idx| {
                h(UiEvent::SelectChanged {
                    id: "select_dst_lang".to_string(),
                    index: idx as usize,
                    value: idx.to_string(),
                });
            });
        }
        {
            let h = handle_event.clone();
            ui.on_trans_engine_changed(move |idx| {
                h(UiEvent::SelectChanged {
                    id: "select_engine".to_string(),
                    index: idx as usize,
                    value: idx.to_string(),
                });
            });
        }
    }

    // Generic Callbacks
    {
        let h = handle_event.clone();
        ui.on_button_clicked(move |id| {
            h(UiEvent::Click { id: id.to_string() });
        });
    }
    {
        let h = handle_event.clone();
        ui.on_input_changed(move |id, val| {
            h(UiEvent::InputChanged {
                id: id.to_string(),
                value: val.to_string(),
            });
        });
    }
    {
        let h = handle_event.clone();
        ui.on_select_changed(move |id, idx| {
            h(UiEvent::SelectChanged {
                id: id.to_string(),
                index: idx as usize,
                value: idx.to_string(),
            });
        });
    }
    {
        let h = handle_event.clone();
        ui.on_tab_changed(move |id, idx| {
            h(UiEvent::TabChanged {
                id: id.to_string(),
                index: idx as usize,
                tab_id: format!("tab_{idx}"),
            });
        });
    }
    {
        let h = handle_event.clone();
        ui.on_tree_toggle_clicked(move |id, node_id| {
            h(UiEvent::JsonTreeToggle {
                id: id.to_string(),
                node_id: node_id as usize,
            });
        });
    }

    let _raise_timer = setup_raise_timer(lock, ui.as_weak());
    #[cfg(unix)]
    let (_skip_timer, _focus_timer) = (
        setup_skip_taskbar_timer(),
        setup_auto_exit_on_focus_loss_timer(),
    );

    ui.run()?;
    Ok(())
}

fn sync_ui_view(ui: &RunnerWindow, view: &UiView, plugin_kind: &str) {
    if let Some(title) = &view.title {
        ui.set_window_title(title.clone().into());
    }

    match plugin_kind {
        "time" => sync_time_view(ui, &view.root),
        "json" => sync_json_view(ui, &view.root),
        "trans" => sync_trans_view(ui, &view.root),
        _ => sync_generic_view(ui, &view.root),
    }
}

// -----------------------------------------------------------------------------
// Time Plugin View Sync
// -----------------------------------------------------------------------------
fn sync_time_view(ui: &RunnerWindow, root: &UiNode) {
    let mut seconds = None;
    let mut millis = None;
    let mut local = None;
    let mut tz_idx = 0;
    let mut tz_options = Vec::new();
    let mut error_seconds = String::new();
    let mut error_millis = String::new();
    let mut error_local = String::new();

    collect_time_nodes(
        root,
        &mut seconds,
        &mut millis,
        &mut local,
        &mut tz_idx,
        &mut tz_options,
        &mut error_seconds,
        &mut error_millis,
        &mut error_local,
    );

    if let Some(s) = seconds {
        ui.set_time_seconds(s.into());
    }
    if let Some(m) = millis {
        ui.set_time_millis(m.into());
    }
    if let Some(l) = local {
        ui.set_time_local(l.into());
    }
    if !tz_options.is_empty() {
        let model: Vec<slint::SharedString> = tz_options.into_iter().map(Into::into).collect();
        ui.set_time_tz_options(slint::ModelRc::new(slint::VecModel::from(model)));
        ui.set_time_tz_index(tz_idx as i32);
    }
    ui.set_time_error_seconds(error_seconds.into());
    ui.set_time_error_millis(error_millis.into());
    ui.set_time_error_local(error_local.into());
}

fn collect_time_nodes(
    node: &UiNode,
    seconds: &mut Option<String>,
    millis: &mut Option<String>,
    local: &mut Option<String>,
    tz_idx: &mut usize,
    tz_options: &mut Vec<String>,
    error_seconds: &mut String,
    error_millis: &mut String,
    error_local: &mut String,
) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                collect_time_nodes(
                    child,
                    seconds,
                    millis,
                    local,
                    tz_idx,
                    tz_options,
                    error_seconds,
                    error_millis,
                    error_local,
                );
            }
        }
        UiNode::TextInput { id, value, .. } => match id.as_str() {
            "input_seconds" => *seconds = Some(value.clone()),
            "input_millis" => *millis = Some(value.clone()),
            "input_local" => *local = Some(value.clone()),
            _ => {}
        },
        UiNode::Select {
            id,
            options,
            selected_index,
            ..
        } => {
            if id == "select_tz" {
                *tz_idx = *selected_index;
                *tz_options = options.iter().map(|o| o.label.clone()).collect();
            }
        }
        UiNode::Label { text, variant, .. } => {
            if *variant == LabelVariant::Error {
                if text.contains("秒") {
                    *error_seconds = text.clone();
                } else if text.contains("毫秒") {
                    *error_millis = text.clone();
                } else {
                    *error_local = text.clone();
                }
            }
        }
        _ => {}
    }
}

// -----------------------------------------------------------------------------
// JSON Plugin View Sync
// -----------------------------------------------------------------------------
fn sync_json_view(ui: &RunnerWindow, root: &UiNode) {
    let mut text = None;
    let mut active_tab = 0;
    let mut tree_nodes = Vec::new();
    let mut error_text = String::new();
    let mut note_text = String::new();

    collect_json_nodes(
        root,
        &mut text,
        &mut active_tab,
        &mut tree_nodes,
        &mut error_text,
        &mut note_text,
    );

    if let Some(t) = text {
        ui.set_json_text(t.into());
    }
    ui.set_json_view_mode(active_tab as i32);
    ui.set_json_error(error_text.into());
    ui.set_json_note(note_text.into());

    let slint_nodes: Vec<TreeNodeItem> = tree_nodes
        .into_iter()
        .map(|n| TreeNodeItem {
            id: n.id as i32,
            depth: n.depth as i32,
            key_text: n.key.into(),
            value_text: n.value_preview.into(),
            node_type: n.node_type.into(),
            summary_text: n.summary_text.into(),
            is_leaf: n.is_leaf,
            collapsed: n.collapsed,
            has_comma: n.has_comma,
        })
        .collect();
    ui.set_json_tree_items(slint::ModelRc::new(slint::VecModel::from(slint_nodes)));
}

fn collect_json_nodes(
    node: &UiNode,
    text: &mut Option<String>,
    active_tab: &mut usize,
    tree_nodes: &mut Vec<JsonTreeNode>,
    error_text: &mut String,
    note_text: &mut String,
) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                collect_json_nodes(
                    child,
                    text,
                    active_tab,
                    tree_nodes,
                    error_text,
                    note_text,
                );
            }
        }
        UiNode::TextInput { id, value, .. } => {
            if id == "json_code" {
                *text = Some(value.clone());
            }
        }
        UiNode::Tabs {
            id,
            active_index,
            tabs,
        } => {
            if id == "json_tabs" {
                *active_tab = *active_index;
                for tab in tabs {
                    collect_json_nodes(
                        &tab.content,
                        text,
                        active_tab,
                        tree_nodes,
                        error_text,
                        note_text,
                    );
                }
            }
        }
        UiNode::JsonTreeViewer { id, nodes } => {
            if id == "json_tree" {
                *tree_nodes = nodes.clone();
            }
        }
        UiNode::Label { text: t, variant, .. } => {
            if *variant == LabelVariant::Error {
                *error_text = t.clone();
            } else if *variant == LabelVariant::Secondary || *variant == LabelVariant::Muted {
                *note_text = t.clone();
            }
        }
        _ => {}
    }
}

// -----------------------------------------------------------------------------
// Trans Plugin View Sync
// -----------------------------------------------------------------------------
fn sync_trans_view(ui: &RunnerWindow, root: &UiNode) {
    let mut source = None;
    let mut target = None;
    let mut src_idx = 0;
    let mut dst_idx = 0;
    let mut engine_idx = 0;
    let mut show_settings = false;
    let mut baidu_appid = None;
    let mut baidu_key = None;
    let mut error_text = String::new();
    let mut status_text = String::new();

    collect_trans_nodes(
        root,
        &mut source,
        &mut target,
        &mut src_idx,
        &mut dst_idx,
        &mut engine_idx,
        &mut show_settings,
        &mut baidu_appid,
        &mut baidu_key,
        &mut error_text,
        &mut status_text,
    );

    if let Some(s) = source {
        ui.set_trans_source(s.into());
    }
    if let Some(t) = target {
        ui.set_trans_target(t.into());
    }
    ui.set_trans_src_idx(src_idx as i32);
    ui.set_trans_dst_idx(dst_idx as i32);
    ui.set_trans_engine_idx(engine_idx as i32);
    ui.set_trans_show_settings(show_settings);
    if let Some(id) = baidu_appid {
        ui.set_trans_baidu_appid(id.into());
    }
    if let Some(k) = baidu_key {
        ui.set_trans_baidu_key(k.into());
    }
    ui.set_trans_error(error_text.into());
    if !status_text.is_empty() {
        ui.set_trans_status(status_text.into());
    }
}

fn collect_trans_nodes(
    node: &UiNode,
    source: &mut Option<String>,
    target: &mut Option<String>,
    src_idx: &mut usize,
    dst_idx: &mut usize,
    engine_idx: &mut usize,
    show_settings: &mut bool,
    baidu_appid: &mut Option<String>,
    baidu_key: &mut Option<String>,
    error_text: &mut String,
    status_text: &mut String,
) {
    match node {
        UiNode::Container { children, .. } => {
            for child in children {
                collect_trans_nodes(
                    child,
                    source,
                    target,
                    src_idx,
                    dst_idx,
                    engine_idx,
                    show_settings,
                    baidu_appid,
                    baidu_key,
                    error_text,
                    status_text,
                );
            }
        }
        UiNode::Card { title, children, .. } => {
            if let Some(t) = title {
                if t.contains("百度翻译") {
                    *show_settings = true;
                }
            }
            for child in children {
                collect_trans_nodes(
                    child,
                    source,
                    target,
                    src_idx,
                    dst_idx,
                    engine_idx,
                    show_settings,
                    baidu_appid,
                    baidu_key,
                    error_text,
                    status_text,
                );
            }
        }
        UiNode::TextInput { id, value, .. } => match id.as_str() {
            "input_source" => *source = Some(value.clone()),
            "input_target" => *target = Some(value.clone()),
            "cfg_baidu_appid" => *baidu_appid = Some(value.clone()),
            "cfg_baidu_key" => *baidu_key = Some(value.clone()),
            _ => {}
        },
        UiNode::Select {
            id, selected_index, ..
        } => match id.as_str() {
            "select_src_lang" => *src_idx = *selected_index,
            "select_dst_lang" => *dst_idx = *selected_index,
            "select_engine" => *engine_idx = *selected_index,
            _ => {}
        },
        UiNode::Label { text: t, variant, .. } => {
            if *variant == LabelVariant::Error {
                *error_text = t.clone();
            } else if *variant == LabelVariant::Secondary || *variant == LabelVariant::Muted {
                if t.contains("引擎") || t.contains("翻译") {
                    *status_text = t.clone();
                }
            }
        }
        _ => {}
    }
}

// -----------------------------------------------------------------------------
// Generic View Sync Fallback
// -----------------------------------------------------------------------------
fn sync_generic_view(ui: &RunnerWindow, root: &UiNode) {
    let mut main_text = String::new();
    let mut tab_labels = Vec::new();
    let mut active_tab = 0;
    let mut error_text = String::new();
    let mut status_text = String::new();

    collect_generic_nodes(
        root,
        &mut main_text,
        &mut tab_labels,
        &mut active_tab,
        &mut error_text,
        &mut status_text,
    );

    if !main_text.is_empty() {
        ui.set_generic_main_text(main_text.into());
    }
    if !tab_labels.is_empty() {
        let model: Vec<slint::SharedString> = tab_labels.into_iter().map(Into::into).collect();
        ui.set_generic_tab_labels(slint::ModelRc::new(slint::VecModel::from(model)));
        ui.set_generic_active_tab(active_tab as i32);
    }
    ui.set_generic_error_text(error_text.into());
    ui.set_generic_status_text(status_text.into());
}

fn collect_generic_nodes(
    node: &UiNode,
    main_text: &mut String,
    tab_labels: &mut Vec<String>,
    active_tab: &mut usize,
    error_text: &mut String,
    status_text: &mut String,
) {
    match node {
        UiNode::Container { children, .. } | UiNode::Card { children, .. } => {
            for child in children {
                collect_generic_nodes(
                    child,
                    main_text,
                    tab_labels,
                    active_tab,
                    error_text,
                    status_text,
                );
            }
        }
        UiNode::TextInput { value, .. } => {
            if main_text.is_empty() {
                *main_text = value.clone();
            }
        }
        UiNode::Tabs {
            active_index, tabs, ..
        } => {
            *active_tab = *active_index;
            *tab_labels = tabs.iter().map(|t| t.label.clone()).collect();
            if let Some(active) = tabs.get(*active_index) {
                collect_generic_nodes(
                    &active.content,
                    main_text,
                    tab_labels,
                    active_tab,
                    error_text,
                    status_text,
                );
            }
        }
        UiNode::Label { text, variant, .. } => {
            if *variant == LabelVariant::Error {
                *error_text = text.clone();
            } else if *variant == LabelVariant::Secondary || *variant == LabelVariant::Muted {
                *status_text = text.clone();
            }
        }
        _ => {}
    }
}
