/// Manages sprite evaluation, OAM, and sprite rendering
pub struct Sprites {
    /// OAM (Object Attribute Memory) - 256 bytes for sprite data
    oam_data: [u8; 256],
    /// Secondary OAM - 32 bytes for up to 8 sprites on current scanline
    secondary_oam: [u8; 32],
    /// Number of sprites found during sprite evaluation (current scanline)
    sprites_found: u8,
    /// Number of sprites to render (from previous scanline's evaluation) - CURRENT scanline
    sprite_count: u8,
    /// Number of sprites for NEXT scanline (swapped at pixel 0)
    next_sprite_count: u8,
    /// Whether we've populated next_sprite buffers at least once
    sprite_buffers_ready: bool,
    /// Index (0-7) of sprite 0 in current scanline's sprite buffers, or None if not present
    sprite_0_index: Option<usize>,
    /// Index of sprite 0 in next scanline's sprite buffers, or None if not present
    next_sprite_0_index: Option<usize>,
    /// Current sprite being evaluated during sprite evaluation
    sprite_eval_n: u8,
    /// Byte offset (0-3) within sprite during overflow checking (for buggy behavior)
    sprite_eval_m: u8,
    /// Sprite pattern shift registers - low bit plane (8 sprites) - CURRENT scanline
    sprite_pattern_shift_lo: [u8; 8],
    /// Sprite pattern shift registers - high bit plane (8 sprites) - CURRENT scanline
    sprite_pattern_shift_hi: [u8; 8],
    /// Sprite X position counters - CURRENT scanline
    sprite_x_positions: [u8; 8],
    /// Sprite attributes (palette, priority, flip bits) - CURRENT scanline
    sprite_attributes: [u8; 8],
    /// Sprite pattern shift registers - low bit plane (8 sprites) - NEXT scanline
    next_sprite_pattern_shift_lo: [u8; 8],
    /// Sprite pattern shift registers - high bit plane (8 sprites) - NEXT scanline
    next_sprite_pattern_shift_hi: [u8; 8],
    /// Sprite X position counters - NEXT scanline
    next_sprite_x_positions: [u8; 8],
    /// Sprite attributes - NEXT scanline
    next_sprite_attributes: [u8; 8],
}

/// OAM attribute byte mask - bits 2-4 are unimplemented and always read as 0
/// Mask: 11100011 (0xE3) - preserves bits 7-5 (priority/palette) and 1-0 (flip bits)
const OAM_ATTRIBUTE_MASK: u8 = 0xE3;

impl Sprites {
    /// Create a new Sprites instance
    pub fn new() -> Self {
        Self {
            oam_data: [0xFF; 256],
            secondary_oam: [0xFF; 32],
            sprites_found: 0,
            sprite_count: 0,
            next_sprite_count: 0,
            sprite_buffers_ready: false,
            sprite_0_index: None,
            next_sprite_0_index: None,
            sprite_eval_n: 0,
            sprite_eval_m: 0,
            sprite_pattern_shift_lo: [0; 8],
            sprite_pattern_shift_hi: [0; 8],
            sprite_x_positions: [0; 8],
            sprite_attributes: [0; 8],
            next_sprite_pattern_shift_lo: [0; 8],
            next_sprite_pattern_shift_hi: [0; 8],
            next_sprite_x_positions: [0; 8],
            next_sprite_attributes: [0; 8],
        }
    }

    /// Reset sprite state
    pub fn reset(&mut self) {
        self.oam_data = [0xFF; 256];
        self.secondary_oam = [0xFF; 32];
        self.sprites_found = 0;
        self.sprite_eval_n = 0;
        self.sprite_eval_m = 0;
    }

    /// Get OAM data at specified address
    pub fn read_oam(&self, addr: u8) -> u8 {
        let value = self.oam_data[addr as usize];
        // Byte 2 of each sprite (attribute byte) has unimplemented bits 2-4
        // These bits should always read as 0
        if (addr & 0x03) == 2 {
            value & OAM_ATTRIBUTE_MASK
        } else {
            value
        }
    }

