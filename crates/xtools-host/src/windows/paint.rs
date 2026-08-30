//! Windows 32-bit ARGB software rendering for the floating ball and orbital menu.
//! 平台无关的纯软件渲染（cfg(any(windows, test)) 下在 Linux 测试中同样编译）。

use xtools_ui::{Color, ORB_FILL, ORB_MARK, func_radius, main_radius};

/// 32-bit ARGB (premultiplied alpha) pixel surface.
pub struct Surface {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Self {
        let size = (width.max(1) * height.max(1)) as usize;
        Self {
            width,
            height,
            pixels: vec![0; size],
        }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    #[inline]
    fn blend_pixel(&mut self, x: i32, y: i32, r: f64, g: f64, b: f64, a: f64) {
        if x < 0 || x >= self.width || y < 0 || y >= self.height || a <= 0.0 {
            return;
        }
        let a = a.clamp(0.0, 1.0);
        let src_a = (a * 255.0).round() as u32;
        let src_r = (r * a * 255.0).round() as u32;
        let src_g = (g * a * 255.0).round() as u32;
        let src_b = (b * a * 255.0).round() as u32;

        let idx = (y * self.width + x) as usize;
        let dst = self.pixels[idx];
        let dst_a = (dst >> 24) & 0xFF;
        let dst_r = (dst >> 16) & 0xFF;
        let dst_g = (dst >> 8) & 0xFF;
        let dst_b = dst & 0xFF;

        let inv_a = 255 - src_a;
        let out_a = src_a + (dst_a * inv_a + 127) / 255;
        let out_r = src_r + (dst_r * inv_a + 127) / 255;
        let out_g = src_g + (dst_g * inv_a + 127) / 255;
        let out_b = src_b + (dst_b * inv_a + 127) / 255;

        self.pixels[idx] = (out_a.min(255) << 24)
            | (out_r.min(255) << 16)
            | (out_g.min(255) << 8)
            | out_b.min(255);
    }

    #[inline]
    pub fn blend_pixel_rgba8(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
        if x < 0 || x >= self.width || y < 0 || y >= self.height || a == 0 {
            return;
        }
        let src_a = a as u32;
        let src_r = r as u32;
        let src_g = g as u32;
        let src_b = b as u32;

        let idx = (y * self.width + x) as usize;
        let dst = self.pixels[idx];
        let dst_a = (dst >> 24) & 0xFF;
        let dst_r = (dst >> 16) & 0xFF;
        let dst_g = (dst >> 8) & 0xFF;
        let dst_b = dst & 0xFF;

        let inv_a = 255 - src_a;
        let out_a = src_a + (dst_a * inv_a + 127) / 255;
        let out_r = src_r + (dst_r * inv_a + 127) / 255;
        let out_g = src_g + (dst_g * inv_a + 127) / 255;
        let out_b = src_b + (dst_b * inv_a + 127) / 255;

        self.pixels[idx] = (out_a.min(255) << 24)
            | (out_r.min(255) << 16)
            | (out_g.min(255) << 8)
            | out_b.min(255);
    }

    /// Blend a tiny_skia Pixmap onto this surface at (px, py).
    pub fn draw_pixmap(&mut self, px: i32, py: i32, pixmap: &tiny_skia::Pixmap) {
        let w = pixmap.width() as i32;
        let h = pixmap.height() as i32;
        let data = pixmap.data();
        for y in 0..h {
            for x in 0..w {
                let offset = ((y * w + x) * 4) as usize;
                let r = data[offset];
                let g = data[offset + 1];
                let b = data[offset + 2];
                let a = data[offset + 3];
                if a > 0 {
                    self.blend_pixel_rgba8(px + x, py + y, r, g, b, a);
                }
            }
        }
    }

    /// Antialiased filled circle.
    pub fn draw_circle_filled(&mut self, cx: f64, cy: f64, radius: f64, color: Color) {
        let min_x = (cx - radius - 1.0).floor() as i32;
        let max_x = (cx + radius + 1.0).ceil() as i32;
        let min_y = (cy - radius - 1.0).floor() as i32;
        let max_y = (cy + radius + 1.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = (x as f64 + 0.5) - cx;
                let dy = (y as f64 + 0.5) - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let cov = (radius + 0.5 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(x, y, color.r, color.g, color.b, color.a * cov);
                }
            }
        }
    }

