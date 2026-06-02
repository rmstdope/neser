use crate::nes::console::Nes;
use crate::nes::ppu::NesPalette;

const CHR_SIZE: usize = 8192;
const NAMETABLE_SIZE: usize = 1024;
const NUM_NAMETABLES: usize = 4;
const PALETTE_SIZE: usize = 32;

const TILE_SIZE: usize = 8;
const CHR_BYTES_PER_TILE: usize = TILE_SIZE * 2;
const PATTERN_TABLE_TILES_PER_ROW: usize = 16;
const PATTERN_TABLE_WIDTH: usize = TILE_SIZE * PATTERN_TABLE_TILES_PER_ROW;
const PATTERN_TABLE_OUTPUT_WIDTH: usize = PATTERN_TABLE_WIDTH * 2;
const PATTERN_TABLE_OUTPUT_HEIGHT: usize = PATTERN_TABLE_WIDTH;

const NAMETABLE_TILE_COLS: usize = 32;
const NAMETABLE_TILE_ROWS: usize = 30;
const NAMETABLE_WIDTH: usize = TILE_SIZE * NAMETABLE_TILE_COLS;
const NAMETABLE_HEIGHT: usize = TILE_SIZE * NAMETABLE_TILE_ROWS;
const NAMETABLE_OUTPUT_WIDTH: usize = NAMETABLE_WIDTH * 2;
const NAMETABLE_OUTPUT_HEIGHT: usize = NAMETABLE_HEIGHT * 2;
const ATTRIBUTE_TABLE_OFFSET: usize = NAMETABLE_TILE_COLS * NAMETABLE_TILE_ROWS;
const ATTRIBUTE_TABLE_COLS: usize = NAMETABLE_TILE_COLS / 4;

/// Snapshot of PPU state needed to render the graphical PPU viewer.
pub struct PpuViewerSnapshot {
    /// CHR ROM/RAM pattern table data ($0000–$1FFF)
    pub chr: [u8; CHR_SIZE],
    /// Four nametable regions ($2000–$2FFF)
    pub nametables: [[u8; NAMETABLE_SIZE]; NUM_NAMETABLES],
    /// Background palette RAM
    pub palette: [u8; PALETTE_SIZE],
    /// Background pattern table base address (0x0000 or 0x1000)
    pub bg_pattern_table: u16,
    /// Current scroll position as (scroll_x, scroll_y) into the 512×480 nametable space
    pub scroll: (u16, u16),
    /// Active preset system palette used to resolve NES colors to RGB.
    pub system_palette: NesPalette,
}

impl PpuViewerSnapshot {
    pub fn from_nes(nes: &Nes) -> Self {
        let binding = nes.ppu();
        let ppu = binding.borrow();
        Self {
            chr: ppu.chr_snapshot_for_debugger(),
            nametables: ppu.nametable_snapshot_for_debugger(),
            palette: ppu.palette_for_debugger(),
            bg_pattern_table: ppu.bg_pattern_table_for_debugger(),
            scroll: ppu.scroll_for_debugger(),
            system_palette: ppu.system_palette(),
        }
    }
}

/// Render both CHR pattern tables to an RGBA pixel buffer.
///
/// Output size: 256 × 128 × 4 bytes (RGBA).
/// Left 128 × 128: pattern table 0 ($0000), right 128 × 128: pattern table 1 ($1000).
/// Each table shows 16 × 16 tiles at 8 × 8 pixels each.
/// Colors are resolved via BG palette 0 and the NES system palette.
pub fn render_pattern_tables_rgba(
    chr: &[u8; CHR_SIZE],
    palette: &[u8; PALETTE_SIZE],
    system_palette: NesPalette,
) -> Vec<u8> {
    let mut pixels = vec![0u8; PATTERN_TABLE_OUTPUT_WIDTH * PATTERN_TABLE_OUTPUT_HEIGHT * 4];
    let ctx = TileContext {
        chr,
        palette,
        system_palette,
    };

    for table in 0u16..2 {
        let table_x_offset = table as usize * PATTERN_TABLE_WIDTH;
        for ty in 0..PATTERN_TABLE_TILES_PER_ROW {
            for tx in 0..PATTERN_TABLE_TILES_PER_ROW {
                let tile_index = ty * PATTERN_TABLE_TILES_PER_ROW + tx;
                let tile_addr = table * 0x1000 + (tile_index as u16) * CHR_BYTES_PER_TILE as u16;
                render_tile_into(
                    &ctx,
                    tile_addr,
                    0,
                    &mut pixels,
                    (table_x_offset + tx * TILE_SIZE, ty * TILE_SIZE),
                    PATTERN_TABLE_OUTPUT_WIDTH,
                );
            }
        }
    }

    pixels
}

