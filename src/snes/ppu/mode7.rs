//! BG Mode 7 affine (rotation/scaling) rendering, EXTBG, and the general-purpose multiply result.
//!
//! Mode 7 replaces the tile pipeline with a per-pixel affine transform. For each visible screen
//! coordinate the matrix registers map to a VRAM coordinate inside a fixed 128x128-tile (1024x1024
//! pixel) field; the 8bpp pixel is sampled from the interleaved Mode 7 VRAM layout (BG map at even
//! byte addresses, tile pixels at odd byte addresses). The formula follows fullsnes
//! (PPU Rotation/Scaling) with bsnes `sfc/ppu/mode7.cpp` as implementation evidence.
//!
//! Screen Y note: the hardware samples display line `N` (1..224) for framebuffer row `N-1`, so the
//! per-pixel `y` passed in (0-based row) is offset by +1 to obtain the hardware SCREEN.Y.

use super::{PixelSource, Ppu, ScreenPixel, ScreenTarget, VRAM_SIZE, WindowLayer};

impl Ppu {
    /// Apply the shared Mode 7 write-twice mechanism: `reg = value * 0x100 + M7_old`, then latch
    /// `M7_old = value`. Used by $210D/$210E (alongside the BG_old scroll latch) and $211B-$2120.
    pub(super) fn write_m7_twice(&mut self, value: u8) -> u16 {
        let combined = ((value as u16) << 8) | self.m7_old as u16;
        self.m7_old = value;
        combined
    }

    /// Signed 24-bit general-purpose multiply result `M7A * M7B`, where M7A is the full signed
    /// 16-bit value and M7B is the most-recently-written byte (the high byte) as a signed 8-bit.
    pub(super) fn mode7_multiply(&self) -> i32 {
        let a = self.m7a as i16 as i32;
        let b = (self.m7b >> 8) as u8 as i8 as i32;
        a * b
    }

    /// Compute the final BGR555 pixel for Mode 7 at visible screen coordinate `(x, y)`.
    ///
    /// Priority chart (BG only; OBJ is added in #2763): BG2(prio=1) over BG1 over BG2(prio=0) over
    /// backdrop. BG2 exists only when EXTBG (SETINI bit 6) is enabled.
    pub(super) fn compute_pixel_mode7(&self, x: u16, y: u16) -> u16 {
        let main = self.resolve_mode7_screen_pixel(ScreenTarget::Main, x, y);
        let sub = self.resolve_mode7_screen_pixel(ScreenTarget::Sub, x, y);
        self.compose_pixels(x, y, main, sub)
    }

