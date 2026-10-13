//! Wayland 全局快捷键。只在宿主进程里建门户会话，设置进程不调用这里。
//! 执行器必须是 async-io，和托盘的 ksni/zbus 一致。启用 zbus 的 tokio 特性会让托盘线程在没有
//! Tokio runtime 时 panic。

use async_channel::{Receiver, Sender};
use futures_util::StreamExt;

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
    tx: Sender<Vec<PortalBind>>,
}

impl PortalShortcutController {
    pub fn start() -> Self {
        let (tx, rx) = async_channel::unbounded();
        std::thread::Builder::new()
            .name("xtools-shortcuts".into())
            .spawn(move || {
                async_io::block_on(portal_loop(rx));
            })
            .ok();
        Self { tx }
    }

    pub fn sync_bindings(&self, bindings: Vec<PortalBind>) -> bool {
        self.tx.try_send(bindings).is_ok()
    }
}

async fn portal_loop(rx: Receiver<Vec<PortalBind>>) {
    while let Ok(binds) = rx.recv().await {
        if binds.is_empty() {
            continue;
        }
        let proxy = match ashpd::desktop::global_shortcuts::GlobalShortcuts::new().await {
            Ok(proxy) => proxy,
            Err(err) => {
                log::warn!("全局快捷键门户不可用，下次配置变化再试: {err}");
                continue;
            }
        };
        let mut activated = match proxy.receive_activated().await {
            Ok(stream) => stream,
            Err(err) => {
                log::warn!("无法订阅快捷键激活，下次配置变化再试: {err}");
                continue;
            }
        };
        let mut changed = match proxy.receive_shortcuts_changed().await {
            Ok(stream) => stream,
            Err(err) => {
                log::warn!("无法订阅快捷键变更，下次配置变化再试: {err}");
                continue;
            }
        };
        if !run_session(&proxy, &mut activated, &mut changed, binds, &rx).await {
            return;
        }
    }
}

enum Step<A, C> {
    Cmd(Option<Vec<PortalBind>>),
    Bound,
    Activated(Option<A>),
    Changed(Option<C>),
}

async fn run_session<'a, A, C>(
    proxy: &'a ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>,
    activated: &mut A,
    changed: &mut C,
    initial: Vec<PortalBind>,
    rx: &Receiver<Vec<PortalBind>>,
) -> bool
where
    A: StreamExt<Item = ashpd::desktop::global_shortcuts::Activated> + Unpin,
    C: StreamExt<Item = ashpd::desktop::global_shortcuts::ShortcutsChanged> + Unpin,
{
    let mut pending = Some(initial);
    let mut session = None;
    loop {
        let waiting_for_cmd = pending.is_none();
        let step = poll_step(
            proxy,
            activated,
            changed,
            rx,
            &mut session,
            &mut pending,
            waiting_for_cmd,
        )
        .await;
        match step {
            Step::Cmd(None) => return false,
            Step::Cmd(Some(binds)) => {
                if let Some(old) = session.take() {
                    if let Err(err) = old.close().await {
                        log::warn!("关闭旧快捷键会话失败: {err}");
                    }
                }
                if !binds.is_empty() {
                    pending = Some(binds);
                }
            }
            Step::Bound => {}
            Step::Activated(None) => continue,
            Step::Activated(Some(event)) => {
                let token = activation_token(&event);
                let id = event.shortcut_id().to_string();
                gtk4::glib::idle_add(move || {
                    crate::unix::open_plugin_by_id(&id, token.as_deref());
                    gtk4::glib::ControlFlow::Break
                });
            }
            Step::Changed(None) => continue,
            Step::Changed(Some(event)) => {
                let updates = shortcut_updates(event.shortcuts());
                gtk4::glib::idle_add(move || {
                    crate::unix::apply_shortcut_portal_updates(&updates);
                    gtk4::glib::ControlFlow::Break
                });
            }
        }
    }
}

