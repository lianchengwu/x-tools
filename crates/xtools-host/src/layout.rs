use xtools_ui::{func_radius, orbit_radius};

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
}

pub fn surface_rect(w: f64, h: f64) -> Rect {
    Rect::new(0.0, 0.0, w, h)
}

/// Calculate visual scaling factor for floating orbs based on monitor geometry and scale factor.
///
/// Priority:
/// 1. `XTOOLS_SCALE` environment variable override (0.5..=4.0)
/// 2. If compositor/GDK is already using integer scale factor >= 2, return 1.0 (since GTK windows are scaled by compositor)
/// 3. If unscaled display on 4K/QHD, scale up 1.25x - 1.5x so a 40px orb doesn't look tiny
pub fn vis_scale(screen_w: f64, screen_h: f64, scale_factor: i32) -> f64 {
    if let Ok(val) = std::env::var("XTOOLS_SCALE") {
        if let Ok(custom) = val.trim().parse::<f64>() {
            if (0.5..=4.0).contains(&custom) {
                return custom;
            }
        }
    }

    if scale_factor >= 2 {
        1.0
    } else if screen_w >= 3400.0 || screen_h >= 2000.0 {
        1.5
    } else if screen_w >= 2500.0 || screen_h >= 1400.0 {
        1.25
    } else {
        1.0
    }
}


pub fn clamp_main(cx: f64, cy: f64, main_r: f64, surface: Rect) -> (f64, f64) {
    let min_x = surface.x + main_r;
    let max_x = surface.x + surface.w - main_r;
    let min_y = surface.y + main_r;
    let max_y = surface.y + surface.h - main_r;
    let x = if min_x <= max_x { cx.clamp(min_x, max_x) } else { surface.x + surface.w * 0.5 };
    let y = if min_y <= max_y { cy.clamp(min_y, max_y) } else { surface.y + surface.h * 0.5 };
    (x, y)
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

fn seat_at(main: (f64, f64), angle: f64, r: f64) -> (f64, f64) {
    (main.0 + r * angle.cos(), main.1 + r * angle.sin())
}

fn disk_inside(c: (f64, f64), r: f64, mon: Rect) -> bool {
    c.0 - r >= mon.x && c.0 + r <= mon.x + mon.w && c.1 - r >= mon.y && c.1 + r <= mon.y + mon.h
}

/// Calculate dynamic orbital seats for N discovered plugins.
pub fn fan_seats_dynamic(
    main: (f64, f64),
    count: usize,
    monitor: Rect,
    scale: f64,
) -> Vec<(f64, f64)> {
    if count == 0 {
        return Vec::new();
    }

    let fr = func_radius() * scale;
    let base_orbit = orbit_radius() * scale;
    // Minimum gap between adjacent function balls so they never overlap
    let min_ball_gap = 8.0 * scale;

    // Distribute angles evenly around top arc (-90 deg center)
    let angles: Vec<f64> = match count {
        1 => vec![deg_to_rad(-90.0)],
        2 => vec![deg_to_rad(-125.0), deg_to_rad(-55.0)],
        3 => vec![deg_to_rad(-150.0), deg_to_rad(-90.0), deg_to_rad(-30.0)],
        4 => {
            let start_deg = -155.0;
            let end_deg = -25.0;
            let step = (end_deg - start_deg) / 3.0;
            (0..4)
                .map(|i| deg_to_rad(start_deg + i as f64 * step))
                .collect()
        }
        _ => {
            let start_deg = -160.0;
            let end_deg = -20.0;
            let step = (end_deg - start_deg) / (count - 1) as f64;
            (0..count)
                .map(|i| deg_to_rad(start_deg + i as f64 * step))
                .collect()
        }
    };

    // Calculate minimum angular separation between adjacent seats
    let min_delta_angle = if angles.len() >= 2 {
        angles
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(f64::INFINITY, f64::min)
    } else {
        f64::INFINITY
    };

    // Ensure orbit radius is large enough so adjacent disks do not overlap:
    // 2 * R * sin(delta / 2) >= 2 * fr + min_ball_gap
    let orbit = if min_delta_angle.is_finite() && min_delta_angle > 0.0 {
        let needed_orbit = (fr + min_ball_gap * 0.5) / (min_delta_angle * 0.5).sin();
        base_orbit.max(needed_orbit)
    } else {
        base_orbit
    };

    // Attempt default radius
    let mut seats: Vec<(f64, f64)> = angles
        .iter()
        .map(|&a| seat_at(main, a, orbit))
        .collect();

    let all_inside = seats.iter().all(|&pt| disk_inside(pt, fr, monitor));
    if all_inside {
        return seats;
    }

    // Try rotational offsets if near screen borders
    for delta_deg in [-30.0, 30.0, -60.0, 60.0, -90.0, 90.0, 180.0] {
        let rot = deg_to_rad(delta_deg);
        let cand: Vec<(f64, f64)> = angles
            .iter()
            .map(|&a| seat_at(main, a + rot, orbit))
            .collect();
        if cand.iter().all(|&pt| disk_inside(pt, fr, monitor)) {
            return cand;
        }
    }

    // Fallback: clamp each seat inside monitor
    for pt in &mut seats {
        pt.0 = pt.0.clamp(monitor.x + fr, monitor.x + monitor.w - fr);
        pt.1 = pt.1.clamp(monitor.y + fr, monitor.y + monitor.h - fr);
    }
    seats
}

pub fn hit_disk(px: f64, py: f64, cx: f64, cy: f64, r: f64) -> bool {
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtools_ui::func_radius;

    #[test]
    fn test_fan_seats_dynamic_no_overlap_for_discovered_plugins() {
        let mon = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let main = (960.0, 540.0);

        for count in 2..=8 {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let fr = func_radius() * scale;
                let min_dist = 2.0 * fr; // diameters must not overlap
                let seats = fan_seats_dynamic(main, count, mon, scale);
                assert_eq!(seats.len(), count);

                for i in 0..seats.len() - 1 {
                    let (x1, y1) = seats[i];
                    let (x2, y2) = seats[i + 1];
                    let dist = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
                    assert!(
                        dist >= min_dist,
                        "count={count} scale={scale}: seat {i} and {} overlap! dist={dist:.2} < min_dist={min_dist:.2}",
                        i + 1
                    );
                }
            }
        }
    }

    #[test]
    fn test_fan_seats_fits_inside_window() {
        let scale = 1.0;
        let win_size = 280.0;
        let mon = Rect::new(0.0, 0.0, win_size, win_size);
        let fr = func_radius() * scale;
        let main = (win_size / 2.0, win_size - 20.0 - 12.0);

        let seats = fan_seats_dynamic(main, 5, mon, scale);
        assert_eq!(seats.len(), 5);
        for (idx, (x, y)) in seats.iter().enumerate() {
            assert!(
                x - fr >= 0.0 && x + fr <= win_size,
                "seat {idx} x out of bounds: x={x}, fr={fr}"
            );
            assert!(
                y - fr >= 0.0 && y + fr <= win_size,
                "seat {idx} y out of bounds: y={y}, fr={fr}"
            );
        }
    }
}
