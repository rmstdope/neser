//! The GSU bitmap unit: COLOR/GETC into COLR, CMODE into POR, PLOT and RPIX (fullsnes "SNES Cart
//! GSU-n Bitmap I/O Ports" and "Pixel-Cache").
//!
//! PLOT draws into SNES bitplane tiles in Game Pak RAM, laid out as a BG map of increasing tile
//! numbers down each column (or as OBJ tiles). Pixels are gathered per eight-pixel row in a
//! primary cache; moving to another row, or filling all eight, hands the row to a secondary cache
//! whose previous row is written out first, merged with RAM when only some of its pixels were
//! plotted. RPIX empties both caches before reading RAM. The cache model and its costs follow
//! Mesen2 `Gsu::DrawPixel`/`FlushPrimaryCache`/`WritePixelCache`/`ReadPixel`.

use super::{Gsu, GsuPixelCache};

/// POR bits (fullsnes "POR - Plot Option Register").
const POR_PLOT_COLOR_0: u8 = 0x01;
const POR_DITHER: u8 = 0x02;
const POR_HIGH_NIBBLE: u8 = 0x04;
const POR_FREEZE_HIGH: u8 = 0x08;
const POR_OBJ_MODE: u8 = 0x10;

/// Screen layouts selected by SCMR HT0/HT1, or forced to OBJ by POR bit 4.
#[derive(Clone, Copy)]
enum ScreenLayout {
    Height128,
    Height160,
    Height192,
    Obj,
}

impl Gsu {
    /// The value COLOR/GETC load into COLR for an incoming byte. POR bit 2 (High-Nibble) puts
    /// the incoming high nibble into COLR's low nibble, and POR bit 3 (Freeze-High) puts the
    /// incoming low nibble there; either way COLR keeps its own high nibble. fullsnes' diagram
    /// could be read as High-Nibble alone also loading the incoming high nibble into COLR's high
    /// nibble, but its text only says the incoming "LSB" is replaced, and Mesen2 (`GetColor`)
    /// and ares (`SuperFX::color`) agree on keeping COLR's.
    pub(super) fn color_register_input(&self, value: u8) -> u8 {
        let por = self.state.por;
        if por & POR_HIGH_NIBBLE != 0 {
            (self.state.colr & 0xF0) | (value >> 4)
        } else if por & POR_FREEZE_HIGH != 0 {
            (self.state.colr & 0xF0) | (value & 0x0F)
        } else {
            value
        }
    }

    /// Bits per pixel from SCMR MD0-1: 4, 16, (reserved), 256 colours. The reserved setting
    /// behaves as 16 colours, as in Mesen2.
    fn plot_bpp(&self) -> usize {
        match self.state.scmr & 0x03 {
            0 => 2,
            3 => 8,
            _ => 4,
        }
    }

    fn screen_layout(&self) -> ScreenLayout {
        if self.state.por & POR_OBJ_MODE != 0 {
            return ScreenLayout::Obj;
        }
        let scmr = self.state.scmr;
        match (scmr >> 2) & 1 | (scmr >> 4) & 2 {
            0 => ScreenLayout::Height128,
            1 => ScreenLayout::Height160,
            2 => ScreenLayout::Height192,
            _ => ScreenLayout::Obj,
        }
    }

    /// fullsnes "The Tile Number is calculated as".
    fn tile_number(&self, x: u8, y: u8) -> usize {
        let (x, y) = (usize::from(x), usize::from(y));
        match self.screen_layout() {
            ScreenLayout::Height128 => (x / 8) * 0x10 + y / 8,
            ScreenLayout::Height160 => (x / 8) * 0x14 + y / 8,
            ScreenLayout::Height192 => (x / 8) * 0x18 + y / 8,
            ScreenLayout::Obj => {
                (y / 0x80) * 0x200 + (x / 0x80) * 0x100 + ((y / 8) & 0x0F) * 0x10 + ((x / 8) & 0x0F)
            }
        }
    }

    /// The Game Pak RAM offset of the plane-0/1 byte pair of pixel row `(x, y)`: fullsnes
    /// "TileNo*(8*bpp) + SCBR*400h + (Y AND 7)*2". Planes 2/3, 4/5, 6/7 follow at +$10, +$20,
    /// +$30.
    pub(super) fn plot_row_address(&self, x: u8, y: u8) -> usize {
        self.tile_number(x, y) * self.plot_bpp() * 8
            + usize::from(self.state.scbr) * 0x400
            + usize::from(y & 7) * 2
    }

    /// Byte offset of bitplane `plane` within a tile row.
    fn plane_offset(plane: usize) -> usize {
        (plane >> 1) * 0x10 + (plane & 1)
    }

    /// `$4E`: COLOR; with ALT1 CMODE (POR = Sreg & $1F).
    pub(super) fn op_color_cmode(&mut self) {
        let value = self.src() as u8;
        if self.state.alt1 {
            self.state.por = value & 0x1F;
        } else {
            self.state.colr = self.color_register_input(value);
        }
        self.reset_prefixes();
    }