    /// Sample the raw 8-bit Mode 7 pixel value at screen `(x, y)`, applying the affine transform,
    /// screen H/V flip, and screen-over handling. Returns 0 (transparent) where appropriate.
    ///
    /// Mosaic: when BG1 mosaic is enabled, horizontal x is snapped to the block left edge and
    /// vertical y is reduced by `mosaic_vcount` before the affine transform (fullsnes; BG2 EXTBG
    /// vertical mosaic also uses BG1 enable per bsnes evidence).
    fn mode7_pixel_value(&self, x: u16, y: u16) -> u8 {
        let xflip = self.m7sel & 0x01 != 0;
        let yflip = self.m7sel & 0x02 != 0;
        let screen_over = (self.m7sel >> 6) & 0x03;

        let a = self.m7a as i16 as i32;
        let b = self.m7b as i16 as i32;
        let c = self.m7c as i16 as i32;
        let d = self.m7d as i16 as i32;
        let hcenter = sign_extend_13(self.m7x);
        let vcenter = sign_extend_13(self.m7y);
        let hoffset = sign_extend_13(self.m7hofs);
        let voffset = sign_extend_13(self.m7vofs);

        // Apply mosaic to the affine sampling coordinates (BG1 enable controls both BG1 and
        // EXTBG BG2 vertical mosaic, matching bsnes behavior).
        let x_eff = if self.mosaic_bg_enabled(0) {
            self.mosaic_apply_x(x)
        } else {
            x
        };
        let y_eff = if self.mosaic_bg_enabled(0) {
            y.saturating_sub(self.mosaic_vcount as u16)
        } else {
            y
        };

        let sx = if xflip { 255 - x_eff } else { x_eff } as i32;
        // Hardware SCREEN.Y is the display line (1..224); framebuffer row `y` is line `y + 1`.
        let screen_y = y_eff as i32 + 1;
        let sy = if yflip { 255 - screen_y } else { screen_y };

        let clip_h = mode7_clip(hoffset - hcenter);
        let clip_v = mode7_clip(voffset - vcenter);
        let origin_x =
            ((a * clip_h) & !63) + ((b * clip_v) & !63) + ((b * sy) & !63) + (hcenter << 8);
        let origin_y =
            ((c * clip_h) & !63) + ((d * clip_v) & !63) + ((d * sy) & !63) + (vcenter << 8);

        let pixel_x = (origin_x + a * sx) >> 8;
        let pixel_y = (origin_y + c * sx) >> 8;
        let out_of_bounds = (pixel_x | pixel_y) & !1023 != 0;

        let tile_x = (pixel_x >> 3) & 127;
        let tile_y = (pixel_y >> 3) & 127;
        let tile_word = (tile_y * 128 + tile_x) as usize;
        // Screen-over 3 fills outside the field with tile 0; 2 makes it transparent.
        let tile = match screen_over {
            // fullsnes: modes 0 and 1 both wrap within the 128x128 tile field.
            0 | 1 => self.vram[(tile_word << 1) & (VRAM_SIZE - 1)],
            2 => {
                if out_of_bounds {
                    return 0;
                }
                self.vram[(tile_word << 1) & (VRAM_SIZE - 1)]
            }
            3 => {
                if out_of_bounds {
                    0
                } else {
                    self.vram[(tile_word << 1) & (VRAM_SIZE - 1)]
                }
            }
            _ => unreachable!(),
        };
        let palette_addr = (((pixel_y & 7) << 3) + (pixel_x & 7)) as usize;
        let pixel_word = ((tile as usize) << 6) | palette_addr;
        self.vram[((pixel_word << 1) | 1) & (VRAM_SIZE - 1)]
    }

    fn resolve_mode7_screen_pixel(&self, target: ScreenTarget, x: u16, y: u16) -> ScreenPixel {
        let bg1_enabled = self.screen_enable_mask(target) & 0x01 != 0;
        let bg2_enabled = self.screen_enable_mask(target) & 0x02 != 0;
        let extbg = self.setini & 0x40 != 0;
        let value = self.mode7_pixel_value(x, y);

        let bg2_color = if extbg && bg2_enabled && value & 0x7F != 0 {
            if self.layer_disabled_by_window(target, WindowLayer::Bg(1), x, y) {
                None
            } else {
                Some((self.cgram_color(value & 0x7F), value & 0x80 != 0))
            }
        } else {
            None
        };
        let bg1_color = if bg1_enabled && value != 0 {
            if self.layer_disabled_by_window(target, WindowLayer::Bg(0), x, y) {
                None
            } else {
                Some(if self.cgwsel & 0x01 != 0 {
                    mode7_direct_color(value)
                } else {
                    self.cgram_color(value)
                })
            }
        } else {
            None
        };

        // Mode 7 chart (front-to-back): OBJ.3, OBJ.2, BG2.1p, OBJ.1, BG1, OBJ.0, BG2.0p, backdrop.
        if !self.layer_disabled_by_window(target, WindowLayer::Obj, x, y) {
            if let Some((color, palette)) = self.obj_pixel_for_screen(target, x, y, 3) {
                return ScreenPixel {
                    color,
                    source: PixelSource::Obj {
                        priority: 3,
                        palette,
                    },
                };
            }
            if let Some((color, palette)) = self.obj_pixel_for_screen(target, x, y, 2) {
                return ScreenPixel {
                    color,
                    source: PixelSource::Obj {
                        priority: 2,
                        palette,
                    },
                };
            }
        }
        if let Some((color, true)) = bg2_color {
            return ScreenPixel {
                color,
                source: PixelSource::Bg(1),
            };
        }
        if !self.layer_disabled_by_window(target, WindowLayer::Obj, x, y)
            && let Some((color, palette)) = self.obj_pixel_for_screen(target, x, y, 1)
        {
            return ScreenPixel {
                color,
                source: PixelSource::Obj {
                    priority: 1,
                    palette,
                },
            };
        }
        if let Some(color) = bg1_color {
            return ScreenPixel {
                color,
                source: PixelSource::Bg(0),
            };
        }
        if !self.layer_disabled_by_window(target, WindowLayer::Obj, x, y)
            && let Some((color, palette)) = self.obj_pixel_for_screen(target, x, y, 0)
        {
            return ScreenPixel {
                color,
                source: PixelSource::Obj {
                    priority: 0,
                    palette,
                },
            };
        }
        if let Some((color, false)) = bg2_color {
            return ScreenPixel {
                color,
                source: PixelSource::Bg(1),
            };
        }
        ScreenPixel {
            color: self.backdrop_color_for(target),
            source: PixelSource::Backdrop,
        }
    }
}

