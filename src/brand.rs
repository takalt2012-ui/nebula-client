//! Geometry of the Nebula mark and wordmark. Standard library only, so it is unit tested.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

/// Left edge of every glyph, measured from the start of the text, and the full width, for glyphs
/// `widths` wide with `tracking` of extra space between neighbours.
pub(crate) fn tracked_layout(widths: &[f32], tracking: f32) -> (Vec<f32>, f32) {
    let mut offsets = Vec::with_capacity(widths.len());
    let mut x = 0.0;
    for (i, width) in widths.iter().enumerate() {
        if i > 0 {
            x += tracking;
        }
        offsets.push(x);
        x += width;
    }
    (offsets, x)
}

/// The eight corners of a four-pointed star around the origin, starting at the top and turning
/// clockwise: tips (at `outer`) alternate with notches (at `inner`).
pub(crate) fn star_vertices(outer: f32, inner: f32) -> [(f32, f32); 8] {
    std::array::from_fn(|i| {
        let angle = i as f32 * FRAC_PI_4 - FRAC_PI_2;
        let radius = if i % 2 == 0 { outer } else { inner };
        (radius * angle.cos(), radius * angle.sin())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_goes_between_glyphs_only() {
        let (offsets, total) = tracked_layout(&[10.0, 20.0, 10.0], 2.0);
        assert_eq!(offsets, [0.0, 12.0, 34.0]);
        assert_eq!(total, 44.0);
    }

    #[test]
    fn empty_text_has_no_width() {
        assert_eq!(tracked_layout(&[], 5.0), (Vec::new(), 0.0));
    }

    #[test]
    fn star_alternates_tips_and_notches() {
        let vertices = star_vertices(10.0, 3.0);
        for (i, (x, y)) in vertices.iter().enumerate() {
            let radius = x.hypot(*y);
            let expected = if i % 2 == 0 { 10.0 } else { 3.0 };
            assert!((radius - expected).abs() < 1e-4, "{i}: {radius}");
        }
        assert!(vertices[0].1 < 0.0 && vertices[0].0.abs() < 1e-4);
    }
}
