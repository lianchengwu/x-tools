//! Hide this process's X11 windows from the pager/taskbar, make them sticky across all
//! virtual desktops, raise them, and check focus.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, InputFocus,
    PropMode, StackMode, Time,
};
use x11rb::wrapper::ConnectionExt as _;

const NET_WM_STATE_ADD: u32 = 1;
const ALL_DESKTOPS: u32 = 0xFFFF_FFFF;

pub fn apply() {
    crate::kwin::pin_self(crate::boot::target_desktop().as_deref());
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return;
    };
    let screen = &conn.setup().roots[screen_num];
    let pid = std::process::id();

    let Ok(client_list) = intern(&conn, b"_NET_CLIENT_LIST") else {
        return;
    };
    let Ok(wm_pid) = intern(&conn, b"_NET_WM_PID") else {
        return;
    };
    let Ok(wm_state) = intern(&conn, b"_NET_WM_STATE") else {
        return;
    };
    let Ok(skip_taskbar) = intern(&conn, b"_NET_WM_STATE_SKIP_TASKBAR") else {
        return;
    };
    let Ok(skip_pager) = intern(&conn, b"_NET_WM_STATE_SKIP_PAGER") else {
        return;
    };
    let Ok(sticky) = intern(&conn, b"_NET_WM_STATE_STICKY") else {
        return;
    };
    let wm_desktop = intern(&conn, b"_NET_WM_DESKTOP").ok();

    let windows = find_process_windows(&conn, screen.root, client_list, wm_pid, pid);
    let mask = EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT;

    for win in windows {
        let _ = conn.change_property32(
            PropMode::REPLACE,
            win,
            wm_state,
            AtomEnum::ATOM,
            &[skip_taskbar, skip_pager, sticky],
        );
        if let Some(wm_desktop) = wm_desktop {
            let _ = conn.change_property32(
                PropMode::REPLACE,
                win,
                wm_desktop,
                AtomEnum::CARDINAL,
                &[ALL_DESKTOPS],
            );
            let event_desktop =
                ClientMessageEvent::new(32, win, wm_desktop, [ALL_DESKTOPS, 2, 0, 0, 0]);
            let _ = conn.send_event(false, screen.root, mask, event_desktop);
        }

        let event_skip = ClientMessageEvent::new(
            32,
            win,
            wm_state,
            [NET_WM_STATE_ADD, skip_taskbar, skip_pager, 1, 0],
        );
        let _ = conn.send_event(false, screen.root, mask, event_skip);

        let event_sticky =
            ClientMessageEvent::new(32, win, wm_state, [NET_WM_STATE_ADD, sticky, 0, 1, 0]);
        let _ = conn.send_event(false, screen.root, mask, event_sticky);
    }
    let _ = conn.flush();
}

/// Raise and focus this process's X11 windows, ensuring they are sticky on all desktops.
pub fn raise_x11_window() {
    crate::kwin::pin_self(crate::boot::target_desktop().as_deref());
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return;
    };
    let screen = &conn.setup().roots[screen_num];
    let pid = std::process::id();

    let Ok(client_list) = intern(&conn, b"_NET_CLIENT_LIST") else {
        return;
    };
    let Ok(wm_pid) = intern(&conn, b"_NET_WM_PID") else {
        return;
    };
    let Ok(active_window) = intern(&conn, b"_NET_ACTIVE_WINDOW") else {
        return;
    };
    let Ok(wm_state) = intern(&conn, b"_NET_WM_STATE") else {
        return;
    };
    let Ok(skip_taskbar) = intern(&conn, b"_NET_WM_STATE_SKIP_TASKBAR") else {
        return;
    };
    let Ok(skip_pager) = intern(&conn, b"_NET_WM_STATE_SKIP_PAGER") else {
        return;
    };
    let Ok(sticky) = intern(&conn, b"_NET_WM_STATE_STICKY") else {
        return;
    };
    let wm_desktop = intern(&conn, b"_NET_WM_DESKTOP").ok();

    let windows = find_process_windows(&conn, screen.root, client_list, wm_pid, pid);
    let mask = EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT;

    for win in windows {
        // Map if unmapped / minimized
        let _ = conn.map_window(win);

        // Ensure sticky & all desktops state
        let _ = conn.change_property32(
            PropMode::REPLACE,
            win,
            wm_state,
            AtomEnum::ATOM,
            &[skip_taskbar, skip_pager, sticky],
        );
        if let Some(wm_desktop) = wm_desktop {
            let _ = conn.change_property32(
                PropMode::REPLACE,
                win,
                wm_desktop,
                AtomEnum::CARDINAL,
                &[ALL_DESKTOPS],
            );
            let event_desktop =
                ClientMessageEvent::new(32, win, wm_desktop, [ALL_DESKTOPS, 2, 0, 0, 0]);
            let _ = conn.send_event(false, screen.root, mask, event_desktop);
        }
        let event_sticky =
            ClientMessageEvent::new(32, win, wm_state, [NET_WM_STATE_ADD, sticky, 0, 1, 0]);
        let _ = conn.send_event(false, screen.root, mask, event_sticky);

        // Raise window above all siblings
        let _ = conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));

        // Set input focus
        let _ = conn.set_input_focus(InputFocus::POINTER_ROOT, win, Time::CURRENT_TIME);

        // Send EWMH _NET_ACTIVE_WINDOW message to root window (source=2: pager/panel)
        let event = ClientMessageEvent::new(32, win, active_window, [2, 0, 0, 0, 0]);
        let _ = conn.send_event(false, screen.root, mask, event);
    }
    let _ = conn.flush();
}

