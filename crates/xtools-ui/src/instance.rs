//! Platform-neutral singleton IPC API.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::InstanceListener;
#[cfg(windows)]
pub use windows::InstanceListener;

/// Command received on single-instance IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceCommand {
    Raise(Option<String>),
    Quit,
}

pub fn claim_instance(name: &str) -> std::io::Result<Option<InstanceListener>> {
    platform::claim_instance(name)
}

pub fn terminate_instance(name: &str) -> std::io::Result<bool> {
    platform::terminate_instance(name)
}

pub fn raise_instance(name: &str, token: Option<&str>) -> std::io::Result<bool> {
    platform::raise_instance(name, token)
}

pub fn accept_command(listener: &InstanceListener) -> Option<InstanceCommand> {
    platform::accept_command(listener)
}

pub fn accept_raise(listener: &InstanceListener) -> Option<Option<String>> {
    match accept_command(listener) {
        Some(InstanceCommand::Raise(token)) => Some(token),
        _ => None,
    }
}

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_name() -> String {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        format!(
            "xtools-test-instance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn claim_is_exclusive_until_listener_drops() {
        let name = test_name();
        let lock = claim_instance(&name)
            .unwrap()
            .expect("should claim instance");
        assert!(claim_instance(&name).unwrap().is_none());
        drop(lock);
        assert!(claim_instance(&name).unwrap().is_some());
    }

    #[test]
    fn quit_round_trip_is_decoded() {
        let name = test_name();
        let lock = claim_instance(&name)
            .unwrap()
            .expect("should claim instance");
        assert!(terminate_instance(&name).unwrap());
        assert_eq!(accept_command(&lock), Some(InstanceCommand::Quit));
    }

    #[test]
    fn raise_token_round_trip_is_decoded() {
        let name = test_name();
        let lock = claim_instance(&name)
            .unwrap()
            .expect("should claim instance");
        assert!(raise_instance(&name, Some("token")).unwrap());
        assert_eq!(
            accept_command(&lock),
            Some(InstanceCommand::Raise(Some("token".into())))
        );
    }
}
