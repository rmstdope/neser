use super::screen_buffer::ScreenBuffer;
#[cfg(test)]
use super::color_effects::{apply_color_emphasis, apply_grayscale};

/// Manages final pixel composition, color effects, and screen output
pub struct Rendering {
    /// Screen buffer for rendered pixels
    screen_buffer: ScreenBuffer,
}

impl Default for Rendering {
    fn default() -> Self {
        Self::new()
    }
}

impl Rendering {
    /// Create a new Rendering instance
    pub fn new() -> Self {
        Self {
            screen_buffer: ScreenBuffer::new(),
        }
    }

    /// Get reference to screen buffer
    pub fn screen_buffer(&self) -> &ScreenBuffer {
        &self.screen_buffer
    }

    /// Get mutable reference to screen buffer
    pub fn screen_buffer_mut(&mut self) -> &mut ScreenBuffer {
        &mut self.screen_buffer
    }

    pub fn screen_buffer_snapshot(&self) -> Vec<u8> {
        self.screen_buffer.snapshot()
    }

    pub fn screen_buffer_crc32(&self) -> u32 {
        self.screen_buffer.crc32()
    }

    pub fn restore_screen_buffer(&mut self, data: &[u8]) {
        self.screen_buffer.restore_from_snapshot(data);
    }

    /// Compose and render a pixel to the screen buffer
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn render_pixel(
        &mut self,
        screen_x: u32,
        screen_y: u32,
        bg_pixel: u8,
        sprite_pixel: Option<(u8, usize, bool)>,
        grayscale: bool,
        color_emphasis: u8,
        palette_lookup: impl Fn(u8) -> u8,
        system_palette_lookup: impl Fn(u8) -> (u8, u8, u8),
    ) -> bool {
        let (palette_index, sprite_0_hit) = select_palette_index(bg_pixel, sprite_pixel);

        // Look up the color in the palette RAM
        let mut color_value = palette_lookup(palette_index);

        // Apply grayscale mode if enabled
        color_value = apply_grayscale(color_value, grayscale);

        // Convert to RGB using the system palette
        let (mut r, mut g, mut b) = system_palette_lookup(color_value);

        // Apply color emphasis/tint
        (r, g, b) = apply_color_emphasis(r, g, b, color_emphasis);

        // Write to the screen buffer
        self.screen_buffer.set_pixel(screen_x, screen_y, r, g, b);

        sprite_0_hit
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderingDebugState {
    pub screen_buffer: super::screen_buffer::ScreenBufferDebugState,
}

#[cfg(test)]
impl Rendering {
    pub fn debug_state(&self) -> RenderingDebugState {
        RenderingDebugState {
            screen_buffer: self.screen_buffer.debug_state(),
        }
    }

    pub fn set_debug_state(&mut self, state: RenderingDebugState) {
        self.screen_buffer.set_debug_state(state.screen_buffer);
    }
}

#[cfg(test)]
#[inline(always)]
fn select_palette_index(bg_pixel: u8, sprite_pixel: Option<(u8, usize, bool)>) -> (u8, bool) {
    let mut palette_index = bg_pixel;
    let mut sprite_0_hit = false;

    if let Some((sprite_palette_idx, sprite_idx, is_foreground)) = sprite_pixel {
        if bg_pixel == 0 || is_foreground {
            palette_index = sprite_palette_idx;
        }
        sprite_0_hit = sprite_idx == 0 && bg_pixel != 0;
    }

    (palette_index, sprite_0_hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rendering_new() {
        let rendering = Rendering::new();
        assert!(rendering.screen_buffer().get_pixel(0, 0) == (0, 0, 0));
    }

    #[test]
    fn test_render_pixel_background() {
        let mut rendering = Rendering::new();
        rendering.render_pixel(
            10,
            10,
            1,               // bg_pixel
            None,            // sprite_pixel
            false,           // grayscale
            0,               // color_emphasis
            |idx| idx,       // palette_lookup
            |_| (255, 0, 0), // system_palette_lookup (red)
        );
        assert_eq!(rendering.screen_buffer().get_pixel(10, 10), (255, 0, 0));
    }

    #[test]
    fn test_render_pixel_sprite_foreground() {
        let mut rendering = Rendering::new();
        rendering.render_pixel(
            10,
            10,
            1,                   // bg_pixel (opaque)
            Some((16, 0, true)), // sprite_pixel (foreground)
            false,
            0,
            |idx| idx,
            |idx| if idx == 16 { (0, 255, 0) } else { (255, 0, 0) },
        );
        // Sprite should be shown (foreground)
        assert_eq!(rendering.screen_buffer().get_pixel(10, 10), (0, 255, 0));
    }

    #[test]
    fn test_render_pixel_sprite_background_priority() {
        let mut rendering = Rendering::new();
        rendering.render_pixel(
            10,
            10,
            1,                    // bg_pixel (opaque)
            Some((16, 0, false)), // sprite_pixel (background priority)
            false,
            0,
            |idx| idx,
            |idx| if idx == 1 { (255, 0, 0) } else { (0, 255, 0) },
        );
        // Background should be shown (sprite has background priority)
        assert_eq!(rendering.screen_buffer().get_pixel(10, 10), (255, 0, 0));
    }

    #[test]
    fn test_render_pixel_grayscale() {
        let mut rendering = Rendering::new();
        rendering.render_pixel(
            10,
            10,
            1,
            None,
            true, // grayscale
            0,
            |idx| idx + 0x20,
            |val| (val, val, val),
        );
        assert_eq!(
            rendering.screen_buffer().get_pixel(10, 10),
            (0x20, 0x20, 0x20)
        );
    }

    #[test]
    fn test_render_pixel_sprite_0_hit() {
        let mut rendering = Rendering::new();
        let hit = rendering.render_pixel(
            10,
            10,
            1,                   // bg_pixel (opaque)
            Some((16, 0, true)), // sprite 0 (index 0)
            false,
            0,
            |idx| idx,
            |_| (255, 255, 255),
        );
        assert!(hit); // Should detect sprite 0 hit
    }

    #[test]
    fn test_render_pixel_no_sprite_0_hit_transparent_bg() {
        let mut rendering = Rendering::new();
        let hit = rendering.render_pixel(
            10,
            10,
            0,                   // bg_pixel (transparent)
            Some((16, 0, true)), // sprite 0
            false,
            0,
            |idx| idx,
            |_| (255, 255, 255),
        );
        assert!(!hit); // No hit when bg is transparent
    }

    #[test]
    fn test_select_palette_index_sprite_0_hit_foreground() {
        let (palette_index, sprite_0_hit) = select_palette_index(1, Some((16, 0, true)));
        assert_eq!(palette_index, 16);
        assert!(sprite_0_hit);
    }

    #[test]
    fn test_apply_color_emphasis_red_only() {
        let (r, g, b) = apply_color_emphasis(100, 100, 100, 0x01);
        assert_eq!(r, 110);
        assert_eq!(g, 75);
        assert_eq!(b, 75);
    }

    #[test]
    fn test_apply_grayscale_enabled() {
        assert_eq!(apply_grayscale(0x2f, true), 0x20);
        assert_eq!(apply_grayscale(0x2f, false), 0x2f);
    }
}
