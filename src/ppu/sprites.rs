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
    /// Cycle counter for sprite evaluation timing (0-7 for copying sprite data)
    sprite_eval_cycle: u8,
    /// Whether current sprite being evaluated is in range
    sprite_eval_in_range: bool,
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
            sprite_eval_cycle: 0,
            sprite_eval_in_range: false,
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
        self.sprite_eval_cycle = 0;
        self.sprite_eval_in_range = false;
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

    /// Evaluate sprites for the current scanline (cycle-accurate)
    /// Hardware performs: read on odd cycles, write on even cycles
    pub fn evaluate_sprites(&mut self, pixel: u16, scanline: u16, sprite_height: u8) -> bool {
        let is_odd_cycle = (pixel % 2) == 1;

        // Stop if we've evaluated all 64 sprites
        if self.sprite_eval_n >= 64 {
            return false;
        }

        let mut overflow = false;

        // If we've already found 8 sprites, enter overflow checking mode
        if self.sprites_found >= 8 {
            // NES PPU Hardware Bug: Sprite Overflow Detection
            // Takes 2 cycles: read Y on odd, check and set flag on even

            if is_odd_cycle {
                // Odd cycle: read Y byte
                let oam_index = (self.sprite_eval_n as usize) * 4 + (self.sprite_eval_m as usize);

                if oam_index < 256 {
                    let sprite_y = self.oam_data[oam_index];

                    let next_scanline = scanline + 1;
                    // Adjust Y position: add 1 to sprite_y (same as normal evaluation)
                    let diff = next_scanline.wrapping_sub((sprite_y.wrapping_add(1)) as u16);

                    // Sprites with Y < 240 (0xF0) participate in overflow detection
                    // Store result for next cycle
                    self.sprite_eval_in_range = diff < sprite_height as u16 && sprite_y < 0xF0;
                } else {
                    self.sprite_eval_in_range = false;
                }
                return false;
            } else {
                // Even cycle: set flag if sprite was in range, then increment indices
                if self.sprite_eval_in_range {
                    overflow = true;
                }

                // THE BUG: Increment BOTH n and m
                self.sprite_eval_n += 1;
                self.sprite_eval_m += 1;

                if self.sprite_eval_m >= 4 {
                    self.sprite_eval_m = 0;
                }
                return overflow;
            }
        }

        // Normal sprite evaluation (first 8 sprites) - cycle accurate
        // Read on odd cycles, write on even cycles
        // Each sprite: Y (read+write), tile (read+write), attr (read+write), X (read+write) = 8 cycles

        if self.sprite_eval_cycle == 0 {
            if !is_odd_cycle {
                // Even cycle but we're at cycle 0 - shouldn't happen
                return false;
            }

            // Odd cycle 0: Read Y byte
            let oam_index = (self.sprite_eval_n as usize) * 4;
            let sprite_y = self.oam_data[oam_index];

            // Sprites with Y >= 240 (0xF0) don't render
            if sprite_y >= 0xF0 {
                self.sprite_eval_in_range = false;
                self.sprite_eval_cycle = 1;
                return false;
            }

            let next_scanline = scanline + 1;
            // Adjust Y position: add 1 to sprite_y
            let diff = next_scanline.wrapping_sub((sprite_y.wrapping_add(1)) as u16);

            self.sprite_eval_in_range = diff < sprite_height as u16;
            self.sprite_eval_cycle = 1;
            return false;
        }

        if self.sprite_eval_cycle == 1 {
            if is_odd_cycle {
                // Odd cycle but we're at cycle 1 (even expected) - shouldn't happen
                return false;
            }

            // Even cycle 1: Write Y or dummy write
            if !self.sprite_eval_in_range {
                // Out of range - done with this sprite
                self.sprite_eval_n += 1;
                self.sprite_eval_cycle = 0;
                return false;
            }

            // In range - write Y to secondary OAM
            let oam_index = (self.sprite_eval_n as usize) * 4;
            let sec_oam_index = (self.sprites_found as usize) * 4;
            self.secondary_oam[sec_oam_index] = self.oam_data[oam_index];
            self.sprite_eval_cycle = 2;
            return false;
        }

        // Cycles 2-7: Copy remaining sprite data
        // Cycle 2 (odd): read tile, Cycle 3 (even): write tile
        // Cycle 4 (odd): read attr, Cycle 5 (even): write attr
        // Cycle 6 (odd): read X, Cycle 7 (even): write X
        if self.sprite_eval_cycle >= 2 && self.sprite_eval_cycle <= 7 {
            let oam_index = (self.sprite_eval_n as usize) * 4;
            let sec_oam_index = (self.sprites_found as usize) * 4;

            // Odd cycles: read from OAM
            // Even cycles: write to secondary OAM
            if self.sprite_eval_cycle == 2 && is_odd_cycle {
                // Read tile byte (actual read happens in hardware, we just advance)
                self.sprite_eval_cycle = 3;
            } else if self.sprite_eval_cycle == 3 && !is_odd_cycle {
                // Write tile byte
                self.secondary_oam[sec_oam_index + 1] = self.oam_data[oam_index + 1];
                self.sprite_eval_cycle = 4;
            } else if self.sprite_eval_cycle == 4 && is_odd_cycle {
                // Read attribute byte
                self.sprite_eval_cycle = 5;
            } else if self.sprite_eval_cycle == 5 && !is_odd_cycle {
                // Write attribute byte
                self.secondary_oam[sec_oam_index + 2] = self.oam_data[oam_index + 2];
                self.sprite_eval_cycle = 6;
            } else if self.sprite_eval_cycle == 6 && is_odd_cycle {
                // Read X byte
                self.sprite_eval_cycle = 7;
            } else if self.sprite_eval_cycle == 7 && !is_odd_cycle {
                // Write X byte - last byte
                self.secondary_oam[sec_oam_index + 3] = self.oam_data[oam_index + 3];

                // Track if this is sprite 0
                if self.sprite_eval_n == 0 {
                    self.next_sprite_0_index = Some(self.sprites_found as usize);
                }

                self.sprites_found += 1;
                self.sprite_eval_n += 1;
                self.sprite_eval_cycle = 0; // Reset for next sprite
            }

            return false;
        }

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
        self.sprite_eval_cycle = 0;
        self.sprite_eval_in_range = false;
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
            // X coordinate maps directly per NES hardware specification
            let shift = screen_x - sprite_x;

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

    /// Get sprite 0's Y position from OAM (byte 0 of sprite 0)
    pub fn sprite_0_oam_y(&self) -> u8 {
        self.oam_data[0]
    }

    /// Check if sprite 0 has a non-transparent pixel at the given screen position
    /// This is used for sprite 0 hit detection and doesn't apply sprite clipping
    /// (clipping is handled separately in hit detection logic)
    pub fn sprite_0_pixel_at(&self, screen_x: i16) -> bool {
        // Sprite 0 hit never occurs at screen X=255 (hardware quirk)
        if screen_x == 255 {
            return false;
        }

        if let Some(sprite_0_idx) = self.sprite_0_index {
            let sprite_x = self.sprite_x_positions[sprite_0_idx] as i16;

            // Check if sprite 0 has a pixel at this screen position
            // X coordinate maps directly per NES hardware specification
            let shift = screen_x - sprite_x;

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

        // Per NES hardware spec: X coordinate maps directly (screen_x = OAM.X)
        // Sprite at X=10 should render at screen pixels 10-17
        // screen_x 10 should hit sprite at X=10
        // because: shift = 10 - 10 = 0
        let result = sprites.get_pixel(10, true);
        assert!(result.is_some());

        // screen_x 9 should miss (shift = 9 - 10 = -1, which is < 0)
        let result = sprites.get_pixel(9, true);
        assert!(result.is_none());

        // screen_x 17 should hit (shift = 17 - 10 = 7, which is < 8)
        let result = sprites.get_pixel(17, true);
        assert!(result.is_some());

        // screen_x 18 should miss (shift = 18 - 10 = 8, which is >= 8)
        let result = sprites.get_pixel(18, true);
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
        // With cycle-accurate evaluation, need to run multiple cycles (every pixel)
        sprites.reset_evaluation();
        for pixel in 65..=72 {
            // 8 cycles for in-range sprite
            sprites.evaluate_sprites(pixel, 11, 8);
        }
        assert_eq!(sprites.sprites_found, 1);

        // Evaluate sprite for scanline 10 (evaluates for next scanline 11)
        // diff = 11 - (10 + 1) = 0, which is < 8, should be included
        sprites.reset_evaluation();
        for pixel in 65..=72 {
            sprites.evaluate_sprites(pixel, 10, 8);
        }
        assert_eq!(sprites.sprites_found, 1);

        // Evaluate sprite for scanline 18 (evaluates for next scanline 19)
        // diff = 19 - (10 + 1) = 8, which is >= 8, should NOT be included
        sprites.reset_evaluation();
        for pixel in 65..=66 {
            // 2 cycles for out-of-range sprite
            sprites.evaluate_sprites(pixel, 18, 8);
        }
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
        // Set up a sprite at X position 0 (maps directly to screen X 0-7)
        sprites.sprite_count = 1;
        sprites.sprite_x_positions[0] = 0;
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

        // screen_x 10 should hit non-transparent pixel (bit 7 of lo = 1)
        let result = sprites.get_pixel(10, true);
        assert!(result.is_some());

        // screen_x 11 should be transparent (bit 6 of lo = 0, bit 6 of hi = 0)
        let result = sprites.get_pixel(11, true);
        assert!(result.is_none());
    }
}