async fn poll_step<'a, A, C>(
    proxy: &'a ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>,
    activated: &mut A,
    changed: &mut C,
    rx: &Receiver<Vec<PortalBind>>,
    session: &mut Option<ashpd::desktop::Session<'a, ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>>>,
    pending: &mut Option<Vec<PortalBind>>,
    waiting_for_cmd: bool,
) -> Step<ashpd::desktop::global_shortcuts::Activated, ashpd::desktop::global_shortcuts::ShortcutsChanged>
where
    A: StreamExt<Item = ashpd::desktop::global_shortcuts::Activated> + Unpin,
    C: StreamExt<Item = ashpd::desktop::global_shortcuts::ShortcutsChanged> + Unpin,
{
    let cmd = async {
        if !waiting_for_cmd {
            std::future::pending().await
        } else {
            rx.recv().await.ok()
        }
    };
    let bind = async {
        if waiting_for_cmd {
            std::future::pending().await
        } else {
            bind_pending(proxy, session, pending).await;
        }
    };
    let activated_next = activated.next();
    let changed_next = changed.next();
    futures_util::pin_mut!(cmd, bind, activated_next, changed_next);
    match futures_util::future::select(
        futures_util::future::select(cmd, bind),
        futures_util::future::select(activated_next, changed_next),
    )
    .await
    {
        futures_util::future::Either::Left((futures_util::future::Either::Left((cmd, _)), _)) => {
            Step::Cmd(cmd)
        }
        futures_util::future::Either::Left((futures_util::future::Either::Right((_, _)), _)) => {
            Step::Bound
        }
        futures_util::future::Either::Right((
            futures_util::future::Either::Left((event, _)),
            _,
        )) => Step::Activated(event),
        futures_util::future::Either::Right((
            futures_util::future::Either::Right((event, _)),
            _,
        )) => Step::Changed(event),
    }
}

async fn bind_pending<'a>(
    proxy: &'a ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>,
    session: &mut Option<ashpd::desktop::Session<'a, ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>>>,
    pending: &mut Option<Vec<PortalBind>>,
) {
    let Some(binds) = pending.take() else { return };
    if let Some(old) = session.take() {
        let _ = old.close().await;
    }
    match bind_once(proxy, &binds).await {
        Ok((next, updates)) => {
            if !updates.is_empty() {
                gtk4::glib::idle_add(move || {
                    crate::unix::apply_shortcut_portal_updates(&updates);
                    gtk4::glib::ControlFlow::Break
                });
            }
            *session = Some(next);
        }
        Err(err) => log::warn!("注册全局快捷键失败: {err}"),
    }
}

fn shortcut_updates(
    shortcuts: &[ashpd::desktop::global_shortcuts::Shortcut],
) -> Vec<(String, String)> {
    shortcuts
        .iter()
        .map(|shortcut| {
            (
                shortcut.id().to_string(),
                shortcut.trigger_description().to_string(),
            )
        })
        .collect()
}

async fn bind_once<'a>(
    proxy: &'a ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>,
    binds: &[PortalBind],
) -> Result<
    (
        ashpd::desktop::Session<'a, ashpd::desktop::global_shortcuts::GlobalShortcuts<'a>>,
        Vec<(String, String)>,
    ),
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
    let bound = match proxy.bind_shortcuts(&session, &shortcuts, None).await {
        Ok(request) => request,
        Err(err) => {
            let _ = session.close().await;
            return Err(err.to_string());
        }
    };
    let response = match bound.response() {
        Ok(response) => response,
        Err(err) => {
            let _ = session.close().await;
            return Err(err.to_string());
        }
    };
    Ok((session, shortcut_updates(response.shortcuts())))
}

fn activation_token(event: &ashpd::desktop::global_shortcuts::Activated) -> Option<String> {
    event
        .options()
        .get("activation_token")
        .and_then(string_from_portal_value)
}

fn string_from_portal_value(value: &ashpd::zvariant::OwnedValue) -> Option<String> {
    <&str>::try_from(value).ok().map(str::to_string)
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

    #[test]
    fn test_activation_token_reads_owned_string() {
        let value = ashpd::zvariant::OwnedValue::from(ashpd::zvariant::Str::from("tok-1"));
        assert_eq!(string_from_portal_value(&value).as_deref(), Some("tok-1"));
    }
}
