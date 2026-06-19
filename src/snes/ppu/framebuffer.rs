//! PPU framebuffer output: backdrop rendering and BGR555 -> RGB888 conversion.
//!
//! The dot pipeline writes a 15-bit BGR555 color per visible pixel into [`Ppu::framebuffer`].
//! Until BG/OBJ layers exist, every visible pixel is the backdrop color (CGRAM entry 0).
//! [`Ppu::screen_snapshot_rgb`] applies INIDISP forced-blank and master brightness at output,
//! converting to packed RGB888.

use super::{Ppu, SCREEN_HEIGHT, SCREEN_WIDTH, VISIBLE_DOT_START, VISIBLE_LINE_START};

impl Ppu {
    /// Render the pixel at the current dot (called once per dot from the timing loop).
    ///
    /// Only visible dots (active display region) write to the framebuffer.
    pub(super) fn render_dot(&mut self) {
        let line = self.position.scanline;
        let dot = self.position.dot;
        if line < VISIBLE_LINE_START || line as usize >= VISIBLE_LINE_START as usize + SCREEN_HEIGHT
        {
            return;
        }
        if dot < VISIBLE_DOT_START || dot as usize >= VISIBLE_DOT_START as usize + SCREEN_WIDTH {
            return;
        }
        let x = (dot - VISIBLE_DOT_START) as usize;
        let y = (line - VISIBLE_LINE_START) as usize;
        // Build the composited OBJ line once, at the first visible dot of each visible scanline.
        // The over-limit flags are evaluated one scanline earlier (see `update_obj_pipeline`); a
        // mid-scanline OAM/OBSEL write between the two points could make STAT77 and the displayed
        // pixels disagree, which is an accepted limitation of this per-scanline model.
        if x == 0 {
            self.obj_line = self.build_obj_line(y as u16);
        }
        self.framebuffer[y * SCREEN_WIDTH + x] = self.compute_pixel(x as u16, y as u16);
    }

    /// The backdrop color (CGRAM entry 0) as a 15-bit BGR555 word.
    pub(super) fn backdrop_color(&self) -> u16 {
        let low = self.cgram[0] as u16;
        let high = self.cgram[1] as u16;
        (low | (high << 8)) & 0x7FFF
    }

    /// Snapshot the visible framebuffer as packed RGB888, applying INIDISP forced-blank and
    /// master brightness. Forced blank or brightness 0 yields a black screen.
    pub fn screen_snapshot_rgb(&self) -> Vec<u8> {
        let mut out = vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 3];
        let forced_blank = self.inidisp & 0x80 != 0;
        let brightness = (self.inidisp & 0x0F) as u32;

        if forced_blank || brightness == 0 {
            return out; // already all-black
        }

        for (pixel, &bgr) in self.framebuffer.iter().enumerate() {
            let (r, g, b) = bgr555_to_rgb888(bgr, brightness);
            out[pixel * 3] = r;
            out[pixel * 3 + 1] = g;
            out[pixel * 3 + 2] = b;
        }
        out
    }
}

/// Convert a 15-bit BGR555 color to RGB888, scaled by master brightness `n` (1..=15).
///
/// Each 5-bit channel is expanded to 8 bits, then scaled by `(n + 1) / 16` per INIDISP.
fn bgr555_to_rgb888(bgr: u16, brightness: u32) -> (u8, u8, u8) {
    let r5 = (bgr & 0x1F) as u32;
    let g5 = ((bgr >> 5) & 0x1F) as u32;
    let b5 = ((bgr >> 10) & 0x1F) as u32;

    let expand = |c5: u32| (c5 << 3) | (c5 >> 2);
    let scale = |c8: u32| ((c8 * (brightness + 1)) / 16) as u8;

    (scale(expand(r5)), scale(expand(g5)), scale(expand(b5)))
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
        set_backdrop(&mut ppu, 0x001F); // full red (255 at full brightness)
        ppu.write_register(0x2100, 0x07); // brightness 7 -> factor 8/16
        render_full_frame(&mut ppu);

        let rgb = ppu.screen_snapshot_rgb();
        // 255 * 8 / 16 = 127
        assert_eq!(rgb[0], 127);
        assert_eq!(rgb[1], 0);
        assert_eq!(rgb[2], 0);
    }

    #[test]
    fn frame_complete_is_set_once_when_entering_vblank() {
        let mut ppu = Ppu::new();
        assert!(!ppu.take_frame_complete());

        // Advance to VBlank entry (scanline 225).
        for _ in 0..(DOTS_PER_SCANLINE as u32 * 225 * MASTER_CYCLES_PER_DOT) {
            ppu.tick();
        }

        assert!(ppu.take_frame_complete(), "frame complete at VBlank entry");
        assert!(!ppu.take_frame_complete(), "flag consumed");
    }
}
