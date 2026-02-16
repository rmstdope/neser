use crate::console::SpritesState;

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
    /// Remaining bytes to read for an in-range sprite during overflow detection.
    /// When a sprite is found in range during overflow, the hardware reads the
    /// remaining 3 bytes (incrementing only m, not n). This counts down from 3 to 0.
    sprite_eval_overflow_reads_remaining: u8,
    /// Set when the overflow flag has been signaled during this evaluation.
    /// After the first in-range detection + remaining reads, the PPU switches to
    /// a simpler mode: m is forced to 0, only n increments, no more remaining reads.
    sprite_eval_overflow_signaled: bool,
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
    /// Internal OAM read latch - reflects the value on the internal OAM bus during rendering.
    /// During rendering, $2004 reads return this latch instead of OAM[OAMADDR].
    oam_read_latch: u8,
}

impl Default for Sprites {
    fn default() -> Self {
        Self::new(crate::console::RamInitMode::Zero)
    }
}

/// OAM attribute byte mask - bits 2-4 are unimplemented and always read as 0
/// Mask: 11100011 (0xE3) - preserves bits 7-5 (priority/palette) and 1-0 (flip bits)
const OAM_ATTRIBUTE_MASK: u8 = 0xE3;

impl Sprites {
    /// Create a new Sprites instance
    pub fn new(ram_init_mode: crate::console::RamInitMode) -> Self {
        let mut oam_data = [0u8; 256];
        crate::console::initialize_ram(&mut oam_data, ram_init_mode);

        Self {
            oam_data,
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
            sprite_eval_overflow_reads_remaining: 0,
            sprite_eval_overflow_signaled: false,
            sprite_pattern_shift_lo: [0; 8],
            sprite_pattern_shift_hi: [0; 8],
            sprite_x_positions: [0; 8],
            sprite_attributes: [0; 8],
            next_sprite_pattern_shift_lo: [0; 8],
            next_sprite_pattern_shift_hi: [0; 8],
            next_sprite_x_positions: [0; 8],
            next_sprite_attributes: [0; 8],
            oam_read_latch: 0,
        }
    }

    /// Reset sprite state
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on/hard reset
    /// - `ram_init_mode`: RAM initialization mode (only used for hard reset)
    pub fn reset(&mut self, soft_reset: bool, ram_init_mode: crate::console::RamInitMode) {
        // On hard reset, re-initialize OAM data based on configured mode
        if !soft_reset {
            crate::console::initialize_ram(&mut self.oam_data, ram_init_mode);
        }

        // Always reset secondary OAM and evaluation state
        self.secondary_oam = [0xFF; 32];
        self.sprites_found = 0;
        self.sprite_eval_n = 0;
        self.sprite_eval_m = 0;
        self.sprite_eval_cycle = 0;
        self.sprite_eval_in_range = false;
        self.sprite_eval_overflow_reads_remaining = 0;
        self.sprite_eval_overflow_signaled = false;
        self.oam_read_latch = 0;
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
        self.oam_read_latch = 0xFF;
    }

