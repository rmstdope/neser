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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextPanelLayout {
    pub rect_min: [f32; 2],
    pub rect_max: [f32; 2],
    pub text_pos: [f32; 2],
}

pub(crate) fn top_left_text_panel(
    origin: [f32; 2],
    text_size: [f32; 2],
    margin: [f32; 2],
    padding: [f32; 2],
) -> TextPanelLayout {
    let text_pos = [origin[0] + margin[0], origin[1] + margin[1]];
    let rect_min = [text_pos[0] - padding[0], text_pos[1] - padding[1]];
    let rect_max = [
        text_pos[0] + text_size[0] + padding[0],
        text_pos[1] + text_size[1] + padding[1],
    ];

    TextPanelLayout {
        rect_min,
        rect_max,
        text_pos,
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

    #[test]
    fn top_left_text_panel_offsets_text_and_padding_from_origin() {
        // Given a letterboxed frame origin, text size, margin, and padding.
        let origin = [100.0, 50.0];
        let text_size = [80.0, 20.0];
        let margin = [8.0, 8.0];
        let padding = [6.0, 4.0];

        // When computing a top-left text panel.
        let layout = top_left_text_panel(origin, text_size, margin, padding);

        // Then the text starts at origin + margin, and the panel includes padding.
        assert_eq!(layout.text_pos, [108.0, 58.0]);
        assert_eq!(layout.rect_min, [102.0, 54.0]);
        assert_eq!(layout.rect_max, [194.0, 82.0]);
    }
}
