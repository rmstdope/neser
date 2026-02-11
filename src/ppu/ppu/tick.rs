use super::Ppu;
use crate::console::{Nes, TvSystem};
use crate::debugging::ppu_trace_level;
use crate::ppu::color_effects::{apply_color_emphasis, apply_grayscale};
use crate::ppu::timing::{
    BG_PREFETCH_END, BG_PREFETCH_START, DUMMY_NT_FETCH_1, DUMMY_NT_FETCH_2,
    FINE_Y_INCREMENT_PIXEL, FIRST_DOT, FIRST_VISIBLE_PIXEL, FIRST_VISIBLE_SCANLINE,
    HORIZONTAL_BITS_COPY_PIXEL, LAST_DOT, LAST_VISIBLE_PIXEL, LAST_VISIBLE_SCANLINE_PLUS_ONE,
    NTSC_PRERENDER_SCANLINE, PAL_PRERENDER_SCANLINE, SPRITE_TILE_LOAD_END,
    SPRITE_TILE_LOAD_START, VBLANK_NMI_LATCH_PIXEL, VBLANK_START_SCANLINE,
    VERTICAL_BITS_COPY_END, VERTICAL_BITS_COPY_START,
};
use crate::trace_ppu;

pub(super) fn prerender_scanline(tv_system: TvSystem) -> u16 {
    match tv_system {
        TvSystem::Ntsc => NTSC_PRERENDER_SCANLINE,
        TvSystem::Pal => PAL_PRERENDER_SCANLINE,
    }
}

#[inline(always)]
fn is_rendering_pixel(pixel: u16) -> bool {
    (FIRST_VISIBLE_PIXEL..=LAST_VISIBLE_PIXEL).contains(&pixel)
}

#[inline(always)]
fn is_bg_fetch_pixel(pixel: u16) -> bool {
    (FIRST_VISIBLE_PIXEL..=LAST_VISIBLE_PIXEL).contains(&pixel) 
        || (BG_PREFETCH_START..=BG_PREFETCH_END).contains(&pixel)
}

#[inline(always)]
fn should_trace_vblank_enter(scanline: u16, pixel: u16, vblank_suppressed: bool) -> bool {
    scanline == VBLANK_START_SCANLINE && pixel == FIRST_VISIBLE_PIXEL && !vblank_suppressed
}

#[inline(always)]
fn should_trace_vblank_exit(scanline: u16, prerender_scanline: u16, pixel: u16) -> bool {
    scanline == prerender_scanline && pixel == FIRST_VISIBLE_PIXEL
}

pub(super) fn tick(ppu: &mut Ppu) {
    // Trace PPU tick with scanline and pixel position
    trace_ppu!(5;
        "tick y={} x={} v={:04X} t={:04X} x={} w={} ctrl={:02X} mask={:02X} status={:02X} oam={:02X} rd={} bg={} sp={} frame={} cyc={}",
        ppu.timing.scanline(),
        ppu.timing.pixel(),
        ppu.registers.v(),
        ppu.registers.t(),
        ppu.registers.x(),
        ppu.registers.w(),
        ppu.registers.control(),
        ppu.registers.mask(),
        ppu.status.peek_status(),
        ppu.registers.oam_address,
        ppu.registers.is_rendering_enabled(),
        ppu.registers.is_background_enabled(),
        ppu.registers.is_sprite_enabled(),
        ppu.timing.frame_count(),
        ppu.timing.total_cycles(),
    );

    tick_timing(ppu);
    tick_vblank_and_nmi(ppu);
    tick_background(ppu);
    tick_sprites(ppu);
    tick_pixel_output(ppu);
}

