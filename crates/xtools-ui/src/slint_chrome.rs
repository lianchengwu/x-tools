//! Shared Slint helpers, theme, and clipboard.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

/// Copy text to system clipboard.
pub fn copy_to_clipboard(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

/// Resize edge direction for window resize operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    East,
    South,
    SouthEast,
}

/// Interactive moves/resizes are compositor-driven on Wayland: the closing
/// button release is consumed by the compositor and never reaches us, so
/// Slint keeps the pointer grabbed by the initiating TouchArea — swallowing
/// every later click. Dispatch a synthetic release right away to hand the
/// pointer back to Slint.
fn dispatch_synthetic_pointer_release(window: &slint::Window) {
    win_debug("synthetic pointer release dispatched");
    let _ = window.try_dispatch_event(slint::platform::WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(0.0, 0.0),
        button: slint::platform::PointerEventButton::Left,
    });
}

/// Compositor-driven interactive resizes on Wayland leave the client cursor
/// latched on the resize shape (KWin restores the pre-grab client cursor, and
/// Slint's hover state never changes afterwards to correct it). Client-driven
/// `set_size` resizing uses plain press/move/release events, so it cannot
/// stick; use it on Wayland and keep the compositor resize elsewhere.
fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn win_debug(msg: &str) {
    if std::env::var_os("XTOOLS_WINDBG").is_some() {
        eprintln!("[xtools-win] {msg}");
    }
}

/// Helper for smooth window dragging on undecorated Slint windows.
#[derive(Clone, Default)]
pub struct WindowDragState {
    start_pos: Arc<Mutex<Option<slint::PhysicalPosition>>>,
    is_native_drag: Arc<Mutex<bool>>,
}

impl WindowDragState {
    pub fn new() -> Self {
        Self {
            start_pos: Arc::new(Mutex::new(None)),
            is_native_drag: Arc::new(Mutex::new(false)),
        }
    }

    pub fn on_drag_started(&self, window: &slint::Window) {
        let mut native_handled = false;
        #[cfg(feature = "slint-chrome")]
        {
            use i_slint_backend_winit::WinitWindowAccessor;
            let res = window.with_winit_window(|winit_win| winit_win.drag_window().is_ok());
            if let Some(true) = res {
                native_handled = true;
            }
        }
        *self.is_native_drag.lock() = native_handled;
        if native_handled {
            win_debug("native drag started");
            dispatch_synthetic_pointer_release(window);
            *self.start_pos.lock() = None;
        } else {
            win_debug("native drag rejected, manual fallback");
            let pos = window.position();
            *self.start_pos.lock() = Some(pos);
        }
    }

    pub fn on_dragged(&self, window: &slint::Window, dx: f32, dy: f32) {
        // If native OS window dragging took over, DO NOT manually set position (otherwise it snaps back)
        if *self.is_native_drag.lock() {
            return;
        }
        if let Some(base_pos) = *self.start_pos.lock() {
            let scale = window.scale_factor();
            let new_x = base_pos.x + (dx * scale).round() as i32;
            let new_y = base_pos.y + (dy * scale).round() as i32;
            window.set_position(slint::PhysicalPosition::new(new_x, new_y));
        }
    }
}

/// Helper for resizing undecorated Slint windows.
#[derive(Clone, Default)]
pub struct WindowResizeState {
    start_size: Arc<Mutex<Option<slint::PhysicalSize>>>,
    saved_size: Arc<Mutex<Option<slint::PhysicalSize>>>,
    is_expanded: Arc<Mutex<bool>>,
    is_native_resize: Arc<Mutex<bool>>,
}

impl WindowResizeState {
    pub fn new() -> Self {
        Self {
            start_size: Arc::new(Mutex::new(None)),
            saved_size: Arc::new(Mutex::new(None)),
            is_expanded: Arc::new(Mutex::new(false)),
            is_native_resize: Arc::new(Mutex::new(false)),
        }
    }

