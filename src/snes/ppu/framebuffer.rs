//! PPU framebuffer output: backdrop rendering and BGR555 -> RGB888 conversion.
//!
//! The dot pipeline writes a 15-bit BGR555 color per visible pixel into [`Ppu::framebuffer`].
//! Until BG/OBJ layers exist, every visible pixel is the backdrop color (CGRAM entry 0).
//! [`Ppu::screen_snapshot_rgb`] applies INIDISP forced-blank and master brightness at output,
//! converting to packed RGB888.

use super::{OVERSCAN_CROP_TOP, Ppu, SCREEN_WIDTH, VISIBLE_DOT_START, VISIBLE_LINE_START};

impl Ppu {
    /// Render the pixel at the current dot (called once per dot from the timing loop).
    ///
    /// Only visible dots (active display region) write to the framebuffer.
    ///
    /// Two independent decisions meet here (Mesen2's model, #3034):
    ///
    /// - **Layout** comes from [`Ppu::use_high_res_output`], latched once per frame.
    ///   Every row of the frame is written 512 columns wide, or none is.
    /// - **Content** comes from the live [`Ppu::hires_output_enabled`]. A dot rendered
    ///   while a hires mode is active emits a true sub/main half-pixel pair; a dot
    ///   rendered in a native mode emits its one composed pixel into both columns
    ///   (Mesen2 `ApplyHiResMode`'s non-`IsDoubleWidth` branch).
    ///
    /// So a **mid-line** BGMODE/SETINI switch splits the row's *content* at the switch
    /// dot -- columns left of it stay column-doubled, columns right of it become half
    /// pixels -- while its *layout* is uniform, because turning hires on retroactively
    /// re-lays-out everything already drawn ([`Ppu::convert_to_hires`]). NESER decides
    /// this per dot where Mesen2 decides per chunk flushed on each register write, but
    /// both place the boundary at `x = dot - 22` ([`VISIBLE_DOT_START`], Mesen2's
    /// `_drawEndX = hPos - 22`), so the boundary column agrees.
    pub(super) fn render_dot(&mut self) {
        let line = self.position.scanline;
        let dot = self.position.dot;
        if line < VISIBLE_LINE_START
            || line as usize >= VISIBLE_LINE_START as usize + self.active_screen_height()
        {
            return;
        }
        if dot < VISIBLE_DOT_START || dot as usize >= VISIBLE_DOT_START as usize + SCREEN_WIDTH {
            return;
        }
        let x = (dot - VISIBLE_DOT_START) as usize;
        let y = (line - VISIBLE_LINE_START) as usize;
        let row = self.framebuffer_row(y);
        let duplicates_row = self.duplicates_row();
        if dot == VISIBLE_DOT_START {
            self.line_inidisp[row] = self.inidisp;
            if duplicates_row {
                // The duplicate row is presented like any other, so it needs this
                // line's brightness/forced-blank too -- otherwise every second output
                // row of a progressive hires frame reads brightness 0 and comes out
                // black.
                self.line_inidisp[row + 1] = self.inidisp;
            }
        }
        let base_x = self.framebuffer_x(x);
        let stride = self.framebuffer_stride();
        let base = row * stride + base_x;
        let (main, sub) = self.resolve_pixel_pair(x as u16, y as u16);
        self.line_main[x] = main;
        self.line_sub[x] = sub;
        if self.use_high_res_output {
            let (even, odd) = if self.hires_output_enabled() {
                // The sub screen supplies the even (left) half-pixel and the main screen
                // the odd (right) one, for both true hires (modes 5/6) and pseudo-hires
                // (Mesen2 ApplyHiResMode: out[2x] = sub, out[2x+1] = main), each
                // finalized through the hires color-math pair.
                self.compose_hires_pair(x as u16, y as u16)
            } else {
                // A native dot inside a hires frame has no sub half-pixel: its one
                // composed pixel fills both columns.
                let out = self.compose_pixels(x as u16, y as u16, main, sub);
                (out, out)
            };
            self.framebuffer[base] = even;
            self.framebuffer[base + 1] = odd;
            if duplicates_row {
                self.framebuffer[base + stride] = even;
                self.framebuffer[base + stride + 1] = odd;
            }
            // The finalized main pixel is the odd half in both branches.
            self.line_main_final[x] = odd;
        } else {
            let out = self.compose_pixels(x as u16, y as u16, main, sub);
            self.line_main_final[x] = out;
            self.framebuffer[base] = out;
        }
        // Mesen2's render pipeline ends every pixel chunk with `RenderBgColor`, which
        // fetches the backdrop for any pixel the sub screen doesn't cover. With no
        // sub-screen layers enabled that is every pixel, so the CGRAM render cursor
        // ([`Ppu::cgram_render_index`]) parks on palette entry 0 after each dot. (With
        // sub-screen layers enabled the cursor keeps the last real fetch instead --
        // a per-pixel sub-coverage check isn't modeled here.)
        if self.ts & 0x1F == 0 {
            self.cgram_render_index.set(0);
        }
    }

