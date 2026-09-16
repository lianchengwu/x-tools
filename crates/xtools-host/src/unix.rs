use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use gtk4::gdk::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Button, CssProvider, DrawingArea, GestureClick, GestureDrag,
    Label, Orientation, Popover, Separator,
};
use xtools_protocol::PluginManifest;
use xtools_runtime::{DiscoveredPlugin, PluginLoader};
use xtools_ui::{
    HOST_INSTANCE, SLOP, claim_instance, func_radius, main_radius, raise_instance,
};

use crate::anim;
use crate::input;
use crate::layout::{Rect, clamp_main, fan_seats_dynamic, hit_disk, surface_rect, vis_scale};
use crate::overlay;
use crate::paint;
use crate::tray;

#[derive(Clone, Copy, Debug)]
enum Menu {
    Collapsed,
    Expanding { start_us: i64 },
    Expanded,
    Collapsing { start_us: i64 },
}

impl Menu {
    fn amount(self, now_us: i64) -> f64 {
        match self {
            Menu::Collapsed => 0.0,
            Menu::Expanded => 1.0,
            Menu::Expanding { start_us } => anim::ease_out_cubic(anim::progress(now_us, start_us)),
            Menu::Collapsing { start_us } => {
                1.0 - anim::ease_out_cubic(anim::progress(now_us, start_us))
            }
        }
    }

    fn is_openish(self) -> bool {
        !matches!(self, Menu::Collapsed)
    }
}

struct Host {
    main: (f64, f64),
    origin_main: (f64, f64),
    monitor: Rect,
    scale: f64,
    menu: Menu,
    dragging: bool,
    last_pointer_event: Option<gtk4::gdk::Event>,
    ticking: bool,
    last_t: f64,
    seated: bool,
    plugins: Vec<DiscoveredPlugin>,
    _instance: Rc<xtools_ui::InstanceListener>,
    _hold_guard: gtk4::gio::ApplicationHoldGuard,
}

impl Host {
    fn vis(&self) -> f64 {
        self.scale
    }

    fn main_r(&self) -> f64 {
        main_radius() * self.vis()
    }

    fn func_r(&self) -> f64 {
        func_radius() * self.vis()
    }

    fn slop(&self) -> f64 {
        SLOP * self.vis()
    }

    fn seats(&self) -> Vec<(f64, f64)> {
        fan_seats_dynamic(self.main, self.plugins.len(), self.monitor, self.vis())
    }

    fn func_at(&self, px: f64, py: f64) -> Option<usize> {
        if matches!(self.menu, Menu::Collapsed) {
            return None;
        }
        let fr = self.func_r();
        let seats = self.seats();
        seats
            .iter()
            .enumerate()
            .find(|(_, (x, y))| hit_disk(px, py, *x, *y, fr))
            .map(|(idx, _)| idx)
    }

