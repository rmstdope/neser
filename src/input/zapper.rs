use super::ControllerInput;
use crate::console::{Config, ZapperState};
use crate::input::Button;
use crate::ppu::Ppu;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Luminance threshold for light detection (0-255)
/// Bright pixels above this threshold will trigger light detection
const LIGHT_DETECTION_THRESHOLD: f32 = 85.0;

/// Maximum number of scanlines behind the beam where light can still be detected
/// This matches real Zapper hardware latency
const MAX_SCANLINES_BEHIND: i32 = 25;

/// NES Zapper controller.
///
/// Implementation based on emulated hardware behavior:
/// - Light detection uses neighboring pixels (configurable size)
/// - Sampling respects PPU timing (cannot detect ahead of beam or too far behind)
/// - Light bit updates on register read, not per-frame
pub struct Zapper {
    x: u8,
    y: u8,
    trigger: bool,
    light: Cell<bool>,
    ppu: Rc<RefCell<Ppu>>,
    config: Rc<RefCell<Config>>,
}

impl Zapper {
    pub fn new(ppu: Rc<RefCell<Ppu>>, config: Rc<RefCell<Config>>) -> Self {
        Self {
            x: 0,
            y: 0,
            trigger: false,
            light: Cell::new(false),
            ppu,
            config,
        }
    }

    pub fn capture_state(&self) -> ZapperState {
        ZapperState {
            x: self.x,
            y: self.y,
            trigger: self.trigger,
            light: self.light.get(),
        }
    }

    pub fn restore_state(&mut self, state: &ZapperState) {
        self.x = state.x;
        self.y = state.y;
        self.trigger = state.trigger;
        self.light.set(state.light);
    }
}

impl crate::input::Controller for Zapper {
    fn write_strobe(&mut self, _value: u8) {}

    fn read(&mut self) -> u8 {
        self.read_no_clock()
    }

    fn read_no_clock(&self) -> u8 {
        let detection_size = self.config.borrow().zapper_detection_size;
        let ppu = self.ppu.borrow();
        let scanline = ppu.timing().scanline();
        let pixel = ppu.timing().pixel();
        let light_now = self.detect_light(scanline, pixel, ppu.screen_buffer(), detection_size);
        self.light.set(light_now);

        let trigger_bit = (self.trigger as u8) << 4;
        let light_bit = if light_now { 0 } else { 1 << 3 };
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

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::Zapper)
    }
}

