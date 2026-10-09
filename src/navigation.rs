//! Canvas navigation math, independent of the window and input device.
use crate::model::{Camera, MAX_ZOOM, MIN_ZOOM};

pub fn step_zoom(zoom: f64, zoom_in: bool) -> f64 {
    // Repeat comfortable steps at every order of magnitude instead of ending
    // at a fixed list of presets. A tolerance avoids getting stuck on a step.
    let decade = 10.0_f64.powf(zoom.log10().floor());
    let steps = [
        0.1, 0.125, 0.15, 0.2, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 2.5, 5.0, 7.5, 10.0, 12.5,
    ];
    let next = if zoom_in {
        steps
            .into_iter()
            .map(|s| s * decade)
            .find(|s| *s > zoom * 1.000001)
    } else {
        steps
            .into_iter()
            .rev()
            .map(|s| s * decade)
            .find(|s| *s < zoom * 0.999999)
    };
    next.unwrap_or(zoom).clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn parse_zoom(value: &str) -> Option<f64> {
    let value = value.trim();
    let percent: f64 = value
        .strip_suffix('%')
        .unwrap_or(value)
        .trim()
        .parse()
        .ok()?;
    let zoom = percent / 100.0;
    (zoom.is_finite() && (MIN_ZOOM * (1.0 - 1e-12)..=MAX_ZOOM * (1.0 + 1e-12)).contains(&zoom))
        .then_some(zoom.clamp(MIN_ZOOM, MAX_ZOOM))
}

pub fn format_zoom(zoom: f64) -> String {
    let percent = zoom * 100.0;
    if !(0.01..100_000.0).contains(&percent) {
        return format!("{percent:.2e}%");
    }
    let digits = if percent < 1.0 {
        3
    } else if percent < 10.0 {
        2
    } else {
        1
    };
    let value = format!("{percent:.digits$}");
    format!("{}%", value.trim_end_matches('0').trim_end_matches('.'))
}

/// Screen-space grid spacing stays in [24, 48), bounding paint work at any zoom.
pub fn grid_spacing(zoom: f64) -> f64 {
    24.0 * 2.0_f64.powf(zoom.log2().rem_euclid(1.0))
}

/// Clip in double precision before converting to GPU coordinates.
pub fn clip_rect(origin: [f64; 2], dimensions: [f64; 2], viewport: [f64; 2]) -> Option<[f64; 4]> {
    let left = origin[0].max(0.0);
    let top = origin[1].max(0.0);
    let right = (origin[0] + dimensions[0]).min(viewport[0]);
    let bottom = (origin[1] + dimensions[1]).min(viewport[1]);
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

#[derive(Clone, Copy)]
pub struct CameraTransition {
    pub from: Camera,
    pub to: Camera,
    pub anchor: [f64; 2],
}

impl CameraTransition {
    pub fn sample(&self, progress: f64) -> Camera {
        if progress >= 1.0 {
            return self.to;
        }
        if progress <= 0.0 {
            return self.from;
        }
        let t = 1.0 - (1.0 - progress).powi(3);
        let zoom = (self.from.zoom.ln() + (self.to.zoom.ln() - self.from.zoom.ln()) * t).exp();
        let a = self.from.world(self.anchor);
        let b = self.to.world(self.anchor);
        Camera {
            x: self.anchor[0] - (a[0] + (b[0] - a[0]) * t) * zoom,
            y: self.anchor[1] - (a[1] + (b[1] - a[1]) * t) * zoom,
            zoom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_continue_past_the_old_limits_in_both_directions() {
        let mut zoom = 1.0;
        for _ in 0..56 {
            let next = step_zoom(zoom, true);
            assert!(next > zoom);
            zoom = next;
        }
        assert!(zoom >= 1e7);
        zoom = 1.0;
        for _ in 0..56 {
            let next = step_zoom(zoom, false);
            assert!(next < zoom);
            zoom = next;
        }
        assert!(zoom <= 1e-7);
    }

    #[test]
    fn percentages_accept_fractional_and_scientific_values_but_reject_invalid_input() {
        assert_eq!(parse_zoom(" 125% "), Some(1.25));
        assert_eq!(parse_zoom("0.001%"), Some(0.00001));
        assert_eq!(parse_zoom("1e6%"), Some(10000.0));
        for bad in ["", "NaN", "inf", "-50", "0", "1e100", "125%%", "abc"] {
            assert_eq!(parse_zoom(bad), None, "{bad}");
        }
        assert_eq!(format_zoom(1.0), "100%");
        assert_eq!(format_zoom(0.125), "12.5%");
        for z in [MIN_ZOOM, 0.00001, 1.0, 100.0, MAX_ZOOM] {
            assert!(parse_zoom(&format_zoom(z)).is_some());
        }
    }

    #[test]
    fn animation_preserves_the_pointer_anchor_at_every_frame() {
        let from = Camera {
            x: -420.0,
            y: 730.0,
            zoom: 0.7,
        };
        let anchor = [291.0, 614.0];
        let mut to = from;
        to.zoom_at(anchor, 1000.0);
        let transition = CameraTransition { from, to, anchor };
        let world = from.world(anchor);
        for frame in 0..=100 {
            let camera = transition.sample(frame as f64 / 100.0);
            let actual = camera.world(anchor);
            assert!((actual[0] - world[0]).abs() < 1e-9);
            assert!((actual[1] - world[1]).abs() < 1e-9);
            assert!((from.zoom..=to.zoom).contains(&camera.zoom));
        }
        assert_eq!(transition.sample(1.0), to);
    }

    #[test]
    fn huge_rectangles_are_clipped_before_reaching_the_gpu() {
        assert_eq!(
            clip_rect([-1e12, -1e12], [2e12, 2e12], [1280.0, 820.0]),
            Some([0.0, 0.0, 1280.0, 820.0])
        );
        assert_eq!(
            clip_rect([500.0, -1e12], [1e12, 2e12], [1280.0, 820.0]),
            Some([500.0, 0.0, 780.0, 820.0])
        );
        assert_eq!(
            clip_rect([1e12, 1e12], [100.0, 100.0], [1280.0, 820.0]),
            None
        );
    }

    #[test]
    fn grid_paint_work_is_bounded_at_extreme_scales() {
        for exponent in -900..=900 {
            let zoom = 10.0_f64.powf(exponent as f64 / 100.0);
            assert!((24.0..48.000001).contains(&grid_spacing(zoom)));
        }
    }
}
