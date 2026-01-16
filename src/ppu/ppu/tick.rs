use super::Ppu;
use crate::nes::TvSystem;
use crate::trace_ppu;

pub(super) fn prerender_scanline(tv_system: TvSystem) -> u16 {
    match tv_system {
        TvSystem::Ntsc => 261,
        TvSystem::Pal => 311,
    }
}

#[inline(always)]
fn is_rendering_pixel(pixel: u16) -> bool {
    (1..=256).contains(&pixel)
}

#[inline(always)]
fn should_fetch_background(pixel: u16) -> bool {
    (1..=256).contains(&pixel) || (321..=336).contains(&pixel)
}

pub(super) fn tick(ppu: &mut Ppu) {
    // Trace PPU tick with scanline and pixel position
    trace_ppu!(
        "tick y={} x={} v={:04X} t={:04X} x={} w={} ctrl={:02X} mask={:02X} status={:02X} oam={:02X} rd={} bg={} sp={} frame={} cyc={}",
        ppu.timing.scanline(),
        ppu.timing.pixel(),
        ppu.registers.v(),
        ppu.registers.t(),
        ppu.registers.x(),
        ppu.registers.w(),
        ppu.registers.control(),
        ppu.registers.mask(),
        ppu.status.read_status(),
        ppu.registers.oam_address,
        ppu.registers.is_rendering_enabled(),
        ppu.registers.is_background_enabled(),
        ppu.registers.is_sprite_enabled(),
        ppu.timing.frame_count(),
        ppu.timing.total_cycles(),
    );

    let is_rendering_enabled = ppu.registers.is_rendering_enabled();
    let prerender_scanline = prerender_scanline(ppu.timing.tv_system());
    let scanline_before_tick = ppu.timing.scanline();
    let pixel_before_tick = ppu.timing.pixel();

    // Advance timing
    let skipped = ppu.timing.tick(is_rendering_enabled);

    // New frame begins when the pre-render scanline wraps back to scanline 0.
    // This also includes the NTSC odd-frame skip path.
    let frame_wrapped =
        skipped || (scanline_before_tick == prerender_scanline && pixel_before_tick == 340);

    if frame_wrapped {
        ppu.with_mapper_mut(|mapper| mapper.ppu_end_frame());
    }

    // Notify mapper at scanline boundaries (start of scanline: pixel == 0).
    // This is a PPU-driven hook for mappers with scanline counters (e.g., MMC5).
    if ppu.timing.pixel() == 0 {
        let scanline = ppu.timing.scanline();
        ppu.with_mapper_mut(|mapper| mapper.ppu_scanline(scanline, is_rendering_enabled));
    }

    // Tick the registers for decay timing
    ppu.registers.tick();

    // Enter VBlank at scanline 241, pixel 1.
    // Note: reading PPUSTATUS right at VBlank set time can suppress VBlank for the frame.
    if ppu.timing.scanline() == 241 && ppu.timing.pixel() == 1 && !ppu.vblank_suppressed_for_frame {
        // Hardware quirk/timing: VBlank flag is set at dot 1, but the NMI edge is observed
        // slightly later. We latch the NMI edge at dot 2 (see below).
        ppu.status.enter_vblank();
        ppu.set_vblank_for_nmi();
    }

    // Latch the VBlank-start NMI edge one dot after the VBlank flag is set.
    if ppu.timing.scanline() == 241
        && ppu.timing.pixel() == 2
        && !ppu.vblank_suppressed_for_frame
        && ppu.status.is_in_vblank()
        && ppu.registers.should_generate_nmi()
    {
        ppu.status.trigger_nmi();
    }

    // Exit VBlank at the pre-render scanline, pixel 1.
    if ppu.timing.scanline() == prerender_scanline && ppu.timing.pixel() == 1 {
        ppu.status.exit_vblank();
    }

    let scanline = ppu.timing.scanline();
    let pixel = ppu.timing.pixel();

    // Clear sprite 0 hit and sprite overflow at dot 0 of pre-render scanline
    // For sprite_hit timing test: clear_time = 6819 cycles after VBL = scanline 261, pixel 0
    if scanline == prerender_scanline && pixel == 0 {
        ppu.status.clear_sprite_flags();

        // For immediate-NMI enable behavior, treat VBlank as ending slightly earlier
        // than the readable $2002 flag clear timing.
        ppu.clear_vblank_for_nmi();

        // New frame is about to start; clear any VBlank suppression state.
        ppu.vblank_suppressed_for_frame = false;
    }
    // Background rendering pipeline during rendering cycles
    let is_visible_scanline = scanline < 240;
    let is_prerender = scanline == prerender_scanline;
    let is_rendering_scanline = is_visible_scanline || is_prerender;
    let is_rendering_pixel = is_rendering_pixel(pixel);

    // Background rendering pipeline during rendering cycles
    // Fetches happen during pixels 1-256 (visible) and 321-336 (pre-fetch for next scanline)
    // Also during pixels 337-340 (two single nametable byte fetches)
    if is_rendering_enabled && is_rendering_scanline {
        let should_fetch = should_fetch_background(pixel);

        if should_fetch {
            // Perform background tile fetches based on cycle (every 8 pixels)
            // Each memory access takes 2 PPU cycles:
            //   Cycles 1-2 (pixels 1-2): Nametable byte
            //   Cycles 3-4 (pixels 3-4): Attribute byte
            //   Cycles 5-6 (pixels 5-6): Pattern table tile low
            //   Cycles 7-8 (pixels 7-8): Pattern table tile high
            //
            // For MMC3 IRQ timing, the A12 transition should be detected on
            // the SECOND cycle of each memory access (when the read completes).
            // This is pixel 6 for pattern_lo and pixel 8 for pattern_hi.
            let cycle_in_tile = (pixel - 1) % 8;
            let fetch_step = cycle_in_tile / 2;

            // Memory reads should only happen once per fetch (on the second cycle)
            // for correct MMC3 A12 timing.
            let is_second_cycle_of_fetch = cycle_in_tile % 2 == 1;

            match fetch_step {
                0 if is_second_cycle_of_fetch => {
                    // Fetch nametable byte (cycle 2 of tile)
                    let v = ppu.registers.v();
                    ppu.background.fetch_nametable(v, |addr| {
                        ppu.memory.read_nametable_mapped(addr, &ppu.cartridge)
                    });
                }
                1 if is_second_cycle_of_fetch => {
                    // Fetch attribute byte (cycle 4 of tile)
                    let v = ppu.registers.v();
                    ppu.background.fetch_attribute(v, |addr| {
                        ppu.memory.read_nametable_mapped(addr, &ppu.cartridge)
                    });
                }
                2 if is_second_cycle_of_fetch => {
                    // Fetch pattern table low byte (cycle 6 of tile)
                    let v = ppu.registers.v();
                    let bg_pattern_table = ppu.registers.bg_pattern_table_addr();
                    let cartridge = &ppu.cartridge;
                    ppu.background
                        .fetch_pattern_lo(bg_pattern_table, v, |addr| {
                            Ppu::notify_chr_fetch_kind(cartridge, false);
                            ppu.memory.read_chr(addr, cartridge)
                        });
                }
                3 if is_second_cycle_of_fetch => {
                    // Fetch pattern table high byte (cycle 8 of tile)
                    let v = ppu.registers.v();
                    let bg_pattern_table = ppu.registers.bg_pattern_table_addr();
                    let cartridge = &ppu.cartridge;
                    ppu.background
                        .fetch_pattern_hi(bg_pattern_table, v, |addr| {
                            Ppu::notify_chr_fetch_kind(cartridge, false);
                            ppu.memory.read_chr(addr, cartridge)
                        });
                }
                _ => {}
            }
        }

        // Load shift registers every 8 pixels
        // Per NES Dev wiki: "The shifters are reloaded during ticks 9, 17, 25, ..., 257"
        // In our pixel numbering (pixel 1 = cycle 1), this is pixels 9, 17, 25, ..., 257
        // Also pre-fetch loads at pixels 329, 337 (cycles 329, 337)
        // Note: pixel 321 is % 8 == 1 but should NOT load (fetch not complete yet)
        if pixel % 8 == 1 && pixel > 1 && (pixel <= 257 || pixel >= 329) {
            ppu.background.load_shift_registers(ppu.registers.v());
            ppu.registers.increment_coarse_x();
        }

        // During pre-fetch, shift happens during cycles 329-336 (8 shifts total)
        // Pixels 321-328: fetch first tile (no shifts)
        // Pixel 329: load first tile, then shift (shift 1/8)
        // Pixels 330-336: continue shifting (shifts 2-8/8)
        // Pixel 337: load second tile
        // This applies to ALL rendering scanlines, not just pre-render!
        if (329..=336).contains(&pixel) {
            ppu.background.shift_registers();
        }

        if pixel == 337 || pixel == 339 {
            // Two dummy nametable fetches at pixels 337 and 339
            // (The NES PPU does these but they're not used)
            let v = ppu.registers.v();
            ppu.background.fetch_nametable(v, |addr| {
                ppu.memory.read_nametable_mapped(addr, &ppu.cartridge)
            });
        }

        // Handle scroll register updates during visible pixels
        if pixel == 256 {
            // Increment fine Y at end of visible scanline
            ppu.registers.increment_fine_y();
        } else if pixel == 257 {
            // Copy horizontal bits from t to v
            ppu.registers.copy_horizontal_bits();
        }
    }

    // Copy horizontal and vertical bits during pre-render scanline
    if is_rendering_enabled && is_prerender {
        if pixel == 257 {
            // Copy horizontal bits from t to v at pixel 257
            ppu.registers.copy_horizontal_bits();
        } else if (280..=304).contains(&pixel) {
            // Copy vertical bits from t to v during pixels 280-304
            ppu.registers.copy_vertical_bits();
        }
    }

    // OAM corruption bug: If OAMADDR >= 8 when sprite tile loading starts,
    // copy 8 bytes from (OAMADDR & 0xF8) to OAM[0..7]
    // This happens at pixel 257 of the pre-render scanline
    if is_rendering_enabled && is_prerender && pixel == 257 && ppu.registers.oam_address >= 8 {
        let source_addr = (ppu.registers.oam_address & 0xF8) as usize;
        // Copy 8 bytes from source to OAM[0..7]
        for i in 0..8 {
            let value = ppu.sprites.read_oam((source_addr + i) as u8);
            ppu.sprites.write_oam(i as u8, value);
        }
    }

    // Clear OAMADDR during sprite tile loading (pixels 257-320) on visible and pre-render scanlines
    // This is critical NES PPU hardware behavior
    if is_rendering_enabled && is_rendering_scanline && (257..=320).contains(&pixel) {
        ppu.registers.oam_address = 0;
    }

    // Sprite evaluation during visible scanlines only (NOT pre-render)
    // Only happens when rendering is enabled (either sprites or background)
    // Per NESdev: "Sprite evaluation does not happen on the pre-render scanline"
    if is_visible_scanline && is_rendering_enabled {
        if pixel == 0 {
            // Reset sprite evaluation at start of scanline
            ppu.sprites.reset_evaluation();
        } else if (1..=64).contains(&pixel) {
            // Initialize secondary OAM
            ppu.sprites.initialize_secondary_oam_byte(pixel);
        } else if (65..=256).contains(&pixel) {
            // Evaluate sprites for next scanline
            let sprite_height = ppu.registers.sprite_height();
            let overflow = ppu.sprites.evaluate_sprites(pixel, scanline, sprite_height);

            // Set overflow flag immediately when detected during evaluation
            if overflow {
                ppu.status.set_sprite_overflow();
            }

            if pixel == 256 {
                // Finalize evaluation
                ppu.sprites.finalize_evaluation();
            }
        } else if pixel == 321 {
            // Swap sprite buffers for rendering
            ppu.sprites.swap_buffers();
            ppu.sprites.mark_buffers_ready();
        }
    }

    // Sprite pattern fetching happens on ALL rendering scanlines (including pre-render)
    // This is critical for MMC3 IRQ timing - the A12 transition from BG ($0xxx) to
    // sprite ($1xxx) pattern fetches must happen 241 times per frame.
    // Note: The PPU fetches 8 sprite patterns even on pre-render, using tile $FF
    // for any sprites not found (since evaluation doesn't happen on pre-render).
    if is_rendering_enabled && is_rendering_scanline && (257..=320).contains(&pixel) {
        let sprite_height = ppu.registers.sprite_height();
        let sprite_pattern_table = ppu.registers.sprite_pattern_table_addr();
        let cartridge = &ppu.cartridge;
        ppu.sprites.fetch_sprite_pattern(
            pixel,
            scanline,
            sprite_height,
            sprite_pattern_table,
            |addr| {
                Ppu::notify_chr_fetch_kind(cartridge, true);
                ppu.memory.read_chr(addr, cartridge)
            },
        );
    }

    // Render pixels to screen buffer during visible scanlines and pixels
    if is_visible_scanline && is_rendering_pixel {
        let screen_x = (pixel - 1) as u32;
        let screen_y = scanline as u32;
        let mut bg_pixel_for_hit = 0u8; // Save for sprite 0 hit detection after shift

        if is_rendering_enabled {
            // Get background pixel (only if background rendering is enabled)
            // Note: Shift registers were shifted above, after load (if any) but before reading
            let fine_x = ppu.registers.x();
            let bg_pixel = if ppu.registers.is_background_enabled() {
                ppu.background.get_pixel(fine_x)
            } else {
                0 // Background disabled, treat as transparent
            };

            // Get sprite pixel
            let show_sprites_left = ppu.registers.show_sprites_left();
            let sprite_pixel = ppu.sprites.get_pixel(screen_x as i16, show_sprites_left);

            // Save bg_pixel for sprite 0 hit detection after pixel output
            bg_pixel_for_hit = bg_pixel;

            // Determine final palette index
            let palette_index =
                if let Some((sprite_palette_idx, _sprite_idx, is_foreground)) = sprite_pixel {
                    if bg_pixel == 0 || is_foreground {
                        sprite_palette_idx // Background transparent or sprite in foreground
                    } else {
                        bg_pixel // Sprite in background
                    }
                } else {
                    bg_pixel // No sprite
                };

            // Apply grayscale if enabled (mask to monochrome palette)
            let final_palette_index = if ppu.registers.is_grayscale() {
                palette_index & 0x30
            } else {
                palette_index
            };

            // Look up color in palette (convert index to address)
            let palette_addr = 0x3F00 + (final_palette_index as u16);
            let color_value = ppu.memory.read_palette(palette_addr);
            let (r, g, b) = crate::nes::Nes::lookup_system_palette(color_value);

            // Apply color emphasis/tint
            let (final_r, final_g, final_b) = if ppu.registers.color_emphasis() != 0 {
                let emphasis = ppu.registers.color_emphasis();
                let emphasize_red = (emphasis & 0x01) != 0;
                let emphasize_green = (emphasis & 0x02) != 0;
                let emphasize_blue = (emphasis & 0x04) != 0;

                const ATTENUATION: f32 = 0.75;
                const BOOST: f32 = 1.1;

                let mut fr = r as f32;
                let mut fg = g as f32;
                let mut fb = b as f32;

                if emphasize_red {
                    fr = (fr * BOOST).min(255.0);
                    if !emphasize_green {
                        fg *= ATTENUATION;
                    }
                    if !emphasize_blue {
                        fb *= ATTENUATION;
                    }
                }
                if emphasize_green {
                    fg = (fg * BOOST).min(255.0);
                    if !emphasize_red {
                        fr *= ATTENUATION;
                    }
                    if !emphasize_blue {
                        fb *= ATTENUATION;
                    }
                }
                if emphasize_blue {
                    fb = (fb * BOOST).min(255.0);
                    if !emphasize_red {
                        fr *= ATTENUATION;
                    }
                    if !emphasize_green {
                        fg *= ATTENUATION;
                    }
                }

                (fr as u8, fg as u8, fb as u8)
            } else {
                (r, g, b)
            };

            // Write pixel to screen buffer
            ppu.rendering
                .screen_buffer_mut()
                .set_pixel(screen_x, screen_y, final_r, final_g, final_b);
        } else {
            // When rendering is disabled, output the backdrop color
            let backdrop_addr = 0x3F00;
            let color_value = ppu.memory.read_palette(backdrop_addr);
            let (r, g, b) = crate::nes::Nes::lookup_system_palette(color_value);

            // Write backdrop color to screen buffer
            ppu.rendering
                .screen_buffer_mut()
                .set_pixel(screen_x, screen_y, r, g, b);
        }

        // Per NES Dev wiki: "On every dot in these background fetch regions, a 4-bit pixel
        // is selected by the fine x register from the low 8 bits of the pattern and
        // attributes shift registers, which are then shifted."
        // So shift happens AFTER rendering, on every visible pixel
        if is_rendering_enabled {
            //Shift registers first
            ppu.background.shift_registers();

            // Sprite 0 hit detection respects the clipping settings:
            // - If sprite clipping is enabled (show_sprites_left=false), sprite pixels
            //   in the leftmost 8 screen pixels (X=0-7) do not trigger hits
            // - If background clipping is enabled (show_background_left=false), background
            //   pixels in the leftmost 8 screen pixels (X=0-7) do not trigger hits
            // - Sprite 0 with Y >= 239 (0xEF) never triggers hits
            // Check for sprite 0 hit AFTER shift_registers (timing fix attempt)
            if ppu.registers.is_background_enabled() && ppu.registers.is_sprite_enabled() {
                // Sprite 0 hit never occurs when sprite 0's OAM Y >= 239
                let sprite_0_y = ppu.sprites.sprite_0_oam_y();
                if sprite_0_y < 0xEF {
                    // Sprite 0 must be on this scanline to trigger hit
                    // Sprites render starting at scanline (Y+1), so check range
                    // Use actual sprite height (8 or 16) from PPUCTRL
                    let sprite_height = ppu.registers.sprite_height() as u16;
                    let sprite_render_start = (sprite_0_y as u16).wrapping_add(1);
                    let sprite_render_end = sprite_render_start.wrapping_add(sprite_height);
                    let in_y_range =
                        scanline >= sprite_render_start && scanline < sprite_render_end;

                    if in_y_range {
                        let show_bg_left = ppu.registers.show_background_left();
                        let show_sp_left = ppu.registers.show_sprites_left();

                        // Check if clipping should prevent the hit at this screen position
                        let bg_clipped = screen_x < 8 && !show_bg_left;
                        let sp_clipped = screen_x < 8 && !show_sp_left;

                        // Only check for hit if neither sprite nor background is clipped here
                        if !bg_clipped && !sp_clipped {
                            let sprite_0_present = ppu.sprites.sprite_0_pixel_at(screen_x as i16);

                            // Sprite 0 hit when both background and sprite have non-transparent pixels
                            if sprite_0_present && bg_pixel_for_hit != 0 {
                                ppu.status.set_sprite_0_hit();
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_rendering_pixel_bounds() {
        assert!(!is_rendering_pixel(0));
        assert!(is_rendering_pixel(1));
        assert!(is_rendering_pixel(256));
        assert!(!is_rendering_pixel(257));
    }

    #[test]
    fn test_should_fetch_background_bounds() {
        assert!(should_fetch_background(1));
        assert!(should_fetch_background(256));
        assert!(!should_fetch_background(257));
        assert!(should_fetch_background(321));
        assert!(should_fetch_background(336));
        assert!(!should_fetch_background(337));
    }
}