impl Zapper {
    /// Detect light at the Zapper's position considering PPU timing constraints.
    ///
    /// The Zapper can only detect light at or behind the current beam position,
    /// and not too far behind (hardware latency limit).
    fn detect_light(
        &self,
        current_scanline: u16,
        current_pixel: u16,
        screen_buffer: &crate::ppu::ScreenBuffer,
        detection_size: u8,
    ) -> bool {
        let zapper_x = self.x as i32;
        let zapper_y = self.y as i32;

        // Calculate the beam position as a linear offset
        // PPU has 341 pixels per scanline (PIXELS_PER_SCANLINE constant)
        let beam_position = (current_scanline as i32) * 341 + (current_pixel as i32);
        let zapper_position = zapper_y * 341 + zapper_x;

        // Check timing constraints:
        // 1. Cannot detect light ahead of the beam
        // 2. Cannot detect light too far behind the beam (hardware latency)
        if zapper_position > beam_position {
            // Zapper is ahead of the beam
            return false;
        }

        let scanlines_behind = (beam_position - zapper_position) / 341;
        if scanlines_behind > MAX_SCANLINES_BEHIND {
            // Too far behind the beam
            return false;
        }

        // Sample pixels in a square around the Zapper position
        let size_i32 = detection_size as i32;
        for dy in -size_i32..=size_i32 {
            for dx in -size_i32..=size_i32 {
                let sample_x = zapper_x + dx;
                let sample_y = zapper_y + dy;

                // Check bounds
                if !(0..256).contains(&sample_x) || !(0..240).contains(&sample_y) {
                    continue;
                }

                // Get luminance at this pixel
                let luminance = screen_buffer.get_luminance(sample_x as u32, sample_y as u32);

                // If any pixel in the detection area is bright enough, light is detected
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
    use crate::console::TvSystem;
    use crate::input::Controller;
    use crate::ppu::Ppu;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn test_config_with_size(size: u8) -> Rc<RefCell<crate::console::Config>> {
        Rc::new(RefCell::new(crate::console::Config {
            zapper_detection_size: size,
            ..Default::default()
        }))
    }

    fn create_zapper_with_ppu(size: u8) -> (Zapper, Rc<RefCell<Ppu>>) {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let config = test_config_with_size(size);
        let zapper = Zapper::new(ppu.clone(), config);
        (zapper, ppu)
    }

    fn advance_ppu_to(ppu: &Rc<RefCell<Ppu>>, scanline: u16, pixel: u16) {
        let current_scanline = ppu.borrow().timing().scanline();
        let current_pixel = ppu.borrow().timing().pixel();
        let current_cycles = (current_scanline as u64) * 341 + (current_pixel as u64);
        let target_cycles = (scanline as u64) * 341 + (pixel as u64);
        let delta = target_cycles.saturating_sub(current_cycles);
        if delta > 0 {
            ppu.borrow_mut().run_ppu_cycles(delta);
        }
    }

    #[test]
    fn test_zapper_trigger_and_light_bits() {
        let (mut zapper, _ppu) = create_zapper_with_ppu(0);

        zapper.set_mouse_left_button(true);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 1);
        assert_eq!((value >> 4) & 0x01, 1);

        zapper.set_mouse_left_button(false);
        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 1);
        assert_eq!((value >> 4) & 0x01, 0);
    }

    #[test]
    fn test_zapper_light_bit_clears_on_light() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(0);
        zapper.set_mouse_y_position(0);

        advance_ppu_to(&ppu, 1, 0);

        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(0, 0, 255, 255, 255);

        let value = zapper.read_no_clock();
        assert_eq!((value >> 3) & 0x01, 0);
    }

    #[test]
    fn test_zapper_capture_restore_roundtrip() {
        let (mut zapper, _ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(0x22);
        zapper.set_mouse_y_position(0x77);
        zapper.set_mouse_left_button(true);

        let state = zapper.capture_state();

        let (mut restored, _ppu) = create_zapper_with_ppu(0);
        restored.restore_state(&state);

        let restored_state = restored.capture_state();
        assert_eq!(restored_state.x, 0x22);
        assert_eq!(restored_state.y, 0x77);
        assert!(restored_state.trigger);
    }

    #[test]
    fn test_zapper_detects_light_on_bright_pixel() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        // Set a bright white pixel at the Zapper position
        // Set PPU timing at scanline 101, pixel 100 (just past the zapper position)
        advance_ppu_to(&ppu, 101, 100);

        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(100, 100, 255, 255, 255);

        // Light should be detected (light bit = 0)
        let value = zapper.read_no_clock();
        assert_eq!(
            (value >> 3) & 0x01,
            0,
            "Light bit should be 0 when light is detected"
        );
        assert!(zapper.capture_state().light);
    }

    #[test]
    fn test_zapper_no_light_on_dark_pixel() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(50);
        zapper.set_mouse_y_position(50);

        // All black pixels by default
        advance_ppu_to(&ppu, 51, 50);

