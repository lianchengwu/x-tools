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
        if !native_handled {
            let pos = window.position();
            *self.start_pos.lock() = Some(pos);
        } else {
            *self.start_pos.lock() = None;
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
        {
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
        if !native_handled {
            let size = window.size();
            *self.start_size.lock() = Some(size);
        } else {
            *self.start_size.lock() = None;
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
                    #[cfg(feature = "x11-skip-taskbar")]
                    crate::skip_taskbar::raise_x11_window();
                }
            }
            None => {}
        },
    );
    timer
}

/// Start a timer that applies X11 skip taskbar for the first few ticks.
#[cfg(feature = "x11-skip-taskbar")]
pub fn setup_skip_taskbar_timer() -> slint::Timer {
    let timer = slint::Timer::default();
    let tries = Arc::new(Mutex::new(0u8));
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(80),
        move || {
            let mut count = tries.lock();
            if *count < 12 {
                crate::skip_taskbar::apply();
                *count += 1;
            }
        },
    );
    timer
}

/// Consecutive failing ticks (60 ms each) required before auto-exit fires.
/// Guards against the transient `_NET_ACTIVE_WINDOW` flapping that XWayland
/// windows exhibit on Plasma Wayland.
const FOCUS_LOSS_EXIT_TICKS: u8 = 10;

/// Window-opening grace: during the first seconds after the first engagement,
/// focus flaps are ignored entirely (the user may still be moving the pointer
/// towards the freshly opened window).
const STARTUP_FOCUS_GRACE: Duration = Duration::from_secs(5);

/// Start a timer that automatically exits the process once the user has moved
/// on from the tool window: focus left and the pointer is elsewhere, sustained
/// for `FOCUS_LOSS_EXIT_TICKS`. Hovering or interacting with the window keeps
/// it alive even when the compositor reports the focus elsewhere.
#[cfg(feature = "x11-skip-taskbar")]
pub fn setup_auto_exit_on_focus_loss_timer() -> slint::Timer {
    let timer = slint::Timer::default();
    let was_engaged = Arc::new(Mutex::new(false));
    let initial_ticks = Arc::new(Mutex::new(0u8));
    let loss_ticks = Arc::new(Mutex::new(0u8));
    let first_engaged = Arc::new(Mutex::new(Option::<std::time::Instant>::None));
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(60),
        move || {
            let engaged = crate::skip_taskbar::is_process_window_engaged();
            let was = *was_engaged.lock();
            let mut ticks = initial_ticks.lock();
            if *ticks < 4 {
                *ticks += 1;
                if engaged == Some(true) {
                    *was_engaged.lock() = true;
                }
                return;
            }

            match engaged {
                Some(true) => {
                    let mut first = first_engaged.lock();
                    if first.is_none() {
                        log::debug!("auto-exit: tool window engaged");
                        *first = Some(std::time::Instant::now());
                    }
                    *was_engaged.lock() = true;
                    *loss_ticks.lock() = 0;
                }
                Some(false) if was => {
                    let in_startup_grace = first_engaged
                        .lock()
                        .is_some_and(|t| t.elapsed() < STARTUP_FOCUS_GRACE);
                    if in_startup_grace {
                        *loss_ticks.lock() = 0;
                        return;
                    }
                    let mut loss = loss_ticks.lock();
                    *loss += 1;
                    if *loss >= FOCUS_LOSS_EXIT_TICKS {
                        log::info!(
                            "auto-exit: focus lost for {} ms, exiting",
                            u16::from(FOCUS_LOSS_EXIT_TICKS) * 60
                        );
                        std::process::exit(0);
                    }
                }
                _ => {
                    *loss_ticks.lock() = 0;
                }
            }
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
