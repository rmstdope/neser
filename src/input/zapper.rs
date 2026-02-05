use super::ControllerInput;
use crate::console::ZapperState;
use crate::input::Button;

/// Luminance threshold for light detection (0-255)
/// Bright pixels above this threshold will trigger light detection
const LIGHT_DETECTION_THRESHOLD: f32 = 85.0;

/// Maximum number of scanlines behind the beam where light can still be detected
/// This matches real Zapper hardware latency
const MAX_SCANLINES_BEHIND: i32 = 20;

/// NES Zapper controller.
///
/// Implementation based on hardware behavior and Mesen reference:
/// - Light detection uses neighboring pixels (configurable radius)
/// - Sampling respects PPU timing (cannot detect ahead of beam or too far behind)
/// - Light bit updates on register read, not per-frame
pub struct Zapper {
    x: u8,
    y: u8,
    trigger: bool,
    light: bool,
}

impl Default for Zapper {
    fn default() -> Self {
        Self::new()
    }
}

impl Zapper {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            trigger: false,
            light: false,
        }
    }

    pub fn new_boxed() -> Box<dyn crate::input::Controller> {
        Box::new(Self::new())
    }

    pub fn capture_state(&self) -> ZapperState {
        ZapperState {
            x: self.x,
            y: self.y,
            trigger: self.trigger,
            light: self.light,
        }
    }

    pub fn restore_state(&mut self, state: &ZapperState) {
        self.x = state.x;
        self.y = state.y;
        self.trigger = state.trigger;
        self.light = state.light;
    }
}

impl crate::input::Controller for Zapper {
    fn write_strobe(&mut self, _value: u8) {}

    fn read(&mut self) -> u8 {
        self.read_no_clock()
    }

    fn read_no_clock(&self) -> u8 {
        let trigger_bit = (self.trigger as u8) << 3;
        let light_bit = if self.light { 0 } else { 1 << 4 };
        trigger_bit | light_bit
    }

    fn capture_state(&self) -> crate::input::ControllerState {
        crate::input::ControllerState::Zapper(self.capture_state())
    }

    fn restore_state(&mut self, state: &crate::input::ControllerState) {
        if let crate::input::ControllerState::Zapper(zapper_state) = state {
            self.restore_state(zapper_state);
        }
    }

    fn new_boxed() -> Box<dyn crate::input::Controller>
    where
        Self: Sized,
    {
        Self::new_boxed()
    }

    fn set_button(&mut self, _button: Button, _pressed: bool) -> bool {
        false
    }

    fn set_mouse_x_position(&mut self, position: u8) -> bool {
        self.x = position;
        true
    }

    fn set_mouse_y_position(&mut self, position: u8) -> bool {
        self.y = position;
        true
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.trigger = pressed;
        true
    }

    fn set_ppu_context(
        &mut self,
        scanline: u16,
        pixel: u16,
        screen_buffer: &crate::ppu::ScreenBuffer,
        light_radius: u8,
    ) {
        // Update light detection based on current PPU timing and screen buffer
        self.light = self.detect_light(scanline, pixel, screen_buffer, light_radius);
    }

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::Zapper)
    }
}