        // Light should not be detected (light bit = 1)
        let value = zapper.read_no_clock();
        assert_eq!(
            (value >> 3) & 0x01,
            1,
            "Light bit should be 1 when no light is detected"
        );
        assert!(!zapper.capture_state().light);
    }

    #[test]
    fn test_zapper_light_threshold() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(30);
        zapper.set_mouse_y_position(30);

        // Just below threshold (85) - use a dim gray
        advance_ppu_to(&ppu, 31, 30);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(30, 30, 84, 84, 84);
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Light should not be detected below threshold"
        );

        // At threshold (85) - should detect
        advance_ppu_to(&ppu, 31, 30);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(30, 30, 85, 85, 85);
        zapper.read_no_clock();
        assert!(
            zapper.capture_state().light,
            "Light should be detected at threshold"
        );

        // Above threshold - should detect
        advance_ppu_to(&ppu, 31, 30);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(30, 30, 200, 200, 200);
        zapper.read_no_clock();
        assert!(
            zapper.capture_state().light,
            "Light should be detected above threshold"
        );
    }

    #[test]
    fn test_zapper_light_detection_with_different_colors() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(60);
        zapper.set_mouse_y_position(60);

        // Bright green (high luminance due to green coefficient)
        advance_ppu_to(&ppu, 61, 60);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(60, 60, 0, 255, 0);
        zapper.read_no_clock();
        assert!(
            zapper.capture_state().light,
            "Bright green should be detected"
        );

        // Bright red (lower luminance)
        advance_ppu_to(&ppu, 61, 60);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(60, 60, 255, 0, 0);
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Pure red alone is below threshold"
        );

        // Bright blue (very low luminance)
        advance_ppu_to(&ppu, 61, 60);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(60, 60, 0, 0, 255);
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Pure blue alone is below threshold"
        );
    }

    #[test]
    fn test_zapper_no_light_ahead_of_beam() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(100, 100, 255, 255, 255);

        // Beam at (scanline 99, pixel 200) occurs before Zapper at (100, 100)
        advance_ppu_to(&ppu, 99, 200);

        // Light should NOT be detected (beam hasn't reached it yet)
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Cannot detect light ahead of beam"
        );
    }

    #[test]
    fn test_zapper_no_light_too_far_behind_beam() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(10);

        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(100, 10, 255, 255, 255);

        // Set PPU timing way past the zapper position (scanline 200)
        advance_ppu_to(&ppu, 200, 100);

        // Light should NOT be detected (too far behind)
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Cannot detect light too far behind beam"
        );
    }

    #[test]
    fn test_zapper_with_radius_detects_nearby_bright_pixel() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);

        // Set a bright pixel one pixel away from the Zapper
        // With radius 0, should not detect
        advance_ppu_to(&ppu, 101, 0);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(101, 100, 255, 255, 255);
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Radius 0 should not detect neighboring pixel"
        );

        // With radius 1 (3x3 area), should detect
        let (mut zapper, ppu) = create_zapper_with_ppu(1);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(100);
        advance_ppu_to(&ppu, 101, 0);
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(101, 100, 255, 255, 255);
        zapper.read_no_clock();
        assert!(
            zapper.capture_state().light,
            "Radius 1 should detect pixel at distance 1"
        );
    }

    #[test]
    fn test_zapper_y_boundary_240() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(240); // At boundary

        // Even with bright pixels, should not detect at y=240
        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(100, 100, 255, 255, 255);

        advance_ppu_to(&ppu, 241, 100);

        // Should not detect light (y >= 240 is out of bounds)
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Should not detect light when Y >= 240"
        );
    }

    #[test]
    fn test_zapper_y_boundary_255() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(255); // Maximum Y value

        advance_ppu_to(&ppu, 260, 100);

        // Should not detect light (y >= 240 is out of bounds)
        zapper.read_no_clock();
        assert!(
            !zapper.capture_state().light,
            "Should not detect light when Y = 255"
        );
    }

    #[test]
    fn test_zapper_y_boundary_239() {
        let (mut zapper, ppu) = create_zapper_with_ppu(0);
        zapper.set_mouse_x_position(100);
        zapper.set_mouse_y_position(239); // Just within bounds

        advance_ppu_to(&ppu, 240, 100);

        ppu.borrow_mut()
            .screen_buffer_mut()
            .set_pixel(100, 239, 255, 255, 255);

        // Should detect light (y = 239 is valid)
        zapper.read_no_clock();
        assert!(
            zapper.capture_state().light,
            "Should detect light when Y = 239 (within bounds)"
        );
    }
}
