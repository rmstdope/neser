use crate::screen_buffer::ScreenBuffer;

/// Manages final pixel composition, color effects, and screen output
pub struct Rendering {
    /// Screen buffer for rendered pixels
    screen_buffer: ScreenBuffer,
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

    /// Compose and render a pixel to the screen buffer
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
        // Determine final palette index based on sprite/background priority
        let mut palette_index = if let Some((sprite_palette_idx, _sprite_idx, is_foreground)) = sprite_pixel {
            if bg_pixel == 0 {
                // Background is transparent, always show sprite
                sprite_palette_idx
            } else if is_foreground {
                // Sprite is in foreground, show sprite
                sprite_palette_idx
            } else {
                // Sprite is in background, show background
                bg_pixel
            }
        } else {
            // No sprite pixel, show background (or backdrop if transparent)
            bg_pixel
        };

        // Palette index 0 represents backdrop color (palette[0])

        // Check for sprite 0 hit (both sprite 0 and background have opaque pixels)
        let sprite_0_hit = if let Some((_sprite_palette_idx, sprite_idx, _is_foreground)) = sprite_pixel {
            sprite_idx == 0 && bg_pixel != 0
        } else {
            false
        };

        // Apply grayscale mode if enabled
        if grayscale {
            palette_index &= 0x30;
        }

        // Look up the color in the palette RAM
        let color_value = palette_lookup(palette_index);

        // Convert to RGB using the system palette
        let (mut r, mut g, mut b) = system_palette_lookup(color_value);

        // Apply color emphasis/tint
        if color_emphasis != 0 {
            let emphasize_red = (color_emphasis & 0x01) != 0;
            let emphasize_green = (color_emphasis & 0x02) != 0;
            let emphasize_blue = (color_emphasis & 0x04) != 0;

            const ATTENUATION: f32 = 0.75;
            const BOOST: f32 = 1.1;

            if emphasize_red {
                r = ((r as f32) * BOOST).min(255.0) as u8;
                if !emphasize_green {
                    g = ((g as f32) * ATTENUATION) as u8;
                }
                if !emphasize_blue {
                    b = ((b as f32) * ATTENUATION) as u8;
                }
            }
            if emphasize_green {
                g = ((g as f32) * BOOST).min(255.0) as u8;
                if !emphasize_red {
                    r = ((r as f32) * ATTENUATION) as u8;
                }
                if !emphasize_blue {
                    b = ((b as f32) * ATTENUATION) as u8;
                }
            }
            if emphasize_blue {
                b = ((b as f32) * BOOST).min(255.0) as u8;
                if !emphasize_red {
                    r = ((r as f32) * ATTENUATION) as u8;
                }
                if !emphasize_green {
                    g = ((g as f32) * ATTENUATION) as u8;
                }
            }
        }

        // Write to the screen buffer
        self.screen_buffer.set_pixel(screen_x, screen_y, r, g, b);

        sprite_0_hit
    }
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
            10, 10,
            1,  // bg_pixel
            None,  // sprite_pixel
            false, // grayscale
            0,     // color_emphasis
            |idx| idx,  // palette_lookup
            |_| (255, 0, 0),  // system_palette_lookup (red)
        );
        assert_eq!(rendering.screen_buffer().get_pixel(10, 10), (255, 0, 0));
    }

    #[test]
    fn test_render_pixel_sprite_foreground() {
        let mut rendering = Rendering::new();
        rendering.render_pixel(
            10, 10,
            1,  // bg_pixel (opaque)
            Some((16, 0, true)),  // sprite_pixel (foreground)
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
            10, 10,
            1,  // bg_pixel (opaque)
            Some((16, 0, false)),  // sprite_pixel (background priority)
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
            10, 10,
            1,
            None,
            true,  // grayscale
            0,
            |idx| idx & 0x30,
            |_| (128, 128, 128),
        );
        assert_eq!(rendering.screen_buffer().get_pixel(10, 10), (128, 128, 128));
    }

    #[test]
    fn test_render_pixel_sprite_0_hit() {
        let mut rendering = Rendering::new();
        let hit = rendering.render_pixel(
            10, 10,
            1,  // bg_pixel (opaque)
            Some((16, 0, true)),  // sprite 0 (index 0)
            false,
            0,
            |idx| idx,
            |_| (255, 255, 255),
        );
        assert!(hit);  // Should detect sprite 0 hit
    }

    #[test]
    fn test_render_pixel_no_sprite_0_hit_transparent_bg() {
        let mut rendering = Rendering::new();
        let hit = rendering.render_pixel(
            10, 10,
            0,  // bg_pixel (transparent)
            Some((16, 0, true)),  // sprite 0
            false,
            0,
            |idx| idx,
            |_| (255, 255, 255),
        );
        assert!(!hit);  // No hit when bg is transparent
    }
}