impl Zapper {
    /// Detect light at the Zapper's position considering PPU timing constraints.
    ///
    /// References:
    /// - https://www.nesdev.org/wiki/Zapper
    /// - https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Input/Zapper.h
    ///
    /// The Zapper can only detect light at or behind the current beam position,
    /// and not too far behind (hardware latency limit).
    fn detect_light(
        &self,
        current_scanline: u16,
        current_pixel: u16,
        screen_buffer: &crate::ppu::ScreenBuffer,
        radius: u8,
    ) -> bool {
        let zapper_x = self.x as i32;
        let zapper_y = self.y as i32;

        // Check if Zapper Y position is within valid screen buffer range (0-239)
        if zapper_y >= 240 {
            return false;
        }

        // Calculate the beam position as a linear offset
        let beam_position = (current_scanline as i32) * 256 + (current_pixel as i32);
        let zapper_position = zapper_y * 256 + zapper_x;

        // Check timing constraints:
        // 1. Cannot detect light ahead of the beam
        // 2. Cannot detect light too far behind the beam (hardware latency)
        if zapper_position > beam_position {
            // Zapper is ahead of the beam
            return false;
        }

        let scanlines_behind = (beam_position - zapper_position) / 256;
        if scanlines_behind > MAX_SCANLINES_BEHIND {
            // Too far behind the beam
            return false;
        }

        // Sample pixels in a radius around the Zapper position
        let radius_i32 = radius as i32;
        for dy in -radius_i32..=radius_i32 {
            for dx in -radius_i32..=radius_i32 {
                let sample_x = zapper_x + dx;
                let sample_y = zapper_y + dy;

                // Check bounds
                if sample_x < 0 || sample_x >= 256 || sample_y < 0 || sample_y >= 240 {
                    continue;
                }

                // Get luminance at this pixel
                let luminance =
                    screen_buffer.get_luminance(sample_x as u32, sample_y as u32);

                // If any pixel in the radius is bright enough, light is detected
                if luminance >= LIGHT_DETECTION_THRESHOLD {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::Zapper;
    use crate::input::Controller;

    #[test]
    fn test_zapper_trigger_and_light_bits() {
        let mut zapper = Zapper::new();

        zapper.set_mouse_left_button(true);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 1);
        assert_eq!((value >> 4) & 0x01, 1);

        zapper.set_mouse_left_button(false);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 0);
    }

    #[test]
    fn test_zapper_light_bit_clears_on_light() {
        let mut zapper = Zapper::new();
        zapper.restore_state(&crate::console::ZapperState {
            x: 0,
            y: 0,
            trigger: false,
            light: true,
        });

        let value = zapper.read_no_clock();
        assert_eq!((value >> 4) & 0x01, 0);
    }

    #[test]
    fn test_zapper_capture_restore_roundtrip() {
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(0x22);
        zapper.set_mouse_y_position(0x77);
        zapper.set_mouse_left_button(true);

        let state = zapper.capture_state();

        let mut restored = Zapper::new();
        restored.restore_state(&state);

        let restored_state = restored.capture_state();
        assert_eq!(restored_state.x, 0x22);
        assert_eq!(restored_state.y, 0x77);
        assert!(restored_state.trigger);
    }

    #[test]
    fn test_zapper_detects_light_on_bright_pixel() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        let mut screen_buffer = ScreenBuffer::new();
        // Set a bright white pixel at the Zapper position
        screen_buffer.set_pixel(100, 100, 255, 255, 255);

        // Set PPU context at scanline 101, pixel 100 (just past the zapper position)
        zapper.set_ppu_context(101, 100, &screen_buffer, 0);

        // Light should be detected (light bit = 0)
        let value = zapper.read_no_clock();
        assert_eq!(
            (value >> 4) & 0x01,
            0,
            "Light bit should be 0 when light is detected"
        );
        assert!(zapper.light);
    }

    #[test]
    fn test_zapper_no_light_on_dark_pixel() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(50);
        zapper.set_mouse_y_position(50);

        let screen_buffer = ScreenBuffer::new(); // All black pixels

        // Set PPU context at scanline 51, pixel 50 (just past the zapper position)
        zapper.set_ppu_context(51, 50, &screen_buffer, 0);

        // Light should not be detected (light bit = 1)
        let value = zapper.read_no_clock();
        assert_eq!(
            (value >> 4) & 0x01,
            1,
            "Light bit should be 1 when no light is detected"
        );
        assert!(!zapper.light);
    }

    #[test]
    fn test_zapper_light_threshold() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(30);
        zapper.set_mouse_y_position(30);

        let mut screen_buffer = ScreenBuffer::new();

        // Just below threshold (85) - use a dim gray
        screen_buffer.set_pixel(30, 30, 84, 84, 84);
        zapper.set_ppu_context(31, 30, &screen_buffer, 0);
        assert!(
            !zapper.light,
            "Light should not be detected below threshold"
        );

        // At threshold (85) - should detect
        screen_buffer.set_pixel(30, 30, 85, 85, 85);
        zapper.set_ppu_context(31, 30, &screen_buffer, 0);
        assert!(zapper.light, "Light should be detected at threshold");

