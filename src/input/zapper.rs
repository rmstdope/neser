use super::ControllerInput;
use crate::console::ZapperState;
use crate::input::Button;

/// Luminance threshold for light detection (0-255)
/// Bright pixels above this threshold will trigger light detection
const LIGHT_DETECTION_THRESHOLD: f32 = 85.0;

/// NES Zapper controller.
///
/// Minimal implementation for save-state support and mouse-driven trigger.
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

    fn update_light_detection(&mut self, screen_buffer: &crate::ppu::ScreenBuffer) -> bool {
        // Sample the luminance at the Zapper's position
        let luminance = screen_buffer.get_luminance(self.x as u32, self.y as u32);

        // Light is detected when luminance exceeds threshold
        self.light = luminance >= LIGHT_DETECTION_THRESHOLD;
        true
    }

    fn input_type(&self) -> ControllerInput {
        crate::input::controller_input_type(crate::input::ControllerType::Zapper)
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

        zapper.update_light_detection(&screen_buffer);

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

        zapper.update_light_detection(&screen_buffer);

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
        zapper.update_light_detection(&screen_buffer);
        assert!(
            !zapper.light,
            "Light should not be detected below threshold"
        );

        // At threshold (85) - should detect
        screen_buffer.set_pixel(30, 30, 85, 85, 85);
        zapper.update_light_detection(&screen_buffer);
        assert!(zapper.light, "Light should be detected at threshold");

        // Above threshold - should detect
        screen_buffer.set_pixel(30, 30, 200, 200, 200);
        zapper.update_light_detection(&screen_buffer);
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
        zapper.update_light_detection(&screen_buffer);
        assert!(zapper.light, "Bright green should be detected");

        // Bright red (lower luminance)
        screen_buffer.set_pixel(60, 60, 255, 0, 0);
        zapper.update_light_detection(&screen_buffer);
        assert!(!zapper.light, "Pure red alone is below threshold");

        // Bright blue (very low luminance)
        screen_buffer.set_pixel(60, 60, 0, 0, 255);
        zapper.update_light_detection(&screen_buffer);
        assert!(!zapper.light, "Pure blue alone is below threshold");
    }
}
