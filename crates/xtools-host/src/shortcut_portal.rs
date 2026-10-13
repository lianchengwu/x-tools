//! Wayland 全局快捷键。只在宿主进程里建门户会话，设置进程不调用这里。



use futures_util::StreamExt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::shortcuts::{Accel, ShortcutBinding};

pub fn should_start_portal(session_type: &str, enabled_count: usize) -> bool {
    session_type == "wayland" && enabled_count > 0
}

pub fn apply_portal_trigger(
    current: &[ShortcutBinding],
    plugin_id: &str,
    trigger: &str,
) -> Vec<ShortcutBinding> {
    let Ok(accel) = Accel::parse_xdg(trigger).or_else(|_| Accel::parse(trigger)) else {
        log::warn!("门户回写的快捷键无法解析，保持原组合键: {plugin_id} {trigger}");
        return current.to_vec();
    };
    let canonical = accel.format_canonical();
    current
        .iter()
        .map(|binding| {
            if binding.plugin_id == plugin_id && binding.enabled {
                ShortcutBinding {
                    accel: canonical.clone(),
                    ..binding.clone()
                }
            } else {
                binding.clone()
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct PortalBind {
    pub plugin_id: String,
    pub description: String,
    pub accel: String,
}

pub struct PortalShortcutController {
    tx: UnboundedSender<Vec<PortalBind>>,
}

impl PortalShortcutController {
    pub fn start() -> Self {
        let (tx, rx) = unbounded_channel();
        std::thread::Builder::new()
            .name("xtools-shortcuts".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        log::warn!("快捷键运行时创建失败: {err}");
                        return;
                    }
                };
                runtime.block_on(portal_loop(rx));
            })
            .ok();
        Self { tx }
    }

    pub fn sync_bindings(&self, bindings: Vec<PortalBind>) {
        let _ = self.tx.send(bindings);
    }
}

async fn portal_loop(mut rx: UnboundedReceiver<Vec<PortalBind>>) {
    let proxy = match ashpd::desktop::global_shortcuts::GlobalShortcuts::new().await {
        Ok(proxy) => proxy,
        Err(err) => {
            log::warn!("全局快捷键门户不可用: {err}");
            return;
        }
    };
    let mut activated = match proxy.receive_activated().await {
        Ok(stream) => stream,
        Err(err) => {
            log::warn!("无法订阅快捷键激活: {err}");
            return;
        }
    };
    let mut changed = match proxy.receive_shortcuts_changed().await {
        Ok(stream) => stream,
        Err(err) => {
            log::warn!("无法订阅快捷键变更: {err}");
            return;
        }
    };
    let mut session: Option<
        ashpd::desktop::Session<'_, ashpd::desktop::global_shortcuts::GlobalShortcuts<'_>>,
    > = None;
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(binds) = cmd else { break };
                if let Some(old) = session.take() {
                    if let Err(err) = old.close().await {
                        log::warn!("关闭旧快捷键会话失败: {err}");
                    }
                }
                if binds.is_empty() {
                    continue;
                }
                match bind_once(&proxy, &binds).await {
                    Ok(next) => session = Some(next),
                    Err(err) => log::warn!("注册全局快捷键失败: {err}"),
                }
            }
            event = activated.next() => {
                let Some(event) = event else { continue };
                let token = activation_token(&event);
                let id = event.shortcut_id().to_string();
                gtk4::glib::idle_add(move || {
                    crate::unix::open_plugin_by_id(&id, token.as_deref());
                    gtk4::glib::ControlFlow::Break
                });
            }
            event = changed.next() => {
                let Some(event) = event else { continue };
                let updates: Vec<(String, String)> = event
                    .shortcuts()
                    .iter()
                    .map(|shortcut| {
                        (
                            shortcut.id().to_string(),
                            shortcut.trigger_description().to_string(),
                        )
                    })
                    .collect();
                gtk4::glib::idle_add(move || {
                    crate::unix::apply_shortcut_portal_updates(&updates);
                    gtk4::glib::ControlFlow::Break
                });
            }
        }
    }
}

async fn bind_once<'a>(
    proxy: &'a ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>,
    binds: &[PortalBind],
) -> Result<
    ashpd::desktop::Session<'a, ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>>,
    String,
> {
    let session = proxy
        .create_session()
        .await
        .map_err(|err| err.to_string())?;
    let mut triggers = Vec::new();
    let mut shortcuts = Vec::new();
    for bind in binds {
        let Ok(accel) = Accel::parse(&bind.accel) else {
            log::warn!("跳过无法交给门户的快捷键 {}", bind.plugin_id);
            continue;
        };
        triggers.push(accel.to_xdg_trigger());
        let trigger = triggers.last().expect("just pushed");
        shortcuts.push(
            ashpd::desktop::global_shortcuts::NewShortcut::new(&bind.plugin_id, &bind.description)
                .preferred_trigger(Some(trigger.as_str())),
        );
    }
    if shortcuts.is_empty() {
        let _ = session.close().await;
        return Err("没有可注册的快捷键".into());
    }
    let request = proxy
        .bind_shortcuts(&session, &shortcuts, None)
        .await
        .map_err(|err| err.to_string())?;
    request.response().map_err(|err| err.to_string())?;
    Ok(session)
}

fn activation_token(event: &ashpd::desktop::global_shortcuts::Activated) -> Option<String> {
    let value = event.options().get("activation_token")?;
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.as_str().map(str::to_string))
}

#[cfg(test)]
mod portal_tests {
    use super::*;
    use crate::shortcuts::{ShortcutBinding, bindings_equal};

    #[test]
    fn test_portal_triggers_update_does_not_loop_if_identical() {
        let current = vec![ShortcutBinding {
            plugin_id: "xtools.trans".into(),
            accel: "Ctrl+Alt+T".into(),
            enabled: true,
        }];
        let updated = apply_portal_trigger(&current, "xtools.trans", "CTRL+ALT+t");
        assert!(bindings_equal(&current, &updated));
    }

    #[test]
    fn test_portal_triggers_update_modifies_accel_only() {
        let current = vec![ShortcutBinding {
            plugin_id: "xtools.trans".into(),
            accel: "Ctrl+Alt+T".into(),
            enabled: true,
        }];
        let updated = apply_portal_trigger(&current, "xtools.trans", "CTRL+ALT+g");
        assert_eq!(updated[0].accel, "Ctrl+Alt+G");
        assert!(updated[0].enabled);
    }

    #[test]
    fn test_unparseable_trigger_keeps_accel() {
        let current = vec![ShortcutBinding {
            plugin_id: "xtools.trans".into(),
            accel: "Ctrl+Alt+T".into(),
            enabled: true,
        }];
        let updated = apply_portal_trigger(&current, "xtools.trans", "按 Ctrl 和 Alt");
        assert!(bindings_equal(&current, &updated));
    }

    #[test]
    fn test_portal_starts_only_for_wayland_with_enabled_bindings() {
        assert!(!should_start_portal("x11", 2));
        assert!(!should_start_portal("wayland", 0));
        assert!(should_start_portal("wayland", 1));
    }
}