    /// Write OAM data at specified address
    pub fn write_oam(&mut self, addr: u8, value: u8) {
        // Note: Unimplemented bits in byte 2 can be written but will read back as 0
        // We store the full value but mask it on read
        self.oam_data[addr as usize] = value;
    }

    /// Initialize secondary OAM byte with 0xFF
    pub fn initialize_secondary_oam_byte(&mut self, pixel: u16) {
        let oam_index = ((pixel - 1) / 2) as usize;
        if oam_index < 32 {
            self.secondary_oam[oam_index] = 0xFF;
        }
    }

    /// Evaluate sprites for the current scanline
    pub fn evaluate_sprites(&mut self, pixel: u16, scanline: u16, sprite_height: u8) -> bool {
        // Only evaluate on odd cycles
        if pixel % 2 == 0 {
            return false;
        }

        // Stop if we've evaluated all 64 sprites
        if self.sprite_eval_n >= 64 {
            return false;
        }

        let mut overflow = false;

        // If we've already found 8 sprites, enter overflow checking mode
        if self.sprites_found >= 8 {
            // NES PPU Hardware Bug: Sprite Overflow Detection
            let oam_index = (self.sprite_eval_n as usize) * 4 + (self.sprite_eval_m as usize);

            if oam_index < 256 {
                let sprite_y = self.oam_data[oam_index];

                let next_scanline = if scanline == 261 { 0 } else { scanline + 1 };
                let diff = next_scanline.wrapping_sub(sprite_y as u16);

                if diff < sprite_height as u16 && sprite_y < 0xEF {
                    overflow = true;
                }
            }

            // THE BUG: Increment BOTH n and m
            self.sprite_eval_n += 1;
            self.sprite_eval_m += 1;

            if self.sprite_eval_m >= 4 {
                self.sprite_eval_m = 0;
            }

            return overflow;
        }

        // Normal sprite evaluation (first 8 sprites)
        let oam_index = (self.sprite_eval_n as usize) * 4;
        let sprite_y = self.oam_data[oam_index];

        if sprite_y >= 0xEF {
            self.sprite_eval_n += 1;
            return false;
        }

        let next_scanline = if scanline == 261 { 0 } else { scanline + 1 };
        // Adjust Y position: add 1 to sprite_y to move sprites 2 pixels down
        let diff = next_scanline.wrapping_sub((sprite_y.wrapping_add(1)) as u16);

        if diff < sprite_height as u16 {
            // Sprite is in range, copy all 4 bytes to secondary OAM
            let sec_oam_index = (self.sprites_found as usize) * 4;
            for i in 0..4 {
                self.secondary_oam[sec_oam_index + i] = self.oam_data[oam_index + i];
            }

            // Track if this is sprite 0
            if self.sprite_eval_n == 0 {
                self.next_sprite_0_index = Some(self.sprites_found as usize);
            }

            self.sprites_found += 1;
        }

        self.sprite_eval_n += 1;
        false
    }