/// Render all four nametable regions to an RGBA pixel buffer.
///
/// Output size: 512 × 480 × 4 bytes (RGBA).
/// Layout: NT0 top-left, NT1 top-right, NT2 bottom-left, NT3 bottom-right.
/// Each nametable region is 256 × 240 pixels (32 × 30 tiles at 8 × 8 pixels).
pub fn render_nametables_rgba(
    chr: &[u8; CHR_SIZE],
    nametables: &[[u8; NAMETABLE_SIZE]; NUM_NAMETABLES],
    palette: &[u8; PALETTE_SIZE],
    bg_pattern_table: u16,
    system_palette: NesPalette,
) -> Vec<u8> {
    let mut pixels = vec![0u8; NAMETABLE_OUTPUT_WIDTH * NAMETABLE_OUTPUT_HEIGHT * 4];
    let ctx = TileContext {
        chr,
        palette,
        system_palette,
    };

    for (nt, nt_data) in nametables.iter().enumerate().take(NUM_NAMETABLES) {
        let nt_x = (nt % 2) * NAMETABLE_WIDTH;
        let nt_y = (nt / 2) * NAMETABLE_HEIGHT;

        for ty in 0..NAMETABLE_TILE_ROWS {
            for tx in 0..NAMETABLE_TILE_COLS {
                let tile_index = nt_data[ty * NAMETABLE_TILE_COLS + tx] as u16;
                let tile_addr = bg_pattern_table + tile_index * CHR_BYTES_PER_TILE as u16;

                let attr_byte =
                    nt_data[ATTRIBUTE_TABLE_OFFSET + (ty / 4) * ATTRIBUTE_TABLE_COLS + (tx / 4)];
                let pal_shift = ((ty / 2) % 2) * 4 + ((tx / 2) % 2) * 2;
                let palette_num = ((attr_byte >> pal_shift) & 0x03) as usize;

                render_tile_into(
                    &ctx,
                    tile_addr,
                    palette_num,
                    &mut pixels,
                    (nt_x + tx * TILE_SIZE, nt_y + ty * TILE_SIZE),
                    NAMETABLE_OUTPUT_WIDTH,
                );
            }
        }
    }

    pixels
}

/// Shared inputs for rendering individual tiles: CHR data, palette RAM, and the
/// active system palette used to resolve NES colors to RGB.
struct TileContext<'a> {
    chr: &'a [u8; CHR_SIZE],
    palette: &'a [u8; PALETTE_SIZE],
    system_palette: NesPalette,
}

