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
    is_native_resize: Arc<Mutex<bool>>,
}

impl WindowResizeState {
    pub fn new() -> Self {
        Self {
            start_size: Arc::new(Mutex::new(None)),
            saved_size: Arc::new(Mutex::new(None)),
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
    pub fn toggle_expand(
        &self,
        window: &slint::Window,
        normal_w: u32,
        normal_h: u32,
        expanded_w: u32,
        expanded_h: u32,
    ) -> bool {
        let current_size = window.size();
        let scale = window.scale_factor();
        let normal_phys_w = (normal_w as f32 * scale).round() as u32;
        let normal_phys_h = (normal_h as f32 * scale).round() as u32;
        let expanded_phys_w = (expanded_w as f32 * scale).round() as u32;
        let expanded_phys_h = (expanded_h as f32 * scale).round() as u32;

        let mut saved = self.saved_size.lock();
        let tolerance = (20.0 * scale).round() as u32;
        if current_size.width >= (expanded_phys_w.saturating_sub(tolerance))
            && current_size.height >= (expanded_phys_h.saturating_sub(tolerance))
        {
            let target = saved
                .take()
                .unwrap_or(slint::PhysicalSize::new(normal_phys_w, normal_phys_h));
            window.set_size(target);
            false
        } else {
            *saved = Some(current_size);
            window.set_size(slint::PhysicalSize::new(expanded_phys_w, expanded_phys_h));
            true
        }
    }
}

/// Start a timer that polls the instance lock and handles raise or quit commands.
pub fn setup_raise_timer(
    listener: crate::InstanceListener,
    window: slint::Weak<impl slint::ComponentHandle + 'static>,
) -> slint::Timer {
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(50),
        move || match crate::instance::accept_command(&listener) {
            Some(crate::instance::InstanceCommand::Quit) => {
                std::process::exit(0);
            }
            Some(crate::instance::InstanceCommand::Raise(_token)) => {
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
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
            None => {}
        },
    );
    timer
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
    }
}
