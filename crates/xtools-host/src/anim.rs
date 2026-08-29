pub const DURATION_US: i64 = 120_000;

pub fn ease_out_cubic(x: f64) -> f64 {
    let t = (1.0 - x).clamp(0.0, 1.0);
    1.0 - t * t * t
}

pub fn progress(now_us: i64, start_us: i64) -> f64 {
    let elapsed = now_us.saturating_sub(start_us);
    (elapsed as f64 / DURATION_US as f64).clamp(0.0, 1.0)
}
