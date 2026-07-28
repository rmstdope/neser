//! Visual theme constants for the ROM browser.
//!
//! Defines colours, spacing, and sizing used across the browser UI.

use egui::Color32;

/// Dark background colour (medium grey for gradient top).
pub const BG_COLOR: Color32 = Color32::from_rgb(52, 52, 62);

/// Lighter background for gradient bottom.
pub const BG_COLOR_LIGHT: Color32 = Color32::from_rgb(72, 72, 82);

/// Sidebar background colour (dark grey floating panel).
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(32, 32, 40);

/// Selection highlight colour (blue accent).
pub const SELECTION_COLOR: Color32 = Color32::from_rgb(51, 115, 217);

/// Selected text colour.
pub const SELECTED_TEXT: Color32 = Color32::WHITE;

/// Normal text colour.
pub const TEXT_COLOR: Color32 = Color32::from_rgb(217, 217, 217);

/// Dimmed/secondary text colour.
pub const DIM_TEXT: Color32 = Color32::from_rgb(140, 140, 153);

/// Header/title text colour.
pub const HEADER_TEXT: Color32 = Color32::WHITE;

/// Favourite heart colour.
pub const FAVORITE_COLOR: Color32 = Color32::from_rgb(242, 64, 64);

/// Placeholder cover art background.
pub const PLACEHOLDER_BG: Color32 = Color32::from_rgb(38, 38, 56);

/// Cover art aspect ratio (width / height), typical NES box art ~0.71.
pub const COVER_ASPECT: f32 = 0.71;

/// Minimum cover art width in logical pixels.
pub const MIN_COVER_WIDTH: f32 = 160.0;

/// Maximum cover art width in logical pixels.
pub const MAX_COVER_WIDTH: f32 = 240.0;

/// Spacing between grid cells in logical pixels.
pub const GRID_SPACING: f32 = 16.0;

/// Padding around the grid area.
pub const GRID_PADDING: f32 = 20.0;

/// Height reserved for the game title below cover art.
pub const TITLE_HEIGHT: f32 = 0.0;

/// Corner radius for cover art and UI elements.
pub const CORNER_RADIUS: f32 = 8.0;

/// Fixed height for the sidebar cover art area.
pub const SIDEBAR_ART_HEIGHT: f32 = 340.0;

/// Blur width for the selection glow effect.
pub const SELECTION_GLOW: f32 = 12.0;

/// Sidebar width as a fraction of window width.
pub const SIDEBAR_FRACTION: f32 = 0.28;

/// Minimum sidebar width in logical pixels.
pub const MIN_SIDEBAR_WIDTH: f32 = 280.0;

/// Maximum sidebar width in logical pixels.
pub const MAX_SIDEBAR_WIDTH: f32 = 420.0;

/// Header bar height.
pub const HEADER_HEIGHT: f32 = 48.0;

/// Smooth scroll speed (higher = faster, 1.0 = instant).
pub const SCROLL_SPEED: f32 = 80.0;

/// Button pill background colour.
pub const BUTTON_PILL_BG: Color32 = Color32::from_rgb(60, 60, 75);

/// Button pill text colour.
pub const BUTTON_PILL_TEXT: Color32 = Color32::WHITE;

/// Button pill label colour (action description).
pub const BUTTON_PILL_LABEL: Color32 = Color32::from_rgb(190, 190, 200);

/// Outer wrapper pill for legend items.
pub const LEGEND_ITEM_BG: Color32 = Color32::from_rgb(45, 45, 58);

/// Gamepad face button colours, following the SNES controller.
pub const BUTTON_COLOR_A: Color32 = Color32::from_rgb(217, 72, 72); // Red
pub const BUTTON_COLOR_B: Color32 = Color32::from_rgb(224, 187, 42); // Yellow
pub const BUTTON_COLOR_X: Color32 = Color32::from_rgb(51, 115, 217); // Blue
pub const BUTTON_COLOR_Y: Color32 = Color32::from_rgb(52, 168, 83); // Green

/// Calculate the number of grid columns and cover width for a given area.
///
/// Returns `(columns, cover_width)`.
pub fn grid_layout(available_width: f32) -> (usize, f32) {
    // Try to fit as many columns as possible at min width.
    let total_spacing = GRID_SPACING; // between columns
    let max_cols = ((available_width - GRID_PADDING * 2.0 + total_spacing)
        / (MIN_COVER_WIDTH + total_spacing))
        .floor() as usize;
    let cols = max_cols.max(1);

    // Distribute the available width evenly.
    let cover_width = ((available_width - GRID_PADDING * 2.0 - (cols as f32 - 1.0) * GRID_SPACING)
        / cols as f32)
        .min(MAX_COVER_WIDTH);

    (cols, cover_width)
}

/// Height of one grid cell (cover art + title).
pub fn cell_height(cover_width: f32) -> f32 {
    cover_width / COVER_ASPECT + TITLE_HEIGHT
}

/// Calculate the sidebar width for a given window width.
pub fn sidebar_width(window_width: f32) -> f32 {
    (window_width * SIDEBAR_FRACTION)
        .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
        .min(window_width * 0.4) // never exceed 40% of window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_layout_produces_reasonable_columns() {
        // 1280 - sidebar ~360 ≈ 920 available for grid.
        let (cols, width) = grid_layout(900.0);
        assert!(cols >= 3, "expected at least 3 columns, got {cols}");
        assert!(cols <= 8, "expected at most 8 columns, got {cols}");
        assert!(width >= MIN_COVER_WIDTH);
        assert!(width <= MAX_COVER_WIDTH);
    }

    #[test]
    fn grid_layout_narrow_window_gives_at_least_one() {
        let (cols, _) = grid_layout(100.0);
        assert_eq!(cols, 1);
    }

    #[test]
    fn sidebar_width_clamps() {
        assert!(sidebar_width(1280.0) >= MIN_SIDEBAR_WIDTH);
        assert!(sidebar_width(1280.0) <= MAX_SIDEBAR_WIDTH);
        // Very narrow: sidebar is at most 40% of window.
        let sw = sidebar_width(400.0);
        assert!(sw <= 400.0 * 0.4 + 1.0);
    }

    #[test]
    fn cell_height_equals_cover_height() {
        let h = cell_height(150.0);
        // With TITLE_HEIGHT = 0, cell height is purely cover art height.
        let expected = 150.0 / COVER_ASPECT;
        assert!(
            (h - expected).abs() < 0.01,
            "cell height should equal cover height when TITLE_HEIGHT is 0"
        );
    }
}