    /// `$4C`: PLOT at (R1, R2), then R1 += 1; with ALT1 RPIX Dreg = pixel at (R1, R2).
    pub(super) fn op_plot_rpix(&mut self) {
        let x = self.state.r[1] as u8;
        let y = self.state.r[2] as u8;
        if self.state.alt1 {
            let value = self.read_pixel(x, y);
            self.write_dest(u16::from(value));
            self.state.zero = value == 0;
            // fullsnes wonders whether RPIX can ever set S; a 2/4/8-bit pixel read into a
            // zero-extended word cannot be negative.
            self.state.sign = false;
        } else {
            self.plot_pixel(x, y);
            self.state.r[1] = self.state.r[1].wrapping_add(1);
        }
        self.reset_prefixes();
    }

    fn plot_pixel(&mut self, x: u8, y: u8) {
        let bpp = self.plot_bpp();
        let por = self.state.por;
        // Transparency is decided on COLR itself, before dithering, as in both Mesen2
        // (`DrawPixel`) and ares (`SuperFX::plot`). A dithered half that comes out as colour 0 is
        // then written as 0, which is transparent to the PPU: fullsnes' "Dither can mix
        // transparent & non-transparent pixels".
        if por & POR_PLOT_COLOR_0 == 0 && Self::is_transparent(self.state.colr, bpp, por) {
            return;
        }
        let mut color = self.state.colr;
        if por & POR_DITHER != 0 && bpp != 8 {
            // fullsnes: "if (r1.bit0 XOR r2.bit0)=1 then COLOR/10h is used".
            if (x ^ y) & 1 != 0 {
                color >>= 4;
            }
            color &= 0x0F;
        }
        let primary = self.state.primary_pixels;
        if primary.x != x & 0xF8 || primary.y != y {
            self.flush_primary_pixels(x, y);
        }
        let bit = (x & 7) ^ 7;
        let primary = &mut self.state.primary_pixels;
        primary.pixels[usize::from(bit)] = color;
        primary.valid |= 1 << bit;
        if primary.valid == 0xFF {
            self.flush_primary_pixels(x, y);
        }
    }

    /// fullsnes: "the color 0 check tests the lower 2/4/8 bits of the drawing color (if POR.Bit3
    /// (Freeze-High) is set, then it checks only the lower 2/4 bits, and ignores upper 4bit even
    /// when in 256-color mode)".
    fn is_transparent(color: u8, bpp: usize, por: u8) -> bool {
        let mask = match bpp {
            2 => 0x03,
            4 => 0x0F,
            _ if por & POR_FREEZE_HIGH != 0 => 0x0F,
            _ => 0xFF,
        };
        color & mask == 0
    }

    /// Hands the primary row to the secondary cache (writing the secondary's row out first) and
    /// points the primary at the row of `(x, y)`.
    fn flush_primary_pixels(&mut self, x: u8, y: u8) {
        let secondary = self.state.secondary_pixels;
        self.write_pixel_row(secondary);
        self.state.secondary_pixels = self.state.primary_pixels;
        self.state.primary_pixels = GsuPixelCache {
            x: x & 0xF8,
            y,
            ..GsuPixelCache::default()
        };
    }

    /// Writes one cached row to RAM, one byte per bitplane, merging with the bytes already there
    /// when not all eight pixels were plotted.
    fn write_pixel_row(&mut self, row: GsuPixelCache) {
        if row.valid == 0 {
            return;
        }
        let address = self.plot_row_address(row.x, row.y);
        let cost = u64::from(self.memory_cost());
        for plane in 0..self.plot_bpp() {
            let mut value = (0..8).fold(0u8, |acc, bit| {
                acc | ((row.pixels[bit] >> plane) & 1) << bit
            });
            let index = address + Self::plane_offset(plane);
            if row.valid != 0xFF {
                self.step(cost);
                value = (value & row.valid) | (self.ram_byte(index) & !row.valid);
            }
            self.step(cost);
            self.wait_for_ram_access();
            self.write_ram_byte(index, value);
        }
    }

    fn read_pixel(&mut self, x: u8, y: u8) -> u8 {
        // RPIX reads RAM, so like LDW it first lets a buffered store land and waits for the bus.
        self.finish_ram_buffer();
        self.wait_for_ram_access();
        let secondary = self.state.secondary_pixels;
        self.write_pixel_row(secondary);
        self.state.secondary_pixels.valid = 0;
        let primary = self.state.primary_pixels;
        self.write_pixel_row(primary);
        self.state.primary_pixels.valid = 0;

        let address = self.plot_row_address(x, y);
        let bit = (x & 7) ^ 7;
        let cost = u64::from(self.memory_cost());
        let mut value = 0;
        for plane in 0..self.plot_bpp() {
            let byte = self.ram_byte(address + Self::plane_offset(plane));
            value |= ((byte >> bit) & 1) << plane;
            self.step(cost);
        }
        value
    }
}
