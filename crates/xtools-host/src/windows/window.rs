//! Windows native layered window management, message loop, input, and plugin spawning.

use std::mem::zeroed;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    HWND, LPARAM, LRESULT, POINT, RECT as WIN_RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    GetMonitorInfoW, HBITMAP, HDC, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, HTCLIENT, HTTRANSPARENT, HWND_TOPMOST,
    KillTimer, MSG, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, SPI_GETWORKAREA, SM_CXSCREEN, SM_CYSCREEN, ULW_ALPHA,
    UpdateLayeredWindow, WM_COMMAND, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCHITTEST, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use xtools_protocol::PluginManifest;
use xtools_runtime::{DiscoveredPlugin, PluginLoader};
use xtools_ui::{
    HOST_INSTANCE, POP_MS, SLOP, accept_command, claim_instance, func_radius, main_radius,
    raise_instance,
};

use crate::anim;
use crate::layout::{Rect, fan_seats_dynamic, hit_disk};
use crate::windows::paint::{Surface, draw_func, draw_main};
use crate::windows::tray::{
    ID_TRAY_QUIT, ID_TRAY_SETTINGS, ID_TRAY_SHOW_HIDE, TrayIcon, WM_TRAY_CALLBACK,
};

const TIMER_ANIM: usize = 1;
const TIMER_IPC: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Menu {
    Collapsed,
    Expanding { start: Instant },
    Expanded,
    Collapsing { start: Instant },
}

impl Menu {
    pub fn amount(self, now: Instant) -> f64 {
        match self {
            Menu::Collapsed => 0.0,
            Menu::Expanded => 1.0,
            Menu::Expanding { start } => {
                let t = (now.saturating_duration_since(start).as_millis() as f64
                    / POP_MS as f64)
                    .clamp(0.0, 1.0);
                anim::ease_out_cubic(t)
            }
            Menu::Collapsing { start } => {
                let t = (now.saturating_duration_since(start).as_millis() as f64
                    / POP_MS as f64)
                    .clamp(0.0, 1.0);
                1.0 - anim::ease_out_cubic(t)
            }
        }
    }

    pub fn is_openish(self) -> bool {
        !matches!(self, Menu::Collapsed)
    }
}

pub struct HostWindow {
    hwnd: HWND,
    menu: Menu,
    scale: f64,
    win_x: i32,
    win_y: i32,
    win_w: i32,
    win_h: i32,
    main_lx: f64,
    main_ly: f64,
    is_dragging: bool,
    is_lbutton_down: bool,
    drag_start_cursor: (i32, i32),
    drag_start_win: (i32, i32),
    is_visible: bool,
    plugins: Vec<DiscoveredPlugin>,
    surface: Surface,
    mem_dc: HDC,
    dib_bmp: HBITMAP,
    dib_bits: *mut u32,
    tray: Option<TrayIcon>,
    listener: Option<xtools_ui::InstanceListener>,
}