    pub fn on_resize_started(&self, window: &slint::Window, edge: Option<ResizeEdge>) {
        let mut native_handled = false;
        #[cfg(feature = "slint-chrome")]
        if !wayland_session() {
            use i_slint_backend_winit::WinitWindowAccessor;
            if let Some(edge) = edge {
                let dir = match edge {
                    ResizeEdge::East => i_slint_backend_winit::winit::window::ResizeDirection::East,
                    ResizeEdge::South => {
                        i_slint_backend_winit::winit::window::ResizeDirection::South
                    }
                    ResizeEdge::SouthEast => {
                        i_slint_backend_winit::winit::window::ResizeDirection::SouthEast
                    }
                };
                let res =
                    window.with_winit_window(|winit_win| winit_win.drag_resize_window(dir).is_ok());
                if let Some(true) = res {
                    native_handled = true;
                }
            }
        }
        *self.is_native_resize.lock() = native_handled;
        if native_handled {
            win_debug("native resize started");
            dispatch_synthetic_pointer_release(window);
            *self.start_size.lock() = None;
        } else {
            win_debug("manual resize (wayland or fallback)");
            *self.is_expanded.lock() = false;
            *self.saved_size.lock() = None;
            let size = window.size();
            *self.start_size.lock() = Some(size);
        }
    }

    pub fn on_resized(&self, window: &slint::Window, dx: f32, dy: f32, min_w: u32, min_h: u32) {
        // If native OS window resizing took over, DO NOT manually set size
        if *self.is_native_resize.lock() {
            return;
        }
        if let Some(base_size) = *self.start_size.lock() {
            let scale = window.scale_factor();
            let min_phys_w = (min_w as f32 * scale).round() as u32;
            let min_phys_h = (min_h as f32 * scale).round() as u32;
            let raw_w = base_size.width as f32 + dx * scale;
            let raw_h = base_size.height as f32 + dy * scale;
            let new_w = (raw_w.round() as u32).max(min_phys_w);
            let new_h = (raw_h.round() as u32).max(min_phys_h);
            window.set_size(slint::PhysicalSize::new(new_w, new_h));
        }
    }
    pub fn is_expanded(&self) -> bool {
        *self.is_expanded.lock()
    }

    pub fn set_expanded(&self, val: bool) {
        *self.is_expanded.lock() = val;
    }

    pub fn toggle_expand(
        &self,
        window: &slint::Window,
        normal_w: u32,
        normal_h: u32,
        expanded_w: u32,
        expanded_h: u32,
    ) -> bool {
        let current_size = window.size();
        let scale = window.scale_factor().max(0.01);
        let current_w = current_size.width as f32 / scale;
        let current_h = current_size.height as f32 / scale;
        let tolerance = 20.0;

        let mut is_expanded = self.is_expanded.lock();
        let mut saved = self.saved_size.lock();

        let should_restore = *is_expanded
            || saved.is_some()
            || (current_w + tolerance >= expanded_w as f32 && current_h + tolerance >= expanded_h as f32);

        if should_restore {
            *is_expanded = false;
            if let Some(target) = saved.take() {
                window.set_size(target);
            } else {
                window.set_size(slint::LogicalSize::new(normal_w as f32, normal_h as f32));
            }
            false
        } else {
            *is_expanded = true;
            *saved = Some(current_size);
            window.set_size(slint::LogicalSize::new(expanded_w as f32, expanded_h as f32));
            true
        }
    }
}

