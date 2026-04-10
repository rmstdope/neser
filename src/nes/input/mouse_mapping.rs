//! Shared coordinate-mapping functions for mouse-emulated NES controllers.
//!
//! These pure functions translate host window coordinates into values
//! consumed by Zapper, Arkanoid paddle, and SNES Mouse controllers.
//! Both the SDL and native frontends share this code.

/// Maps a window coordinate axis value to a Zapper position in 0..=255.
///
/// The mapping is linear: the input is normalized to `[0.0, 1.0]` across
/// `window_extent` and then scaled to `[0, 255]`.
pub fn map_mouse_axis_to_zapper_position(axis: i32, window_extent: u32) -> u8 {
    if window_extent <= 1 {
        return 0;
    }

    let max_axis = window_extent.saturating_sub(1) as i32;
    let clamped_axis = axis.clamp(0, max_axis);
    let normalized = clamped_axis as f32 / max_axis as f32;
    (normalized * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Maps an SDL mouse X position into an Arkanoid paddle position (0x62..=0xF2).
///
/// The input is normalized to `[-1.0, 1.0]` across the current window width,
/// then shaped using a non-linear curve `sign(x)·|x|^1.5` to make the edges
/// respond faster than the center. The result is scaled to the Arkanoid
/// controller range (0x62–0xF2) and clamped.
pub fn map_mouse_x_to_paddle_position(x: i32, window_width: u32) -> u8 {
    const MIN_POSITION: f32 = 0x62 as f32;
    const MAX_POSITION: f32 = 0xF2 as f32;
    const RANGE: f32 = MAX_POSITION - MIN_POSITION;

    if window_width <= 1 {
        return MIN_POSITION as u8;
    }

    let max_x = window_width.saturating_sub(1) as i32;
    let clamped_x = x.clamp(0, max_x);
    let width_minus_one = max_x as f32;
    let normalized = (clamped_x as f32 / width_minus_one) * 2.0 - 1.0;
    let curved = normalized.signum() * normalized.abs().powf(1.5);
    let scaled = (curved + 1.0) * 0.5 * RANGE + MIN_POSITION;
    scaled.round().clamp(MIN_POSITION, MAX_POSITION) as u8
}

/// Scales a relative mouse delta to a controller axis delta.
///
/// The raw pixel delta is scaled by `255 / (window_extent - 1)` so that
/// traversing the full window produces a full-range delta, then clamped
/// to `[-255, 255]`.
pub fn map_relative_mouse_delta_to_axis_delta(delta: i32, window_extent: u32) -> i16 {
    if window_extent <= 1 {
        return 0;
    }
    let scaled = (delta as f32) * (255.0 / (window_extent.saturating_sub(1) as f32));
    scaled.round().clamp(-255.0, 255.0) as i16
}

/// Returns `true` when the mouse cursor should be grabbed by the window.
///
/// Grab is enabled when a mouse-emulated controller is connected, the
/// window has focus, and the user has not released the grab with Escape.
pub fn should_grab_mouse_input(
    mouse_controller_active: bool,
    window_focused: bool,
    mouse_released_by_escape: bool,
) -> bool {
    mouse_controller_active && window_focused && !mouse_released_by_escape
}

/// Returns `true` when relative (locked) mouse mode should be used.
///
/// Relative mode is only needed for the SNES Mouse, which requires raw
/// delta information rather than absolute window coordinates.
pub fn should_use_relative_mouse_mode(should_grab: bool, snes_mouse_active: bool) -> bool {
    should_grab && snes_mouse_active
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Zapper mapping ───────────────────────────────────────────────────

    #[test]
    fn zapper_edges_and_center() {
        let window_width = 320;
        let window_height = 240;

        assert_eq!(map_mouse_axis_to_zapper_position(0, window_width), 0);
        assert_eq!(map_mouse_axis_to_zapper_position(319, window_width), 255);
        assert_eq!(map_mouse_axis_to_zapper_position(0, window_height), 0);
        assert_eq!(map_mouse_axis_to_zapper_position(239, window_height), 255);
    }

    #[test]
    fn zapper_degenerate_window_extent() {
        assert_eq!(map_mouse_axis_to_zapper_position(0, 0), 0);
        assert_eq!(map_mouse_axis_to_zapper_position(0, 1), 0);
    }

    #[test]
    fn zapper_clamps_out_of_range() {
        assert_eq!(map_mouse_axis_to_zapper_position(-10, 320), 0);
        assert_eq!(map_mouse_axis_to_zapper_position(500, 320), 255);
    }

    // ── Paddle mapping ───────────────────────────────────────────────────

    #[test]
    fn paddle_edges_and_center() {
        let window_width = 300;

        let left = map_mouse_x_to_paddle_position(0, window_width);
        let right = map_mouse_x_to_paddle_position(299, window_width);
        let center_x = ((window_width - 1) / 2) as i32;
        let center = map_mouse_x_to_paddle_position(center_x, window_width);

        assert_eq!(left, 0x62);
        assert_eq!(right, 0xF2);
        assert!((165..=175).contains(&center));
    }

    #[test]
    fn paddle_non_linear_curve() {
        let window_width = 400;
        let center_a = 200;
        let center_b = 220;
        let edge_a = 360;
        let edge_b = 380;

        let center_delta = map_mouse_x_to_paddle_position(center_b, window_width)
            - map_mouse_x_to_paddle_position(center_a, window_width);
        let edge_delta = map_mouse_x_to_paddle_position(edge_b, window_width)
            - map_mouse_x_to_paddle_position(edge_a, window_width);

        assert!(edge_delta > center_delta);
    }

    #[test]
    fn paddle_degenerate_window_width() {
        assert_eq!(map_mouse_x_to_paddle_position(0, 0), 0x62);
        assert_eq!(map_mouse_x_to_paddle_position(0, 1), 0x62);
    }

    // ── Relative delta mapping ───────────────────────────────────────────

    #[test]
    fn delta_scales_proportionally() {
        let d = map_relative_mouse_delta_to_axis_delta(10, 320);
        assert!(d > 0);
        assert!(d < 255);
    }

    #[test]
    fn delta_degenerate_window() {
        assert_eq!(map_relative_mouse_delta_to_axis_delta(10, 0), 0);
        assert_eq!(map_relative_mouse_delta_to_axis_delta(10, 1), 0);
    }

    #[test]
    fn delta_clamps_large_values() {
        let d = map_relative_mouse_delta_to_axis_delta(10000, 100);
        assert!(d <= 255);
        let d_neg = map_relative_mouse_delta_to_axis_delta(-10000, 100);
        assert!(d_neg >= -255);
    }

    // ── Grab decision ────────────────────────────────────────────────────

    #[test]
    fn should_grab_when_all_conditions_met() {
        assert!(should_grab_mouse_input(true, true, false));
    }

    #[test]
    fn should_not_grab_when_window_not_focused() {
        assert!(!should_grab_mouse_input(true, false, false));
    }

    #[test]
    fn should_not_grab_when_no_mouse_controller() {
        assert!(!should_grab_mouse_input(false, true, false));
    }

    #[test]
    fn should_not_grab_after_escape_release() {
        assert!(!should_grab_mouse_input(true, true, true));
    }

    // ── Relative mode decision ───────────────────────────────────────────

    #[test]
    fn relative_mode_for_grabbed_snes_mouse() {
        assert!(should_use_relative_mouse_mode(true, true));
        assert!(!should_use_relative_mouse_mode(false, true));
        assert!(!should_use_relative_mouse_mode(true, false));
    }
}
