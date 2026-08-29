//! KWin Wayland helpers.
//!
//! Plasma ignores X11 sticky hints. A child window spawned from xtools-host is
//! assigned to the host process's birth desktop and then activated, which
//! switches the view. Fix: a long-lived KWin script pins new tool windows to
//! every desktop on `windowAdded`, and we restore the user's desktop via DBus.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const SCRIPT_NAME: &str = "xtools-desktop-pin";

const PIN_SCRIPT: &str = r#"
function isTool(w) {
    var cls = "";
    try { cls = String(w.resourceClass); } catch (e) { return false; }
    if (cls === "xtools-host" || cls === "xtools-host-wasm") return false;
    return cls.indexOf("xtools") !== -1;
}
function pin(w) {
    if (!isTool(w)) return;
    try { w.skipTaskbar = true; } catch (e) {}
    try { w.skipPager = true; } catch (e) {}
    try { w.onAllDesktops = true; } catch (e) {}
}
workspace.windowAdded.connect(pin);
var wins = workspace.windowList();
for (var i = 0; i < wins.length; i++) {
    pin(wins[i]);
}
"#;

static SCRIPT_READY: AtomicBool = AtomicBool::new(false);

/// Current Plasma virtual-desktop UUID, if this is a KWin session.
pub fn current_desktop() -> Option<String> {
    let out = Command::new("busctl")
        .args([
            "--user",
            "get-property",
            "org.kde.KWin",
            "/VirtualDesktopManager",
            "org.kde.KWin.VirtualDesktopManager",
            "current",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_busctl_string(&String::from_utf8_lossy(&out.stdout))
}

/// Switch KWin back to `id` (virtual-desktop UUID).
pub fn restore_desktop(id: &str) -> bool {
    if !valid_desktop_id(id) {
        return false;
    }
    Command::new("busctl")
        .args([
            "--user",
            "set-property",
            "org.kde.KWin",
            "/VirtualDesktopManager",
            "org.kde.KWin.VirtualDesktopManager",
            "current",
            "s",
            id,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Install the long-lived pin script once. Safe to call from host startup.
pub fn ensure_pin_script() {
    if SCRIPT_READY.load(Ordering::Relaxed) {
        return;
    }
    if script_loaded() {
        let _ = Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.kde.KWin",
                "/Scripting",
                "org.kde.kwin.Scripting",
                "unloadScript",
                "s",
                SCRIPT_NAME,
            ])
            .status();
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let path = dir.join("xtools-desktop-pin.js");
    if fs::write(&path, PIN_SCRIPT).is_err() {
        return;
    }
    let loaded = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
            "loadScript",
            "ss",
        ])
        .arg(&path)
        .arg(SCRIPT_NAME)
        .output();
    let ok = loaded
        .as_ref()
        .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("-1"))
        .unwrap_or(false);
    let _ = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
            "start",
        ])
        .status();
    if ok {
        SCRIPT_READY.store(true, Ordering::Relaxed);
    }
}

/// Pin this pid's windows (one-shot) and optionally restore a desktop.
pub fn pin_pid(pid: u32, restore: Option<&str>) {
    let _ = pid;
    ensure_pin_script();
    if let Some(id) = restore {
        let _ = restore_desktop(id);
    }
}

/// Pin this process and restore the captured desktop.
pub fn pin_self(restore: Option<&str>) {
    pin_pid(std::process::id(), restore);
}

fn script_loaded() -> bool {
    let out = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
            "isScriptLoaded",
            "s",
            SCRIPT_NAME,
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).contains("true"),
        _ => false,
    }
}

fn valid_desktop_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 80 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

fn parse_busctl_string(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(start) = raw.find('"') {
        let rest = &raw[start + 1..];
        let end = rest.find('"')?;
        let s = &rest[..end];
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_busctl_string, valid_desktop_id};

    #[test]
    fn parse_quoted_uuid() {
        assert_eq!(
            parse_busctl_string(r#"s "61f09d5f-1d87-4390-858d-b51dd1d34658""#).as_deref(),
            Some("61f09d5f-1d87-4390-858d-b51dd1d34658")
        );
    }

    #[test]
    fn rejects_bad_desktop_id() {
        assert!(!valid_desktop_id(""));
        assert!(!valid_desktop_id("foo;rm -rf /"));
        assert!(valid_desktop_id("61f09d5f-1d87-4390-858d-b51dd1d34658"));
    }
}
