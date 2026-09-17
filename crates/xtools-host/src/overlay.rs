use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn attach_overlay(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("xtools"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 0);
    window.set_margin(Edge::Bottom, 0);
    window.set_margin(Edge::Left, 0);
    window.set_margin(Edge::Right, 0);
    window.set_exclusive_zone(-1);
}