/// Mode 7 13-bit clip / sign-extend per fullsnes (`AND NOT 1C00h`; the magnitude is bits 0-9 with
/// the sign at bit 13). Matches bsnes `clip()`.
fn mode7_clip(n: i32) -> i32 {
    if n & 0x2000 != 0 { n | !1023 } else { n & 1023 }
}

/// Sign-extend a raw 13-bit register value (sign bit 12) to i32.
fn sign_extend_13(v: u16) -> i32 {
    let v = (v & 0x1FFF) as i32;
    if v & 0x1000 != 0 { v | !0x1FFF } else { v }
}

/// Mode 7 BG1 direct-color conversion: the 8-bit pixel value (`BBGGGRRR`) expands to BGR555 with
/// the palette group fixed at 0 (per bsnes `directColor(0, value)`).
fn mode7_direct_color(value: u8) -> u16 {
    let red = (value as u16 & 0x07) << 2;
    let green = ((value as u16 >> 3) & 0x07) << 2;
    let blue = ((value as u16 >> 6) & 0x03) << 3;
    (blue << 10) | (green << 5) | red
}

#[cfg(test)]
mod tests {
    use super::super::{DOTS_PER_SCANLINE, MASTER_CYCLES_PER_DOT, NTSC_SCANLINES_PER_FRAME, Ppu};

    fn render_frame(ppu: &mut Ppu) {
        let ticks =
            DOTS_PER_SCANLINE as u32 * NTSC_SCANLINES_PER_FRAME as u32 * MASTER_CYCLES_PER_DOT;
        for _ in 0..ticks {
            ppu.tick();
        }
    }

    fn set_cgram(ppu: &mut Ppu, index: usize, bgr555: u16) {
        ppu.cgram[index * 2] = (bgr555 & 0xFF) as u8;
        ppu.cgram[index * 2 + 1] = (bgr555 >> 8) as u8;
    }

    /// Set a Mode 7 BG map entry (char number) at tile (tx, ty). Map entries are the even bytes.
    fn set_map(ppu: &mut Ppu, tx: usize, ty: usize, char_num: u8) {
        let word = ty * 128 + tx;
        ppu.vram[word * 2] = char_num;
    }

    /// Set a Mode 7 tile pixel (8bpp) at (px, py) within char `char_num`. Pixels are odd bytes.
    fn set_pixel(ppu: &mut Ppu, char_num: usize, px: usize, py: usize, value: u8) {
        let word = char_num * 64 + py * 8 + px;
        ppu.vram[word * 2 + 1] = value;
    }

    /// Write a Mode 7 matrix/center register via its two byte writes (low then high).
    fn write_m7(ppu: &mut Ppu, addr: u16, value: u16) {
        ppu.write_register(addr, (value & 0xFF) as u8);
        ppu.write_register(addr, (value >> 8) as u8);
    }

    /// Configure an identity Mode 7 transform (A=D=1.0, B=C=0, no scroll/center) and enable BG1.
    fn setup_identity(ppu: &mut Ppu) {
        ppu.write_register(0x2105, 0x07); // BGMODE = 7
        ppu.write_register(0x212C, 0x01); // TM: BG1 enabled
        ppu.write_register(0x2100, 0x0F); // brightness 15, no forced blank
        write_m7(ppu, 0x211B, 0x0100); // M7A = 1.0
        write_m7(ppu, 0x211C, 0x0000); // M7B = 0
        write_m7(ppu, 0x211D, 0x0000); // M7C = 0
        write_m7(ppu, 0x211E, 0x0100); // M7D = 1.0
        write_m7(ppu, 0x211F, 0x0000); // M7X = 0
        write_m7(ppu, 0x2120, 0x0000); // M7Y = 0
        ppu.write_register(0x210D, 0x00); // M7HOFS = 0
        ppu.write_register(0x210D, 0x00);
        ppu.write_register(0x210E, 0x00); // M7VOFS = 0
        ppu.write_register(0x210E, 0x00);
    }

