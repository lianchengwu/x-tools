use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn attach_overlay(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("xtools"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Bottom, false);
    window.set_anchor(Edge::Left, false);
    window.set_exclusive_zone(-1);
}

pub fn place_mid_right(window: &gtk4::ApplicationWindow, screen_h: i32, orb_cy: f64) {
    let margin_top = ((f64::from(screen_h) / 2.0) - orb_cy).round() as i32;
    window.set_margin(Edge::Top, margin_top.max(0));
    window.set_margin(Edge::Right, 16);
}