/// Start a timer that polls the instance lock and handles raise or quit commands,
/// with a custom callback invoked whenever a raise command is received.
pub fn setup_raise_timer_with_callback<F>(
    listener: crate::InstanceListener,
    window: slint::Weak<impl slint::ComponentHandle + 'static>,
    mut on_raise: F,
) -> slint::Timer
where
    F: FnMut(Option<String>) + 'static,
{
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(50),
        move || match crate::instance::accept_command(&listener) {
            Some(crate::instance::InstanceCommand::Quit) => {
                std::process::exit(0);
            }
            Some(crate::instance::InstanceCommand::Raise(token)) => {
                if let Some(ui) = window.upgrade() {
                    let _ = ui.window().show();
                    #[cfg(feature = "slint-chrome")]
                    {
                        use i_slint_backend_winit::WinitWindowAccessor;
                        ui.window().with_winit_window(|w| {
                            w.set_minimized(false);
                            w.set_visible(true);
                            w.focus_window();
                            w.request_user_attention(Some(i_slint_backend_winit::winit::window::UserAttentionType::Critical));
                            #[cfg(unix)]
                            {
                                crate::kwin::raise_window(std::process::id(), None);
                            }

                            #[cfg(windows)]
                            {
                                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                                use windows_sys::Win32::System::Threading::{
                                    AttachThreadInput, GetCurrentThreadId,
                                };
                                use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
                                use windows_sys::Win32::UI::WindowsAndMessaging::{
                                    BringWindowToTop, GetForegroundWindow,
                                    GetWindowThreadProcessId, IsIconic,
                                    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
                                };
                                if let Ok(handle) = w.window_handle() {
                                    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                                        let hwnd = win32_handle.hwnd.get() as isize
                                            as windows_sys::Win32::Foundation::HWND;
                                        unsafe {
                                            if IsIconic(hwnd) != 0 {
                                                ShowWindow(hwnd, SW_RESTORE);
                                            } else {
                                                ShowWindow(hwnd, SW_SHOW);
                                            }
                                            let foreground_hwnd = GetForegroundWindow();
                                            let foreground_thread = GetWindowThreadProcessId(
                                                foreground_hwnd,
                                                std::ptr::null_mut(),
                                            );
                                            let current_thread = GetCurrentThreadId();
                                            if foreground_thread != 0
                                                && foreground_thread != current_thread
                                            {
                                                AttachThreadInput(foreground_thread, current_thread, 1);
                                                BringWindowToTop(hwnd);
                                                SetForegroundWindow(hwnd);
                                                AttachThreadInput(foreground_thread, current_thread, 0);
                                            } else {
                                                BringWindowToTop(hwnd);
                                                SetForegroundWindow(hwnd);
                                            }
                                            SetFocus(hwnd);
                                            hide_hwnd_from_taskbar(hwnd);
                                        }
                                    }
                                }
                            }
                        });
                    }
                    on_raise(token);
                }
            }
            None => {}
        },
    );
    timer
}

/// Start a timer that polls the instance lock and handles raise or quit commands.
pub fn setup_raise_timer(
    listener: crate::InstanceListener,
    window: slint::Weak<impl slint::ComponentHandle + 'static>,
) -> slint::Timer {
    setup_raise_timer_with_callback(listener, window, |_| {})
}

/// Tracks whether a window has lost focus continuously for a specified duration.
#[derive(Clone, Debug)]
pub struct FocusLossTracker {
    timeout: Duration,
    startup_grace: Duration,
    started_at: std::time::Instant,
    has_ever_focused: bool,
    lost_focus_at: Option<std::time::Instant>,
}

impl FocusLossTracker {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            startup_grace: Duration::from_secs(3),
            started_at: std::time::Instant::now(),
            has_ever_focused: false,
            lost_focus_at: None,
        }
    }

    pub fn with_startup_grace(
        timeout: Duration,
        startup_grace: Duration,
        started_at: std::time::Instant,
    ) -> Self {
        Self {
            timeout,
            startup_grace,
            started_at,
            has_ever_focused: false,
            lost_focus_at: None,
        }
    }

    /// Ticks the state machine with the current engagement status.
    /// Returns `true` if the timeout has expired and the window should exit.
    pub fn tick(&mut self, is_engaged: bool, now: std::time::Instant) -> bool {
        if is_engaged {
            self.has_ever_focused = true;
            self.lost_focus_at = None;
            return false;
        }

        // If never engaged yet and within startup grace period, do not start countdown
        if !self.has_ever_focused && now.saturating_duration_since(self.started_at) < self.startup_grace {
            return false;
        }

        match self.lost_focus_at {
            None => {
                self.lost_focus_at = Some(now);
                false
            }
            Some(lost_at) => now.saturating_duration_since(lost_at) >= self.timeout,
        }
    }

    pub fn is_counting_down(&self) -> bool {
        self.lost_focus_at.is_some()
    }
}