    /// Evaluate sprites for the current scanline (cycle-accurate)
    /// Hardware performs: read on odd cycles, write on even cycles
    pub fn evaluate_sprites(&mut self, pixel: u16, scanline: u16, sprite_height: u8) -> bool {
        let is_odd_cycle = (pixel % 2) == 1;

        // Stop if we've evaluated all 64 sprites.
        // In post-overflow mode, n wraps (6-bit counter) and continues
        // reading Y bytes until the evaluation dot window ends.
        if self.sprite_eval_n >= 64 {
            if self.sprite_eval_overflow_signaled {
                self.sprite_eval_n = 0;
            } else {
                return false;
            }
        }

        let mut overflow = false;

        // If we've already found 8 sprites, enter overflow checking mode
        if self.sprites_found >= 8 {
            // NES PPU Hardware Bug: Sprite Overflow Detection
            //
            // Before overflow flag is set:
            //   Not in range: both n and m increment (THE BUG)
            //   In range: set overflow flag, read remaining 3 bytes (m increments
            //   with carry to n), then force m=0 and switch to post-overflow mode.
            //
            // After overflow flag is set (post-overflow):
            //   Only n increments, m stays at 0. Reads Y byte of each sprite.
            //   No more remaining reads or n+m bug.

            if is_odd_cycle {
                // Odd cycle: read from primary OAM
                let oam_index = (self.sprite_eval_n as usize) * 4 + (self.sprite_eval_m as usize);

                if oam_index < 256 {
                    let value = self.oam_data[oam_index];
                    self.oam_read_latch = value;

                    if self.sprite_eval_overflow_signaled
                        && self.sprite_eval_overflow_reads_remaining == 0
                    {
                        // Post-overflow: just read, no range check needed
                        self.sprite_eval_in_range = false;
                    } else if self.sprite_eval_overflow_reads_remaining > 0 {
                        // Reading remaining bytes of an in-range sprite — no range check
                        self.sprite_eval_in_range = false;
                    } else {
                        // Initial Y check: compare value against scanline
                        let next_scanline = scanline + 1;
                        let diff = next_scanline.wrapping_sub((value.wrapping_add(1)) as u16);
                        self.sprite_eval_in_range = diff < sprite_height as u16 && value < 0xF0;
                    }
                } else {
                    self.sprite_eval_in_range = false;
                }
                return false;
            } else {
                // Even cycle: process the result
                // On write cycles during overflow detection, the secondary OAM bus
                // is driven at address 0 (write pointer wrapped after filling 8 slots).
                self.oam_read_latch = self.secondary_oam[0];

                if self.sprite_eval_overflow_reads_remaining > 0 {
                    // Reading remaining bytes of an in-range sprite
                    self.sprite_eval_overflow_reads_remaining -= 1;
                    self.sprite_eval_m += 1;
                    if self.sprite_eval_m >= 4 {
                        self.sprite_eval_m = 0;
                        // m wraps 3→0: carry to n (same as normal sprite copy)
                        self.sprite_eval_n += 1;
                    }

                    if self.sprite_eval_overflow_reads_remaining == 0 {
                        // Remaining reads done: force m=0 for post-overflow mode
                        self.sprite_eval_m = 0;
                        self.sprite_eval_overflow_signaled = true;
                    }
                    return false;
                }

                if self.sprite_eval_overflow_signaled {
                    // Post-overflow: only increment n, m stays at 0
                    self.sprite_eval_n += 1;
                    return false;
                }

                if self.sprite_eval_in_range {
                    // Found in range: set overflow flag and begin reading remaining bytes
                    overflow = true;
                    self.sprite_eval_overflow_reads_remaining = 3;
                    // Increment m to read next byte (carry to n if m wraps)
                    self.sprite_eval_m += 1;
                    if self.sprite_eval_m >= 4 {
                        self.sprite_eval_m = 0;
                        self.sprite_eval_n += 1;
                    }
                } else {
                    // Not in range: THE BUG — increment BOTH n and m
                    self.sprite_eval_n += 1;
                    self.sprite_eval_m += 1;
                    if self.sprite_eval_m >= 4 {
                        self.sprite_eval_m = 0;
                    }
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
            self.oam_read_latch = sprite_y;

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
                // Read tile byte
                self.oam_read_latch = self.oam_data[oam_index + 1];
                self.sprite_eval_cycle = 3;
            } else if self.sprite_eval_cycle == 3 && !is_odd_cycle {
                // Write tile byte
                self.secondary_oam[sec_oam_index + 1] = self.oam_data[oam_index + 1];
                self.sprite_eval_cycle = 4;
            } else if self.sprite_eval_cycle == 4 && is_odd_cycle {
                // Read attribute byte
                self.oam_read_latch = self.oam_data[oam_index + 2];
                self.sprite_eval_cycle = 5;
            } else if self.sprite_eval_cycle == 5 && !is_odd_cycle {
                // Write attribute byte
                self.secondary_oam[sec_oam_index + 2] = self.oam_data[oam_index + 2];
                self.sprite_eval_cycle = 6;
            } else if self.sprite_eval_cycle == 6 && is_odd_cycle {
                // Read X byte
                self.oam_read_latch = self.oam_data[oam_index + 3];
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
    ///
    /// IMPORTANT: The PPU hardware ALWAYS fetches 8 sprites worth of pattern data during
    /// pixels 257-320, regardless of how many sprites were found during evaluation.
    /// For sprites not found (sprite_index >= sprites_found), the PPU fetches using
    /// tile $FF at Y coordinate $FF. This is critical for MMC3 IRQ timing as the
    /// A12 transitions must happen consistently even when no sprites are on screen.
    ///
    /// PPU sprite fetch timing within each 8-cycle sprite slot:
    /// - Cycles 0-1: Garbage nametable byte (no CHR read)
    /// - Cycles 2-3: Garbage nametable byte (no CHR read)
    /// - Cycles 4-5: Pattern table tile low byte (CHR read, A12 may rise)
    /// - Cycles 6-7: Pattern table tile high byte (CHR read)
    ///
    /// For sprite 0 (pixels 257-264):
    /// - Pattern low read at pixel 261 (cycle 4 within sprite fetch)
    /// - Pattern high read at pixel 263 (cycle 6 within sprite fetch)
    ///
    /// This timing is critical for MMC3 IRQ - the A12 rising edge should occur
    /// at around PPU cycle 260-261 (first sprite pattern fetch).
    pub fn fetch_sprite_pattern<F>(
        &mut self,
        pixel: u16,
        scanline: u16,
        prerender_scanline: u16,
        sprite_height: u8,
        sprite_pattern_table_base: u16,
        mut read_chr: F,
    ) where
        F: FnMut(u16) -> u8,
    {
        let cycle_offset = pixel - 257;
        let sprite_index = (cycle_offset / 8) as usize;
        let fetch_step = cycle_offset % 8;

        // Only process valid sprite slots
        if sprite_index >= 8 {
            return;
        }

        let is_pattern_lo_cycle = fetch_step == 5;
        let is_pattern_hi_cycle = fetch_step == 7;

        // On pre-render scanline, sprite evaluation doesn't happen, so
        // secondary_oam contains stale data from scanline 239's evaluation.
        // Those sprites were evaluated for scanline 240, not scanline 0.
        // We must treat all sprites as "dummy" on pre-render to avoid using
        // stale data that would cause calculation errors.
        let is_prerender = scanline == prerender_scanline;

        // Determine if this is a real sprite or a "dummy" fetch
        let is_real_sprite = !is_prerender && sprite_index < self.sprites_found as usize;

        let (tile_index, attributes, sprite_y) = if is_real_sprite {
            let sec_oam_offset = sprite_index * 4;
            (
                self.secondary_oam[sec_oam_offset + 1],
                self.secondary_oam[sec_oam_offset + 2],
                self.secondary_oam[sec_oam_offset],
            )
        } else {
            // For dummy fetches, use tile $FF at Y=$FF
            // The NES hardware reads these even though no sprite is visible
            (0xFF, 0, 0xFF)
        };

        // Update the OAM read latch with the secondary OAM byte being read
        // during this fetch step. Each 8-cycle slot reads: Y(0-1), tile(2-3), attr(4-5), X(6-7)
        {
            let sec_oam_offset = sprite_index * 4;
            let byte_index = (fetch_step / 2) as usize; // 0=Y, 1=tile, 2=attr, 3=X
            self.oam_read_latch = self.secondary_oam[sec_oam_offset + byte_index];
        }

        let next_scanline = if scanline == prerender_scanline {
            0
        } else {
            scanline + 1
        };
        // Adjust Y position: add 1 to sprite_y to move sprites 2 pixels down
        let sprite_row = next_scanline.wrapping_sub((sprite_y.wrapping_add(1)) as u16) as u8;

        // Mask sprite_row to valid range for sprite height
        // For 8x8 sprites: 0-7, for 8x16 sprites: 0-15
        let sprite_row = if sprite_height == 8 {
            sprite_row & 0x07
        } else {
            sprite_row & 0x0F
        };

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

        if is_pattern_lo_cycle {
            // Read pattern low byte - this triggers the A12 rising edge for MMC3
            let pattern_lo = read_chr(addr);

            if is_real_sprite {
                self.next_sprite_pattern_shift_lo[sprite_index] = if (attributes & 0x40) != 0 {
                    pattern_lo.reverse_bits()
                } else {
                    pattern_lo
                };
                self.next_sprite_attributes[sprite_index] = attributes;
                let sec_oam_offset = sprite_index * 4;
                self.next_sprite_x_positions[sprite_index] = self.secondary_oam[sec_oam_offset + 3];
            }
        } else if is_pattern_hi_cycle {
            // Read pattern high byte
            let pattern_hi = read_chr(addr + 8);

            if is_real_sprite {
                self.next_sprite_pattern_shift_hi[sprite_index] = if (attributes & 0x40) != 0 {
                    pattern_hi.reverse_bits()
                } else {
                    pattern_hi
                };
            }
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

    /// Update OAM read latch to secondary_oam[0] during idle sprite bus cycles.
    /// Per NES hardware: during cycles 321-340, the PPU repeatedly reads the first
    /// byte of secondary OAM onto the internal bus.
    pub fn update_idle_oam_latch(&mut self) {
        self.oam_read_latch = self.secondary_oam[0];
    }

    /// Reset sprite evaluation state for a new scanline
    pub fn reset_evaluation(&mut self) {
        self.sprites_found = 0;
        self.sprite_eval_n = 0;
        self.sprite_eval_m = 0;
        self.sprite_eval_cycle = 0;
        self.sprite_eval_in_range = false;
        self.sprite_eval_overflow_reads_remaining = 0;
        self.sprite_eval_overflow_signaled = false;
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

            if (0..8).contains(&shift) {
                let bit_pos = 7 - (shift as u8);
                let pattern_lo_bit = (self.sprite_pattern_shift_lo[sprite_idx] >> bit_pos) & 0x01;
                let pattern_hi_bit = (self.sprite_pattern_shift_hi[sprite_idx] >> bit_pos) & 0x01;
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

    // /// Check if sprite 0 is in the current sprite buffer at the given index
    // pub fn is_sprite_0(&self, sprite_idx: usize) -> bool {
    //     self.sprite_0_index.map_or(false, |idx| idx == sprite_idx)
    // }

    // /// Get sprite 0 X position (if sprite 0 is in the current scanline)
    // pub fn sprite_0_x_position(&self) -> Option<u8> {
    //     self.sprite_0_index.map(|idx| self.sprite_x_positions[idx])
    // }

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

            if (0..8).contains(&shift) {
                let bit_pos = 7 - (shift as u8);
                let pattern_lo_bit = (self.sprite_pattern_shift_lo[sprite_0_idx] >> bit_pos) & 0x01;
                let pattern_hi_bit = (self.sprite_pattern_shift_hi[sprite_0_idx] >> bit_pos) & 0x01;
                let pattern = (pattern_hi_bit << 1) | pattern_lo_bit;

                return pattern != 0;
            }
        }
        false
    }

    /// Get sprite count for rendering
    #[cfg(test)]
    pub fn sprite_count(&self) -> u8 {
        self.sprite_count
    }

    /// Create a snapshot of OAM for save-state.
    pub fn oam_snapshot(&self) -> Vec<u8> {
        self.oam_data.to_vec()
    }

    /// Create a snapshot of secondary OAM for save-state.
    pub fn secondary_oam_snapshot(&self) -> Vec<u8> {
        self.secondary_oam.to_vec()
    }
}

impl Sprites {
    pub fn capture_state(&self) -> SpritesState {
        SpritesState {
            oam_data: self.oam_data,
            secondary_oam: self.secondary_oam,
            sprites_found: self.sprites_found,
            sprite_count: self.sprite_count,
            next_sprite_count: self.next_sprite_count,
            sprite_buffers_ready: self.sprite_buffers_ready,
            sprite_0_index: self.sprite_0_index,
            next_sprite_0_index: self.next_sprite_0_index,
            sprite_eval_n: self.sprite_eval_n,
            sprite_eval_m: self.sprite_eval_m,
            sprite_eval_cycle: self.sprite_eval_cycle,
            sprite_eval_in_range: self.sprite_eval_in_range,
            sprite_eval_overflow_reads_remaining: self.sprite_eval_overflow_reads_remaining,
            sprite_eval_overflow_signaled: self.sprite_eval_overflow_signaled,
            sprite_pattern_shift_lo: self.sprite_pattern_shift_lo,
            sprite_pattern_shift_hi: self.sprite_pattern_shift_hi,
            sprite_x_positions: self.sprite_x_positions,
            sprite_attributes: self.sprite_attributes,
            next_sprite_pattern_shift_lo: self.next_sprite_pattern_shift_lo,
            next_sprite_pattern_shift_hi: self.next_sprite_pattern_shift_hi,
            next_sprite_x_positions: self.next_sprite_x_positions,
            next_sprite_attributes: self.next_sprite_attributes,
            oam_read_latch: self.oam_read_latch,
        }
    }

    pub fn restore_state(&mut self, state: &SpritesState) {
        self.oam_data = state.oam_data;
        self.secondary_oam = state.secondary_oam;
        self.sprites_found = state.sprites_found;
        self.sprite_count = state.sprite_count;
        self.next_sprite_count = state.next_sprite_count;
        self.sprite_buffers_ready = state.sprite_buffers_ready;
        self.sprite_0_index = state.sprite_0_index;
        self.next_sprite_0_index = state.next_sprite_0_index;
        self.sprite_eval_n = state.sprite_eval_n;
        self.sprite_eval_m = state.sprite_eval_m;
        self.sprite_eval_cycle = state.sprite_eval_cycle;
        self.sprite_eval_in_range = state.sprite_eval_in_range;
        self.sprite_eval_overflow_reads_remaining = state.sprite_eval_overflow_reads_remaining;
        self.sprite_eval_overflow_signaled = state.sprite_eval_overflow_signaled;
        self.sprite_pattern_shift_lo = state.sprite_pattern_shift_lo;
        self.sprite_pattern_shift_hi = state.sprite_pattern_shift_hi;
        self.sprite_x_positions = state.sprite_x_positions;
        self.sprite_attributes = state.sprite_attributes;
        self.next_sprite_pattern_shift_lo = state.next_sprite_pattern_shift_lo;
        self.next_sprite_pattern_shift_hi = state.next_sprite_pattern_shift_hi;
        self.next_sprite_x_positions = state.next_sprite_x_positions;
        self.next_sprite_attributes = state.next_sprite_attributes;
        self.oam_read_latch = state.oam_read_latch;
    }
}

impl Sprites {
    /// Returns the internal OAM read latch value.
    /// During rendering, $2004 reads return this instead of OAM[OAMADDR].
    pub fn oam_read_latch(&self) -> u8 {
        self.oam_read_latch
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpritesDebugState {
    pub oam_data: [u8; 256],
    pub secondary_oam: [u8; 32],
    pub sprites_found: u8,
    pub sprite_count: u8,
    pub next_sprite_count: u8,
    pub sprite_buffers_ready: bool,
    pub sprite_0_index: Option<usize>,
    pub next_sprite_0_index: Option<usize>,
    pub sprite_eval_n: u8,
    pub sprite_eval_m: u8,
    pub sprite_eval_cycle: u8,
    pub sprite_eval_in_range: bool,
    pub sprite_eval_overflow_reads_remaining: u8,
    pub sprite_eval_overflow_signaled: bool,
    pub sprite_pattern_shift_lo: [u8; 8],
    pub sprite_pattern_shift_hi: [u8; 8],
    pub sprite_x_positions: [u8; 8],
    pub sprite_attributes: [u8; 8],
    pub next_sprite_pattern_shift_lo: [u8; 8],
    pub next_sprite_pattern_shift_hi: [u8; 8],
    pub next_sprite_x_positions: [u8; 8],
    pub next_sprite_attributes: [u8; 8],
    pub oam_read_latch: u8,
}

#[cfg(test)]
impl Sprites {
    pub fn debug_state(&self) -> SpritesDebugState {
        SpritesDebugState {
            oam_data: self.oam_data,
            secondary_oam: self.secondary_oam,
            sprites_found: self.sprites_found,
            sprite_count: self.sprite_count,
            next_sprite_count: self.next_sprite_count,
            sprite_buffers_ready: self.sprite_buffers_ready,
            sprite_0_index: self.sprite_0_index,
            next_sprite_0_index: self.next_sprite_0_index,
            sprite_eval_n: self.sprite_eval_n,
            sprite_eval_m: self.sprite_eval_m,
            sprite_eval_cycle: self.sprite_eval_cycle,
            sprite_eval_in_range: self.sprite_eval_in_range,
            sprite_eval_overflow_reads_remaining: self.sprite_eval_overflow_reads_remaining,
            sprite_eval_overflow_signaled: self.sprite_eval_overflow_signaled,
            sprite_pattern_shift_lo: self.sprite_pattern_shift_lo,
            sprite_pattern_shift_hi: self.sprite_pattern_shift_hi,
            sprite_x_positions: self.sprite_x_positions,
            sprite_attributes: self.sprite_attributes,
            next_sprite_pattern_shift_lo: self.next_sprite_pattern_shift_lo,
            next_sprite_pattern_shift_hi: self.next_sprite_pattern_shift_hi,
            next_sprite_x_positions: self.next_sprite_x_positions,
            next_sprite_attributes: self.next_sprite_attributes,
            oam_read_latch: self.oam_read_latch,
        }
    }

    pub fn set_debug_state(&mut self, state: SpritesDebugState) {
        self.oam_data = state.oam_data;
        self.secondary_oam = state.secondary_oam;
        self.sprites_found = state.sprites_found;
        self.sprite_count = state.sprite_count;
        self.next_sprite_count = state.next_sprite_count;
        self.sprite_buffers_ready = state.sprite_buffers_ready;
        self.sprite_0_index = state.sprite_0_index;
        self.next_sprite_0_index = state.next_sprite_0_index;
        self.sprite_eval_n = state.sprite_eval_n;
        self.sprite_eval_m = state.sprite_eval_m;
        self.sprite_eval_cycle = state.sprite_eval_cycle;
        self.sprite_eval_in_range = state.sprite_eval_in_range;
        self.sprite_eval_overflow_reads_remaining = state.sprite_eval_overflow_reads_remaining;
        self.sprite_eval_overflow_signaled = state.sprite_eval_overflow_signaled;
        self.sprite_pattern_shift_lo = state.sprite_pattern_shift_lo;
        self.sprite_pattern_shift_hi = state.sprite_pattern_shift_hi;
        self.sprite_x_positions = state.sprite_x_positions;
        self.sprite_attributes = state.sprite_attributes;
        self.next_sprite_pattern_shift_lo = state.next_sprite_pattern_shift_lo;
        self.next_sprite_pattern_shift_hi = state.next_sprite_pattern_shift_hi;
        self.next_sprite_x_positions = state.next_sprite_x_positions;
        self.next_sprite_attributes = state.next_sprite_attributes;
        self.oam_read_latch = state.oam_read_latch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sprites_new() {
        let sprites = Sprites::new(crate::console::RamInitMode::Zero);
        assert_eq!(sprites.sprite_count(), 0);
    }

    #[test]
    fn test_write_read_oam() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.write_oam(0, 0x42);
        assert_eq!(sprites.read_oam(0), 0x42);
    }

    #[test]
    fn test_initialize_secondary_oam() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.initialize_secondary_oam_byte(1);
        assert_eq!(sprites.secondary_oam[0], 0xFF);
    }

    #[test]
    fn test_reset_evaluation() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.sprites_found = 5;
        sprites.reset_evaluation();
        assert_eq!(sprites.sprites_found, 0);
    }

    #[test]
    fn test_get_pixel_no_sprites() {
        let sprites = Sprites::new(crate::console::RamInitMode::Zero);
        assert!(sprites.get_pixel(10, true).is_none());
    }

    #[test]
    fn test_sprite_x_position_offset() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
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
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
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
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
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
        // Need to call for both low byte (fetch_step 5) and high byte (fetch_step 7)
        sprites.fetch_sprite_pattern(257 + 5, 51, 261, 8, 0x0000, read_chr);
        sprites.fetch_sprite_pattern(257 + 7, 51, 261, 8, 0x0000, read_chr);

        // Verify pattern data was fetched
        assert_eq!(sprites.next_sprite_pattern_shift_lo[0], 0xAA);
        assert_eq!(sprites.next_sprite_pattern_shift_hi[0], 0x55);
        assert_eq!(sprites.next_sprite_x_positions[0], 100);
    }

    #[test]
    fn test_sprite_pattern_fetch_uses_dummy_tiles_on_pal_prerender() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.sprites_found = 1;
        sprites.secondary_oam[0] = 0x00; // Y position
        sprites.secondary_oam[1] = 0x01; // Tile index
        sprites.secondary_oam[2] = 0x00; // Attributes
        sprites.secondary_oam[3] = 0x20; // X position

        let seen_addr = std::cell::RefCell::new(None);
        let read_chr = |addr: u16| -> u8 {
            *seen_addr.borrow_mut() = Some(addr);
            0x00
        };

        // PAL pre-render scanline is 311, should use dummy tile $FF.
        sprites.fetch_sprite_pattern(257 + 5, 311, 311, 8, 0x0000, read_chr);

        assert_eq!(seen_addr.into_inner(), Some(0x0FF0));
    }

    #[test]
    fn test_sprite_clipping_left_8_pixels() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
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
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
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

    #[test]
    fn test_sprite_overflow_evaluation_branch() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.sprites_found = 8;
        sprites.sprite_eval_n = 0;
        sprites.sprite_eval_m = 0;
        sprites.oam_data[0] = 0x00; // Y position in range

        // Odd cycle: read OAM[0]=0x00, find in range
        let overflow = sprites.evaluate_sprites(1, 0, 8);
        assert!(!overflow);

        // Even cycle: in range → set overflow flag, begin reading remaining bytes
        // Only m increments (to 1), n stays at 0 until remaining bytes are read
        let overflow = sprites.evaluate_sprites(2, 0, 8);
        assert!(overflow);
        assert_eq!(
            sprites.sprite_eval_n, 0,
            "n should stay at 0 during in-range remaining reads"
        );
        assert_eq!(
            sprites.sprite_eval_m, 1,
            "m should advance to 1 for next byte read"
        );
        assert_eq!(sprites.sprite_eval_overflow_reads_remaining, 3);

        // Read remaining 3 bytes (m=1, m=2, m=3), each takes 2 cycles
        for i in 0..3 {
            let _overflow = sprites.evaluate_sprites(3 + i * 2, 0, 8); // odd: read
            let _overflow = sprites.evaluate_sprites(4 + i * 2, 0, 8); // even: process
        }

        // After all 4 bytes: n should have incremented to 1, m stayed at 0 (wrapped from 3+1=4→0)
        assert_eq!(
            sprites.sprite_eval_n, 1,
            "n should increment after all 4 bytes"
        );
        assert_eq!(sprites.sprite_eval_overflow_reads_remaining, 0);
    }

    /// Given sprite overflow detection encounters a sprite in range,
    /// When the remaining bytes of that sprite are read,
    /// Then the hardware should increment only m (not n) for the remaining 3 reads,
    /// and after all 4 bytes, increment n without resetting m.
    ///
    /// This test verifies the correct in-range overflow behavior:
    /// - When found in range at (n, m): set overflow flag
    /// - Read remaining bytes: m+1, m+2, m+3 (all with same n)
    /// - After 4 reads: increment n, keep m at its current position
    ///
    /// The overflow bug causes m to NOT be 0 when checking Y, so in-range
    /// detection may trigger on non-Y bytes (tile, attr, X). The remaining
    /// bytes are still read sequentially from the same sprite.
    #[test]
    fn test_sprite_overflow_in_range_reads_remaining_bytes() {
        let mut sprites = Sprites::new(crate::console::RamInitMode::Zero);
        sprites.sprites_found = 8;

        // Set up at n=5, m=2 (reading attribute byte due to overflow bug)
        sprites.sprite_eval_n = 5;
        sprites.sprite_eval_m = 2;

        // Sprite 5: Y=$10, tile=$20, attr=$00, X=$30
        sprites.oam_data[20] = 0x10; // Y
        sprites.oam_data[21] = 0x20; // Tile
        sprites.oam_data[22] = 0x00; // Attr (will be treated as Y for range check)
        sprites.oam_data[23] = 0x30; // X

        // OAM[5*4+2] = 0x00 (attr). For scanline 0: diff = 1 - 1 = 0 < 8 → IN RANGE
        // Odd cycle: read OAM[22] = 0x00, find in range
        let overflow = sprites.evaluate_sprites(1, 0, 8);
        assert!(
            !overflow,
            "Overflow flag should not be set on the read cycle"
        );
        assert_eq!(
            sprites.oam_read_latch, 0x00,
            "Latch should hold the read value (OAM[22] = 0x00)"
        );

        // Even cycle: set overflow flag, begin reading remaining bytes
        let overflow = sprites.evaluate_sprites(2, 0, 8);
        assert!(overflow, "Overflow flag should be set on the even cycle");

        // After the even cycle when in range: the hardware should now read the
        // remaining bytes (m=3) before moving to the next sprite.
        // Odd cycle: read m=3 → OAM[5*4+3] = OAM[23] = 0x30
        let overflow = sprites.evaluate_sprites(3, 0, 8);
        assert!(!overflow);
        assert_eq!(
            sprites.oam_read_latch, 0x30,
            "After in-range detection at m=2, next read should be m=3 (OAM[23]=0x30), \
             got {:#04X}. The hardware reads remaining sprite bytes without incrementing n.",
            sprites.oam_read_latch
        );
    }
}
