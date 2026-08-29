use std::sync::OnceLock;

static TARGET_DESKTOP: OnceLock<Option<String>> = OnceLock::new();

/// Snapshot the active virtual desktop before any tool window maps.
#[cfg(unix)]
pub fn capture_target_desktop() {
    TARGET_DESKTOP.get_or_init(|| {
        std::env::var("XTOOLS_TARGET_DESKTOP")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(crate::kwin::current_desktop)
    });
}

#[cfg(windows)]
pub fn capture_target_desktop() {
    TARGET_DESKTOP.get_or_init(|| None);
}

/// Desktop UUID captured by [`capture_target_desktop`].
pub fn target_desktop() -> Option<String> {
    TARGET_DESKTOP.get().cloned().flatten()
}

#[cfg(unix)]
pub fn take_activation_token() -> Option<String> {
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    unsafe {
        std::env::remove_var("XDG_ACTIVATION_TOKEN");
        std::env::remove_var("DESKTOP_STARTUP_ID");
    }
    token
}

#[cfg(windows)]
pub fn take_activation_token() -> Option<String> {
    None
}

/// Initialize input method and locale environment for Linux.
///
/// Ensures XMODIFIERS is configured to `@im=fcitx` if unset so X11/XWayland (XIM)
/// applications connect to fcitx5 properly.
#[cfg(unix)]
pub fn init_input_method_env() {
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
    }

    if std::env::var("XMODIFIERS").map_or(true, |v| v.trim().is_empty()) {
        unsafe {
            std::env::set_var("XMODIFIERS", "@im=fcitx");
        }
    }

    if std::env::var("GTK_IM_MODULE").map_or(true, |v| v.trim().is_empty()) {
        unsafe {
            std::env::set_var("GTK_IM_MODULE", "fcitx");
        }
    }
    if std::env::var("QT_IM_MODULE").map_or(true, |v| v.trim().is_empty()) {
        unsafe {
            std::env::set_var("QT_IM_MODULE", "fcitx");
        }
    }
}

#[cfg(windows)]
pub fn init_input_method_env() {}