        // Above threshold - should detect
        screen_buffer.set_pixel(30, 30, 200, 200, 200);
        zapper.set_ppu_context(31, 30, &screen_buffer, 0);
        assert!(zapper.light, "Light should be detected above threshold");
    }

    #[test]
    fn test_zapper_light_detection_with_different_colors() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(60);
        zapper.set_mouse_y_position(60);

        let mut screen_buffer = ScreenBuffer::new();

        // Bright green (high luminance due to green coefficient)
        screen_buffer.set_pixel(60, 60, 0, 255, 0);
        zapper.set_ppu_context(61, 60, &screen_buffer, 0);
        assert!(zapper.light, "Bright green should be detected");

        // Bright red (lower luminance)
        screen_buffer.set_pixel(60, 60, 255, 0, 0);
        zapper.set_ppu_context(61, 60, &screen_buffer, 0);
        assert!(!zapper.light, "Pure red alone is below threshold");

        // Bright blue (very low luminance)
        screen_buffer.set_pixel(60, 60, 0, 0, 255);
        zapper.set_ppu_context(61, 60, &screen_buffer, 0);
        assert!(!zapper.light, "Pure blue alone is below threshold");
    }

    #[test]
    fn test_zapper_no_light_ahead_of_beam() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(100, 100, 255, 255, 255);

        // Set PPU context BEFORE the zapper position (scanline 99, pixel 200)
        // This is ahead of the Zapper's position at (100, 100)
        zapper.set_ppu_context(99, 200, &screen_buffer, 0);

        // Light should NOT be detected (beam hasn't reached it yet)
        assert!(!zapper.light, "Cannot detect light ahead of beam");
    }

    #[test]
    fn test_zapper_no_light_too_far_behind_beam() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(10);

        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(100, 10, 255, 255, 255);

        // Set PPU context way past the zapper position (scanline 200)
        // More than MAX_SCANLINES_BEHIND (20) past the Zapper
        zapper.set_ppu_context(200, 100, &screen_buffer, 0);

        // Light should NOT be detected (too far behind)
        assert!(!zapper.light, "Cannot detect light too far behind beam");
    }

    #[test]
    fn test_zapper_with_radius_detects_nearby_bright_pixel() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        let mut screen_buffer = ScreenBuffer::new();
        // Set a bright pixel one pixel away from the Zapper
        screen_buffer.set_pixel(101, 100, 255, 255, 255);

        // With radius 0, should not detect
        zapper.set_ppu_context(101, 0, &screen_buffer, 0);
        assert!(
            !zapper.light,
            "Radius 0 should not detect neighboring pixel"
        );

        // With radius 1 (3x3 area), should detect
        zapper.set_ppu_context(101, 0, &screen_buffer, 1);
        assert!(
            zapper.light,
            "Radius 1 should detect pixel at distance 1"
        );
    }

    #[test]
    fn test_zapper_y_boundary_240() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(240); // At boundary

        let mut screen_buffer = ScreenBuffer::new();
        // Even with bright pixels, should not detect at y=240
        screen_buffer.set_pixel(100, 100, 255, 255, 255);

        zapper.set_ppu_context(241, 100, &screen_buffer, 0);

        // Should not detect light (y >= 240 is out of bounds)
        assert!(
            !zapper.light,
            "Should not detect light when Y >= 240"
        );
    }

    #[test]
    fn test_zapper_y_boundary_255() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(255); // Maximum Y value

        let screen_buffer = ScreenBuffer::new();

        zapper.set_ppu_context(260, 100, &screen_buffer, 0);

        // Should not detect light (y >= 240 is out of bounds)
        assert!(
            !zapper.light,
            "Should not detect light when Y = 255"
        );
    }

    #[test]
    fn test_zapper_y_boundary_239() {
        use crate::ppu::ScreenBuffer;
        let mut zapper = Zapper::new();
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(239); // Just within bounds

        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(100, 239, 255, 255, 255);

        zapper.set_ppu_context(240, 100, &screen_buffer, 0);

        // Should detect light (y = 239 is valid)
        assert!(
            zapper.light,
            "Should detect light when Y = 239 (within bounds)"
        );
    }
}