impl HostWindow {
    pub fn new(hwnd: HWND, listener: xtools_ui::InstanceListener) -> Self {
        let work = get_work_area(hwnd);
        let work_w = work.right - work.left;
        let work_h = work.bottom - work.top;

        // Real per-monitor DPI (96 = 100%), replacing the old resolution heuristic.
        let scale: f64 = (get_window_dpi(hwnd) as f64 / 96.0).clamp(1.0, 3.0);

        let win_w = (320.0 * scale).round() as i32;
        let win_h = (320.0 * scale).round() as i32;

        let main_r = main_radius() * scale;
        // Main ball rests near the right-middle of the window bounding box
        let main_lx = win_w as f64 - main_r - (16.0 * scale);
        let main_ly = win_h as f64 / 2.0;

        let win_x = work.right - win_w;
        let win_y = work.top + ((work_h - win_h) / 2).max(0);

        let hdc_screen = unsafe { GetDC(std::ptr::null_mut()) };
        let mem_dc = unsafe { CreateCompatibleDC(hdc_screen) };

        let mut bmi: BITMAPINFO = unsafe { zeroed() };
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = win_w;
        bmi.bmiHeader.biHeight = -win_h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib_bmp = unsafe {
            CreateDIBSection(
                mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            )
        };
        unsafe {
            SelectObject(mem_dc, dib_bmp);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
        }

        let dib_bits = bits as *mut u32;
        let surface = Surface::new(win_w, win_h);
        let tray = Some(TrayIcon::new(hwnd));

        let mut host = Self {
            hwnd,
            menu: Menu::Collapsed,
            scale,
            win_x,
            win_y,
            win_w,
            win_h,
            main_lx,
            main_ly,
            is_dragging: false,
            is_lbutton_down: false,
            drag_start_cursor: (0, 0),
            drag_start_win: (0, 0),
            is_visible: true,
            plugins: Vec::new(),
            surface,
            mem_dc,
            dib_bmp,
            dib_bits,
            tray,
            listener: Some(listener),
        };
        host.reload_plugins();

        host.redraw();
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                win_x,
                win_y,
                win_w,
                win_h,
                SWP_NOACTIVATE,
            );
            ShowWindow(hwnd, SW_SHOW);
            SetTimer(hwnd, TIMER_IPC, 100, None);
        }

        host
    }

    /// 扫描插件目录（可执行文件旁 plugins/ 等），空目录时回落到内置三项
    pub fn reload_plugins(&mut self) {
        let loader = PluginLoader::new();
        let mut discovered: Vec<DiscoveredPlugin> = Vec::new();

        for dir in xtools_runtime::plugin_search_dirs() {
            if dir.exists() {
                for p in loader.scan_dir(&dir) {
                    if !discovered.iter().any(|d| d.manifest.id == p.manifest.id) {
                        discovered.push(p);
                    }
                }
            }
        }

        if discovered.is_empty() {
            let fallback = [
                ("xtools.time", "时间戳转换", "clock"),
                ("xtools.json", "JSON 格式化", "{}"),
                ("xtools.trans", "划词翻译", "文"),
            ];
            for (id, name, mark) in fallback {
                discovered.push(DiscoveredPlugin {
                    path: PathBuf::from(format!("{}.wasm", id.trim_prefix("xtools."))),
                    manifest: PluginManifest {
                        id: id.into(),
                        name: name.into(),
                        version: "0.0.0".into(),
                        description: "".into(),
                        author: "".into(),
                        mark: mark.into(),
                        icon_svg: None,
                        window: Default::default(),
                        permissions: vec![],
                    },
                });
            }
        }

        self.plugins = discovered;
    }

    pub fn seats(&self) -> Vec<(f64, f64)> {
        let mon = Rect::new(0.0, 0.0, self.win_w as f64, self.win_h as f64);
        fan_seats_dynamic((self.main_lx, self.main_ly), self.plugins.len(), mon, self.scale)
    }

    pub fn redraw(&mut self) {
        let now = Instant::now();
        let amount = self.menu.amount(now);

        self.surface.clear();

        // 1. Draw function balls if expanded/expanding/collapsing
        if amount > 0.0 {
            let seats = self.seats();
            let marks: Vec<String> = self
                .plugins
                .iter()
                .map(|p| p.manifest.mark.clone())
                .collect();
            for (idx, (sx, sy)) in seats.iter().enumerate() {
                let Some(mark) = marks.get(idx) else {
                    continue;
                };
                let cur_cx = anim::lerp(self.main_lx, *sx, amount);
                let cur_cy = anim::lerp(self.main_ly, *sy, amount);
                draw_func(&mut self.surface, mark, cur_cx, cur_cy, amount, self.scale);
            }
        }

        // 2. Draw main floating ball
        draw_main(&mut self.surface, self.main_lx, self.main_ly, self.scale);

        // 3. Copy pixels to DIBSection
        if !self.dib_bits.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.surface.pixels.as_ptr(),
                    self.dib_bits,
                    self.surface.pixels.len(),
                );
            }
        }

        // 4. Update layered window
        let pt_src = POINT { x: 0, y: 0 };
        let pt_dst = POINT {
            x: self.win_x,
            y: self.win_y,
        };
        let size = SIZE {
            cx: self.win_w,
            cy: self.win_h,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        unsafe {
            let hdc_screen = GetDC(std::ptr::null_mut());
            UpdateLayeredWindow(
                self.hwnd,
                hdc_screen,
                &pt_dst,
                &size,
                self.mem_dc,
                &pt_src,
                0,
                &blend,
                ULW_ALPHA,
            );
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
        }
    }

    pub fn begin_expand(&mut self) {
        self.menu = Menu::Expanding {
            start: Instant::now(),
        };
        unsafe {
            SetTimer(self.hwnd, TIMER_ANIM, 16, None);
        }
        self.redraw();
    }

    pub fn begin_collapse(&mut self) {
        self.menu = Menu::Collapsing {
            start: Instant::now(),
        };
        unsafe {
            SetTimer(self.hwnd, TIMER_ANIM, 16, None);
        }
        self.redraw();
    }

    pub fn snap_collapse(&mut self) {
        self.menu = Menu::Collapsed;
        unsafe {
            KillTimer(self.hwnd, TIMER_ANIM);
        }
        self.redraw();
    }

    pub fn on_timer(&mut self, timer_id: usize) {
        if timer_id == TIMER_ANIM {
            let now = Instant::now();
            match self.menu {
                Menu::Expanding { start } => {
                    if now.saturating_duration_since(start).as_millis() >= POP_MS as u128 {
                        self.menu = Menu::Expanded;
                        unsafe {
                            KillTimer(self.hwnd, TIMER_ANIM);
                        }
                    }
                    self.redraw();
                }
                Menu::Collapsing { start } => {
                    if now.saturating_duration_since(start).as_millis() >= POP_MS as u128 {
                        self.menu = Menu::Collapsed;
                        unsafe {
                            KillTimer(self.hwnd, TIMER_ANIM);
                        }
                    }
                    self.redraw();
                }
                _ => unsafe {
                    KillTimer(self.hwnd, TIMER_ANIM);
                },
            }
        } else if timer_id == TIMER_IPC {
            if let Some(listener) = &self.listener {
                match accept_command(listener) {
                    Some(xtools_ui::InstanceCommand::Quit) => unsafe {
                        DestroyWindow(self.hwnd);
                        PostQuitMessage(0);
                    },
                    Some(xtools_ui::InstanceCommand::Raise(_)) => {
                        self.is_visible = true;
                        unsafe {
                            ShowWindow(self.hwnd, SW_SHOW);
                            SetWindowPos(
                                self.hwnd,
                                HWND_TOPMOST,
                                self.win_x,
                                self.win_y,
                                self.win_w,
                                self.win_h,
                                SWP_NOACTIVATE,
                            );
                        }
                        self.redraw();
                    }
                    None => {}
                }
            }
        }
    }

    pub fn hit_test(&self, screen_x: i32, screen_y: i32) -> LRESULT {
        let lx = (screen_x - self.win_x) as f64;
        let ly = (screen_y - self.win_y) as f64;

        // Check main ball
        let main_r = main_radius() * self.scale;
        if hit_disk(lx, ly, self.main_lx, self.main_ly, main_r) {
            return HTCLIENT as LRESULT;
        }

        // Check function balls if visible
        let amount = self.menu.amount(Instant::now());
        if amount > 0.0 {
            let func_r = func_radius() * self.scale;
            for (cx, cy) in self.seats() {
                let cur_cx = anim::lerp(self.main_lx, cx, amount);
                let cur_cy = anim::lerp(self.main_ly, cy, amount);
                if hit_disk(lx, ly, cur_cx, cur_cy, func_r) {
                    return HTCLIENT as LRESULT;
                }
            }
        }

        HTTRANSPARENT as LRESULT
    }

    pub fn on_lbutton_down(&mut self, x: i32, y: i32) {
        self.is_lbutton_down = true;
        self.is_dragging = false;
        self.drag_start_cursor = (x, y);
        self.drag_start_win = (self.win_x, self.win_y);
        unsafe {
            SetCapture(self.hwnd);
        }
    }

    pub fn on_mouse_move(&mut self, cur_x: i32, cur_y: i32) {
        if !self.is_lbutton_down {
            return;
        }

        let dx = cur_x - self.drag_start_cursor.0;
        let dy = cur_y - self.drag_start_cursor.1;

        if !self.is_dragging && (dx.abs() as f64 > SLOP || dy.abs() as f64 > SLOP) {
            self.is_dragging = true;
            if self.menu.is_openish() {
                self.snap_collapse();
            }
        }

        if self.is_dragging {
            let new_x = self.drag_start_win.0 + dx;
            let new_y = self.drag_start_win.1 + dy;

            let work = get_work_area(self.hwnd);
            let clamped_x = new_x.clamp(work.left - self.win_w / 2, work.right - self.win_w / 2);
            let clamped_y = new_y.clamp(work.top - self.win_h / 2, work.bottom - self.win_h / 2);

            self.win_x = clamped_x;
            self.win_y = clamped_y;

            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    self.win_x,
                    self.win_y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            self.redraw();
        }
    }

    pub fn on_lbutton_up(&mut self, cur_x: i32, cur_y: i32) {
        unsafe {
            ReleaseCapture();
        }
        self.is_lbutton_down = false;

        if self.is_dragging {
            self.is_dragging = false;
            return;
        }

        // It was a click!
        let lx = (cur_x - self.win_x) as f64;
        let ly = (cur_y - self.win_y) as f64;

        // Check if main ball clicked
        let main_r = main_radius() * self.scale;
        if hit_disk(lx, ly, self.main_lx, self.main_ly, main_r) {
            match self.menu {
                Menu::Collapsed | Menu::Collapsing { .. } => self.begin_expand(),
                Menu::Expanded | Menu::Expanding { .. } => self.begin_collapse(),
            }
            return;
        }

        // Check if function ball clicked
        let amount = self.menu.amount(Instant::now());
        if amount > 0.0 {
            let func_r = func_radius() * self.scale;
            for (idx, (cx, cy)) in self.seats().iter().enumerate() {
                let cur_cx = anim::lerp(self.main_lx, *cx, amount);
                let cur_cy = anim::lerp(self.main_ly, *cy, amount);
                if hit_disk(lx, ly, cur_cx, cur_cy, func_r) {
                    if let Some(p) = self.plugins.get(idx).cloned() {
                        launch_plugin(&p);
                    }
                    self.snap_collapse();
                    return;
                }
            }

            // 点击扇形区域外的空白处收起
            if self.menu.is_openish() {
                self.begin_collapse();
            }
        }
    }

    pub fn on_rbutton_up(&mut self) {
        if let Some(tray) = &self.tray {
            tray.show_menu(self.hwnd, self.is_visible);
        }
    }

    pub fn toggle_visibility(&mut self) {
        self.is_visible = !self.is_visible;
        if self.is_visible {
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
            }
            self.redraw();
        } else {
            if self.menu.is_openish() {
                self.snap_collapse();
            }
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.mem_dc.is_null() {
                DeleteDC(self.mem_dc);
            }
            if !self.dib_bmp.is_null() {
                DeleteObject(self.dib_bmp);
            }
        }
    }
}

