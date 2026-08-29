use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Icon, ToolTip, Tray};

pub struct XtoolsTray {
    open: Arc<AtomicBool>,
}

impl Tray for XtoolsTray {
    fn id(&self) -> String {
        "dev.xtools.host.wasm".into()
    }

    fn title(&self) -> String {
        "xtools (WASM)".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "xtools (WASM)".into(),
            description: "桌面悬浮工具箱 (WASM 插件架构)".into(),
            icon_name: "xtools".into(),
            icon_pixmap: Vec::new(),
        }
    }

    fn icon_name(&self) -> String {
        "xtools".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        Vec::new()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let is_open = self.open.load(Ordering::Relaxed);
        let toggle_label = if is_open { "收起悬浮球" } else { "展开悬浮球" };
        let open_flag = self.open.clone();

        vec![
            StandardItem {
                label: toggle_label.into(),
                activate: Box::new(move |_| {
                    let prev = open_flag.load(Ordering::Relaxed);
                    open_flag.store(!prev, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出 xtools".into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn spawn_tray(open_flag: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("xtools-tray".into())
        .spawn(move || {
            let tray = XtoolsTray { open: open_flag };
            if let Ok(handle) = tray.assume_sni_available(true).spawn() {
                std::mem::forget(handle);
            }
        })
        .ok();
}
