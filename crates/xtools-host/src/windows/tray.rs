//! Windows system tray (Notification Area) integration via Shell_NotifyIconW.

use std::mem::zeroed;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIcon, CreatePopupMenu, DestroyIcon, DestroyMenu,
    GetCursorPos, HICON, HMENU, MF_SEPARATOR, MF_STRING, SetForegroundWindow,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu,
};

pub const WM_TRAY_CALLBACK: u32 = 0x8000 + 100; // WM_APP + 100
pub const ID_TRAY_SHOW_HIDE: usize = 1001;
pub const ID_TRAY_SETTINGS: usize = 1002;
pub const ID_TRAY_QUIT: usize = 1003;

pub struct TrayIcon {
    nid: NOTIFYICONDATAW,
    hicon: HICON,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Self {
        let hicon = create_default_icon();

        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = hicon;

        let tip = "xtools\0".encode_utf16().collect::<Vec<u16>>();
        let copy_len = tip.len().min(nid.szTip.len());
        nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);

        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
        }

        Self { nid, hicon }
    }

    pub fn show_menu(&self, hwnd: HWND, is_visible: bool) {
        unsafe {
            let hmenu: HMENU = CreatePopupMenu();
            let show_text = if is_visible {
                "隐藏悬浮球\0"
            } else {
                "显示悬浮球\0"
            };
            let show_w: Vec<u16> = show_text.encode_utf16().collect();
            let settings_w: Vec<u16> = "设置\0".encode_utf16().collect();
            let quit_w: Vec<u16> = "退出 xtools\0".encode_utf16().collect();

            AppendMenuW(hmenu, MF_STRING, ID_TRAY_SHOW_HIDE, show_w.as_ptr());
            AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(hmenu, MF_STRING, ID_TRAY_SETTINGS, settings_w.as_ptr());
            AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(hmenu, MF_STRING, ID_TRAY_QUIT, quit_w.as_ptr());

            let mut pt: POINT = zeroed();
            GetCursorPos(&mut pt);

            SetForegroundWindow(hwnd);
            TrackPopupMenu(
                hmenu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            DestroyMenu(hmenu);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &self.nid);
            if !self.hicon.is_null() {
                DestroyIcon(self.hicon);
            }
        }
    }
}

/// Create a 32x32 clean circle icon for the tray.
fn create_default_icon() -> HICON {
    const SIZE: usize = 32;
    let mut and_mask = [0xFFu8; (SIZE * SIZE) / 8];
    let mut xor_mask = [0u8; SIZE * SIZE * 4];

    let cx = 15.5f64;
    let cy = 15.5f64;
    let r = 13.0f64;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= r {
                let mask_idx = (y * SIZE + x) / 8;
                let bit_idx = 7 - ((y * SIZE + x) % 8);
                and_mask[mask_idx] &= !(1 << bit_idx);

                let pixel_idx = (y * SIZE + x) * 4;
                // Dark circular icon with white outline
                if (dist - r).abs() < 1.5 {
                    xor_mask[pixel_idx] = 0xFF;     // B
                    xor_mask[pixel_idx + 1] = 0xFF; // G
                    xor_mask[pixel_idx + 2] = 0xFF; // R
                } else {
                    xor_mask[pixel_idx] = 0x26;     // B
                    xor_mask[pixel_idx + 1] = 0x1E; // G
                    xor_mask[pixel_idx + 2] = 0x1C; // R
                }
                xor_mask[pixel_idx + 3] = 0xFF; // A
            }
        }
    }

    unsafe {
        CreateIcon(
            std::ptr::null_mut(),
            SIZE as i32,
            SIZE as i32,
            1,
            32,
            and_mask.as_ptr(),
            xor_mask.as_ptr(),
        )
    }
}