    /// Upgrade the frame in progress to the hires layout, re-laying-out every row
    /// already drawn (Mesen2 `SnesPpu::ConvertToHiRes`).
    ///
    /// Called from the `$2105` and `$2133` write paths. The upgrade is one-way: a
    /// frame that has committed to 512-column rows can never go back, because the
    /// rows already written cannot be un-written. Two write positions are skipped,
    /// both because the per-frame latch already covers them:
    ///
    /// - **scanline 0**, which the latch at the top of scanline 1 samples anyway;
    /// - **VBlank**, where there is nothing left to draw and the *next* frame's
    ///   latch will pick the new mode up.
    ///
    /// The rewrite walks rows downwards and columns rightwards to leftwards. Row `y`
    /// lands on rows `2y`/`2y+1`, and `2y` is never below any row still to be read,
    /// so the in-place expansion cannot clobber an unconverted source -- the same
    /// argument as Mesen2's backwards loop. The row in progress is converted whole:
    /// the columns to the right of the beam hold stale pixels either way, and this
    /// line's remaining dots overwrite them.
    pub(super) fn convert_to_hires(&mut self) {
        let wanted = self.hires_output_enabled() || self.interlace_enabled();
        let line = self.position.scanline;
        if !wanted
            || self.use_high_res_output
            || line < VISIBLE_LINE_START
            || line >= self.vblank_start_line()
        {
            return;
        }
        self.use_high_res_output = true;

        let current = (line - VISIBLE_LINE_START) as usize;
        let stride = self.framebuffer_stride();
        for y in (0..=current).rev() {
            let src = y * stride;
            let dst = y * 2 * stride;
            for x in (0..SCREEN_WIDTH).rev() {
                let color = self.framebuffer[src + x];
                self.framebuffer[dst + x * 2] = color;
                self.framebuffer[dst + x * 2 + 1] = color;
            }
            self.framebuffer
                .copy_within(dst..dst + stride, dst + stride);
            // The per-row brightness latch moves with its row, or the converted rows
            // would render black.
            self.line_inidisp[y * 2 + 1] = self.line_inidisp[y];
            self.line_inidisp[y * 2] = self.line_inidisp[y];
        }
    }

    /// The backdrop color (CGRAM entry 0) as a 15-bit BGR555 word.
    ///
    /// Records palette word 0 as the renderer's current fetch address, mirroring Mesen2's
    /// `RenderBgColor` (backdrop pixels reset `InternalCgramAddress` to 0).
    pub(super) fn backdrop_color(&self) -> u16 {
        self.cgram_render_index.set(0);
        let low = self.cgram[0] as u16;
        let high = self.cgram[1] as u16;
        (low | (high << 8)) & 0x7FFF
    }

