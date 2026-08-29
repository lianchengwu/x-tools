//! Shared tokens, instance lock, and Slint chrome.

pub mod boot;
pub mod ids;
pub mod instance;
pub mod kwin;
pub mod theme;

#[cfg(feature = "slint-chrome")]
pub mod slint_chrome;

pub use boot::{
    capture_target_desktop, init_input_method_env, take_activation_token, target_desktop,
};
pub use ids::{HOST_INSTANCE, JSON_INSTANCE, TIME_INSTANCE, TRANS_INSTANCE, ToolId};
pub use instance::{
    InstanceCommand, InstanceListener, accept_command, accept_raise, claim_instance,
    raise_instance, terminate_instance,
};
pub use theme::{
    CLEAR_COLOR, Color, FUNC_D, GAP, MAIN_D, MARK_PX, ORB_FILL, ORB_MARK, POP_MS, SLOP,
    func_radius, main_radius, orbit_radius,
};