#[cfg(windows)]
pub fn is_window_engaged(hwnd: Option<windows_sys::Win32::Foundation::HWND>) -> bool {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.is_null() {
        let mut foreground_pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(foreground, &mut foreground_pid);
        }
        if foreground_pid == unsafe { GetCurrentProcessId() } {
            return true;
        }
    }

    if let Some(hwnd) = hwnd {
        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } != 0 {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
                if pt.x >= rect.left && pt.x < rect.right && pt.y >= rect.top && pt.y < rect.bottom {
                    return true;
                }
            }
        }
    }

    false
}
/// Applies Windows taskbar exclusion to a native window handle (HWND).
/// Sets `WS_EX_TOOLWINDOW` and clears `WS_EX_APPWINDOW` in the extended window style,
/// updates the window frame, and deletes the taskbar tab via `ITaskbarList::DeleteTab`.
#[cfg(all(windows, feature = "slint-chrome"))]
pub fn hide_hwnd_from_taskbar(hwnd: windows_sys::Win32::Foundation::HWND) {
    if hwnd.is_null() {
        return;
    }

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let new_ex_style = (ex_style & !WS_EX_APPWINDOW) | WS_EX_TOOLWINDOW;
        if new_ex_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style as isize);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }

    remove_from_taskbar_list(hwnd);
}

#[cfg(all(windows, feature = "slint-chrome"))]
fn remove_from_taskbar_list(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    #[repr(C)]
    struct ITaskbarList {
        lp_vtbl: *const ITaskbarListVtbl,
    }

    #[allow(non_snake_case)]
    #[repr(C)]
    struct ITaskbarListVtbl {
        pub QueryInterface: unsafe extern "system" fn(
            this: *mut core::ffi::c_void,
            riid: *const windows_sys::core::GUID,
            ppv: *mut *mut core::ffi::c_void,
        ) -> windows_sys::core::HRESULT,
        pub AddRef: unsafe extern "system" fn(this: *mut core::ffi::c_void) -> u32,
        pub Release: unsafe extern "system" fn(this: *mut core::ffi::c_void) -> u32,
        pub HrInit: unsafe extern "system" fn(this: *mut core::ffi::c_void) -> windows_sys::core::HRESULT,
        pub AddTab: unsafe extern "system" fn(
            this: *mut core::ffi::c_void,
            hwnd: windows_sys::Win32::Foundation::HWND,
        ) -> windows_sys::core::HRESULT,
        pub DeleteTab: unsafe extern "system" fn(
            this: *mut core::ffi::c_void,
            hwnd: windows_sys::Win32::Foundation::HWND,
        ) -> windows_sys::core::HRESULT,
        pub ActivateTab: unsafe extern "system" fn(
            this: *mut core::ffi::c_void,
            hwnd: windows_sys::Win32::Foundation::HWND,
        ) -> windows_sys::core::HRESULT,
        pub SetActiveAlt: unsafe extern "system" fn(
            this: *mut core::ffi::c_void,
            hwnd: windows_sys::Win32::Foundation::HWND,
        ) -> windows_sys::core::HRESULT,
    }

    const CLSID_TASKBAR_LIST: windows_sys::core::GUID = windows_sys::core::GUID {
        data1: 0x56FDF344,
        data2: 0xFD6D,
        data3: 0x11D0,
        data4: [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
    };

    const IID_ITASKBAR_LIST: windows_sys::core::GUID = windows_sys::core::GUID {
        data1: 0x56FDF342,
        data2: 0xFD6D,
        data3: 0x11D0,
        data4: [0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90],
    };

    unsafe {
        let mut taskbar_list: *mut core::ffi::c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_TASKBAR_LIST,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ITASKBAR_LIST,
            &mut taskbar_list,
        );
        if hr >= 0 && !taskbar_list.is_null() {
            let tbl = taskbar_list as *mut ITaskbarList;
            let vtbl = &*(*tbl).lp_vtbl;
            let _ = (vtbl.HrInit)(taskbar_list);
            let _ = (vtbl.DeleteTab)(taskbar_list, hwnd);
            let _ = (vtbl.Release)(taskbar_list);
        }
    }
}