    fn pixel_at(ppu: &mut Ppu, x: u16, y: u16) -> u16 {
        ppu.compute_pixel_mode7(x, y)
    }

    #[test]
    fn identity_transform_samples_screen_coordinate_with_line_offset() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        set_cgram(&mut ppu, 0, 0x0000); // backdrop black
        set_cgram(&mut ppu, 5, 0x7FFF); // color 5 = white
        // Identity maps screen (x, y) -> mode7 pixel (x, y + 1).
        set_map(&mut ppu, 0, 0, 1); // tile (0,0) uses char 1
        set_pixel(&mut ppu, 1, 3, 5, 5); // char 1, pixel (3,5) = color 5

        // Screen (3, 4) samples mode7 pixel (3, 5).
        assert_eq!(pixel_at(&mut ppu, 3, 4), 0x7FFF);
        // A neighboring pixel is transparent -> backdrop.
        assert_eq!(pixel_at(&mut ppu, 4, 4), 0x0000);
    }

    #[test]
    fn scaling_by_two_repeats_each_source_pixel() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        // M7A = M7D = 0.5 -> source advances half a pixel per screen pixel (2x magnification).
        write_m7(&mut ppu, 0x211B, 0x0080);
        write_m7(&mut ppu, 0x211E, 0x0080);
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 7, 0x03E0); // green
        set_map(&mut ppu, 0, 0, 1);
        // Source pixel (0,0) of char 1.
        set_pixel(&mut ppu, 1, 0, 0, 7);

        // With 0.5 step and SCREEN.Y = y+1, screen (0,0)->src(0, 0) since (y+1)*0.5 floors to 0.
        assert_eq!(pixel_at(&mut ppu, 0, 0), 0x03E0);
        assert_eq!(pixel_at(&mut ppu, 1, 0), 0x03E0); // x=1 -> src x = 0 (0.5 floor)
    }

    #[test]
    fn screen_over_wrap_repeats_the_field() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x00); // screen-over = 0 (wrap)
        // Zoom out 8x so screen x=200 -> source x=1600, which wraps to tile 72 within the field.
        write_m7(&mut ppu, 0x211B, 0x0800); // M7A = 8.0
        write_m7(&mut ppu, 0x211E, 0x0800); // M7D = 8.0
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 9, 0x7C00); // blue
        set_map(&mut ppu, 72, 1, 1); // wrapped tile (source 1600,8 -> tile 72,1)
        set_pixel(&mut ppu, 1, 0, 0, 9);
        assert_eq!(
            pixel_at(&mut ppu, 200, 0),
            0x7C00,
            "wraps to the in-field tile"
        );
    }

    #[test]
    fn screen_over_mode1_wraps_like_mode0() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x40); // screen-over = 1 (wrap, same as mode 0)
        write_m7(&mut ppu, 0x211B, 0x0800); // M7A = 8.0
        write_m7(&mut ppu, 0x211E, 0x0800); // M7D = 8.0
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 9, 0x7C00);
        set_map(&mut ppu, 72, 1, 1);
        set_pixel(&mut ppu, 1, 0, 0, 9);
        assert_eq!(
            pixel_at(&mut ppu, 200, 0),
            0x7C00,
            "mode 1 wraps to the in-field tile like mode 0"
        );
    }

    #[test]
    fn screen_over_transparent_outside_field() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x80); // screen-over = 2 (transparent outside)
        write_m7(&mut ppu, 0x211B, 0x0800);
        write_m7(&mut ppu, 0x211E, 0x0800);
        set_cgram(&mut ppu, 0, 0x1234); // backdrop
        set_cgram(&mut ppu, 9, 0x7C00);
        set_map(&mut ppu, 72, 1, 1);
        set_pixel(&mut ppu, 1, 0, 0, 9);
        // Same coordinate as the wrap test, but outside the field is transparent -> backdrop.
        assert_eq!(pixel_at(&mut ppu, 200, 0), 0x1234);
    }

    #[test]
    fn screen_over_fill_tile_zero_outside_field() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0xC0); // screen-over = 3 (fill with tile 0)
        write_m7(&mut ppu, 0x211B, 0x0800);
        write_m7(&mut ppu, 0x211E, 0x0800);
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 4, 0x03FF);
        // The wrapped tile (72,1) holds a different char, but fill mode forces tile 0.
        set_map(&mut ppu, 72, 1, 5);
        // Tile 0's pixel at the within-tile coordinate (1600&7, 8&7) = (0,0).
        set_pixel(&mut ppu, 0, 0, 0, 4);
        assert_eq!(
            pixel_at(&mut ppu, 200, 0),
            0x03FF,
            "outside field uses tile 0"
        );
    }

    #[test]
    fn h_flip_mirrors_the_screen_horizontally() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x01); // H-flip
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 6, 0x7FFF);
        set_map(&mut ppu, 31, 0, 1); // tile (31,0) covers source x 248..255
        set_pixel(&mut ppu, 1, 7, 1, 6); // src (255, 1)
        // With H-flip, screen x=0 maps to source x=255; SCREEN.Y(y=0)=1.
        assert_eq!(pixel_at(&mut ppu, 0, 0), 0x7FFF);
    }

    #[test]
    fn v_flip_mirrors_the_screen_vertically() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x02); // V-flip
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 6, 0x7FFF);
        // V-flip: SCREEN.Y for y=0 is (0+1) -> 255-1 = 254. Source y = 254 -> tile (.,31), py 6.
        set_map(&mut ppu, 0, 31, 1);
        set_pixel(&mut ppu, 1, 0, 6, 6); // src (0, 254)
        assert_eq!(pixel_at(&mut ppu, 0, 0), 0x7FFF);
    }

    #[test]
    fn extbg_bg2_high_priority_pixel_covers_bg1() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x212C, 0x03); // TM: BG1 + BG2
        ppu.write_register(0x2133, 0x40); // SETINI: EXTBG
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 0x10, 0x7FFF); // BG2 color 0x10 (white)
        set_map(&mut ppu, 0, 0, 1);
        // Value 0x90 -> bit7 priority set, color 0x10. BG1 would show CGRAM[0x90], BG2 wins.
        set_pixel(&mut ppu, 1, 0, 1, 0x90);
        set_cgram(&mut ppu, 0x90, 0x001F); // what BG1 would have shown (red)
        assert_eq!(
            pixel_at(&mut ppu, 0, 0),
            0x7FFF,
            "high-priority BG2 covers BG1"
        );
    }

    #[test]
    fn extbg_bg2_low_priority_pixel_is_below_bg1() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x212C, 0x03);
        ppu.write_register(0x2133, 0x40);
        set_cgram(&mut ppu, 0, 0x0000);
        set_map(&mut ppu, 0, 0, 1);
        // Value 0x10 -> bit7 clear (low priority). BG1 shows CGRAM[0x10], on top of BG2.
        set_pixel(&mut ppu, 1, 0, 1, 0x10);
        set_cgram(&mut ppu, 0x10, 0x001F); // BG1 color (red)
        assert_eq!(
            pixel_at(&mut ppu, 0, 0),
            0x001F,
            "BG1 covers low-priority BG2"
        );
    }

    #[test]
    fn direct_color_resolves_bg1_value_to_bgr555() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x2130, 0x01); // CGWSEL: direct color
        set_cgram(&mut ppu, 0, 0x0000);
        set_map(&mut ppu, 0, 0, 1);
        set_pixel(&mut ppu, 1, 0, 1, 0xFF); // BBGGGRRR = 0xFF
        // direct: red=(7)<<2, green=(7)<<2, blue=(3)<<3 -> 0x7FDC.
        let expected = (0x18u16 << 10) | (0x1Cu16 << 5) | 0x1C;
        assert_eq!(pixel_at(&mut ppu, 0, 0), expected);
    }

    #[test]
    fn mpy_result_is_signed_m7a_times_m7b_high_byte() {
        let mut ppu = Ppu::new();
        // M7A = 0x0100 (256), M7B high byte = 0x02 -> product = 512.
        write_m7(&mut ppu, 0x211B, 0x0100);
        write_m7(&mut ppu, 0x211C, 0x0200);
        assert_eq!(ppu.read_register(0x2134), 0x00);
        assert_eq!(ppu.read_register(0x2135), 0x02);
        assert_eq!(ppu.read_register(0x2136), 0x00);

        // Signed: M7A = -1 (0xFFFF), M7B high = 0x02 -> product = -2 = 0xFFFFFE.
        write_m7(&mut ppu, 0x211B, 0xFFFF);
        write_m7(&mut ppu, 0x211C, 0x0200);
        assert_eq!(ppu.read_register(0x2134), 0xFE);
        assert_eq!(ppu.read_register(0x2135), 0xFF);
        assert_eq!(ppu.read_register(0x2136), 0xFF);
    }

    #[test]
    fn m7hofs_write_updates_both_bg1_scroll_and_mode7_scroll() {
        let mut ppu = Ppu::new();
        // Write $210D twice: low=0x34, high=0x12.
        ppu.write_register(0x210D, 0x34);
        ppu.write_register(0x210D, 0x12);
        // M7HOFS uses the M7_old mechanism: value = 0x1234.
        assert_eq!(ppu.m7hofs, 0x1234);
        // BG1HOFS uses the BG_old mechanism and is non-zero (shared write).
        assert_ne!(ppu.bg_hofs[0], 0);
    }

    #[test]
    fn rotation_90_degrees_maps_screen_rows_to_source_columns() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        ppu.write_register(0x211A, 0x00); // wrap
        // 90-degree rotation: A=0, B=1.0, C=-1.0, D=0.
        write_m7(&mut ppu, 0x211B, 0x0000); // M7A = 0
        write_m7(&mut ppu, 0x211C, 0x0100); // M7B = 1.0
        write_m7(&mut ppu, 0x211D, 0xFF00); // M7C = -1.0
        write_m7(&mut ppu, 0x211E, 0x0000); // M7D = 0
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 2, 0x7FFF);
        set_map(&mut ppu, 0, 0, 1);
        // With this rotation, screen (0, y) samples source (y + 1, 0).
        set_pixel(&mut ppu, 1, 1, 0, 2); // source (1, 0)
        set_pixel(&mut ppu, 1, 6, 0, 2); // source (6, 0)
        assert_eq!(pixel_at(&mut ppu, 0, 0), 0x7FFF); // y=0 -> src x=1
        assert_eq!(pixel_at(&mut ppu, 0, 5), 0x7FFF); // y=5 -> src x=6
        assert_eq!(pixel_at(&mut ppu, 0, 1), 0x0000); // y=1 -> src x=2 (empty)
    }

    #[test]
    fn mode7_scene_matches_known_crc() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        // A small rotation+scale to exercise the affine path across the whole frame.
        write_m7(&mut ppu, 0x211B, 0x00E0); // M7A
        write_m7(&mut ppu, 0x211C, 0x0040); // M7B
        write_m7(&mut ppu, 0x211D, 0xFFC0); // M7C = -0.25
        write_m7(&mut ppu, 0x211E, 0x00E0); // M7D
        write_m7(&mut ppu, 0x211F, 0x0040); // M7X
        write_m7(&mut ppu, 0x2120, 0x0030); // M7Y
        // Build a checkerboard of two chars across the field, each a solid color.
        for ty in 0..16 {
            for tx in 0..16 {
                set_map(&mut ppu, tx, ty, if (tx + ty) & 1 == 0 { 1 } else { 2 });
            }
        }
        for px in 0..8 {
            for py in 0..8 {
                set_pixel(&mut ppu, 1, px, py, 10);
                set_pixel(&mut ppu, 2, px, py, 20);
            }
        }
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 10, 0x7C00);
        set_cgram(&mut ppu, 20, 0x001F);
        render_frame(&mut ppu);
        let pixels = ppu.screen_snapshot_rgb();
        let crc = crate::platform::crc32::crc32(&[&pixels]);
        assert_eq!(crc, 0x8AA6_9117, "Mode 7 scene CRC drifted: {crc:#010X}");
    }

    #[test]
    fn mode7_renders_into_the_framebuffer() {
        let mut ppu = Ppu::new();
        setup_identity(&mut ppu);
        set_cgram(&mut ppu, 0, 0x0000);
        set_cgram(&mut ppu, 3, 0x7FFF);
        // Fill the visible area's source tiles with char 1, all pixels color 3.
        for t in 0..32 {
            set_map(&mut ppu, t, 0, 1);
        }
        for px in 0..8 {
            for py in 0..8 {
                set_pixel(&mut ppu, 1, px, py, 3);
            }
        }
        render_frame(&mut ppu);
        let rgb = ppu.screen_snapshot_rgb();
        // Top-left visible pixel should be white.
        assert_eq!(&rgb[0..3], &[255, 255, 255]);
    }
}