    /// Fetch sprite pattern data
    pub fn fetch_sprite_pattern<F>(
        &mut self,
        pixel: u16,
        scanline: u16,
        sprite_height: u8,
        sprite_pattern_table_base: u16,
        read_chr: F,
    ) where
        F: Fn(u16) -> u8,
    {
        let cycle_offset = pixel - 257;
        let sprite_index = (cycle_offset / 8) as usize;
        let fetch_step = cycle_offset % 8;

        if fetch_step == 7 && sprite_index < self.sprites_found as usize {
            let sec_oam_offset = sprite_index * 4;
            let sprite_y = self.secondary_oam[sec_oam_offset];
            let tile_index = self.secondary_oam[sec_oam_offset + 1];
            let attributes = self.secondary_oam[sec_oam_offset + 2];

            let next_scanline = if scanline == 261 { 0 } else { scanline + 1 };
            // Adjust Y position: add 1 to sprite_y to move sprites 2 pixels down
            let sprite_row = next_scanline.wrapping_sub((sprite_y.wrapping_add(1)) as u16) as u8;

            // Calculate pattern address
            let pattern_table_base = if sprite_height == 8 {
                // Use pattern table base from PPUCTRL (provided by caller)
                sprite_pattern_table_base
            } else {
                // 8x16 sprites: use bit 0 of tile index
                ((tile_index & 0x01) as u16) << 12
            };

            let tile_offset = if sprite_height == 8 {
                (tile_index as u16) << 4
            } else {
                ((tile_index & 0xFE) as u16) << 4
            };

            let effective_row = if (attributes & 0x80) != 0 {
                if sprite_height == 8 {
                    7 - sprite_row
                } else {
                    15 - sprite_row
                }
            } else {
                sprite_row
            };

            let tile_row = if sprite_height == 16 && effective_row >= 8 {
                effective_row - 8 + 16
            } else {
                effective_row
            };

            let addr = pattern_table_base | tile_offset | (tile_row as u16);

            let pattern_lo = read_chr(addr);
            let pattern_hi = read_chr(addr + 8);

            let (final_lo, final_hi) = if (attributes & 0x40) != 0 {
                (pattern_lo.reverse_bits(), pattern_hi.reverse_bits())
            } else {
                (pattern_lo, pattern_hi)
            };

            self.next_sprite_pattern_shift_lo[sprite_index] = final_lo;
            self.next_sprite_pattern_shift_hi[sprite_index] = final_hi;
            self.next_sprite_attributes[sprite_index] = attributes;
            self.next_sprite_x_positions[sprite_index] = self.secondary_oam[sec_oam_offset + 3];
        }
    }

    /// Swap sprite buffers for next scanline
    pub fn swap_buffers(&mut self) {
        if self.sprite_buffers_ready {
            std::mem::swap(
                &mut self.sprite_pattern_shift_lo,
                &mut self.next_sprite_pattern_shift_lo,
            );
            std::mem::swap(
                &mut self.sprite_pattern_shift_hi,
                &mut self.next_sprite_pattern_shift_hi,
            );
            std::mem::swap(
                &mut self.sprite_x_positions,
                &mut self.next_sprite_x_positions,
            );
            std::mem::swap(
                &mut self.sprite_attributes,
                &mut self.next_sprite_attributes,
            );
            std::mem::swap(&mut self.sprite_count, &mut self.next_sprite_count);
            std::mem::swap(&mut self.sprite_0_index, &mut self.next_sprite_0_index);
        }
    }

    /// Reset sprite evaluation state for a new scanline
    pub fn reset_evaluation(&mut self) {
        self.sprites_found = 0;
        self.sprite_eval_n = 0;
        self.sprite_eval_m = 0;
        self.next_sprite_0_index = None;
    }

    /// Finalize sprite count for next scanline
    pub fn finalize_evaluation(&mut self) {
        self.next_sprite_count = self.sprites_found;
    }

    /// Mark buffers as ready
    pub fn mark_buffers_ready(&mut self) {
        self.sprite_buffers_ready = true;
    }

    /// Get sprite pixel at current position
    /// Returns (palette_index, sprite_index, is_foreground) or None
    pub fn get_pixel(&self, screen_x: i16, show_sprites_left: bool) -> Option<(u8, usize, bool)> {
        // Check if we should clip sprites in leftmost 8 pixels
        if screen_x < 8 && !show_sprites_left {
            return None;
        }

        for sprite_idx in 0..(self.sprite_count as usize) {
            let sprite_x = self.sprite_x_positions[sprite_idx] as i16;
            // Sprites render with a 2-pixel pipeline offset
            // This emerges from how the sprite shifters are loaded and advanced
            let shift = screen_x - (sprite_x - 2);

            if shift >= 0 && shift < 8 {
                let bit_pos = 7 - (shift as u8);
                let pattern_lo_bit =
                    ((self.sprite_pattern_shift_lo[sprite_idx] >> bit_pos) & 0x01) as u8;
                let pattern_hi_bit =
                    ((self.sprite_pattern_shift_hi[sprite_idx] >> bit_pos) & 0x01) as u8;
                let pattern = (pattern_hi_bit << 1) | pattern_lo_bit;

                if pattern == 0 {
                    continue;
                }

                let attributes = self.sprite_attributes[sprite_idx];
                let palette = attributes & 0x03;
                let is_foreground = (attributes & 0x20) == 0;

                let palette_index = 16 + palette * 4 + pattern;

                return Some((palette_index, sprite_idx, is_foreground));
            }
        }

        None
    }

