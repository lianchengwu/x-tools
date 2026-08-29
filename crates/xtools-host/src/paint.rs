use gtk4::cairo;
use xtools_ui::{MARK_PX, ORB_FILL, ORB_MARK, func_radius};

fn mark_color(cr: &cairo::Context) {
    cr.set_source_rgba(ORB_MARK.r, ORB_MARK.g, ORB_MARK.b, ORB_MARK.a);
}

fn draw_clock(cr: &cairo::Context, cx: f64, cy: f64, fr: f64) {
    mark_color(cr);
    cr.set_line_width((1.6 * fr / func_radius()).max(1.0));
    cr.new_sub_path();
    cr.arc(cx, cy, fr * 0.38, 0.0, std::f64::consts::TAU);
    cr.stroke().ok();
    cr.set_line_width((1.4 * fr / func_radius()).max(1.0));
    cr.move_to(cx, cy);
    cr.line_to(cx, cy - fr * 0.22);
    cr.stroke().ok();
    cr.move_to(cx, cy);
    cr.line_to(cx + fr * 0.16, cy + fr * 0.04);
    cr.stroke().ok();
}

fn draw_text_mark(cr: &cairo::Context, cx: f64, cy: f64, text: &str, scale: f64) {
    cr.select_font_face(
        "sans-serif",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(MARK_PX * scale);
    let ext = cr
        .text_extents(text)
        .unwrap_or_else(|_| cairo::TextExtents::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    mark_color(cr);
    cr.move_to(
        cx - ext.width() / 2.0 - ext.x_bearing(),
        cy - ext.height() / 2.0 - ext.y_bearing(),
    );
    cr.show_text(text).ok();
}

const XTOOLS_SVG: &[u8] = include_bytes!("../../../xtools.svg");

fn render_svg_to_cairo(logical_size: f64, device_scale: f64) -> Option<cairo::ImageSurface> {
    let target_px = (logical_size * device_scale).round().max(1.0) as u32;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(XTOOLS_SVG, &opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(target_px, target_px)?;
    let sx = target_px as f32 / tree.size().width();
    let sy = target_px as f32 / tree.size().height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut surface = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        target_px as i32,
        target_px as i32,
    ).ok()?;
    {
        let mut data = surface.data().ok()?;
        let src = pixmap.data();
        let num_pixels = (target_px * target_px) as usize;
        for i in 0..num_pixels {
            let r = src[i * 4] as u32;
            let g = src[i * 4 + 1] as u32;
            let b = src[i * 4 + 2] as u32;
            let a = src[i * 4 + 3] as u32;
            let pixel = (a << 24) | (r << 16) | (g << 8) | b;
            let offset = i * 4;
            data[offset..offset + 4].copy_from_slice(&pixel.to_ne_bytes());
        }
    }
    surface.set_device_scale(device_scale, device_scale);
    Some(surface)
}

pub fn draw_main(cr: &cairo::Context, cx: f64, cy: f64, scale: f64) {
    let logical_icon_size = 28.0 * scale;
    let matrix = cr.matrix();
    let scale_x = matrix.xx().hypot(matrix.xy());
    let scale_y = matrix.yy().hypot(matrix.yx());
    let device_scale = scale_x.max(scale_y).max(1.0);
    if let Some(surface) = render_svg_to_cairo(logical_icon_size, device_scale) {
        let px = cx - logical_icon_size / 2.0;
        let py = cy - logical_icon_size / 2.0;
        cr.set_source_surface(&surface, px, py).ok();
        cr.paint().ok();
    } else {
        draw_text_mark(cr, cx, cy, "x", scale);
    }
}

pub fn draw_func_dynamic(cr: &cairo::Context, mark: &str, cx: f64, cy: f64, t: f64, scale: f64) {
    if t <= 0.0 {
        return;
    }
    let fr = func_radius() * scale;
    cr.save().ok();
    cr.set_source_rgba(ORB_FILL.r, ORB_FILL.g, ORB_FILL.b, t.clamp(0.0, 1.0));
    cr.new_sub_path();
    cr.arc(cx, cy, fr, 0.0, std::f64::consts::TAU);
    cr.fill().ok();
    cr.set_source_rgba(ORB_MARK.r, ORB_MARK.g, ORB_MARK.b, 0.20 * t.clamp(0.0, 1.0));
    cr.set_line_width((1.0 * scale).max(1.0));
    cr.new_sub_path();
    cr.arc(cx, cy, fr, 0.0, std::f64::consts::TAU);
    cr.stroke().ok();

    if mark == "clock" {
        draw_clock(cr, cx, cy, fr);
    } else {
        draw_text_mark(cr, cx, cy, mark, scale);
    }
    cr.restore().ok();
}
