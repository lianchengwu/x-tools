use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use gtk4::gdk::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, CssProvider, DrawingArea, GestureDrag};
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
                        mark: "文".into(),
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
    provider.load_from_string("window { background: transparent; }");
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

        window.set_child(Some(&area));
        window.present();

        // 4. Spawn tray
        let tray_open = Arc::new(AtomicBool::new(false));
        tray::spawn_tray(tray_open);
    });

    app.run();
}
