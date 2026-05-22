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

pub(crate) fn top_right_text_panel(
    origin: [f32; 2],
    width: f32,
    text_size: [f32; 2],
    margin: [f32; 2],
    padding: [f32; 2],
) -> TextPanelLayout {
    let rect_w = text_size[0] + padding[0] * 2.0;
    let rect_h = text_size[1] + padding[1] * 2.0;
    let rect_min = [
        origin[0] + width - rect_w - margin[0],
        origin[1] + margin[1],
    ];
    let rect_max = [rect_min[0] + rect_w, rect_min[1] + rect_h];
    let text_pos = [rect_min[0] + padding[0], rect_min[1] + padding[1]];

    TextPanelLayout {
        rect_min,
        rect_max,
        text_pos,
    }
}

pub(crate) fn bottom_center_text_panel(
    origin: [f32; 2],
    size: [f32; 2],
    text_size: [f32; 2],
    stack_index: usize,
    bottom_margin: f32,
    spacing: f32,
    padding: [f32; 2],
) -> TextPanelLayout {
    let rect_w = text_size[0] + padding[0] * 2.0;
    let rect_h = text_size[1] + padding[1] * 2.0;
    let rect_x = origin[0] + (size[0] - rect_w) * 0.5;
    let rect_max_y = origin[1] + size[1] - bottom_margin - stack_index as f32 * (rect_h + spacing);
    let rect_min = [rect_x, rect_max_y - rect_h];
    let rect_max = [rect_x + rect_w, rect_max_y];
    let text_pos = [rect_min[0] + padding[0], rect_min[1] + padding[1]];

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

    #[test]
    fn top_right_text_panel_aligns_to_right_edge() {
        // Given a letterboxed frame origin, frame width, text size, margin, and padding.
        let origin = [100.0, 50.0];
        let width = 800.0;
        let text_size = [60.0, 20.0];
        let margin = [8.0, 8.0];
        let padding = [6.0, 4.0];

        // When computing a top-right text panel.
        let layout = top_right_text_panel(origin, width, text_size, margin, padding);

        // Then the panel is inset from the right edge and text starts after padding.
        assert_eq!(layout.rect_min, [820.0, 58.0]);
        assert_eq!(layout.rect_max, [892.0, 86.0]);
        assert_eq!(layout.text_pos, [826.0, 62.0]);
    }

    #[test]
    fn bottom_center_text_panel_stacks_up_from_bottom_edge() {
        // Given a letterboxed frame origin, frame size, toast text size, and stack spacing.
        let origin = [100.0, 50.0];
        let size = [800.0, 600.0];
        let text_size = [120.0, 24.0];
        let bottom_margin = 12.0;
        let spacing = 8.0;
        let padding = [8.0, 6.0];

        // When computing bottom and third-from-bottom toast panels.
        let bottom_layout =
            bottom_center_text_panel(origin, size, text_size, 0, bottom_margin, spacing, padding);
        let third_layout =
            bottom_center_text_panel(origin, size, text_size, 2, bottom_margin, spacing, padding);

        // Then panels are horizontally centered and stack upward by their padded height plus spacing.
        assert_eq!(bottom_layout.rect_min, [432.0, 602.0]);
        assert_eq!(bottom_layout.rect_max, [568.0, 638.0]);
        assert_eq!(bottom_layout.text_pos, [440.0, 608.0]);

        assert_eq!(third_layout.rect_min, [432.0, 514.0]);
        assert_eq!(third_layout.rect_max, [568.0, 550.0]);
        assert_eq!(third_layout.text_pos, [440.0, 520.0]);
    }
}