/// Work area (excluding the taskbar) of the monitor that currently owns `hwnd`,
/// so dragging across monitors clamps against the right display.
fn get_work_area(hwnd: HWND) -> WIN_RECT {
    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if !hmon.is_null() {
        let mut info: MONITORINFO = unsafe { zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(hmon, &mut info) } != 0 {
            return info.rcWork;
        }
    }

    // Fallback: primary monitor work area.
    let mut rect: WIN_RECT = unsafe { zeroed() };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut rect as *mut WIN_RECT as *mut std::ffi::c_void,
            0,
        )
    };
    if ok == 0 {
        rect.right = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        rect.bottom = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    }
    rect
}

/// Effective DPI of the window's monitor (96 = 100% scale).
fn get_window_dpi(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 96 } else { dpi }
}

/// 已有窗口则拉起，否则用自身二进制 `xtools run <wasm>` 启动插件窗口
fn launch_plugin(plugin: &DiscoveredPlugin) {
    let instance_name = plugin.manifest.id.replace('.', "-");
    if raise_instance(&instance_name, None).unwrap_or(false) {
        log::info!("Raised existing window for {instance_name}");
        return;
    }

    let Ok(self_exe) = std::env::current_exe() else {
        return;
    };
    let wasm_arg = plugin.path.to_string_lossy().to_string();
    log::info!("Spawning {self_exe:?} run {wasm_arg}");

    match Command::new(&self_exe).arg("run").arg(&wasm_arg).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => log::error!("Failed to spawn runner for {wasm_arg}: {err}"),
    }
}