/// Check if any window belonging to this process currently has active input focus in X11.
pub fn is_process_window_active() -> Option<bool> {
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];
    let pid = std::process::id();

    let active_atom = intern(&conn, b"_NET_ACTIVE_WINDOW").ok()?;
    let wm_pid_atom = intern(&conn, b"_NET_WM_PID").ok()?;

    let cookie = conn
        .get_property(false, screen.root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()?;
    let reply = cookie.reply().ok()?;
    let active_win = reply.value32().and_then(|mut v| v.next())?;

    if active_win == 0 {
        return Some(false);
    }

    Some(is_window_pid(&conn, active_win, wm_pid_atom, pid))
}

/// True when the user is visibly engaged with this process's tool window: one of
/// its X11 windows has input focus (`_NET_ACTIVE_WINDOW`) or the pointer hovers
/// inside one of them.
///
/// On Plasma Wayland the active-window property is unreliable for XWayland
/// clients: XWayland rewrites it to a pid-less 1x1 placeholder whenever Wayland
/// keyboard focus changes, which happens spontaneously even while the user is
/// interacting with the window. The pointer check keeps an in-use window from
/// being treated as abandoned.
pub fn is_process_window_engaged() -> Option<bool> {
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];
    let pid = std::process::id();

    let client_list_atom = intern(&conn, b"_NET_CLIENT_LIST").ok()?;
    let wm_pid_atom = intern(&conn, b"_NET_WM_PID").ok()?;
    let active_atom = intern(&conn, b"_NET_ACTIVE_WINDOW").ok()?;

    let windows = find_process_windows(&conn, screen.root, client_list_atom, wm_pid_atom, pid);
    if windows.is_empty() {
        return Some(false);
    }

    let active_is_ours = match conn
        .get_property(false, screen.root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()
        .and_then(|mut v| v.next())
    {
        Some(active_win) if active_win != 0 => is_window_pid(&conn, active_win, wm_pid_atom, pid),
        _ => false,
    };
    if active_is_ours {
        return Some(true);
    }

    let pointer = conn.query_pointer(screen.root).ok()?.reply().ok()?;
    for win in &windows {
        let Some(geom) = conn.get_geometry(*win).ok().and_then(|c| c.reply().ok()) else {
            continue;
        };
        let Some(origin) = conn
            .translate_coordinates(*win, screen.root, 0, 0)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            continue;
        };
        let (px, py) = (i32::from(pointer.root_x), i32::from(pointer.root_y));
        let (x0, y0) = (i32::from(origin.dst_x), i32::from(origin.dst_y));
        let inside = px >= x0
            && px < x0 + i32::from(geom.width)
            && py >= y0
            && py < y0 + i32::from(geom.height);
        if inside {
            return Some(true);
        }
    }

    Some(false)
}

fn find_process_windows(
    conn: &impl Connection,
    root: u32,
    client_list_atom: u32,
    wm_pid_atom: u32,
    target_pid: u32,
) -> Vec<u32> {
    let mut matched = Vec::new();

    // 1. Try _NET_CLIENT_LIST
    if let Ok(cookie) = conn.get_property(false, root, client_list_atom, AtomEnum::WINDOW, 0, 1024)
        && let Ok(reply) = cookie.reply()
        && let Some(windows) = reply.value32()
    {
        for win in windows {
            if is_window_pid(conn, win, wm_pid_atom, target_pid) {
                matched.push(win);
            }
        }
    }

    // 2. Fallback: inspect top-level children via query_tree if none found
    if matched.is_empty()
        && let Ok(tree_cookie) = conn.query_tree(root)
        && let Ok(tree_reply) = tree_cookie.reply()
    {
        for &win in &tree_reply.children {
            if is_window_pid(conn, win, wm_pid_atom, target_pid) {
                matched.push(win);
            }
        }
    }

    matched
}

fn is_window_pid(conn: &impl Connection, win: u32, wm_pid_atom: u32, target_pid: u32) -> bool {
    let Ok(pid_cookie) = conn.get_property(false, win, wm_pid_atom, AtomEnum::CARDINAL, 0, 1)
    else {
        return false;
    };
    let Ok(pid_reply) = pid_cookie.reply() else {
        return false;
    };
    let Some(wpid) = pid_reply.value32().and_then(|mut v| v.next()) else {
        return false;
    };
    wpid == target_pid
}

fn intern(conn: &impl Connection, name: &[u8]) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}
