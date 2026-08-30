//! Windows native implementation of xtools-host.

pub mod paint;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod window;

#[cfg(windows)]
pub use window::run;
