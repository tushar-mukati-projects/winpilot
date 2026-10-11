use std::{f32::consts::TAU, time::Duration};

pub(crate) fn progress(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
}

#[cfg(test)]
pub(crate) fn close(value: f32, wanted: f32) -> bool {
    (value - wanted).abs() < 1e-5
}

pub(crate) fn progress_of(value: f32, span: f32) -> f32 {
    if span <= 0.0 || !value.is_finite() {
        return 1.0;
    }
    (value / span).clamp(0.0, 1.0)
}

pub(crate) fn ease_in_out_cubic(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (2.0 - 2.0 * t).powi(3) / 2.0 }
}

pub(crate) fn ease_out_cubic(progress: f32) -> f32 {
    let rest = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - rest * rest * rest
}

pub(crate) fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

pub(crate) fn breath(elapsed: Duration, period: Duration) -> f32 {
    if period.is_zero() {
        return 1.0;
    }
    let phase = (elapsed.as_secs_f64() / period.as_secs_f64()).fract() as f32;
    0.5 - 0.5 * (phase * TAU).cos()
}

pub(crate) fn lap_fraction(elapsed: Duration, lap: Duration) -> f32 {
    if lap.is_zero() {
        return 0.0;
    }
    (elapsed.as_secs_f64() / lap.as_secs_f64()).fract() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn progress_is_clamped_and_handles_zero_durations() {
        assert!(close(progress(MS(50), MS(100)), 0.5));
        assert!(close(progress(MS(500), MS(100)), 1.0));
        assert!(close(progress(MS(5), Duration::ZERO), 1.0));
    }

    #[test]
    fn ease_out_starts_fast_and_lands_softly() {
        assert!(close(ease_out_cubic(0.0), 0.0));
        assert!(close(ease_out_cubic(1.0), 1.0));
        assert!(ease_out_cubic(0.5) > 0.8);
    }

    #[test]
    fn fractional_progress_and_in_out_easing() {
        assert!(close(progress_of(0.2, 0.4), 0.5));
        assert!(close(progress_of(3.0, 0.0), 1.0));
        assert!(close(ease_in_out_cubic(0.5), 0.5));
        assert!(ease_in_out_cubic(0.1) < 0.01 && ease_in_out_cubic(0.9) > 0.99);
    }

    #[test]
    fn breathing_cycles_between_zero_and_one() {
        let period = MS(1600);
        assert!(breath(Duration::ZERO, period) < 1e-6);
        assert!((breath(MS(800), period) - 1.0).abs() < 1e-6);
        assert!(breath(MS(1600), period) < 1e-6);
    }

    #[test]
    fn laps_wrap_around() {
        assert!((lap_fraction(MS(600), MS(2400)) - 0.25).abs() < 1e-6);
        assert!((lap_fraction(MS(3000), MS(2400)) - 0.25).abs() < 1e-6);
        assert!(close(lap_fraction(MS(10), Duration::ZERO), 0.0));
    }

    #[test]
    fn smoothstep_is_flat_at_both_ends() {
        assert!(close(smoothstep(0.0, 1.0, -1.0), 0.0));
        assert!(close(smoothstep(0.0, 1.0, 2.0), 1.0));
        assert!(close(smoothstep(0.0, 1.0, 0.5), 0.5));
        assert!(close(smoothstep(1.0, 1.0, 0.5), 0.0));
    }
}