    /// Antialiased stroked circle outline.
    pub fn draw_circle_stroked(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        stroke_width: f64,
        color: Color,
    ) {
        let half_w = stroke_width * 0.5;
        let min_x = (cx - radius - half_w - 1.0).floor() as i32;
        let max_x = (cx + radius + half_w + 1.0).ceil() as i32;
        let min_y = (cy - radius - half_w - 1.0).floor() as i32;
        let max_y = (cy + radius + half_w + 1.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = (x as f64 + 0.5) - cx;
                let dy = (y as f64 + 0.5) - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let delta = (dist - radius).abs();
                let cov = (half_w + 0.5 - delta).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(x, y, color.r, color.g, color.b, color.a * cov);
                }
            }
        }
    }

    /// Antialiased thick line segment with rounded caps.
    pub fn draw_line(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        stroke_width: f64,
        color: Color,
    ) {
        let half_w = stroke_width * 0.5;
        let min_x = (x0.min(x1) - half_w - 1.0).floor() as i32;
        let max_x = (x0.max(x1) + half_w + 1.0).ceil() as i32;
        let min_y = (y0.min(y1) - half_w - 1.0).floor() as i32;
        let max_y = (y0.max(y1) + half_w + 1.0).ceil() as i32;

        let seg_dx = x1 - x0;
        let seg_dy = y1 - y0;
        let len_sq = seg_dx * seg_dx + seg_dy * seg_dy;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let dist = if len_sq <= 1e-6 {
                    let dx = px - x0;
                    let dy = py - y0;
                    (dx * dx + dy * dy).sqrt()
                } else {
                    let t = (((px - x0) * seg_dx + (py - y0) * seg_dy) / len_sq).clamp(0.0, 1.0);
                    let proj_x = x0 + t * seg_dx;
                    let proj_y = y0 + t * seg_dy;
                    let dx = px - proj_x;
                    let dy = py - proj_y;
                    (dx * dx + dy * dy).sqrt()
                };

                let cov = (half_w + 0.5 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend_pixel(x, y, color.r, color.g, color.b, color.a * cov);
                }
            }
        }
    }

    /// Draw a subtle drop shadow under a disk.
    pub fn draw_disk_shadow(&mut self, cx: f64, cy: f64, radius: f64, alpha: f64) {
        let shadow_r = radius + 3.0;
        let shadow_cy = cy + 2.0;
        let min_x = (cx - shadow_r - 2.0).floor() as i32;
        let max_x = (cx + shadow_r + 2.0).ceil() as i32;
        let min_y = (shadow_cy - shadow_r - 2.0).floor() as i32;
        let max_y = (shadow_cy + shadow_r + 2.0).ceil() as i32;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = (x as f64 + 0.5) - cx;
                let dy = (y as f64 + 0.5) - shadow_cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= shadow_r {
                    let edge_t = (shadow_r - dist) / 4.0;
                    let cov = edge_t.clamp(0.0, 1.0) * 0.15 * alpha;
                    if cov > 0.0 {
                        self.blend_pixel(x, y, 0.0, 0.0, 0.0, cov);
                    }
                }
            }
        }
    }
}

/// Draw clock face on the timestamp function disk.
fn draw_clock(surface: &mut Surface, cx: f64, cy: f64, fr: f64, color: Color) {
    let stroke = (1.5 * fr / func_radius()).max(1.0);
    // Outer circle
    surface.draw_circle_stroked(cx, cy, fr * 0.38, stroke, color);
    // Hour hand (points up)
    surface.draw_line(cx, cy, cx, cy - fr * 0.22, stroke * 0.9, color);
    // Minute hand (points up-right)
    surface.draw_line(cx, cy, cx + fr * 0.16, cy + fr * 0.04, stroke * 0.9, color);
}

const XTOOLS_SVG: &[u8] = include_bytes!("../../../../xtools.svg");

/// Render the embedded SVG icon to a tiny_skia Pixmap.
fn render_svg_icon(size: u32) -> Option<tiny_skia::Pixmap> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(XTOOLS_SVG, &opt).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let sx = size as f32 / tree.size().width();
    let sy = size as f32 / tree.size().height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

/// Draw stylized 'x' logo mark on main ball (fallback if SVG cannot be rendered).
fn draw_main_mark(surface: &mut Surface, cx: f64, cy: f64, scale: f64, color: Color) {
    let size = 7.5 * scale;
    let stroke = (2.4 * scale).max(1.5);
    surface.draw_line(cx - size, cy - size, cx + size, cy + size, stroke, color);
    surface.draw_line(cx - size, cy + size, cx + size, cy - size, stroke, color);
}