    /// Check if sprite 0 is in the current sprite buffer at the given index
    pub fn is_sprite_0(&self, sprite_idx: usize) -> bool {
        self.sprite_0_index.map_or(false, |idx| idx == sprite_idx)
    }

    /// Get sprite 0 X position (if sprite 0 is in the current scanline)
    pub fn sprite_0_x_position(&self) -> Option<u8> {
        self.sprite_0_index.map(|idx| self.sprite_x_positions[idx])
    }

    /// Check if sprite 0 has a non-transparent pixel at the given screen position
    /// This is used for sprite 0 hit detection and doesn't apply sprite clipping
    /// (clipping is handled separately in hit detection logic)
    pub fn sprite_0_pixel_at(&self, screen_x: i16) -> bool {
        if let Some(sprite_0_idx) = self.sprite_0_index {
            let sprite_x = self.sprite_x_positions[sprite_0_idx] as i16;
            
            // Check if sprite 0 has a pixel at this screen position
            // Uses same offset as sprite rendering (pipeline effect)
            let shift = screen_x - (sprite_x - 2);

            if shift >= 0 && shift < 8 {
                let bit_pos = 7 - (shift as u8);
                let pattern_lo_bit =
                    ((self.sprite_pattern_shift_lo[sprite_0_idx] >> bit_pos) & 0x01) as u8;
                let pattern_hi_bit =
                    ((self.sprite_pattern_shift_hi[sprite_0_idx] >> bit_pos) & 0x01) as u8;
                let pattern = (pattern_hi_bit << 1) | pattern_lo_bit;

                return pattern != 0;
            }
        }
        false
    }