    fn reload_plugins(&mut self) {
        let loader = PluginLoader::new();
        let mut discovered = Vec::new();

        for dir in &xtools_runtime::plugin_search_dirs() {
            if dir.exists() {
                let found = loader.scan_dir(dir);
                for p in found {
                    if !discovered.iter().any(|d: &DiscoveredPlugin| d.manifest.id == p.manifest.id) {
                        discovered.push(p);
                    }
                }
            }
        }

        // Fallback default definitions if scanning on empty folder
        if discovered.is_empty() {
            discovered = vec![
                DiscoveredPlugin {
                    path: PathBuf::from("time.wasm"),
                    manifest: PluginManifest {
                        id: "xtools.time".into(),
                        name: "时间戳转换".into(),
                        version: "0.4.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: "clock".into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                },
                DiscoveredPlugin {
                    path: PathBuf::from("json.wasm"),
                    manifest: PluginManifest {
                        id: "xtools.json".into(),
                        name: "JSON 格式化".into(),
                        version: "0.4.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: "{}".into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                },
                DiscoveredPlugin {
                    path: PathBuf::from("trans.wasm"),
                    manifest: PluginManifest {
                        id: "xtools.trans".into(),
                        name: "划词翻译".into(),
                        version: "0.4.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: "译".into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                },
                DiscoveredPlugin {
                    path: PathBuf::from("ai.wasm"),
                    manifest: PluginManifest {
                        id: "xtools.ai".into(),
                        name: "AI 问答".into(),
                        version: "0.4.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: "AI".into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                },
                DiscoveredPlugin {
                    path: PathBuf::from("codec.wasm"),
                    manifest: PluginManifest {
                        id: "xtools.codec".into(),
                        name: "编码解码".into(),
                        version: "0.4.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: "码".into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                },
            ];
        }

        self.plugins = discovered;
    }
}

fn load_css() {
    let provider = CssProvider::new();
    let css = r#"
window { background: transparent; }

popover.xtools-popover {
    padding: 0;
}
popover.xtools-popover contents {
    border-radius: 14px;
    padding: 6px;
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.22), 0 2px 8px rgba(0, 0, 0, 0.1);
    border: 1px solid alpha(currentColor, 0.12);
}
.xtools-menu-header {
    padding: 4px 8px 4px 8px;
}
.xtools-title {
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.5px;
}
.xtools-version-tag {
    font-size: 11px;
    opacity: 0.55;
    margin-top: 1px;
}
.xtools-menu-section-title {
    font-size: 10px;
    font-weight: 700;
    opacity: 0.45;
    letter-spacing: 0.8px;
    margin: 4px 8px 2px 8px;
}
.xtools-menu-btn {
    border-radius: 8px;
    padding: 6px 8px;
    margin: 1px 0;
    border: none;
    background: transparent;
    transition: background 120ms ease, color 120ms ease;
}
.xtools-menu-btn:hover {
    background: alpha(currentColor, 0.08);
}
.xtools-menu-btn:active {
    background: alpha(currentColor, 0.15);
}
.xtools-menu-icon {
    font-size: 14px;
    min-width: 22px;
}
.xtools-menu-label {
    font-size: 13px;
    font-weight: 500;
}
.xtools-badge {
    border-radius: 6px;
    padding: 1px 6px;
    font-size: 11px;
    font-weight: 600;
    background: alpha(#3B82F6, 0.15);
    color: #2563EB;
}
.xtools-badge-update {
    background: alpha(#10B981, 0.18);
    color: #059669;
}
.xtools-danger-btn:hover {
    background: alpha(#EF4444, 0.12);
    color: #DC2626;
}
separator.xtools-separator {
    margin: 4px 4px;
    opacity: 0.25;
}
"#;
    provider.load_from_string(css);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorInfo {
    pub logical_width: i32,
    pub logical_height: i32,
    pub scale_factor: i32,
}

fn primary_monitor_info() -> MonitorInfo {
    let Some(display) = gtk4::gdk::Display::default() else {
        return MonitorInfo {
            logical_width: 1920,
            logical_height: 1080,
            scale_factor: 1,
        };
    };
    let monitors = display.monitors();
    let mut best = (1920, 1080, 1);
    let mut area = best.0 * best.1;
    for i in 0..monitors.n_items() {
        if let Some(obj) = monitors.item(i) {
            if let Ok(mon) = obj.downcast::<gtk4::gdk::Monitor>() {
                let g = mon.geometry();
                let a = g.width() * g.height();
                let scale = mon.scale_factor();
                if a > area {
                    best = (g.width(), g.height(), scale);
                    area = a;
                }
            }
        }
    }
    eprintln!(
        "xtools-host-wasm: monitor size {}x{} (scale factor {})",
        best.0, best.1, best.2
    );
    MonitorInfo {
        logical_width: best.0,
        logical_height: best.1,
        scale_factor: best.2,
    }
}

fn primary_output_size() -> (i32, i32) {
    let info = primary_monitor_info();
    (info.logical_width, info.logical_height)
}

fn seat_surface(area: &DrawingArea, host: &mut Host) {
    let w = f64::from(area.width());
    let h = f64::from(area.height());
    if w < 2.0 || h < 2.0 {
        return;
    }
    let rect = surface_rect(w, h);
    host.monitor = rect;
    if !host.seated {
        let r = host.main_r();
        let bottom_margin = 12.0 * host.vis();
        host.main = (w / 2.0, h - r - bottom_margin);
        host.origin_main = host.main;
        host.seated = true;
        eprintln!(
            "xtools-host-wasm: surface {:.0}x{:.0} vis={:.2} main=({:.0},{:.0})",
            w,
            h,
            host.vis(),
            host.main.0,
            host.main.1
        );
    } else {
        host.main = clamp_main(host.main.0, host.main.1, host.main_r(), rect);
    }
}

fn sync_region(area: &DrawingArea, host: &Host) {
    match host.menu {
        Menu::Collapsed => {
            input::apply_collapsed_from_widget(area, host.main.0, host.main.1, host.main_r())
        }
        _ => input::apply_expanded_from_widget(area),
    }
}

fn ensure_tick(area: &DrawingArea, state: &Rc<RefCell<Host>>) {
    if state.borrow().ticking {
        return;
    }
    state.borrow_mut().ticking = true;
    let state = Rc::clone(state);
    area.add_tick_callback(move |widget, clock| {
        let now = clock.frame_time();
        let mut host = state.borrow_mut();
        host.last_t = host.menu.amount(now);
        let finished = match host.menu {
            Menu::Expanding { start_us } if anim::progress(now, start_us) >= 1.0 => {
                host.menu = Menu::Expanded;
                host.last_t = 1.0;
                true
            }
            Menu::Collapsing { start_us } if anim::progress(now, start_us) >= 1.0 => {
                host.menu = Menu::Collapsed;
                host.last_t = 0.0;
                true
            }
            Menu::Expanding { .. } | Menu::Collapsing { .. } => false,
            _ => true,
        };
        widget.queue_draw();
        if finished {
            host.ticking = false;
            if let Some(area) = widget.downcast_ref::<DrawingArea>() {
                sync_region(area, &host);
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn begin_expand(area: &DrawingArea, state: &Rc<RefCell<Host>>) {
    let now = area.frame_clock().map(|c| c.frame_time()).unwrap_or(0);
    {
        let mut host = state.borrow_mut();
        host.menu = Menu::Expanding { start_us: now };
        host.last_t = 0.0;
    }
    input::apply_expanded_from_widget(area);
    ensure_tick(area, state);
    area.queue_draw();
}

fn begin_collapse(area: &DrawingArea, state: &Rc<RefCell<Host>>) {
    let now = area.frame_clock().map(|c| c.frame_time()).unwrap_or(0);
    {
        let mut host = state.borrow_mut();
        host.menu = Menu::Collapsing { start_us: now };
    }
    ensure_tick(area, state);
    area.queue_draw();
}

fn snap_collapse(area: &DrawingArea, state: &Rc<RefCell<Host>>) {
    let mut host = state.borrow_mut();
    host.menu = Menu::Collapsed;
    host.last_t = 0.0;
    host.ticking = false;
    sync_region(area, &host);
    area.queue_draw();
}

fn launch_plugin(plugin: &DiscoveredPlugin) {
    let instance_name = plugin.manifest.id.replace('.', "-");
    let wasm_arg = plugin.path.to_string_lossy().to_string();
    let desktop = xtools_ui::kwin::current_desktop();

    if raise_instance(&instance_name, None).unwrap_or(false) {
        log::info!("Raised existing window for {}", instance_name);
        xtools_ui::kwin::raise_window(0, Some(&plugin.manifest.name));
        return;
    }
    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("xtools"));

    log::info!("Spawning {:?} run {:?}", self_exe, wasm_arg);
    let mut cmd = Command::new(self_exe);
    cmd.arg("run");
    cmd.arg(&wasm_arg);
    cmd.env_remove("XDG_ACTIVATION_TOKEN");
    cmd.env_remove("DESKTOP_STARTUP_ID");
    cmd.env_remove("GIO_LAUNCHED_DESKTOP_FILE_PID");
    cmd.env_remove("GIO_LAUNCHED_DESKTOP_FILE");

    if let Some(desk) = &desktop {
        cmd.env("XTOOLS_TARGET_DESKTOP", desk);
    }
    if std::env::var("XMODIFIERS").map_or(true, |v| v.trim().is_empty()) {
        cmd.env("XMODIFIERS", "@im=fcitx");
    }
    if std::env::var("GTK_IM_MODULE").map_or(true, |v| v.trim().is_empty()) {
        cmd.env("GTK_IM_MODULE", "fcitx");
    }
    if std::env::var("QT_IM_MODULE").map_or(true, |v| v.trim().is_empty()) {
        cmd.env("QT_IM_MODULE", "fcitx");
    }

    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => {
            log::error!("Failed to spawn runner for {wasm_arg}: {e}");
        }
    }
}

fn handle_click(
    area: &DrawingArea,
    state: &Rc<RefCell<Host>>,
    x: f64,
    y: f64,
    event: Option<gtk4::gdk::Event>,
) {
    let (on_main, on_func_idx, openish) = {
        let mut host = state.borrow_mut();
        if let Some(ev) = event {
            host.last_pointer_event = Some(ev);
        }
        let on_main = hit_disk(x, y, host.main.0, host.main.1, host.main_r());
        let on_func = host.func_at(x, y);
        (on_main, on_func, host.menu.is_openish())
    };

    if let Some(idx) = on_func_idx {
        let plugin = state.borrow().plugins.get(idx).cloned();
        if let Some(p) = plugin {
            launch_plugin(&p);
        }
        begin_collapse(area, state);
        return;
    }

    if on_main {
        if openish {
            begin_collapse(area, state);
        } else {
            begin_expand(area, state);
        }
        return;
    }

    if openish {
        begin_collapse(area, state);
    }
}
fn make_menu_row(
    icon: &str,
    title: &str,
    badge: Option<(&str, bool)>,
    is_danger: bool,
) -> Button {
    let btn = Button::new();
    btn.add_css_class("flat");
    btn.add_css_class("xtools-menu-btn");
    if is_danger {
        btn.add_css_class("xtools-danger-btn");
    }

    let hbox = gtk4::Box::new(Orientation::Horizontal, 8);
    hbox.set_hexpand(true);

    let icon_lbl = Label::new(Some(icon));
    icon_lbl.add_css_class("xtools-menu-icon");
    icon_lbl.set_xalign(0.5);
    hbox.append(&icon_lbl);

    let title_lbl = Label::new(Some(title));
    title_lbl.add_css_class("xtools-menu-label");
    title_lbl.set_xalign(0.0);
    title_lbl.set_hexpand(true);
    hbox.append(&title_lbl);

    if let Some((badge_text, is_update)) = badge {
        let badge_lbl = Label::new(Some(badge_text));
        badge_lbl.add_css_class("xtools-badge");
        if is_update {
            badge_lbl.add_css_class("xtools-badge-update");
        }
        hbox.append(&badge_lbl);
    }

    btn.set_child(Some(&hbox));
    btn
}

fn show_context_menu(area: &DrawingArea, state: &Rc<RefCell<Host>>, x: f64, y: f64) {
    let (on_main, is_open, plugins) = {
        let host = state.borrow();
        let on_main = hit_disk(x, y, host.main.0, host.main.1, host.main_r());
        (on_main, host.menu.is_openish(), host.plugins.clone())
    };

    if !on_main {
        return;
    }

    // Expand the input region so the popover can receive clicks outside the main circle
    input::apply_expanded_from_widget(area);

    let popover = Popover::new();
    popover.set_parent(area);
    popover.add_css_class("xtools-popover");
    popover.set_has_arrow(true);
    let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
    popover.set_pointing_to(Some(&rect));

    let content = gtk4::Box::new(Orientation::Vertical, 2);
    content.set_margin_start(4);
    content.set_margin_end(4);
    content.set_margin_top(4);
    content.set_margin_bottom(4);

    // 1. Header with App Title & Version
    let header_box = gtk4::Box::new(Orientation::Horizontal, 6);
    header_box.add_css_class("xtools-menu-header");
    let title_lbl = Label::new(Some("xtools"));
    title_lbl.add_css_class("xtools-title");
    header_box.append(&title_lbl);

    let ver_lbl = Label::new(Some(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    ver_lbl.add_css_class("xtools-version-tag");
    ver_lbl.set_hexpand(true);
    ver_lbl.set_xalign(0.0);
    header_box.append(&ver_lbl);
    content.append(&header_box);

    let sep1 = Separator::new(Orientation::Horizontal);
    sep1.add_css_class("xtools-separator");
    content.append(&sep1);

    // 2. Toggle expand / collapse
    let toggle_icon = if is_open { "⭕" } else { "🔘" };
    let toggle_label = if is_open { "收起悬浮球" } else { "展开悬浮球" };
    let toggle_btn = make_menu_row(toggle_icon, toggle_label, None, false);
    {
        let state = Rc::clone(state);
        let area = area.clone();
        let popover = popover.clone();
        toggle_btn.connect_clicked(move |_| {
            popover.popdown();
            let openish = state.borrow().menu.is_openish();
            if openish {
                begin_collapse(&area, &state);
            } else {
                begin_expand(&area, &state);
            }
        });
    }
    content.append(&toggle_btn);

    let sep2 = Separator::new(Orientation::Horizontal);
    sep2.add_css_class("xtools-separator");
    content.append(&sep2);

    // 3. Plugins
    if !plugins.is_empty() {
        let sec_title = Label::new(Some("功能插件"));
        sec_title.add_css_class("xtools-menu-section-title");
        sec_title.set_xalign(0.0);
        content.append(&sec_title);

        for p in &plugins {
            let icon = match p.manifest.mark.as_str() {
                "clock" => "🕒",
                "{}" => "{ }",
                "译" | "文" | "globe" => "🌐",
                "AI" | "智" => "✨",
                "码" => "⇄",
                other if !other.is_empty() => other,
                _ => "•",
            };
            let btn = make_menu_row(icon, &p.manifest.name, None, false);
            let p = p.clone();
            let popover = popover.clone();
            btn.connect_clicked(move |_| {
                popover.popdown();
                launch_plugin(&p);
            });
            content.append(&btn);
        }

        let sep3 = Separator::new(Orientation::Horizontal);
        sep3.add_css_class("xtools-separator");
        content.append(&sep3);
    }

    // 4. Settings
    let settings_btn = make_menu_row("⚙️", "设置", None, false);
    {
        let popover = popover.clone();
        settings_btn.connect_clicked(move |_| {
            popover.popdown();
            tray::spawn_settings_window(false);
        });
    }
    content.append(&settings_btn);

    // 5. Update check
    let cached_update = crate::updater::get_cached_update();
    let (badge_info, update_text) = if let Some(info) = &cached_update {
        if info.has_update {
            (Some((info.latest_version.as_str(), true)), "检查更新")
        } else {
            (Some(("最新", false)), "检查更新")
        }
    } else {
        (None, "检查更新")
    };
    let update_btn = make_menu_row("🚀", update_text, badge_info, false);
    {
        let popover = popover.clone();
        update_btn.connect_clicked(move |_| {
            popover.popdown();
            tray::spawn_settings_window(true);
        });
    }
    content.append(&update_btn);

    let sep4 = Separator::new(Orientation::Horizontal);
    sep4.add_css_class("xtools-separator");
    content.append(&sep4);

    // 6. Exit
    let quit_btn = make_menu_row("✕", "退出 xtools", None, true);
    quit_btn.connect_clicked(|_| {
        std::process::exit(0);
    });
    content.append(&quit_btn);

    popover.set_child(Some(&content));

    // On close, restore input region and unparent popover
    {
        let state = Rc::clone(state);
        let area = area.clone();
        let popover_c = popover.clone();
        popover.connect_closed(move |_| {
            let host = state.borrow();
            sync_region(&area, &host);
            popover_c.unparent();
        });
    }

    popover.popup();
}

pub fn run() {
    xtools_ui::boot::init_input_method_env();
    xtools_ui::kwin::ensure_pin_script();

    let app = Application::builder()
        .application_id("com.github.xtools.host.wasm")
        .build();

    app.connect_activate(|app| {
        load_css();

        let display = match gtk4::gdk::Display::default() {
            Some(d) => d,
            None => {
                eprintln!("No default GDK display available");
                return;
            }
        };

        if !input::refuse_if_no_input_shapes(&display) {
            std::process::exit(1);
        }

        let instance_lock = match claim_instance(HOST_INSTANCE) {
            Ok(Some(lock)) => Rc::new(lock),
            _ => {
                let _ = raise_instance(HOST_INSTANCE, None);
                std::process::exit(0);
            }
        };

        let hold_guard = app.hold();

        let mon_info = primary_monitor_info();
        let scale = vis_scale(
            f64::from(mon_info.logical_width),
            f64::from(mon_info.logical_height),
            mon_info.scale_factor,
        );
        let win_size = (280.0 * scale).round() as i32;

        let window = ApplicationWindow::builder()
            .application(app)
            .title("xtools host (WASM)")
            .default_width(win_size)
            .default_height(win_size)
            .decorated(false)
            .build();

        overlay::attach_overlay(&window);
        window.set_default_size(win_size, win_size);

        let area = DrawingArea::builder()
            .content_width(win_size)
            .content_height(win_size)
            .hexpand(true)
            .vexpand(true)
            .build();

        let state = Rc::new(RefCell::new(Host {
            main: (win_size as f64 / 2.0, win_size as f64 / 2.0),
            origin_main: (win_size as f64 / 2.0, win_size as f64 / 2.0),
            monitor: Rect::new(0.0, 0.0, f64::from(mon_info.logical_width), f64::from(mon_info.logical_height)),
            scale,
            menu: Menu::Collapsed,
            dragging: false,
            last_pointer_event: None,
            ticking: false,
            last_t: 0.0,
            seated: false,
            plugins: Vec::new(),
            _instance: instance_lock,
            _hold_guard: hold_guard,
        }));
        state.borrow_mut().reload_plugins();

        // 1. Draw function
        {
            let state = Rc::clone(&state);
            area.set_draw_func(move |_, cr, _w, _h| {
                let host = state.borrow();
                cr.set_operator(gtk4::cairo::Operator::Source);
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                cr.paint().ok();
                cr.set_operator(gtk4::cairo::Operator::Over);

                let t = host.last_t;
                let vis = host.vis();

                if t > 0.0 {
                    let seats = host.seats();
                    for (idx, plugin) in host.plugins.iter().enumerate() {
                        if let Some(&(sx, sy)) = seats.get(idx) {
                            let cur_x = host.main.0 + (sx - host.main.0) * t;
                            let cur_y = host.main.1 + (sy - host.main.1) * t;
                            paint::draw_func_dynamic(cr, &plugin.manifest.mark, cur_x, cur_y, t, vis);
                        }
                    }
                }

                paint::draw_main(cr, host.main.0, host.main.1, vis);
            });
        }

        // 2. Realize & resize
        {
            let state = Rc::clone(&state);
            area.connect_realize(move |area| {
                if !input::refuse_if_no_input_shapes(&area.display()) {
                    return;
                }
                {
                    let mut host = state.borrow_mut();
                    seat_surface(area, &mut host);
                }
                let host = state.borrow();
                if host.seated {
                    sync_region(area, &host);
                    if let Some(win) = area.root().and_downcast::<ApplicationWindow>() {
                        let (_sw, sh) = primary_output_size();
                        overlay::place_mid_right(&win, sh, host.main.1);
                    }
                }
                area.queue_draw();
            });
        }

        {
            let state = Rc::clone(&state);
            area.connect_resize(move |area, _w, _h| {
                {
                    let mut host = state.borrow_mut();
                    seat_surface(area, &mut host);
                }
                let host = state.borrow();
                if host.seated {
                    sync_region(area, &host);
                    if let Some(win) = area.root().and_downcast::<ApplicationWindow>() {
                        let (_sw, sh) = primary_output_size();
                        overlay::place_mid_right(&win, sh, host.main.1);
                    }
                }
                area.queue_draw();
            });
        }

        // 3. Drag and click handling
        let drag = GestureDrag::new();
        {
            let state = Rc::clone(&state);
            drag.connect_drag_begin(move |g, _x, _y| {
                let mut host = state.borrow_mut();
                host.last_pointer_event = g.last_event(None);
                host.origin_main = host.main;
                host.dragging = false;
            });
        }
        {
            let state = Rc::clone(&state);
            let area = area.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                let slop = state.borrow().slop();
                let dist = (dx * dx + dy * dy).sqrt();
                let should_snap = {
                    let host = state.borrow();
                    !host.dragging && dist > slop && host.menu.is_openish()
                };
                if !state.borrow().dragging && dist > slop {
                    if should_snap {
                        snap_collapse(&area, &state);
                    }
                    state.borrow_mut().dragging = true;
                }
                if state.borrow().dragging {
                    let mut host = state.borrow_mut();
                    let (cx, cy) = (host.origin_main.0 + dx, host.origin_main.1 + dy);
                    let r = host.main_r();
                    let mon = host.monitor;
                    host.main = clamp_main(cx, cy, r, mon);
                }
                area.queue_draw();
            });
        }
        {
            let state = Rc::clone(&state);
            let area = area.clone();
            drag.connect_drag_end(move |g, dx, dy| {
                let start = g.start_point();
                let dragged = state.borrow().dragging;
                if dragged {
                    let host = state.borrow();
                    sync_region(&area, &host);
                    area.queue_draw();
                    return;
                }
                let Some((sx, sy)) = start else {
                    return;
                };
                handle_click(&area, &state, sx + dx, sy + dy, g.last_event(None));
            });
        }
        area.add_controller(drag);

        // 4. Right-click context menu
        let right_click = GestureClick::new();
        right_click.set_button(gtk4::gdk::BUTTON_SECONDARY);
        {
            let state = Rc::clone(&state);
            let area = area.clone();
            right_click.connect_pressed(move |_, _n, x, y| {
                show_context_menu(&area, &state, x, y);
            });
        }
        area.add_controller(right_click);

        window.set_child(Some(&area));
        window.present();

        // 4. Spawn tray
        let tray_open = Arc::new(AtomicBool::new(false));
        tray::spawn_tray(tray_open);
    });

    app.run();
}
