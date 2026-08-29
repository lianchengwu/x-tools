use gtk4::cairo::{self, RectangleInt};
use gtk4::gdk::{self, prelude::*};
use gtk4::prelude::*;

pub fn disk_region(cx: f64, cy: f64, radius: f64) -> cairo::Region {
    let region = cairo::Region::create();
    let r = radius.ceil() as i32;
    let cx_i = cx.round() as i32;
    let cy_i = cy.round() as i32;
    for dy in -r..=r {
        let y = f64::from(dy);
        let remain = radius * radius - y * y;
        if remain < 0.0 {
            continue;
        }
        let w = remain.sqrt().ceil() as i32;
        let rect = RectangleInt::new(cx_i - w, cy_i + dy, 2 * w + 1, 1);
        let _ = region.union_rectangle(&rect);
    }
    region
}

pub fn apply_collapsed_region(surface: &gdk::Surface, cx: f64, cy: f64, radius: f64) {
    let region = disk_region(cx, cy, radius);
    surface.set_input_region(Some(&region));
}

pub fn apply_expanded_region(surface: &gdk::Surface) {
    surface.set_input_region(None::<&cairo::Region>);
}

pub fn refuse_if_no_input_shapes(display: &gdk::Display) -> bool {
    if display.supports_input_shapes() {
        true
    } else {
        eprintln!("xtools-host-wasm: compositor does not support input shapes");
        false
    }
}

pub fn apply_collapsed_from_widget(widget: &impl IsA<gtk4::Widget>, cx: f64, cy: f64, radius: f64) {
    if let Some(native) = widget.native() {
        if let Some(surface) = native.surface() {
            apply_collapsed_region(&surface, cx, cy, radius);
        }
    }
}

pub fn apply_expanded_from_widget(widget: &impl IsA<gtk4::Widget>) {
    if let Some(native) = widget.native() {
        if let Some(surface) = native.surface() {
            apply_expanded_region(&surface);
        }
    }
}