/// Draw the main floating ball.
pub fn draw_main(surface: &mut Surface, cx: f64, cy: f64, scale: f64) {
    let r = main_radius() * scale;
    // Shadow
    surface.draw_disk_shadow(cx, cy, r, 1.0);
    // White body
    surface.draw_circle_filled(cx, cy, r, ORB_FILL);
    // Outer hairline border
    surface.draw_circle_stroked(
        cx,
        cy,
        r - 0.5,
        (1.0 * scale).max(1.0),
        Color::rgba(ORB_MARK.r, ORB_MARK.g, ORB_MARK.b, 0.15),
    );
    // Render the real app icon (SVG)
    let icon_size = (32.0 * scale).round() as u32;
    if let Some(pixmap) = render_svg_icon(icon_size) {
        let px = (cx - f64::from(icon_size) / 2.0).round() as i32;
        let py = (cy - f64::from(icon_size) / 2.0).round() as i32;
        surface.draw_pixmap(px, py, &pixmap);
    } else {
        draw_main_mark(surface, cx, cy, scale, ORB_MARK);
    }
}

/// Draw a function ball identified by its manifest mark (clock / {} / 文 / 智…).
pub fn draw_func(surface: &mut Surface, mark: &str, cx: f64, cy: f64, t: f64, scale: f64) {
    if t <= 0.0 {
        return;
    }
    let t = t.clamp(0.0, 1.0);
    let fr = func_radius() * scale;
    let fill = Color::rgba(ORB_FILL.r, ORB_FILL.g, ORB_FILL.b, t);
    let border = Color::rgba(ORB_MARK.r, ORB_MARK.g, ORB_MARK.b, 0.20 * t);
    let color = Color::rgba(ORB_MARK.r, ORB_MARK.g, ORB_MARK.b, t);

    // Shadow
    surface.draw_disk_shadow(cx, cy, fr, t);
    // Fill
    surface.draw_circle_filled(cx, cy, fr, fill);
    // Border
    surface.draw_circle_stroked(cx, cy, fr - 0.5, (1.0 * scale).max(1.0), border);

    match mark {
        "clock" => draw_clock(surface, cx, cy, fr, color),
        "{}" => draw_json_mark(surface, cx, cy, scale, color),
        "文" => draw_trans_mark(surface, cx, cy, scale, color),
        // AI 问答等其余工具：四角星光标
        _ => draw_ai_mark(surface, cx, cy, scale, color),
    }
}

/// Draw JSON `{}` mark with line segments for antialiasing.
fn draw_json_mark(surface: &mut Surface, cx: f64, cy: f64, scale: f64, color: Color) {
    let s = 1.0 * scale;
    let stroke = (1.4 * scale).max(1.0);

    // Left brace '{'
    let lx = cx - 4.5 * s;
    surface.draw_line(lx + 2.0 * s, cy - 6.0 * s, lx, cy - 4.0 * s, stroke, color);
    surface.draw_line(lx, cy - 4.0 * s, lx, cy - 1.5 * s, stroke, color);
    surface.draw_line(lx, cy - 1.5 * s, lx - 2.0 * s, cy, stroke, color);
    surface.draw_line(lx - 2.0 * s, cy, lx, cy + 1.5 * s, stroke, color);
    surface.draw_line(lx, cy + 1.5 * s, lx, cy + 4.0 * s, stroke, color);
    surface.draw_line(lx, cy + 4.0 * s, lx + 2.0 * s, cy + 6.0 * s, stroke, color);

    // Right brace '}'
    let rx = cx + 4.5 * s;
    surface.draw_line(rx - 2.0 * s, cy - 6.0 * s, rx, cy - 4.0 * s, stroke, color);
    surface.draw_line(rx, cy - 4.0 * s, rx, cy - 1.5 * s, stroke, color);
    surface.draw_line(rx, cy - 1.5 * s, rx + 2.0 * s, cy, stroke, color);
    surface.draw_line(rx + 2.0 * s, cy, rx, cy + 1.5 * s, stroke, color);
    surface.draw_line(rx, cy + 1.5 * s, rx, cy + 4.0 * s, stroke, color);
    surface.draw_line(rx, cy + 4.0 * s, rx - 2.0 * s, cy + 6.0 * s, stroke, color);
}