/// Hides the Slint window from the Windows taskbar immediately if its native window is available.
pub fn hide_from_taskbar(window: &slint::Window) -> bool {
    #[cfg(all(windows, feature = "slint-chrome"))]
    {
        use i_slint_backend_winit::WinitWindowAccessor;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        window
            .with_winit_window(|w| {
                if let Ok(handle) = w.window_handle() {
                    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                        let hwnd = win32.hwnd.get() as isize as windows_sys::Win32::Foundation::HWND;
                        hide_hwnd_from_taskbar(hwnd);
                        return true;
                    }
                }
                false
            })
            .unwrap_or(false)
    }

    #[cfg(not(all(windows, feature = "slint-chrome")))]
    {
        let _ = window;
        false
    }
}

/// Configures Windows taskbar exclusion for the given Slint window.
///
/// On Windows, sets `WS_EX_TOOLWINDOW` and clears `WS_EX_APPWINDOW` on the underlying `HWND`
/// and deletes the taskbar button using `ITaskbarList::DeleteTab`, matching the Linux behavior.
/// Can be disabled by setting the `XTOOLS_SHOW_IN_TASKBAR=1` environment variable.
pub fn setup_taskbar_exclusion<C: slint::ComponentHandle + 'static>(window: slint::Weak<C>) {
    if std::env::var_os("XTOOLS_SHOW_IN_TASKBAR").is_some() {
        return;
    }

    #[cfg(all(windows, feature = "slint-chrome"))]
    {
        if let Some(ui) = window.upgrade() {
            if hide_from_taskbar(ui.window()) {
                return;
            }
        }

        fn poll_taskbar_exclusion<C: slint::ComponentHandle + 'static>(
            window: slint::Weak<C>,
            attempts_left: u32,
        ) {
            if attempts_left == 0 {
                return;
            }
            slint::Timer::single_shot(Duration::from_millis(20), move || {
                let Some(ui) = window.upgrade() else {
                    return;
                };
                if !hide_from_taskbar(ui.window()) {
                    poll_taskbar_exclusion(window, attempts_left - 1);
                }
            });
        }

        poll_taskbar_exclusion(window, 50);
    }

    #[cfg(not(all(windows, feature = "slint-chrome")))]
    {
        let _ = window;
    }
}


/// Start a timer that automatically exits the process when the window has lost
/// focus continuously for `timeout`.
///
/// If the window regains focus, the mouse is hovered over the window, or `is_busy()` returns true,
/// the countdown is reset.
#[cfg(all(windows, feature = "slint-chrome"))]
pub fn setup_focus_loss_timer<C, B>(
    window: slint::Weak<C>,
    timeout: Duration,
    mut is_busy: B,
) -> slint::Timer
where
    C: slint::ComponentHandle + 'static,
    B: FnMut(&C) -> bool + 'static,
{
    if std::env::var_os("XTOOLS_NO_AUTOCLOSE").is_some() {
        return slint::Timer::default();
    }

    use std::cell::RefCell;
    use std::rc::Rc;
    use i_slint_backend_winit::WinitWindowAccessor;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let timer = slint::Timer::default();
    let tracker = Rc::new(RefCell::new(FocusLossTracker::new(timeout)));
    let cached_hwnd = Rc::new(RefCell::new(Option::<windows_sys::Win32::Foundation::HWND>::None));

    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            let Some(ui) = window.upgrade() else {
                return;
            };

            // If the application is busy (e.g. AI generating, translation in progress), keep alive
            if is_busy(&ui) {
                tracker.borrow_mut().tick(true, std::time::Instant::now());
                return;
            }

            let mut hwnd = *cached_hwnd.borrow();
            if hwnd.is_none() {
                ui.window().with_winit_window(|w| {
                    if let Ok(handle) = w.window_handle() {
                        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                            let h = win32.hwnd.get() as isize as windows_sys::Win32::Foundation::HWND;
                            hwnd = Some(h);
                            hide_hwnd_from_taskbar(h);
                        }
                    }
                });
                *cached_hwnd.borrow_mut() = hwnd;
            }

            let engaged = is_window_engaged(hwnd);
            let should_exit = tracker.borrow_mut().tick(engaged, std::time::Instant::now());
            if should_exit {
                log::info!("xtools: window lost focus for {:?}, auto-exiting", timeout);
                let _ = ui.hide();
                std::process::exit(0);
            }
        },
    );

    timer
}

