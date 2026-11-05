//! Pure selection geometry (unit-tested; no AppKit), shared by native overlays.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectF {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// Normalised rect spanned by two drag points (any drag direction), clamped to `0..bw × 0..bh`.
pub(crate) fn drag_rect(a: (f64, f64), b: (f64, f64), bw: f64, bh: f64) -> RectF {
    let (x0, x1) = (a.0.min(b.0).clamp(0.0, bw), a.0.max(b.0).clamp(0.0, bw));
    let (y0, y1) = (a.1.min(b.1).clamp(0.0, bh), a.1.max(b.1).clamp(0.0, bh));
    RectF {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// A selection in view points (origin bottom-left, like AppKit) → frame pixels (origin top-left).
pub(crate) fn view_to_pixels(r: RectF, view_h: f64, scale: f64) -> (i64, i64, i64, i64) {
    let x = (r.x * scale).round() as i64;
    let y = ((view_h - (r.y + r.h)) * scale).round() as i64;
    let w = (r.w * scale).round() as i64;
    let h = (r.h * scale).round() as i64;
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_in_any_direction_is_normalised() {
        let r = RectF {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };
        assert_eq!(drag_rect((10.0, 20.0), (40.0, 60.0), 100.0, 100.0), r);
        assert_eq!(drag_rect((40.0, 60.0), (10.0, 20.0), 100.0, 100.0), r);
        assert_eq!(drag_rect((40.0, 20.0), (10.0, 60.0), 100.0, 100.0), r);
    }

    #[test]
    fn drag_is_clamped_to_the_view() {
        let r = drag_rect((-5.0, 90.0), (120.0, 140.0), 100.0, 100.0);
        assert_eq!(
            r,
            RectF {
                x: 0.0,
                y: 90.0,
                w: 100.0,
                h: 10.0
            }
        );
    }

    #[test]
    fn view_rect_maps_to_top_left_pixels() {
        // 1440x900 pt view, retina 2x: a 100x50 pt box whose bottom is 20 pt above the bottom edge
        let r = RectF {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 50.0,
        };
        assert_eq!(view_to_pixels(r, 900.0, 2.0), (20, 1660, 200, 100));
        // a box touching the top edge starts at pixel row 0
        let top = RectF {
            x: 0.0,
            y: 850.0,
            w: 10.0,
            h: 50.0,
        };
        assert_eq!(view_to_pixels(top, 900.0, 2.0).1, 0);
    }
}