/// Draw Chinese character '文' mark for translate tool.
fn draw_trans_mark(surface: &mut Surface, cx: f64, cy: f64, scale: f64, color: Color) {
    let s = 1.0 * scale;
    let stroke = (1.5 * scale).max(1.0);

    // Top dot/tick: '丶'
    surface.draw_line(cx, cy - 6.0 * s, cx, cy - 4.0 * s, stroke * 1.1, color);
    // Horizontal bar: '一'
    surface.draw_line(cx - 6.0 * s, cy - 3.2 * s, cx + 6.0 * s, cy - 3.2 * s, stroke, color);
    // Left diagonal sweep: '丿'
    surface.draw_line(cx + 1.5 * s, cy - 2.0 * s, cx - 5.5 * s, cy + 6.0 * s, stroke, color);
    // Right diagonal sweep: '捺'
    surface.draw_line(cx - 1.5 * s, cy - 2.0 * s, cx + 5.5 * s, cy + 6.0 * s, stroke, color);
}

/// Draw a four-point sparkle mark for the AI tool (fallback for unknown marks).
fn draw_ai_mark(surface: &mut Surface, cx: f64, cy: f64, scale: f64, color: Color) {
    let stroke = (1.8 * scale).max(1.2);
    let r_out = 6.0 * scale;
    let r_in = 1.8 * scale;
    // 四角星：上、右、下、左四条折线
    surface.draw_line(cx, cy - r_out, cx + r_in, cy - r_in, stroke, color);
    surface.draw_line(cx + r_in, cy - r_in, cx + r_out, cy, stroke, color);
    surface.draw_line(cx + r_out, cy, cx + r_in, cy + r_in, stroke, color);
    surface.draw_line(cx + r_in, cy + r_in, cx, cy + r_out, stroke, color);
    surface.draw_line(cx, cy + r_out, cx - r_in, cy + r_in, stroke, color);
    surface.draw_line(cx - r_in, cy + r_in, cx - r_out, cy, stroke, color);
    surface.draw_line(cx - r_out, cy, cx - r_in, cy - r_in, stroke, color);
    surface.draw_line(cx - r_in, cy - r_in, cx, cy - r_out, stroke, color);
    // 中心小点
    surface.draw_circle_filled(cx, cy, 0.9 * scale, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_initializes_empty_and_blends() {
        let mut surface = Surface::new(100, 100);
        assert_eq!(surface.pixels.len(), 10000);
        assert!(surface.pixels.iter().all(|&p| p == 0));

        draw_main(&mut surface, 50.0, 50.0, 1.0);
        // Center of main ball should be non-transparent
        let center_idx = 50 * 100 + 50;
        let center_pixel = surface.pixels[center_idx];
        let alpha = (center_pixel >> 24) & 0xFF;
        assert!(alpha > 200, "center of main ball should have high alpha");

        // Corner (0, 0) should remain 100% transparent
        assert_eq!(surface.pixels[0], 0, "corner should be transparent");
    }

    #[test]
    fn svg_icon_renders_and_draws() {
        let pixmap = render_svg_icon(32);
        assert!(pixmap.is_some(), "xtools.svg should render to 32x32 Pixmap");
        let pixmap = pixmap.unwrap();
        assert_eq!(pixmap.width(), 32);
        assert_eq!(pixmap.height(), 32);

        let mut surface = Surface::new(100, 100);
        draw_main(&mut surface, 50.0, 50.0, 1.0);
        let center_idx = 50 * 100 + 50;
        let center_pixel = surface.pixels[center_idx];
        let alpha = (center_pixel >> 24) & 0xFF;
        assert!(alpha > 0, "main ball with SVG icon should have alpha at center");
    }

    #[test]
    fn draw_func_renders_all_marks() {
        let mut surface = Surface::new(200, 200);
        for mark in ["clock", "{}", "文", "智", "unknown-fallback"] {
            surface.clear();
            draw_func(&mut surface, mark, 100.0, 100.0, 1.0, 1.0);
            let center_idx = 100 * 200 + 100;
            let center_pixel = surface.pixels[center_idx];
            let alpha = (center_pixel >> 24) & 0xFF;
            assert!(alpha > 150, "mark {mark} center should have alpha");
        }
    }
}