/// Phase 1: Advance timing, detect frame boundaries, and notify mappers.
fn tick_timing(ppu: &mut Ppu) {
    let is_rendering_enabled = ppu.registers.is_rendering_enabled();
    let prerender = prerender_scanline(ppu.timing.tv_system());
    let scanline_before_tick = ppu.timing.scanline();
    let pixel_before_tick = ppu.timing.pixel();

    // Advance timing
    let skipped = ppu.timing.tick(is_rendering_enabled);

    // New frame begins when the pre-render scanline wraps back to scanline 0.
    // This also includes the NTSC odd-frame skip path.
    let frame_wrapped = skipped || (scanline_before_tick == prerender && pixel_before_tick == LAST_DOT);

    if frame_wrapped {
        trace_ppu!(1; "frame wrap y={} x={} frame={} cyc={}",
            scanline_before_tick,
            pixel_before_tick,
            ppu.timing.frame_count(),
            ppu.timing.total_cycles(),
        );
        if ppu_trace_level() >= 1 {
            trace_ppu!(1; "frame crc={:08X}", ppu.rendering.screen_buffer_crc32());
        }
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
}

/// Phase 2: VBlank enter/exit and NMI generation.
fn tick_vblank_and_nmi(ppu: &mut Ppu) {
    let scanline = ppu.timing.scanline();
    let pixel = ppu.timing.pixel();
    let prerender = prerender_scanline(ppu.timing.tv_system());

    // Enter VBlank at scanline 241, pixel 1.
    // Note: reading PPUSTATUS right at VBlank set time can suppress VBlank for the frame.
    if should_trace_vblank_enter(scanline, pixel, ppu.vblank_suppressed_for_frame) {
        trace_ppu!(1; "vblank enter y={} x={} status={:02X}",
            scanline,
            pixel,
            ppu.status.peek_status(),
        );
    }

    if scanline == VBLANK_START_SCANLINE && pixel == FIRST_VISIBLE_PIXEL && !ppu.vblank_suppressed_for_frame {
        // Hardware quirk/timing: VBlank flag is set at dot 1, but the NMI edge is observed
        // slightly later. We latch the NMI edge at dot 2 (see below).
        ppu.status.enter_vblank();
        ppu.set_vblank_for_nmi();
    }

    // Latch the VBlank-start NMI edge one dot after the VBlank flag is set.
    if scanline == VBLANK_START_SCANLINE
        && pixel == VBLANK_NMI_LATCH_PIXEL
        && !ppu.vblank_suppressed_for_frame
        && ppu.status.is_in_vblank()
        && ppu.registers.should_generate_nmi()
    {
        trace_ppu!(2; "vblank nmi edge y={} x={} status={:02X}",
            scanline,
            pixel,
            ppu.status.peek_status(),
        );
        ppu.status.trigger_nmi();
    }

    // Exit VBlank at the pre-render scanline, pixel 1.
    if should_trace_vblank_exit(scanline, prerender, pixel) {
        trace_ppu!(1; "vblank exit y={} x={} status={:02X}",
            scanline,
            pixel,
            ppu.status.peek_status(),
        );
    }

    if scanline == prerender && pixel == 1 {
        ppu.status.exit_vblank();
    }

    // Clear sprite 0 hit and sprite overflow at dot 0 of pre-render scanline
    // For sprite_hit timing test: clear_time = 6819 cycles after VBL = scanline 261, pixel 0
    if scanline == prerender && pixel == 0 {
        ppu.status.clear_sprite_flags();

        // For immediate-NMI enable behavior, treat VBlank as ending slightly earlier
        // than the readable $2002 flag clear timing.
        ppu.clear_vblank_for_nmi();

        // New frame is about to start; clear any VBlank suppression state.
        ppu.vblank_suppressed_for_frame = false;
    }
}

/// Phase 3: Background rendering pipeline (tile fetches, shift registers, scroll updates).
fn tick_background(ppu: &mut Ppu) {
    let scanline = ppu.timing.scanline();
    let pixel = ppu.timing.pixel();
    let prerender = prerender_scanline(ppu.timing.tv_system());
    let is_rendering_enabled = ppu.registers.is_rendering_enabled();
    let is_visible_scanline = scanline < LAST_VISIBLE_SCANLINE_PLUS_ONE;
    let is_prerender = scanline == prerender;
    let is_rendering_scanline = is_visible_scanline || is_prerender;

    // Background rendering pipeline during rendering cycles
    // Fetches happen during pixels 1-256 (visible) and 321-336 (pre-fetch for next scanline)
    // Also during pixels 337-340 (two single nametable byte fetches)
    if is_rendering_enabled && is_rendering_scanline {
        let cartridge = &ppu.cartridge;
        let should_fetch = is_bg_fetch_pixel(pixel);

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
            let bg_pattern_table = ppu.registers.bg_pattern_table_addr();

            if is_second_cycle_of_fetch {
                let v = ppu.registers.v();
                match fetch_step {
                    0 => {
                        // Fetch nametable byte (cycle 2 of tile)
                        ppu.background.fetch_nametable(v, |addr| {
                            ppu.memory.read_nametable_mapped(addr, cartridge)
                        });
                    }
                    1 => {
                        // Fetch attribute byte (cycle 4 of tile)
                        ppu.background.fetch_attribute(v, |addr| {
                            ppu.memory.read_nametable_mapped(addr, cartridge)
                        });
                    }
                    2 => {
                        // Fetch pattern table low byte (cycle 6 of tile)
                        ppu.background
                            .fetch_pattern_lo(bg_pattern_table, v, |addr| {
                                Ppu::notify_chr_fetch_kind(cartridge, false);
                                ppu.memory.read_chr(addr, cartridge)
                            });
                    }
                    3 => {
                        // Fetch pattern table high byte (cycle 8 of tile)
                        ppu.background
                            .fetch_pattern_hi(bg_pattern_table, v, |addr| {
                                Ppu::notify_chr_fetch_kind(cartridge, false);
                                ppu.memory.read_chr(addr, cartridge)
                            });
                    }
                    _ => {}
                }
            }
        }

        // Load shift registers every 8 pixels
        // Per NES Dev wiki: "The shifters are reloaded during ticks 9, 17, 25, ..., 257"
        // In our pixel numbering (pixel 1 = cycle 1), this is pixels 9, 17, 25, ..., 257
        // Also pre-fetch loads at pixels 329, 337 (cycles 329, 337)
        // Note: pixel 321 is % 8 == 1 but should NOT load (fetch not complete yet)
        if pixel % 8 == 1 && pixel > FIRST_VISIBLE_PIXEL && (pixel <= HORIZONTAL_BITS_COPY_PIXEL || pixel >= 329) {
            ppu.background.load_shift_registers(ppu.registers.v());
            ppu.registers.increment_coarse_x();
        }

        // During pre-fetch, shift happens during cycles 329-336 (8 shifts total)
        // Pixels 321-328: fetch first tile (no shifts)
        // Pixel 329: load first tile, then shift (shift 1/8)
        // Pixels 330-336: continue shifting (shifts 2-8/8)
        // Pixel 337: load second tile
        // This applies to ALL rendering scanlines, not just pre-render!
        if (329..=BG_PREFETCH_END).contains(&pixel) {
            ppu.background.shift_registers();
        }

        if pixel == DUMMY_NT_FETCH_1 || pixel == DUMMY_NT_FETCH_2 {
            // Two dummy nametable fetches at pixels 337 and 339
            // (The NES PPU does these but they're not used)
            let v = ppu.registers.v();
            ppu.background
                .fetch_nametable(v, |addr| ppu.memory.read_nametable_mapped(addr, cartridge));
        }

        // Handle scroll register updates during visible pixels
        if pixel == FINE_Y_INCREMENT_PIXEL {
            // Increment fine Y at end of visible scanline
            trace_ppu!(3; "fine_y inc y={} x={} t_before={:04X} v_before={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
            ppu.registers.increment_fine_y();
            trace_ppu!(3; "fine_y inc y={} x={} t_after={:04X} v_after={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
        } else if pixel == HORIZONTAL_BITS_COPY_PIXEL {
            // Copy horizontal bits from t to v
            trace_ppu!(3; "hcopy y={} x={} t={:04X} v_before={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
            ppu.registers.copy_horizontal_bits();
            trace_ppu!(3; "hcopy y={} x={} t={:04X} v_after={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
        }
    }

    // Copy horizontal and vertical bits during pre-render scanline
    if is_rendering_enabled && is_prerender {
        if pixel == HORIZONTAL_BITS_COPY_PIXEL {
            // Copy horizontal bits from t to v at pixel 257
            trace_ppu!(3; "prerender hcopy y={} x={} t={:04X} v_before={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
            ppu.registers.copy_horizontal_bits();
            trace_ppu!(3; "prerender hcopy y={} x={} t={:04X} v_after={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
        } else if (VERTICAL_BITS_COPY_START..=VERTICAL_BITS_COPY_END).contains(&pixel) {
            // Copy vertical bits from t to v during pixels 280-304
            trace_ppu!(3; "vcopy y={} x={} t={:04X} v_before={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
            ppu.registers.copy_vertical_bits();
            trace_ppu!(3; "vcopy y={} x={} t={:04X} v_after={:04X}",
                scanline,
                pixel,
                ppu.registers.t(),
                ppu.registers.v(),
            );
        }
    }
}

/// Phase 4: Sprite evaluation, OAM handling, and sprite pattern fetching.
fn tick_sprites(ppu: &mut Ppu) {
    let scanline = ppu.timing.scanline();
    let pixel = ppu.timing.pixel();
    let prerender = prerender_scanline(ppu.timing.tv_system());
    let is_rendering_enabled = ppu.registers.is_rendering_enabled();
    let is_visible_scanline = scanline < LAST_VISIBLE_SCANLINE_PLUS_ONE;
    let is_prerender = scanline == prerender;
    let is_rendering_scanline = is_visible_scanline || is_prerender;
    let sprite_height = ppu.registers.sprite_height();

    // OAM corruption bug: If OAMADDR >= 8 when sprite tile loading starts,
    // copy 8 bytes from (OAMADDR & 0xF8) to OAM[0..7]
    // This happens at pixel 257 of the pre-render scanline
    if is_rendering_enabled && is_prerender && pixel == SPRITE_TILE_LOAD_START {
        let oam_address = ppu.registers.oam_address;
        if oam_address >= 8 {
            let source_addr = (oam_address & 0xF8) as usize;
            // Copy 8 bytes from source to OAM[0..7]
            for i in 0..8 {
                let value = ppu.sprites.read_oam((source_addr + i) as u8);
                ppu.sprites.write_oam(i as u8, value);
            }
        }
    }

    // Clear OAMADDR during sprite tile loading (pixels 257-320) on visible and pre-render scanlines
    // This is critical NES PPU hardware behavior
    if is_rendering_enabled && is_rendering_scanline && (SPRITE_TILE_LOAD_START..=SPRITE_TILE_LOAD_END).contains(&pixel) {
        ppu.registers.oam_address = 0;
    }

    // Sprite evaluation during visible scanlines only (NOT pre-render)
    // Only happens when rendering is enabled (either sprites or background)
    // Per NESdev: "Sprite evaluation does not happen on the pre-render scanline"
    if is_visible_scanline && is_rendering_enabled {
        if pixel == FIRST_DOT {
            // Reset sprite evaluation at start of scanline (dot 0)
            ppu.sprites.reset_evaluation();
        } else if (1..=64).contains(&pixel) {
            // Initialize secondary OAM
            ppu.sprites.initialize_secondary_oam_byte(pixel);
        } else if (65..=LAST_VISIBLE_PIXEL).contains(&pixel) {
            // Evaluate sprites for next scanline
            let overflow = ppu.sprites.evaluate_sprites(pixel, scanline, sprite_height);

            // Set overflow flag immediately when detected during evaluation
            if overflow {
                ppu.status.set_sprite_overflow();
            }

            if pixel == LAST_VISIBLE_PIXEL {
                // Finalize evaluation
                ppu.sprites.finalize_evaluation();
            }
        } else if pixel == BG_PREFETCH_START {
            // Swap sprite buffers for rendering
            ppu.sprites.mark_buffers_ready();
            ppu.sprites.swap_buffers();
        }
    }

    // Sprite pattern fetching happens on ALL rendering scanlines (including pre-render)
    // This is critical for MMC3 IRQ timing - the A12 transition from BG ($0xxx) to
    // sprite ($1xxx) pattern fetches must happen 241 times per frame.
    // Note: The PPU fetches 8 sprite patterns even on pre-render, using tile $FF
    // for any sprites not found (since evaluation doesn't happen on pre-render).
    if is_rendering_enabled && is_rendering_scanline && (SPRITE_TILE_LOAD_START..=SPRITE_TILE_LOAD_END).contains(&pixel) {
        let sprite_pattern_table = ppu.registers.sprite_pattern_table_addr();
        let cartridge = &ppu.cartridge;
        ppu.sprites.fetch_sprite_pattern(
            pixel,
            scanline,
            prerender,
            sprite_height,
            sprite_pattern_table,
            |addr| {
                Ppu::notify_chr_fetch_kind(cartridge, true);
                ppu.memory.read_chr(addr, cartridge)
            },
        );
    }
}

/// Phase 5: Pixel composition and screen output.
fn tick_pixel_output(ppu: &mut Ppu) {
    let scanline = ppu.timing.scanline();
    let pixel = ppu.timing.pixel();
    let is_visible_scanline = scanline < LAST_VISIBLE_SCANLINE_PLUS_ONE;
    let rendering_pixel = is_rendering_pixel(pixel);

    // Render pixels to screen buffer during visible scanlines and pixels
    if is_visible_scanline && rendering_pixel {
        let is_rendering_enabled = ppu.registers.is_rendering_enabled();
        let palette_base: u16 = 0x3F00;
        let screen_x = (pixel - 1) as u32;
        let screen_y = scanline as u32;
        let screen_x_i16 = screen_x as i16;

        if is_rendering_enabled {
            let bg_enabled = ppu.registers.is_background_enabled();
            let sp_enabled = ppu.registers.is_sprite_enabled();
            let show_sprites_left = ppu.registers.show_sprites_left();
            let show_background_left = ppu.registers.show_background_left();
            let grayscale = ppu.registers.is_grayscale();
            let color_emphasis = ppu.registers.color_emphasis();
            let sprite_height = ppu.registers.sprite_height();
            let sprite_0_y = ppu.sprites.sprite_0_oam_y();

            // Get background pixel (only if background rendering is enabled)
            // Note: Shift registers were shifted above, after load (if any) but before reading
            let fine_x = ppu.registers.x();
            let bg_pixel = if bg_enabled {
                ppu.background.get_pixel(fine_x)
            } else {
                0 // Background disabled, treat as transparent
            };

            // Get sprite pixel
            let sprite_pixel = ppu.sprites.get_pixel(screen_x_i16, show_sprites_left);
            let has_sprite_pixel = sprite_pixel.is_some();
            let (sprite_palette_idx, sprite_is_foreground) =
                if let Some((idx, _sprite_idx, fg)) = sprite_pixel {
                    (idx, fg)
                } else {
                    (0, false)
                };

            // Determine final palette index
            let palette_index = if has_sprite_pixel {
                if bg_pixel == 0 || sprite_is_foreground {
                    sprite_palette_idx // Background transparent or sprite in foreground
                } else {
                    bg_pixel // Sprite in background
                }
            } else {
                bg_pixel // No sprite
            };
            let palette_index_u16 = u16::from(palette_index);

            ppu.track_recent_pixel(screen_x, screen_y, palette_index);

            // Look up color in palette (convert index to address)
            let palette_addr = palette_base + palette_index_u16;
            let mut color_value = ppu.memory.read_palette(palette_addr);
            // PPUMASK grayscale removes color by masking the palette *value* (hardware behavior),
            // which affects only chroma while preserving brightness selection.
            color_value = apply_grayscale(color_value, grayscale);
            let (r, g, b) = Nes::lookup_system_palette(color_value);

            // Apply color emphasis/tint
            let (final_r, final_g, final_b) = apply_color_emphasis(r, g, b, color_emphasis);

            // Write pixel to screen buffer
            ppu.rendering
                .screen_buffer_mut()
                .set_pixel(screen_x, screen_y, final_r, final_g, final_b);

            // Per NES Dev wiki: "On every dot in these background fetch regions, a 4-bit pixel
            // is selected by the fine x register from the low 8 bits of the pattern and
            // attributes shift registers, which are then shifted."
            // So shift happens AFTER rendering, on every visible pixel
            //Shift registers first
            ppu.background.shift_registers();

            // Sprite 0 hit detection respects the clipping settings:
            // - If sprite clipping is enabled (show_sprites_left=false), sprite pixels
            //   in the leftmost 8 screen pixels (X=0-7) do not trigger hits
            // - If background clipping is enabled (show_background_left=false), background
            //   pixels in the leftmost 8 screen pixels (X=0-7) do not trigger hits
            // - Sprite 0 with Y >= 239 (0xEF) never triggers hits
            // Check for sprite 0 hit AFTER shift_registers (timing fix attempt)
            if bg_enabled && sp_enabled {
                let left_edge = screen_x < 8;
                let show_bg_left = show_background_left;
                let show_sp_left = show_sprites_left;

                // Sprite 0 hit never occurs when sprite 0's OAM Y >= 239
                if sprite_0_y < 0xEF {
                    // Sprite 0 must be on this scanline to trigger hit
                    // Sprites render starting at scanline (Y+1), so check range
                    // Use actual sprite height (8 or 16) from PPUCTRL
                    let sprite_render_start = (sprite_0_y as u16).wrapping_add(1);
                    let sprite_render_end = sprite_render_start.wrapping_add(sprite_height as u16);
                    let in_y_range =
                        scanline >= sprite_render_start && scanline < sprite_render_end;

                    if in_y_range {
                        // Check if clipping should prevent the hit at this screen position
                        let bg_clipped = left_edge && !show_bg_left;
                        let sp_clipped = left_edge && !show_sp_left;

                        // Only check for hit if neither sprite nor background is clipped here
                        if !bg_clipped && !sp_clipped {
                            let sprite_0_present = ppu.sprites.sprite_0_pixel_at(screen_x_i16);

                            // Sprite 0 hit when both background and sprite have non-transparent pixels
                            if sprite_0_present && bg_pixel != 0 {
                                ppu.status.set_sprite_0_hit();
                            }
                        }
                    }
                }
            }
        } else {
            // When rendering is disabled, output the backdrop color
            let color_value = ppu.memory.read_palette(palette_base);
            let (r, g, b) = Nes::lookup_system_palette(color_value);

            // Write backdrop color to screen buffer
            ppu.rendering
                .screen_buffer_mut()
                .set_pixel(screen_x, screen_y, r, g, b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_rendering_pixel_bounds() {
        assert!(!is_rendering_pixel(FIRST_DOT));
        assert!(is_rendering_pixel(FIRST_VISIBLE_PIXEL));
        assert!(is_rendering_pixel(LAST_VISIBLE_PIXEL));
        assert!(!is_rendering_pixel(HORIZONTAL_BITS_COPY_PIXEL));
    }

    #[test]
    fn test_is_bg_fetch_pixel_bounds() {
        assert!(!is_bg_fetch_pixel(FIRST_DOT));
        assert!(is_bg_fetch_pixel(FIRST_VISIBLE_PIXEL));
        assert!(is_bg_fetch_pixel(LAST_VISIBLE_PIXEL));
        assert!(!is_bg_fetch_pixel(HORIZONTAL_BITS_COPY_PIXEL));
        assert!(is_bg_fetch_pixel(BG_PREFETCH_START));
        assert!(is_bg_fetch_pixel(BG_PREFETCH_END));
        assert!(!is_bg_fetch_pixel(DUMMY_NT_FETCH_1));
        assert!(!is_bg_fetch_pixel(LAST_DOT));
    }

    #[test]
    fn test_should_trace_vblank_enter() {
        assert!(should_trace_vblank_enter(VBLANK_START_SCANLINE, FIRST_VISIBLE_PIXEL, false));
        assert!(!should_trace_vblank_enter(VBLANK_START_SCANLINE, FIRST_VISIBLE_PIXEL, true));
        assert!(!should_trace_vblank_enter(LAST_VISIBLE_SCANLINE_PLUS_ONE - 1, FIRST_VISIBLE_PIXEL, false));
        assert!(!should_trace_vblank_enter(VBLANK_START_SCANLINE, VBLANK_NMI_LATCH_PIXEL, false));
    }

    #[test]
    fn test_should_trace_vblank_exit() {
        let prerender = prerender_scanline(TvSystem::Ntsc);
        assert!(should_trace_vblank_exit(prerender, prerender, FIRST_VISIBLE_PIXEL));
        assert!(!should_trace_vblank_exit(prerender, prerender, FIRST_DOT));
        // Test that scanline 0 (FIRST_VISIBLE_SCANLINE) is not the pre-render scanline
        assert!(!should_trace_vblank_exit(FIRST_VISIBLE_SCANLINE, prerender, FIRST_VISIBLE_PIXEL));
    }
}