/// Start a timer that automatically exits the process when the window has lost
/// focus or the mouse has left continuously for `timeout`.
///
/// If the window regains focus, the mouse is hovered over the window, or `is_busy()` returns true,
/// the countdown is reset.
#[cfg(all(unix, feature = "slint-chrome"))]
pub fn setup_focus_loss_timer<C, B>(
    window: slint::Weak<C>,
    timeout: Duration,
    mut is_busy: B,
) -> slint::Timer
where
    C: slint::ComponentHandle + 'static,
    B: FnMut(&C) -> bool + 'static,
{
    if std::env::var_os("XTOOLS_NO_AUTOCLOSE").is_some() {
        return slint::Timer::default();
    }

    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use i_slint_backend_winit::{EventResult, WinitWindowAccessor};

    let timer = slint::Timer::default();
    let tracker = Rc::new(RefCell::new(FocusLossTracker::with_startup_grace(
        timeout,
        Duration::from_secs(3),
        std::time::Instant::now(),
    )));

    let mouse_inside = Rc::new(Cell::new(false));
    let has_focus = Rc::new(Cell::new(false));
    let last_input_at = Rc::new(Cell::new(std::time::Instant::now()));

    // Attach event filter to track pointer enter/leave and focus events from winit (Wayland / X11)
    if let Some(ui) = window.upgrade() {
        let mi = mouse_inside.clone();
        let hf = has_focus.clone();
        let lia = last_input_at.clone();

        ui.window().on_winit_window_event(move |_window, event| {
            match event {
                i_slint_backend_winit::winit::event::WindowEvent::CursorEntered { .. } => {
                    mi.set(true);
                }
                i_slint_backend_winit::winit::event::WindowEvent::CursorMoved { .. } => {
                    mi.set(true);
                }
                i_slint_backend_winit::winit::event::WindowEvent::CursorLeft { .. } => {
                    mi.set(false);
                }
                i_slint_backend_winit::winit::event::WindowEvent::Focused(focused) => {
                    hf.set(*focused);
                    if *focused {
                        lia.set(std::time::Instant::now());
                    }
                }
                i_slint_backend_winit::winit::event::WindowEvent::KeyboardInput { .. } => {
                    lia.set(std::time::Instant::now());
                }
                i_slint_backend_winit::winit::event::WindowEvent::MouseInput { .. } => {
                    mi.set(true);
                    lia.set(std::time::Instant::now());
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            let Some(ui) = window.upgrade() else {
                return;
            };

            // If the application is busy (e.g. AI generating, translation in progress), keep alive
            if is_busy(&ui) {
                tracker.borrow_mut().tick(true, std::time::Instant::now());
                return;
            }

            // A window is considered engaged if:
            // 1. The cursor is currently inside the window (Wayland pointer enter/leave).
            // 2. OR the window has focus and user interacted recently (within 5 seconds).
            let is_inside = mouse_inside.get();
            let recent_input =
                has_focus.get() && last_input_at.get().elapsed() < Duration::from_secs(5);
            let engaged = is_inside || recent_input;

            let should_exit = tracker.borrow_mut().tick(engaged, std::time::Instant::now());
            if should_exit {
                log::info!(
                    "xtools: window lost focus / mouse left for {:?}, auto-exiting",
                    timeout
                );
                let _ = ui.hide();
                std::process::exit(0);
            }
        },
    );

    timer
}

#[cfg(not(any(
    all(windows, feature = "slint-chrome"),
    all(unix, feature = "slint-chrome")
)))]
pub fn setup_focus_loss_timer<C, B>(
    _window: slint::Weak<C>,
    _timeout: Duration,
    _is_busy: B,
) -> slint::Timer
where
    C: slint::ComponentHandle + 'static,
    B: FnMut(&C) -> bool + 'static,
{
    slint::Timer::default()
}