    /// Snapshot the visible framebuffer as packed RGB888, applying INIDISP forced-blank and
    /// master brightness. Forced blank or brightness 0 yields a black screen.
    ///
    /// The output dimensions match [`Self::frame_dimensions`]. No normalization happens
    /// here: a hires or interlaced frame was already written 512 columns wide, and
    /// row-doubled where applicable, by [`Self::render_dot`] (#3034). The only
    /// adjustment left is the 239-line overscan clip to the 224-line Mesen2 window
    /// starting at internal row 7 (#3001).
    pub fn screen_snapshot_rgb(&self) -> Vec<u8> {
        let (width, height) = self.frame_dimensions();
        let width = width as usize;
        let height = height as usize;
        let mut out = vec![0u8; width * height * 3];

        let stride = self.framebuffer_stride();

        // Overscan: the Mesen2-compatible window starts 7 lines into the rendered
        // 239-line frame.  In the hires layout each source line occupies two
        // framebuffer rows, so the framebuffer offset is doubled.
        let row_mult = if self.use_high_res_output { 2 } else { 1 };
        let y_fb_offset = if self.overscan_239_enabled() {
            OVERSCAN_CROP_TOP * row_mult
        } else {
            0
        };

        for y in 0..height {
            let fb_y = y + y_fb_offset;
            let line_inidisp = self.line_inidisp[fb_y];
            let forced_blank = line_inidisp & 0x80 != 0;
            let brightness = (line_inidisp & 0x0F) as u32;
            if forced_blank || brightness == 0 {
                continue; // row already all-black
            }
            for x in 0..width {
                let pixel = self.framebuffer[fb_y * stride + x];
                let (r, g, b) = bgr555_to_rgb888(pixel, brightness);
                let idx = (y * width + x) * 3;
                out[idx] = r;
                out[idx + 1] = g;
                out[idx + 2] = b;
            }
        }
        out
    }
}

/// Convert a 15-bit BGR555 color to RGB888, scaled by master brightness `n` (0..=15).
///
/// Hardware scales each raw 5-bit channel *first* (`c5 * n / 15`), then expands the
/// scaled 5-bit result to 8 bits -- confirmed against Mesen2 (`Core/SNES/SnesPpu.cpp`:
/// `(pixel & 0x1F) * ScreenBrightness / 15`, `Core/Shared/ColorUtilities.h`:
/// `Convert5BitTo8Bit`). Scaling the already-8-bit-expanded value instead rounds
/// differently and was a real (if subtle) rendering divergence from real hardware.
fn bgr555_to_rgb888(bgr: u16, brightness: u32) -> (u8, u8, u8) {
    let r5 = (bgr & 0x1F) as u32;
    let g5 = ((bgr >> 5) & 0x1F) as u32;
    let b5 = ((bgr >> 10) & 0x1F) as u32;

    let expand = |c5: u32| (c5 << 3) | (c5 >> 2);
    let scale = |c5: u32| expand((c5 * brightness) / 15) as u8;

    (scale(r5), scale(g5), scale(b5))
}