/// Render a single 8×8 tile into `pixels` at position (`px`, `py`) with the given `stride`.
fn render_tile_into(
    ctx: &TileContext,
    tile_addr: u16,
    palette_num: usize,
    pixels: &mut [u8],
    position: (usize, usize),
    stride: usize,
) {
    let (px, py) = position;
    let table = ctx.system_palette.table();
    for row in 0..TILE_SIZE {
        let base = tile_addr as usize + row;
        let lo = ctx.chr[base];
        let hi = ctx.chr[base + TILE_SIZE];
        for col in 0..TILE_SIZE {
            let lo_bit = (lo >> (7 - col)) & 1;
            let hi_bit = (hi >> (7 - col)) & 1;
            let color_idx = (hi_bit << 1) | lo_bit;

            let palette_ram_idx = if color_idx == 0 {
                0
            } else {
                palette_num * 4 + color_idx as usize
            };
            let nes_color = ctx.palette[palette_ram_idx] & 0x3F;
            let (r, g, b) = table[nes_color as usize];

            let offset = ((py + row) * stride + (px + col)) * 4;
            pixels[offset] = r;
            pixels[offset + 1] = g;
            pixels[offset + 2] = b;
            pixels[offset + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::{Cartridge, NametableLayout};
    use crate::nes::console::{Config, Nes};

    #[test]
    fn test_render_pattern_tables_rgba_has_correct_dimensions() {
        let chr = [0u8; 8192];
        let palette = [0u8; 32];
        let pixels = render_pattern_tables_rgba(&chr, &palette, NesPalette::Default);
        assert_eq!(pixels.len(), 256 * 128 * 4);
    }

    #[test]
    fn test_render_nametables_rgba_has_correct_dimensions() {
        let chr = [0u8; 8192];
        let nametables = [[0u8; 1024]; 4];
        let palette = [0u8; 32];
        let pixels =
            render_nametables_rgba(&chr, &nametables, &palette, 0x0000, NesPalette::Default);
        assert_eq!(pixels.len(), 512 * 480 * 4);
    }

    #[test]
    fn test_render_pattern_tables_rgba_pixel_color_from_chr_data() {
        // Tile 0 in table 0: lo=0xFF, hi=0x00 → all pixels color index 1 in BG palette 0
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[row] = 0xFF; // lo bits all set
            chr[row + 8] = 0x00; // hi bits clear → color 1
        }
        let mut palette = [0u8; 32];
        palette[0] = 0x0F; // universal BG
        palette[1] = 0x20; // BG palette 0, color 1 → NES color 0x20

        let pixels = render_pattern_tables_rgba(&chr, &palette, NesPalette::Default);

        let (r, g, b) = Nes::lookup_system_palette(0x20);
        // Pixel at (0,0): top-left corner of tile 0, table 0
        assert_eq!(pixels[0], r, "R mismatch at pixel (0,0)");
        assert_eq!(pixels[1], g, "G mismatch at pixel (0,0)");
        assert_eq!(pixels[2], b, "B mismatch at pixel (0,0)");
        assert_eq!(pixels[3], 255, "Alpha should be 255");
    }

    #[test]
    fn test_render_pattern_table_1_starts_at_pixel_x_128() {
        // Tile 0 in table 1: lo=0xFF → all pixels color 1
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[0x1000 + row] = 0xFF;
        }
        let mut palette = [0u8; 32];
        palette[1] = 0x11; // BG palette 0, color 1

        let pixels = render_pattern_tables_rgba(&chr, &palette, NesPalette::Default);

        // Table 1 starts at x=128. Pixel at (128, 0):
        let offset = 128 * 4;
        let (r, g, b) = Nes::lookup_system_palette(0x11);
        assert_eq!(pixels[offset], r);
        assert_eq!(pixels[offset + 1], g);
        assert_eq!(pixels[offset + 2], b);
    }

    #[test]
    fn test_render_nametables_rgba_renders_tile_from_index() {
        // NT0 position (0,0) = tile index 1. Tile 1 has all pixels color 3.
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[16 + row] = 0xFF; // lo bits
            chr[16 + row + 8] = 0xFF; // hi bits → color 3
        }
        let mut nametables = [[0u8; 1024]; 4];
        nametables[0][0] = 1; // tile index 1 at (0,0) of NT0

        let mut palette = [0u8; 32];
        palette[0] = 0x0F;
        palette[3] = 0x20; // BG palette 0, color 3

        let pixels =
            render_nametables_rgba(&chr, &nametables, &palette, 0x0000, NesPalette::Default);

        let (r, g, b) = Nes::lookup_system_palette(0x20);
        // NT0 is at top-left (0,0), so pixel (0,0) of output:
        assert_eq!(pixels[0], r);
        assert_eq!(pixels[1], g);
        assert_eq!(pixels[2], b);
        assert_eq!(pixels[3], 255);
    }

    #[test]
    fn test_render_nametables_rgba_nt1_starts_at_x256() {
        // NT1 starts at x=256 in the output (top-right). Fill NT1 tile (0,0) with tile 2 (all color 1).
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[2 * 16 + row] = 0xFF; // lo=1, hi=0 → color 1
        }
        let mut nametables = [[0u8; 1024]; 4];
        nametables[1][0] = 2;

        let mut palette = [0u8; 32];
        palette[1] = 0x15;

        let pixels =
            render_nametables_rgba(&chr, &nametables, &palette, 0x0000, NesPalette::Default);

        // NT1 top-left is at pixel (256, 0):
        let offset = 256 * 4;
        let (r, g, b) = Nes::lookup_system_palette(0x15);
        assert_eq!(pixels[offset], r);
        assert_eq!(pixels[offset + 1], g);
        assert_eq!(pixels[offset + 2], b);
    }

    #[test]
    fn test_render_nametables_rgba_nt2_starts_at_y240() {
        // NT2 starts at y=240 (bottom-left). Fill NT2 tile (0,0) with tile 3 (all color 2).
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[3 * 16 + row] = 0x00; // lo=0
            chr[3 * 16 + row + 8] = 0xFF; // hi=1 → color 2
        }
        let mut nametables = [[0u8; 1024]; 4];
        nametables[2][0] = 3;

        let mut palette = [0u8; 32];
        palette[2] = 0x1A;

        let pixels =
            render_nametables_rgba(&chr, &nametables, &palette, 0x0000, NesPalette::Default);

        // NT2 top-left is at pixel (0, 240):
        let offset = (240 * 512) * 4;
        let (r, g, b) = Nes::lookup_system_palette(0x1A);
        assert_eq!(pixels[offset], r);
        assert_eq!(pixels[offset + 1], g);
        assert_eq!(pixels[offset + 2], b);
    }

    #[test]
    fn test_render_nametables_rgba_uses_bg_pattern_table_offset() {
        // With bg_pattern_table=0x1000, tile index 0 refers to tile at $1000 in CHR
        let mut chr = [0u8; 8192];
        for row in 0..8 {
            chr[0x1000 + row] = 0xFF; // tile 0 in table 1: lo=1, hi=0 → color 1
        }
        let nametables = [[0u8; 1024]; 4]; // all tile index 0

        let mut palette = [0u8; 32];
        palette[1] = 0x16;

        let pixels =
            render_nametables_rgba(&chr, &nametables, &palette, 0x1000, NesPalette::Default);

        let (r, g, b) = Nes::lookup_system_palette(0x16);
        assert_eq!(pixels[0], r);
        assert_eq!(pixels[1], g);
        assert_eq!(pixels[2], b);
    }

    #[test]
    fn test_ppu_viewer_snapshot_from_nes_has_correct_field_sizes() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let cart = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
        nes.insert_cartridge(cart);

        let snap = PpuViewerSnapshot::from_nes(&nes);

        assert_eq!(snap.chr.len(), 8192);
        assert_eq!(snap.nametables.len(), 4);
        assert_eq!(snap.nametables[0].len(), 1024);
        assert_eq!(snap.palette.len(), 32);
        assert!(snap.bg_pattern_table == 0x0000 || snap.bg_pattern_table == 0x1000);
        let (sx, sy) = snap.scroll;
        assert!(sx < 512, "scroll_x {sx} out of range");
        assert!(sy < 480, "scroll_y {sy} out of range");
    }

    #[test]
    fn test_scroll_for_debugger_default_is_origin() {
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let (sx, sy) = nes.ppu().borrow().scroll_for_debugger();
        // After reset, t=0 and fine_x=0, so scroll is at (0, 0).
        assert_eq!(sx, 0);
        assert_eq!(sy, 0);
    }

    #[test]
    fn test_scroll_for_debugger_reflects_x_scroll_write() {
        let nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        // Write $80 (128) to PPUSCROLL twice: X=128, Y=0.
        // This sets coarse_x=16, fine_x=0 → scroll_x = 128.
        nes.ppu().borrow_mut().write_scroll(0x80, false);
        nes.ppu().borrow_mut().write_scroll(0x00, false);
        let (sx, sy) = nes.ppu().borrow().scroll_for_debugger();
        assert_eq!(sx, 128, "scroll_x should be 128");
        assert_eq!(sy, 0, "scroll_y should be 0");
    }

    #[test]
    fn test_render_tile_follows_selected_system_palette() {
        // Same CHR/palette, but render under two different system palettes;
        // the resulting RGB should match each palette's table for the NES color.
        let mut chr = [0u8; 8192];
        for byte in chr.iter_mut().take(8) {
            *byte = 0xFF; // color index 1
        }
        let mut palette = [0u8; 32];
        palette[1] = 0x05; // BG palette 0, color 1 → NES color 0x05

        let default_pixels = render_pattern_tables_rgba(&chr, &palette, NesPalette::Default);
        let smooth_pixels = render_pattern_tables_rgba(&chr, &palette, NesPalette::Smooth);

        let (dr, dg, db) = NesPalette::Default.table()[0x05];
        let (sr, sg, sb) = NesPalette::Smooth.table()[0x05];
        assert_eq!(
            (default_pixels[0], default_pixels[1], default_pixels[2]),
            (dr, dg, db)
        );
        assert_eq!(
            (smooth_pixels[0], smooth_pixels[1], smooth_pixels[2]),
            (sr, sg, sb)
        );
        assert_ne!(
            (default_pixels[0], default_pixels[1], default_pixels[2]),
            (smooth_pixels[0], smooth_pixels[1], smooth_pixels[2]),
            "Smooth palette should differ from Default for NES color 0x05"
        );
    }

    #[test]
    fn test_ppu_viewer_snapshot_captures_active_palette() {
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let cart = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
        nes.insert_cartridge(cart);
        nes.ppu()
            .borrow_mut()
            .set_system_palette(NesPalette::Classic);

        let snap = PpuViewerSnapshot::from_nes(&nes);
        assert_eq!(snap.system_palette, NesPalette::Classic);
    }
}