/// Alias for `setup_focus_loss_timer`, making the mouse leave detection explicit.
pub fn setup_mouse_leave_timer<C, B>(
    window: slint::Weak<C>,
    timeout: Duration,
    is_busy: B,
) -> slint::Timer
where
    C: slint::ComponentHandle + 'static,
    B: FnMut(&C) -> bool + 'static,
{
    setup_focus_loss_timer(window, timeout, is_busy)
}

pub fn setup_focus_loss_timer_simple<C>(
    window: slint::Weak<C>,
    timeout: Duration,
) -> slint::Timer
where
    C: slint::ComponentHandle + 'static,
{
    setup_focus_loss_timer(window, timeout, |_| false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_drag_and_focus() {
        slint::slint! {
            export component TestWindow inherits Window {
                width: 100px;
                height: 100px;
                callback focus-lost();
                forward-focus: fs;
                fs := FocusScope {
                    changed has-focus => {
                        if (!self.has-focus) {
                            root.focus-lost();
                        }
                    }
                    Text { text: "Hello"; }
                }
            }
        }
        let Ok(Ok(win)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(TestWindow::new))
        else {
            // Headless CI without display server or missing xkbcommon libs: skip GUI window test
            return;
        };
        let drag = WindowDragState::new();
        drag.on_drag_started(win.window());
        drag.on_dragged(win.window(), 10.0, 20.0);

        let resize = WindowResizeState::new();
        resize.on_resize_started(win.window(), Some(ResizeEdge::SouthEast));
        resize.on_resized(win.window(), 50.0, 60.0, 50, 50);
        let exp = resize.toggle_expand(win.window(), 100, 100, 200, 200);
        assert!(exp);
        let restored = resize.toggle_expand(win.window(), 100, 100, 200, 200);
        assert!(!restored);
        let exp2 = resize.toggle_expand(win.window(), 100, 100, 200, 200);
        assert!(exp2);
        let restored2 = resize.toggle_expand(win.window(), 100, 100, 200, 200);
        assert!(!restored2);
    }

    #[test]
    fn test_winit_window_event_filter() {
        use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
        use std::sync::atomic::{AtomicBool, Ordering};

        slint::slint! {
            export component TestFilterWindow inherits Window {
                width: 100px;
                height: 100px;
            }
        }
        let Ok(Ok(win)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(TestFilterWindow::new))
        else {
            return;
        };
        let mouse_inside = Arc::new(AtomicBool::new(false));
        let mi = mouse_inside.clone();
        win.window().on_winit_window_event(move |_window, event| {
            match event {
                i_slint_backend_winit::winit::event::WindowEvent::CursorEntered { .. } => {
                    mi.store(true, Ordering::SeqCst);
                }
                i_slint_backend_winit::winit::event::WindowEvent::CursorLeft { .. } => {
                    mi.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    #[test]
    fn test_setup_mouse_leave_timer() {
        slint::slint! {
            export component TestLeaveWindow inherits Window {
                width: 100px;
                height: 100px;
            }
        }
        let Ok(Ok(win)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(TestLeaveWindow::new))
        else {
            return;
        };
        let _timer = setup_mouse_leave_timer(win.as_weak(), Duration::from_secs(10), |_| false);
        // Also verify XTOOLS_NO_AUTOCLOSE
        unsafe {
            std::env::set_var("XTOOLS_NO_AUTOCLOSE", "1");
        }
        let _timer_disabled = setup_focus_loss_timer(win.as_weak(), Duration::from_secs(10), |_| false);
        unsafe {
            std::env::remove_var("XTOOLS_NO_AUTOCLOSE");
        }
    }

    #[test]
    fn test_focus_loss_tracker_stays_alive_while_engaged() {
        let t0 = std::time::Instant::now();
        let mut tracker = FocusLossTracker::with_startup_grace(
            Duration::from_secs(10),
            Duration::from_secs(3),
            t0,
        );

        // Kept engaged
        assert!(!tracker.tick(true, t0));
        assert!(!tracker.is_counting_down());

        let t1 = t0 + Duration::from_secs(15);
        assert!(!tracker.tick(true, t1));
        assert!(!tracker.is_counting_down());
    }

    #[test]
    fn test_focus_loss_tracker_startup_grace() {
        let t0 = std::time::Instant::now();
        let mut tracker = FocusLossTracker::with_startup_grace(
            Duration::from_secs(10),
            Duration::from_secs(3),
            t0,
        );

        // Not engaged at start (window launching)
        let t_grace = t0 + Duration::from_secs(2);
        assert!(!tracker.tick(false, t_grace));
        assert!(!tracker.is_counting_down());

        // Focus gained during grace
        let t_focus = t0 + Duration::from_secs(2);
        assert!(!tracker.tick(true, t_focus));

        // Lost focus right after
        let t_lost = t0 + Duration::from_secs(4);
        assert!(!tracker.tick(false, t_lost));
        assert!(tracker.is_counting_down());

        // After 9.9s of losing focus -> does not exit yet
        assert!(!tracker.tick(false, t_lost + Duration::from_millis(9900)));

        // After 10.0s of losing focus -> exits!
        assert!(tracker.tick(false, t_lost + Duration::from_secs(10)));
    }

    #[test]
    fn test_focus_loss_tracker_resets_on_refocus() {
        let t0 = std::time::Instant::now();
        let mut tracker = FocusLossTracker::with_startup_grace(
            Duration::from_secs(10),
            Duration::from_secs(3),
            t0,
        );

        // Initially focused
        assert!(!tracker.tick(true, t0));

        // Lost focus at 1s
        let t1 = t0 + Duration::from_secs(1);
        assert!(!tracker.tick(false, t1));
        assert!(tracker.is_counting_down());

        // 8 seconds pass without focus (total lost 8s)
        let t8 = t1 + Duration::from_secs(8);
        assert!(!tracker.tick(false, t8));

        // User clicks back at 8.5s -> focus regained!
        let t_refocus = t1 + Duration::from_millis(8500);
        assert!(!tracker.tick(true, t_refocus));
        assert!(!tracker.is_counting_down());

        // Lost focus again at 9s
        let t_lost2 = t1 + Duration::from_secs(9);
        assert!(!tracker.tick(false, t_lost2));
        assert!(tracker.is_counting_down());

        // Another 8 seconds pass (8s since t_lost2) -> should NOT exit because countdown was reset
        assert!(!tracker.tick(false, t_lost2 + Duration::from_secs(8)));

        // Full 10s elapsed since t_lost2 -> now it exits!
        assert!(tracker.tick(false, t_lost2 + Duration::from_secs(10)));
    }

    #[test]
    fn test_taskbar_exclusion_safe() {
        slint::slint! {
            export component TestExclusionWindow inherits Window {
                width: 100px;
                height: 100px;
            }
        }
        let Ok(Ok(win)) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(TestExclusionWindow::new))
        else {
            return;
        };
        let _ = hide_from_taskbar(win.window());
        setup_taskbar_exclusion(win.as_weak());
    }
}