#[cfg(test)]
mod tests {
    use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME, Ppu};

    fn render_full_frame(ppu: &mut Ppu) {
        let ticks =
            DOTS_PER_SCANLINE as u32 * NTSC_SCANLINES_PER_FRAME as u32 * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks {
            ppu.tick();
        }
    }

    fn set_backdrop(ppu: &mut Ppu, bgr555: u16) {
        ppu.write_register(0x2121, 0x00); // CGADD = color 0
        ppu.write_register(0x2122, (bgr555 & 0xFF) as u8);
        ppu.write_register(0x2122, (bgr555 >> 8) as u8);
    }

    #[test]
    fn snapshot_outputs_the_backdrop_color_at_full_brightness() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red
        ppu.write_register(0x2100, 0x0F); // brightness 15, no forced blank
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(&rgb[0..3], &[255, 0, 0], "top-left pixel is full red");
        let last = (256 * 224 - 1) * 3;
        assert_eq!(&rgb[last..last + 3], &[255, 0, 0], "bottom-right pixel too");
    }

    #[test]
    fn snapshot_outputs_blue_and_green_channels() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x7C00); // full blue
        ppu.write_register(0x2100, 0x0F);
        render_full_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(&rgb[0..3], &[0, 0, 255]);

        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x03E0); // full green
        ppu.write_register(0x2100, 0x0F);
        render_full_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(&rgb[0..3], &[0, 255, 0]);
    }

    #[test]
    fn forced_blank_outputs_black() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x7FFF); // white
        ppu.write_register(0x2100, 0x8F); // forced blank + brightness 15
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert!(rgb.iter().all(|&c| c == 0), "forced blank is black");
    }

    #[test]
    fn brightness_zero_outputs_black() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x7FFF);
        ppu.write_register(0x2100, 0x00); // brightness 0
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert!(rgb.iter().all(|&c| c == 0), "brightness 0 is black");
    }

    #[test]
    fn half_brightness_scales_the_output() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red (raw 5-bit red = 31)
        ppu.write_register(0x2100, 0x07); // brightness 7
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // Hardware scales the raw 5-bit component first (31 * 7 / 15 = 14,
        // truncated), then expands to 8-bit ((14 << 3) + (14 >> 2) = 115) --
        // confirmed against Mesen2 (Core/SNES/SnesPpu.cpp: `(pixel & 0x1F) *
        // ScreenBrightness / 15`, Core/Shared/ColorUtilities.h:
        // Convert5BitTo8Bit). Scaling the already-8-bit-expanded value
        // instead (255 * 8 / 16 = 127, the previous behavior here) rounds
        // differently and was a real, if subtle, rendering divergence.
        assert_eq!(rgb[0], 115);
        assert_eq!(rgb[1], 0);
        assert_eq!(rgb[2], 0);
    }

    #[test]
    fn inidisp_changed_mid_frame_only_affects_later_scanlines() {
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red
        ppu.write_register(0x2100, 0x0F); // brightness 15, no forced blank

        // Render the first 50 scanlines at full brightness (this is exactly
        // what HDMA-driven per-scanline INIDISP writes rely on, e.g. the
        // undisbeliever scpu-a-dma-bug/hdmaen_latch_test ROMs' scanline
        // brightness banding).
        let ticks_per_line = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT;
        for _ in 0..(ticks_per_line * 50) {
            ppu.tick();
        }

        // Switch to forced blank partway through the frame.
        ppu.write_register(0x2100, 0x80);
        for _ in 0..(ticks_per_line * (u32::from(NTSC_SCANLINES_PER_FRAME) - 50)) {
            ppu.tick();
        }

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            &rgb[0..3],
            &[255, 0, 0],
            "scanline rendered before the INIDISP change keeps its original brightness"
        );
        let later_row = 100usize;
        let idx = later_row * 256 * 3;
        assert_eq!(
            &rgb[idx..idx + 3],
            &[0, 0, 0],
            "scanline rendered after the INIDISP change reflects forced blank"
        );
    }

    #[test]
    fn overscan_239_mode_outputs_mesen2_compatible_224_line_snapshot() {
        // Internal rendering still covers 239 lines, but the Mesen2-compatible
        // snapshot is clipped to the 224-line window starting at rendered row 7
        // (#3001: rows 7..231, Rust-exclusive = 224 rows).
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x2133, 0x04); // SETINI overscan/tall-screen bit
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(
            rgb.len(),
            256 * 224 * 3,
            "output is 256×224 (Mesen2-compatible)"
        );
        assert_eq!(
            &rgb[0..3],
            &[255, 0, 0],
            "top output row is red (rendered row 7)"
        );
        let last = (256 * 224 - 1) * 3;
        assert_eq!(
            &rgb[last..last + 3],
            &[255, 0, 0],
            "bottom output row is red (rendered row 230)"
        );
    }

    #[test]
    fn overscan_snapshot_skips_top_7_rendered_rows() {
        // Sets rendered rows 0..6 to blue, rows 7..238 to red.  The output
        // window starts at row 7, so the snapshot must be fully red (no blue).
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x2133, 0x04); // overscan

        // Render a frame: the backdrop color changes mid-frame by writing CGRAM
        // at the start of lines 0 and 7 via a simple tick-step approach.
        // Because changing the backdrop mid-frame is done indirectly, we use the
        // lower-level API: render lines manually with two different colors.
        use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT};
        let ticks_per_line = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT;

        // Rendered rows 0..6 (scanlines 1..7) must be blue.  Scanline 0 is not
        // visible (VISIBLE_LINE_START = 1), so we must tick 8 scanlines (0..7)
        // to cause scanlines 1..7 to be rendered, giving us 7 blue rows.
        set_backdrop(&mut ppu, 0x7C00); // full blue
        for _ in 0..(ticks_per_line * 8) {
            ppu.tick();
        }
        // Rows 7..238: red backdrop.
        set_backdrop(&mut ppu, 0x001F); // full red
        let remaining = NTSC_SCANLINES_PER_FRAME as u32 - 8;
        for _ in 0..(ticks_per_line * remaining) {
            ppu.tick();
        }

        let rgb = ppu.screen_snapshot_rgb();
        assert_eq!(rgb.len(), 256 * 224 * 3);
        // Every pixel in the output must be red (none of the blue rows 0..6 leaked in).
        for chunk in rgb.chunks_exact(3) {
            assert_eq!(
                chunk,
                &[255, 0, 0],
                "overscan output must start at rendered row 7"
            );
        }
    }

    /// Backdrop-only scene whose *columns* differ: the CGWSEL clip-to-black region
    /// (colour window 1, native x = 64..191) blacks out the backdrop inside the window
    /// and leaves it alone outside, without any BG/OBJ setup. Gives the doubling tests
    /// a real column pattern to check against instead of a uniform field.
    fn set_clipped_backdrop(ppu: &mut Ppu, bgr555: u16) {
        set_backdrop(ppu, bgr555);
        ppu.write_register(0x2125, 0x20); // WOBJSEL: colour window 1 enabled, inside
        ppu.write_register(0x2126, 64); // WH0: left
        ppu.write_register(0x2127, 191); // WH1: right
        ppu.write_register(0x2130, 0x80); // CGWSEL: clip main to black inside the window
        ppu.write_register(0x2100, 0x0F);
    }

    fn pixel(rgb: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let i = (y * width + x) * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLACK: [u8; 3] = [0, 0, 0];

    #[test]
    fn a_progressive_hires_frame_duplicates_every_line_into_the_next_row() {
        // Mesen2 renders hires into a 478-row buffer, copying each non-interlaced
        // line to the following row (ApplyHiResMode's `memcpy(baseAddr + 512, ...)`),
        // and NESER now matches that geometry byte for byte (#3034).
        //
        // Rows are made distinguishable by stepping INIDISP brightness per scanline,
        // so "row 2y equals row 2y+1" is a real claim rather than a tautology about a
        // uniform screen.
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red
        ppu.write_register(0x2133, 0x08); // pseudo-hires, no interlace
        ppu.write_register(0x2100, 0x0F);

        let ticks_per_line = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT;
        for line in 0..NTSC_SCANLINES_PER_FRAME as u32 {
            // Brightness 15 down to 1, one step every 16 lines.
            ppu.write_register(0x2100, 15 - ((line / 16) as u8 & 0x0E));
            for _ in 0..ticks_per_line {
                ppu.tick();
            }
        }

        let (width, height) = ppu.frame_dimensions();
        assert_eq!((width, height), (512, 448));
        let rgb = ppu.screen_snapshot_rgb();
        let width = width as usize;

        let mut distinct_pairs = 0;
        for y in 0..(height as usize / 2) {
            let top = pixel(&rgb, width, 0, y * 2);
            let bottom = pixel(&rgb, width, 0, y * 2 + 1);
            assert_eq!(
                top, bottom,
                "display line {y} must fill both of its framebuffer rows"
            );
            if y > 0 && top != pixel(&rgb, width, 0, (y - 1) * 2) {
                distinct_pairs += 1;
            }
        }
        assert!(
            distinct_pairs >= 6,
            "the brightness ramp must make rows differ, or the pair check is vacuous \
             (saw {distinct_pairs} changes)"
        );
    }

    #[test]
    fn native_content_in_a_hires_frame_is_column_doubled_at_write_time() {
        // Screen interlace forces the hires layout with no hires *content*
        // (Mesen2 `_useHighResOutput = IsDoubleWidth() || ScreenInterlace`), so each
        // composed pixel fills both columns of its pair. That doubling used to happen
        // at snapshot time; it now happens as the dot is written (#3034).
        let mut ppu = Ppu::new();
        set_clipped_backdrop(&mut ppu, 0x001F); // red, blacked out at native x 64..191
        ppu.write_register(0x2133, 0x01); // screen interlace
        render_full_frame(&mut ppu);

        let (width, height) = ppu.frame_dimensions();
        assert_eq!((width, height), (512, 448));
        let rgb = ppu.screen_snapshot_rgb();
        let width = width as usize;

        let row = 20;
        for x in 0..256 {
            assert_eq!(
                pixel(&rgb, width, x * 2, row),
                pixel(&rgb, width, x * 2 + 1, row),
                "native column {x} must fill both output columns"
            );
        }
        // The clip boundary pins the doubling: native 63/64 -> output 127/128.
        assert_eq!(
            pixel(&rgb, width, 127, row),
            RED,
            "last column before the clip"
        );
        assert_eq!(pixel(&rgb, width, 128, row), BLACK, "first clipped column");
        assert_eq!(pixel(&rgb, width, 383, row), BLACK, "last clipped column");
        assert_eq!(
            pixel(&rgb, width, 384, row),
            RED,
            "first column after the clip"
        );
    }

    #[test]
    fn interlace_writes_only_its_own_field_row_and_does_not_duplicate() {
        // The two hires row-fill rules are mutually exclusive: interlace weaves the
        // fields into alternating rows (#3017) instead of duplicating a line.
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F);
        ppu.write_register(0x2133, 0x01); // screen interlace
        ppu.write_register(0x2100, 0x0F);
        render_full_frame(&mut ppu);

        assert!(ppu.use_high_res_output, "interlace forces the hires layout");
        assert!(
            !ppu.duplicates_row(),
            "an interlaced line must not be copied over the other field's row"
        );
        let field = ppu.interlace_field as usize;
        assert_eq!(
            ppu.framebuffer_row(10),
            10 * 2 + field,
            "display line 10 renders into its own field's row of the pair"
        );
    }

    #[test]
    fn brightness_applies_to_both_rows_of_a_doubled_line() {
        // line_inidisp is indexed by framebuffer row, so the duplicate row needs its
        // own latch -- otherwise every second output row reads brightness 0 and the
        // frame comes out half black.
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F); // full red
        ppu.write_register(0x2133, 0x08); // pseudo-hires, no interlace
        ppu.write_register(0x2100, 0x0F);
        render_full_frame(&mut ppu);

        let (width, _) = ppu.frame_dimensions();
        let rgb = ppu.screen_snapshot_rgb();
        let width = width as usize;
        for y in [0usize, 1, 100, 101, 446, 447] {
            assert_eq!(
                pixel(&rgb, width, 0, y),
                RED,
                "row {y} must carry this line's brightness"
            );
        }
    }

    /// Render `lines` scanlines' worth of ticks.
    fn tick_lines(ppu: &mut Ppu, lines: u32) {
        let ticks = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT * lines;
        for _ in 0..ticks {
            ppu.tick();
        }
    }

    /// Render a frame that starts native and turns hires at display line 40, then
    /// return its RGB snapshot. `switch` performs the mid-frame register write.
    fn frame_with_midframe_hires_switch(switch: &[(u16, u8)]) -> (Ppu, Vec<u8>) {
        let mut ppu = Ppu::new();
        set_clipped_backdrop(&mut ppu, 0x001F); // red, blacked out at native x 64..191
        tick_lines(&mut ppu, 41); // scanlines 0..40 -> display lines 0..39 drawn native
        for &(addr, value) in switch {
            ppu.write_register(addr, value);
        }
        tick_lines(&mut ppu, NTSC_SCANLINES_PER_FRAME as u32 - 41);
        let rgb = ppu.screen_snapshot_rgb();
        (ppu, rgb)
    }

    /// Assert that display line `y` of a converted frame carries the clipped-backdrop
    /// column pattern, column-doubled, in both of its framebuffer rows.
    fn assert_converted_line(rgb: &[u8], y: usize, label: &str) {
        const W: usize = 512;
        for row in [y * 2, y * 2 + 1] {
            assert_eq!(
                pixel(rgb, W, 0, row),
                RED,
                "{label}: row {row} must keep its pre-switch colour"
            );
            assert_eq!(pixel(rgb, W, 127, row), RED, "{label}: row {row} @127");
            assert_eq!(pixel(rgb, W, 128, row), BLACK, "{label}: row {row} @128");
            assert_eq!(pixel(rgb, W, 383, row), BLACK, "{label}: row {row} @383");
            assert_eq!(pixel(rgb, W, 384, row), RED, "{label}: row {row} @384");
        }
    }

    #[test]
    fn a_mid_frame_bgmode_switch_to_hires_converts_the_rows_already_drawn() {
        // Mesen2 `ConvertToHiRes`: turning on a double-width mode part-way down a
        // frame re-lays-out everything drawn so far, so the whole frame ends up in
        // one layout (#3034). Without it, display lines 0..39 would still sit in the
        // 256-column rows 0..39 while the snapshot reads 512-column rows 0..447.
        let (ppu, rgb) = frame_with_midframe_hires_switch(&[(0x2105, 0x05)]);

        assert_eq!(ppu.frame_dimensions(), (512, 448));
        assert_converted_line(&rgb, 0, "first line of the frame");
        assert_converted_line(&rgb, 39, "last line before the switch");
    }

    #[test]
    fn a_mid_frame_setini_pseudo_hires_switch_converts_the_rows_already_drawn() {
        // SETINI bit 3 reaches ConvertToHiRes exactly like BGMODE does.
        let (ppu, rgb) = frame_with_midframe_hires_switch(&[(0x2133, 0x08)]);

        assert_eq!(ppu.frame_dimensions(), (512, 448));
        assert_converted_line(&rgb, 0, "first line of the frame");
        assert_converted_line(&rgb, 39, "last line before the switch");
    }

    #[test]
    fn a_mid_frame_switch_to_interlace_converts_the_rows_already_drawn() {
        // Mesen2 folds ScreenInterlace into the same latch and the same conversion,
        // so enabling it mid-frame upgrades the layout just like hires does.
        let (ppu, rgb) = frame_with_midframe_hires_switch(&[(0x2133, 0x01)]);

        assert_eq!(ppu.frame_dimensions(), (512, 448));
        assert_converted_line(&rgb, 0, "first line of the frame");
    }

    #[test]
    fn leaving_hires_mid_frame_neither_shrinks_nor_converts_the_frame() {
        // The conversion is one-way. A frame that began hires stays 512 wide, and
        // its native rows are simply column-doubled as they are written.
        let mut ppu = Ppu::new();
        set_clipped_backdrop(&mut ppu, 0x001F);
        ppu.write_register(0x2133, 0x08); // pseudo-hires before the frame starts
        tick_lines(&mut ppu, 41);
        ppu.write_register(0x2133, 0x00); // back to native, mid-frame
        tick_lines(&mut ppu, NTSC_SCANLINES_PER_FRAME as u32 - 41);

        assert_eq!(ppu.frame_dimensions(), (512, 448));
        let rgb = ppu.screen_snapshot_rgb();
        assert_converted_line(&rgb, 10, "line drawn while hires");
        assert_converted_line(&rgb, 100, "line drawn after leaving hires");
    }

    #[test]
    fn a_hires_write_during_vblank_defers_to_the_next_frame_latch() {
        // Mesen2 `ConvertToHiRes` bails once the beam is past the last visible
        // scanline: there is nothing to convert, and the next frame's latch picks
        // the new mode up anyway.
        let mut ppu = Ppu::new();
        set_backdrop(&mut ppu, 0x001F);
        ppu.write_register(0x2100, 0x0F);
        render_full_frame(&mut ppu); // finish a native frame, land in the next one
        tick_lines(&mut ppu, 230); // into VBlank
        assert!(
            ppu.vblank_active,
            "the beam must be in VBlank for this test"
        );

        ppu.write_register(0x2133, 0x08);
        assert_eq!(
            ppu.frame_dimensions(),
            (256, 224),
            "the frame just rendered keeps its native geometry"
        );

        render_full_frame(&mut ppu);
        assert_eq!(
            ppu.frame_dimensions(),
            (512, 448),
            "the next frame latches the new mode"
        );
    }

    #[test]
    fn overscan_in_a_hires_frame_crops_14_framebuffer_rows() {
        // The 7-line Mesen2 crop is expressed in display lines, so in the hires
        // layout it skips 14 framebuffer rows (#3001 + #3034).
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x0C); // overscan + pseudo-hires
        ppu.write_register(0x2100, 0x0F);

        let ticks_per_line = u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT;
        // Display lines 0..6 blue (scanlines 1..7), the rest red.
        set_backdrop(&mut ppu, 0x7C00);
        for _ in 0..(ticks_per_line * 8) {
            ppu.tick();
        }
        set_backdrop(&mut ppu, 0x001F);
        for _ in 0..(ticks_per_line * (NTSC_SCANLINES_PER_FRAME as u32 - 8)) {
            ppu.tick();
        }

        let (width, height) = ppu.frame_dimensions();
        assert_eq!((width, height), (512, 448));
        let rgb = ppu.screen_snapshot_rgb();
        for chunk in rgb.chunks_exact(3) {
            assert_eq!(chunk, &RED, "the hires overscan window starts at row 14");
        }
    }

    #[test]
    fn frame_complete_is_set_once_when_entering_vblank() {
        let mut ppu = Ppu::new();
        assert_eq!(ppu.take_completed_frames(), 0);

        // Advance to VBlank entry (scanline 225).
        for _ in 0..(DOTS_PER_SCANLINE as u32 * 225 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }

        assert_eq!(
            ppu.take_completed_frames(),
            1,
            "frame complete at VBlank entry"
        );
        assert_eq!(ppu.take_completed_frames(), 0, "count drained");
    }

    #[test]
    fn frame_complete_in_239_line_mode_triggers_at_scanline_240() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2133, 0x04); // SETINI overscan/tall-screen bit
        assert_eq!(ppu.take_completed_frames(), 0);

        // 239 visible lines are completed, but VBlank has not started yet.
        for _ in 0..(DOTS_PER_SCANLINE as u32 * 239 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }
        assert_eq!(ppu.take_completed_frames(), 0);

        // One more scanline enters VBlank at line 240.
        for _ in 0..(DOTS_PER_SCANLINE as u32 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }
        assert_eq!(
            ppu.take_completed_frames(),
            1,
            "frame complete at overscan VBlank entry"
        );
        assert_eq!(ppu.take_completed_frames(), 0, "count drained");
    }
}