    /// Get sprite count for rendering
    pub fn sprite_count(&self) -> u8 {
        self.sprite_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sprites_new() {
        let sprites = Sprites::new();
        assert_eq!(sprites.sprite_count(), 0);
    }

    #[test]
    fn test_write_read_oam() {
        let mut sprites = Sprites::new();
        sprites.write_oam(0, 0x42);
        assert_eq!(sprites.read_oam(0), 0x42);
    }

    #[test]
    fn test_initialize_secondary_oam() {
        let mut sprites = Sprites::new();
        sprites.initialize_secondary_oam_byte(1);
        assert_eq!(sprites.secondary_oam[0], 0xFF);
    }

    #[test]
    fn test_reset_evaluation() {
        let mut sprites = Sprites::new();
        sprites.sprites_found = 5;
        sprites.reset_evaluation();
        assert_eq!(sprites.sprites_found, 0);
    }

    #[test]
    fn test_get_pixel_no_sprites() {
        let sprites = Sprites::new();
        assert!(sprites.get_pixel(10, true).is_none());
    }

    #[test]
    fn test_sprite_x_position_offset() {
        let mut sprites = Sprites::new();
        // Set up a sprite at X position 10
        sprites.sprite_count = 1;
        sprites.sprite_x_positions[0] = 10;
        sprites.sprite_pattern_shift_lo[0] = 0b11111111;
        sprites.sprite_pattern_shift_hi[0] = 0b00000000;
        sprites.sprite_attributes[0] = 0x00; // Palette 0, foreground

        // With our 2-pixel left adjustment, screen_x 8 should hit sprite at X=10
        // because: shift = 8 - (10 - 2) = 8 - 8 = 0
        let result = sprites.get_pixel(8, true);
        assert!(result.is_some());

        // screen_x 7 should miss (shift = 7 - 8 = -1, which is < 0)
        let result = sprites.get_pixel(7, true);
        assert!(result.is_none());

        // screen_x 15 should hit (shift = 15 - 8 = 7, which is < 8)
        let result = sprites.get_pixel(15, true);
        assert!(result.is_some());

        // screen_x 16 should miss (shift = 16 - 8 = 8, which is >= 8)
        let result = sprites.get_pixel(16, true);
        assert!(result.is_none());
    }

    #[test]
    fn test_sprite_y_position_offset() {
        let mut sprites = Sprites::new();
        // Set up OAM data for a sprite at Y position 10
        sprites.oam_data[0] = 10; // Y position
        sprites.oam_data[1] = 0; // Tile index
        sprites.oam_data[2] = 0; // Attributes
        sprites.oam_data[3] = 50; // X position

        // Evaluate sprites for scanline 11 (evaluates for next scanline 12)
        // With our +1 adjustment: diff = 12 - (10 + 1) = 1, which is < 8 (sprite height)
        // So the sprite should be included
        sprites.reset_evaluation();
        sprites.evaluate_sprites(65, 11, 8); // pixel 65 triggers evaluation
        assert_eq!(sprites.sprites_found, 1);

        // Evaluate sprite for scanline 10 (evaluates for next scanline 11)
        // diff = 11 - (10 + 1) = 0, which is < 8, should be included
        sprites.reset_evaluation();
        sprites.evaluate_sprites(65, 10, 8);
        assert_eq!(sprites.sprites_found, 1);

        // Evaluate sprite for scanline 18 (evaluates for next scanline 19)
        // diff = 19 - (10 + 1) = 8, which is >= 8, should NOT be included
        sprites.reset_evaluation();
        sprites.evaluate_sprites(65, 18, 8);
        assert_eq!(sprites.sprites_found, 0);
    }

    #[test]
    fn test_sprite_pattern_fetch_with_y_offset() {
        let mut sprites = Sprites::new();
        // Set up sprite in secondary OAM
        sprites.sprites_found = 1;
        sprites.secondary_oam[0] = 50; // Y position
        sprites.secondary_oam[1] = 0x00; // Tile index
        sprites.secondary_oam[2] = 0x00; // Attributes (no flip)
        sprites.secondary_oam[3] = 100; // X position

        // Mock CHR read function that returns different values for pattern lo/hi
        let read_chr = |addr: u16| -> u8 {
            if addr & 0x08 == 0 {
                0xAA // Pattern low
            } else {
                0x55 // Pattern high
            }
        };

        // Fetch pattern for scanline 52
        // With our +1 adjustment: sprite_row = 52 - (50 + 1) = 1
        sprites.fetch_sprite_pattern(257 + 7, 51, 8, 0x0000, read_chr);

        // Verify pattern data was fetched
        assert_eq!(sprites.next_sprite_pattern_shift_lo[0], 0xAA);
        assert_eq!(sprites.next_sprite_pattern_shift_hi[0], 0x55);
        assert_eq!(sprites.next_sprite_x_positions[0], 100);
    }

    #[test]
    fn test_sprite_clipping_left_8_pixels() {
        let mut sprites = Sprites::new();
        // Set up a sprite at X position 2 (which with -2 offset becomes screen X 0)
        sprites.sprite_count = 1;
        sprites.sprite_x_positions[0] = 2;
        sprites.sprite_pattern_shift_lo[0] = 0xFF;
        sprites.sprite_pattern_shift_hi[0] = 0x00;
        sprites.sprite_attributes[0] = 0x00;

        // With show_sprites_left = false, sprites in X < 8 should be clipped
        let result = sprites.get_pixel(0, false);
        assert!(result.is_none());

        // With show_sprites_left = true, sprites in X < 8 should be shown
        let result = sprites.get_pixel(0, true);
        assert!(result.is_some());
    }

    #[test]
    fn test_sprite_transparent_pixels() {
        let mut sprites = Sprites::new();
        sprites.sprite_count = 1;
        sprites.sprite_x_positions[0] = 10;
        // Pattern with some transparent pixels (pattern = 0)
        sprites.sprite_pattern_shift_lo[0] = 0b10101010;
        sprites.sprite_pattern_shift_hi[0] = 0b00000000;
        sprites.sprite_attributes[0] = 0x00;

        // screen_x 8 should hit non-transparent pixel (bit 7 of lo = 1)
        let result = sprites.get_pixel(8, true);
        assert!(result.is_some());

        // screen_x 9 should be transparent (bit 6 of lo = 0, bit 6 of hi = 0)
        let result = sprites.get_pixel(9, true);
        assert!(result.is_none());
    }
}