/// 从托盘拉起独立设置窗口（独立进程，重复点击由单实例机制合并）
fn spawn_settings_window() {
    let Ok(self_exe) = std::env::current_exe() else {
        return;
    };
    if let Err(err) = Command::new(self_exe).arg("settings").spawn() {
        log::error!("Failed to spawn settings window: {err}");
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut HostWindow;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let host = &mut *ptr;

        match msg {
            WM_NCHITTEST => {
                let x = (lparam & 0xFFFF) as i16 as i32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                host.hit_test(x, y)
            }
            WM_LBUTTONDOWN => {
                let mut pt: POINT = zeroed();
                GetCursorPos(&mut pt);
                host.on_lbutton_down(pt.x, pt.y);
                0
            }
            WM_MOUSEMOVE => {
                let mut pt: POINT = zeroed();
                GetCursorPos(&mut pt);
                host.on_mouse_move(pt.x, pt.y);
                0
            }
            WM_LBUTTONUP => {
                let mut pt: POINT = zeroed();
                GetCursorPos(&mut pt);
                host.on_lbutton_up(pt.x, pt.y);
                0
            }
            WM_RBUTTONUP => {
                host.on_rbutton_up();
                0
            }
            WM_TIMER => {
                host.on_timer(wparam);
                0
            }
            WM_COMMAND => {
                let id = wparam & 0xFFFF;
                if id == ID_TRAY_SHOW_HIDE {
                    host.toggle_visibility();
                } else if id == ID_TRAY_SETTINGS {
                    spawn_settings_window();
                } else if id == ID_TRAY_QUIT {
                    DestroyWindow(hwnd);
                    PostQuitMessage(0);
                }
                0
            }
            WM_TRAY_CALLBACK => {
                let event = lparam as u32;
                if event == WM_LBUTTONUP {
                    host.toggle_visibility();
                } else if event == WM_RBUTTONUP {
                    host.on_rbutton_up();
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn run() {
    // Opt into per-monitor DPI awareness so the window is not bitmap-stretched
    // on high-DPI displays. The call is a no-op failure on legacy systems.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let mut lock = None;
    for _ in 0..15 {
        match claim_instance(HOST_INSTANCE) {
            Ok(Some(listener)) => {
                lock = Some(listener);
                break;
            }
            Ok(None) => {
                if let Ok(true) = raise_instance(HOST_INSTANCE, None) {
                    log::info!("Raised existing host window");
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(err) => {
                log::warn!("Instance lock attempt for '{HOST_INSTANCE}': {err}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
    let Some(instance) = lock else {
        let _ = raise_instance(HOST_INSTANCE, None);
        return;
    };

    let class_name: Vec<u16> = "XToolsHostWindow\0".encode_utf16().collect();
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };

    unsafe {
        RegisterClassExW(&wc);
    }

    let title: Vec<u16> = "xtools\0".encode_utf16().collect();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            320,
            320,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };

    if hwnd.is_null() {
        eprintln!("xtools: failed to create Win32 layered window");
        return;
    }

    let mut host = HostWindow::new(hwnd, instance);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut host as *mut HostWindow as isize);
    }

    let mut msg: MSG = unsafe { zeroed() };
    unsafe {
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
}
