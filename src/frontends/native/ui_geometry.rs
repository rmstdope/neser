//! Shared geometry helpers for native UI rendering.

/// Returns the largest size that fits inside the container while preserving aspect.
pub(crate) fn letterbox_size(container_w: f32, container_h: f32, aspect: f32) -> (f32, f32) {
    if !aspect.is_finite() || aspect <= 0.0 {
        return (container_w, container_h);
    }

    if container_h == 0.0 {
        return (container_w, 0.0);
    }

    let container_aspect = container_w / container_h;
    if container_aspect > aspect {
        (container_h * aspect, container_h)
    } else {
        (container_w, container_w / aspect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NTSC_ASPECT: f32 = 8.0 / 7.0 * 16.0 / 15.0;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn letterbox_size_wide_container_preserves_aspect() {
        // Given a wide container and a narrower target aspect.
        let container_w = 1920.0;
        let container_h = 1080.0;

        // When computing the letterboxed size.
        let (w, h) = letterbox_size(container_w, container_h, NTSC_ASPECT);

        // Then the height fills the container and width preserves the target aspect.
        assert_close(w, 1316.5714);
        assert_close(h, 1080.0);
    }

    #[test]
    fn letterbox_size_tall_container_preserves_aspect() {
        // Given a tall container and a wider target aspect.
        let container_w = 800.0;
        let container_h = 1200.0;

        // When computing the letterboxed size.
        let (w, h) = letterbox_size(container_w, container_h, NTSC_ASPECT);

        // Then the width fills the container and height preserves the target aspect.
        assert_close(w, 800.0);
        assert_close(h, 656.25);
    }

    #[test]
    fn letterbox_size_zero_height_preserves_width() {
        // Given a zero-height container.
        let container_w = 800.0;
        let container_h = 0.0;

        // When computing the letterboxed size.
        let (w, h) = letterbox_size(container_w, container_h, NTSC_ASPECT);

        // Then the result avoids division by zero while preserving width.
        assert_close(w, 800.0);
        assert_close(h, 0.0);
    }

    #[test]
    fn letterbox_size_invalid_aspect_falls_back_to_container() {
        for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            // Given an invalid target aspect.
            let container_w = 800.0;
            let container_h = 600.0;

            // When computing the letterboxed size.
            let (w, h) = letterbox_size(container_w, container_h, aspect);

            // Then the result stays finite and non-negative by using the container.
            assert_close(w, container_w);
            assert_close(h, container_h);
        }
    }
}
