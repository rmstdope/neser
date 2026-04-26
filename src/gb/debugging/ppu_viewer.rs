//! Game Boy PPU viewer for debugging.
//!
//! Provides snapshots and rendering functions for visualizing VRAM tiles,
//! background maps, OAM, and palettes.

use crate::gb::bus::GbBus;

// ── Constants ──────────────────────────────────────────────────────────────

/// VRAM size per bank (8KB)
const VRAM_SIZE: usize = 0x2000;
/// OAM size (160 bytes for 40 sprites)
const OAM_SIZE: usize = 0xA0;
/// CGB palette RAM size per type (64 bytes for 8 palettes × 4 colors × 2 bytes)
const PALETTE_RAM_SIZE: usize = 64;

/// Bytes per tile (8×8 pixels, 2 bits per pixel, 2 bytes per row)
const BYTES_PER_TILE: usize = 16;
/// Tile size in pixels
const TILE_SIZE: usize = 8;

/// Tiles per VRAM bank
const TILES_PER_BANK: usize = 512;
/// Tiles per row in tile viewer (32 tiles for DMG, 64 for CGB with both banks)
const TILES_PER_ROW_DMG: usize = 32;
const TILES_PER_ROW_CGB: usize = 64;
/// Tile viewer dimensions
const TILE_VIEWER_ROWS: usize = 16;
const TILE_VIEWER_WIDTH_DMG: usize = TILES_PER_ROW_DMG * TILE_SIZE; // 256
const TILE_VIEWER_HEIGHT: usize = TILE_VIEWER_ROWS * TILE_SIZE; // 128
const TILE_VIEWER_WIDTH_CGB: usize = TILES_PER_ROW_CGB * TILE_SIZE; // 512

/// BG map dimensions (32×32 tiles = 256×256 pixels)
const BG_MAP_TILE_COLS: usize = 32;
const BG_MAP_TILE_ROWS: usize = 32;
const BG_MAP_WIDTH: usize = BG_MAP_TILE_COLS * TILE_SIZE; // 256
const BG_MAP_HEIGHT: usize = BG_MAP_TILE_ROWS * TILE_SIZE; // 256
/// BG map viewer output (two maps side-by-side)
const BG_MAP_VIEWER_WIDTH: usize = BG_MAP_WIDTH * 2; // 512
const BG_MAP_VIEWER_HEIGHT: usize = BG_MAP_HEIGHT; // 256

/// BG map addresses in VRAM
const BG_MAP_0_OFFSET: usize = 0x1800;
const BG_MAP_1_OFFSET: usize = 0x1C00;

/// OAM entry size (4 bytes per sprite)
const OAM_ENTRY_SIZE: usize = 4;
/// Number of OAM entries
const OAM_ENTRIES: usize = 40;

/// DMG grayscale palette (4 shades)
const DMG_PALETTE: [(u8, u8, u8); 4] = [
    (0xFF, 0xFF, 0xFF), // White
    (0xAA, 0xAA, 0xAA), // Light gray
    (0x55, 0x55, 0x55), // Dark gray
    (0x00, 0x00, 0x00), // Black
];

// ── Snapshot ───────────────────────────────────────────────────────────────

/// Snapshot of GB PPU state for the debugger viewer.
#[derive(Debug, Clone)]
pub struct GbPpuViewerSnapshot {
    /// VRAM bank 0 ($8000–$9FFF)
    pub vram: [u8; VRAM_SIZE],
    /// VRAM bank 1 (CGB only, $8000–$9FFF)
    pub vram_bank1: [u8; VRAM_SIZE],
    /// Object Attribute Memory (160 bytes)
    pub oam: [u8; OAM_SIZE],
    /// CGB BG palette RAM (64 bytes)
    pub bg_palette_ram: [u8; PALETTE_RAM_SIZE],
    /// CGB OBJ palette RAM (64 bytes)
    pub obj_palette_ram: [u8; PALETTE_RAM_SIZE],
    /// LCDC register ($FF40)
    pub lcdc: u8,
    /// Scroll X register ($FF43)
    pub scx: u8,
    /// Scroll Y register ($FF42)
    pub scy: u8,
    /// DMG BG palette register ($FF47)
    pub bgp: u8,
    /// DMG OBJ palette 0 register ($FF48)
    pub obp0: u8,
    /// DMG OBJ palette 1 register ($FF49)
    pub obp1: u8,
    /// True if running in CGB mode
    pub cgb_mode: bool,
}

impl GbPpuViewerSnapshot {
    /// Capture a snapshot from the current GB console state.
    pub fn from_gb<B: GbBus>(gb: &crate::gb::console::Gb<B>) -> Self {
        let ppu = gb.cpu.bus.ppu();
        let (lcdc, scx, scy, bgp, obp0, obp1) = ppu.registers_snapshot_for_debugger();

        Self {
            vram: ppu.vram_snapshot_for_debugger(),
            vram_bank1: ppu.vram_bank1_snapshot_for_debugger(),
            oam: ppu.oam_snapshot_for_debugger(),
            bg_palette_ram: ppu.bg_palette_ram_snapshot_for_debugger(),
            obj_palette_ram: ppu.obj_palette_ram_snapshot_for_debugger(),
            lcdc,
            scx,
            scy,
            bgp,
            obp0,
            obp1,
            cgb_mode: ppu.cgb_mode,
        }
    }
}

// ── Rendering Functions ────────────────────────────────────────────────────

/// Render all tiles from VRAM to an RGBA pixel buffer.
///
/// DMG mode: 512 tiles from bank 0, output size 256×128×4 (32×16 grid).
/// CGB mode: 1024 tiles from both banks, output size 512×128×4 (64×16 grid).
pub fn render_tiles_rgba(
    vram: &[u8; VRAM_SIZE],
    vram_bank1: &[u8; VRAM_SIZE],
    bgp: u8,
    _bg_palette_ram: &[u8; PALETTE_RAM_SIZE],
    cgb_mode: bool,
) -> Vec<u8> {
    let width = if cgb_mode {
        TILE_VIEWER_WIDTH_CGB
    } else {
        TILE_VIEWER_WIDTH_DMG
    };
    let tiles_per_row = if cgb_mode {
        TILES_PER_ROW_CGB
    } else {
        TILES_PER_ROW_DMG
    };

    let mut pixels = vec![0u8; width * TILE_VIEWER_HEIGHT * 4];

    // DMG grayscale palette from BGP register
    let dmg_palette = if cgb_mode {
        // CGB uses palette RAM, but we'll use a default grayscale for now
        DMG_PALETTE
    } else {
        [
            DMG_PALETTE[(bgp & 0x03) as usize],
            DMG_PALETTE[((bgp >> 2) & 0x03) as usize],
            DMG_PALETTE[((bgp >> 4) & 0x03) as usize],
            DMG_PALETTE[((bgp >> 6) & 0x03) as usize],
        ]
    };

    // Render bank 0 tiles
    for tile_idx in 0..TILES_PER_BANK {
        let tx = tile_idx % tiles_per_row;
        let ty = tile_idx / tiles_per_row;
        let tile_addr = tile_idx * BYTES_PER_TILE;

        render_tile_into(
            &vram[tile_addr..tile_addr + BYTES_PER_TILE],
            &dmg_palette,
            &mut pixels,
            (tx * TILE_SIZE, ty * TILE_SIZE),
            width,
        );
    }

    // Render bank 1 tiles (CGB only)
    if cgb_mode {
        for tile_idx in 0..TILES_PER_BANK {
            let tx = (tile_idx % TILES_PER_ROW_DMG) + TILES_PER_ROW_DMG; // Offset to right half
            let ty = tile_idx / TILES_PER_ROW_DMG;
            let tile_addr = tile_idx * BYTES_PER_TILE;

            render_tile_into(
                &vram_bank1[tile_addr..tile_addr + BYTES_PER_TILE],
                &dmg_palette,
                &mut pixels,
                (tx * TILE_SIZE, ty * TILE_SIZE),
                width,
            );
        }
    }

    pixels
}

/// Render both background maps to an RGBA pixel buffer.
///
/// Output size: 512×256×4 (two 256×256 maps side-by-side).
/// Left map: $9800, right map: $9C00.
pub fn render_bg_maps_rgba(
    vram: &[u8; VRAM_SIZE],
    _vram_bank1: &[u8; VRAM_SIZE],
    lcdc: u8,
    bgp: u8,
    _bg_palette_ram: &[u8; PALETTE_RAM_SIZE],
    cgb_mode: bool,
) -> Vec<u8> {
    let mut pixels = vec![0u8; BG_MAP_VIEWER_WIDTH * BG_MAP_VIEWER_HEIGHT * 4];

    // DMG grayscale palette from BGP register
    let dmg_palette = if cgb_mode {
        DMG_PALETTE
    } else {
        [
            DMG_PALETTE[(bgp & 0x03) as usize],
            DMG_PALETTE[((bgp >> 2) & 0x03) as usize],
            DMG_PALETTE[((bgp >> 4) & 0x03) as usize],
            DMG_PALETTE[((bgp >> 6) & 0x03) as usize],
        ]
    };

    // LCDC bit 4: BG tile data select (0=$8800-97FF signed, 1=$8000-8FFF unsigned)
    let tile_data_unsigned = (lcdc & 0x10) != 0;

    // Render both maps
    for map_idx in 0..2 {
        let map_offset = if map_idx == 0 {
            BG_MAP_0_OFFSET
        } else {
            BG_MAP_1_OFFSET
        };
        let map_x_offset = map_idx * BG_MAP_WIDTH;

        for ty in 0..BG_MAP_TILE_ROWS {
            for tx in 0..BG_MAP_TILE_COLS {
                let tile_map_idx = vram[map_offset + ty * BG_MAP_TILE_COLS + tx];

                // Calculate tile address based on addressing mode
                let tile_addr = if tile_data_unsigned {
                    // Unsigned mode: $8000 + tile_idx * 16
                    (tile_map_idx as usize) * BYTES_PER_TILE
                } else {
                    // Signed mode: $9000 + (tile_idx as i8) * 16
                    let offset = (tile_map_idx as i8 as isize) * BYTES_PER_TILE as isize;
                    (0x1000isize + offset) as usize
                };

                render_tile_into(
                    &vram[tile_addr..tile_addr + BYTES_PER_TILE],
                    &dmg_palette,
                    &mut pixels,
                    (map_x_offset + tx * TILE_SIZE, ty * TILE_SIZE),
                    BG_MAP_VIEWER_WIDTH,
                );
            }
        }
    }

    pixels
}

/// Format OAM entries as human-readable text.
///
/// Returns a vector of strings, one per sprite (40 total).
/// Format: "# Y X tile attr [pal]" where pal is only shown in CGB mode.
pub fn format_oam_entries(oam: &[u8; OAM_SIZE], cgb_mode: bool) -> Vec<String> {
    let mut entries = Vec::with_capacity(OAM_ENTRIES);

    for i in 0..OAM_ENTRIES {
        let offset = i * OAM_ENTRY_SIZE;
        let y = oam[offset];
        let x = oam[offset + 1];
        let tile = oam[offset + 2];
        let attr = oam[offset + 3];

        let entry = if cgb_mode {
            let pal = attr & 0x07; // CGB palette number is bits 0-2
            format!(
                "{} Y={} X={} tile={:02X} attr={:02X} pal={}",
                i, y, x, tile, attr, pal
            )
        } else {
            format!("{} Y={} X={} tile={:02X} attr={:02X}", i, y, x, tile, attr)
        };

        entries.push(entry);
    }

    entries
}

/// Format palette information as human-readable text.
///
/// DMG mode: Returns BGP, OBP0, OBP1 register values.
/// CGB mode: Returns RGB values for all 8 BG and 8 OBJ palettes.
pub fn format_palette_info(
    bgp: u8,
    obp0: u8,
    obp1: u8,
    bg_palette_ram: &[u8; PALETTE_RAM_SIZE],
    obj_palette_ram: &[u8; PALETTE_RAM_SIZE],
    cgb_mode: bool,
) -> Vec<String> {
    let mut info = Vec::new();

    if cgb_mode {
        // CGB mode: show RGB values for all palettes
        info.push("BG Palettes:".to_string());
        for pal_idx in 0..8 {
            let mut colors = Vec::new();
            for color_idx in 0..4 {
                let offset = (pal_idx * 4 + color_idx) * 2;
                let lo = bg_palette_ram[offset];
                let hi = bg_palette_ram[offset + 1];
                let rgb555 = (hi as u16) << 8 | lo as u16;

                // Convert 5-bit to 8-bit (0-31 → 0-255): val * 8 + val / 4
                let r = {
                    let val = (rgb555 & 0x001F) as u8;
                    val * 8 + val / 4
                };
                let g = {
                    let val = ((rgb555 & 0x03E0) >> 5) as u8;
                    val * 8 + val / 4
                };
                let b = {
                    let val = ((rgb555 & 0x7C00) >> 10) as u8;
                    val * 8 + val / 4
                };

                colors.push(format!("({},{},{})", r, g, b));
            }
            info.push(format!("  Pal{}: {}", pal_idx, colors.join(" ")));
        }

        info.push("OBJ Palettes:".to_string());
        for pal_idx in 0..8 {
            let mut colors = Vec::new();
            for color_idx in 0..4 {
                let offset = (pal_idx * 4 + color_idx) * 2;
                let lo = obj_palette_ram[offset];
                let hi = obj_palette_ram[offset + 1];
                let rgb555 = (hi as u16) << 8 | lo as u16;

                // Convert 5-bit to 8-bit (0-31 → 0-255): val * 8 + val / 4
                let r = {
                    let val = (rgb555 & 0x001F) as u8;
                    val * 8 + val / 4
                };
                let g = {
                    let val = ((rgb555 & 0x03E0) >> 5) as u8;
                    val * 8 + val / 4
                };
                let b = {
                    let val = ((rgb555 & 0x7C00) >> 10) as u8;
                    val * 8 + val / 4
                };

                colors.push(format!("({},{},{})", r, g, b));
            }
            info.push(format!("  Pal{}: {}", pal_idx, colors.join(" ")));
        }
    } else {
        // DMG mode: show register values
        info.push(format!("BGP:  ${:02X}", bgp));
        info.push(format!("OBP0: ${:02X}", obp0));
        info.push(format!("OBP1: ${:02X}", obp1));
    }

    info
}

/// Render a single 8×8 tile into the pixel buffer.
///
/// Helper function used by tile and BG map renderers.
fn render_tile_into(
    tile_data: &[u8],
    palette: &[(u8, u8, u8)],
    pixels: &mut [u8],
    position: (usize, usize),
    stride: usize,
) {
    let (px, py) = position;

    for row in 0..TILE_SIZE {
        let lo = tile_data[row * 2];
        let hi = tile_data[row * 2 + 1];

        for col in 0..TILE_SIZE {
            let lo_bit = (lo >> (7 - col)) & 1;
            let hi_bit = (hi >> (7 - col)) & 1;
            let color_idx = (hi_bit << 1) | lo_bit;

            let (r, g, b) = palette[color_idx as usize];

            let offset = ((py + row) * stride + (px + col)) * 4;
            pixels[offset] = r;
            pixels[offset + 1] = g;
            pixels[offset + 2] = b;
            pixels[offset + 3] = 255;
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tile Rendering Tests ───────────────────────────────────────────────

    #[test]
    fn test_render_tiles_rgba_dmg_has_correct_dimensions() {
        let vram = [0u8; VRAM_SIZE];
        let vram_bank1 = [0u8; VRAM_SIZE];
        let bgp = 0xE4; // Standard grayscale palette
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let pixels = render_tiles_rgba(&vram, &vram_bank1, bgp, &bg_palette_ram, false);

        // DMG: 256×128×4 = 131,072 bytes
        assert_eq!(pixels.len(), TILE_VIEWER_WIDTH_DMG * TILE_VIEWER_HEIGHT * 4);
    }

    #[test]
    fn test_render_tiles_rgba_cgb_has_correct_dimensions() {
        let vram = [0u8; VRAM_SIZE];
        let vram_bank1 = [0u8; VRAM_SIZE];
        let bgp = 0xE4;
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let pixels = render_tiles_rgba(&vram, &vram_bank1, bgp, &bg_palette_ram, true);

        // CGB: 512×128×4 = 262,144 bytes
        assert_eq!(pixels.len(), TILE_VIEWER_WIDTH_CGB * TILE_VIEWER_HEIGHT * 4);
    }

    #[test]
    fn test_render_tiles_rgba_dmg_grayscale_colors() {
        // Create a tile with all four grayscale shades
        let mut vram = [0u8; VRAM_SIZE];
        // Tile 0: first row has pixels with colors 3,1,0,0...
        // GB format: lo byte, hi byte for each row
        vram[0] = 0b11000000; // lo bits: pixel 0=1, pixel 1=1, rest=0
        vram[1] = 0b10000000; // hi bits: pixel 0=1, pixel 1=0, rest=0
        // → pixel 0: hi=1,lo=1 → color 3
        // → pixel 1: hi=0,lo=1 → color 1

        let vram_bank1 = [0u8; VRAM_SIZE];
        let bgp = 0xE4; // 11 10 01 00 = color 3→black, 2→dark_gray, 1→light_gray, 0→white
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let pixels = render_tiles_rgba(&vram, &vram_bank1, bgp, &bg_palette_ram, false);

        // Check first pixel (0,0) should be color 3 (black)
        assert_eq!(pixels[0], 0x00, "Pixel (0,0) R should be black");
        assert_eq!(pixels[1], 0x00, "Pixel (0,0) G should be black");
        assert_eq!(pixels[2], 0x00, "Pixel (0,0) B should be black");
        assert_eq!(pixels[3], 255, "Alpha should be 255");
    }

    // ── BG Map Rendering Tests ─────────────────────────────────────────────

    #[test]
    fn test_render_bg_maps_rgba_has_correct_dimensions() {
        let vram = [0u8; VRAM_SIZE];
        let vram_bank1 = [0u8; VRAM_SIZE];
        let lcdc = 0x91;
        let bgp = 0xE4;
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let pixels = render_bg_maps_rgba(&vram, &vram_bank1, lcdc, bgp, &bg_palette_ram, false);

        // 512×256×4 = 524,288 bytes
        assert_eq!(pixels.len(), BG_MAP_VIEWER_WIDTH * BG_MAP_VIEWER_HEIGHT * 4);
    }

    #[test]
    fn test_render_bg_maps_renders_tile_from_map() {
        // Set up tile 1 with all white pixels
        let mut vram = [0u8; VRAM_SIZE];
        // Tile 1 at offset 16: all pixels color 0 (white)
        for i in 0..16 {
            vram[16 + i] = 0x00;
        }

        // Set BG map 0 first entry to tile 1
        vram[BG_MAP_0_OFFSET] = 1;

        let vram_bank1 = [0u8; VRAM_SIZE];
        let lcdc = 0x91; // BG tile data at $8000, BG map at $9800
        let bgp = 0xE4;
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let pixels = render_bg_maps_rgba(&vram, &vram_bank1, lcdc, bgp, &bg_palette_ram, false);

        // First pixel of first map should be white (color 0)
        assert_eq!(pixels[0], 0xFF, "Should be white");
        assert_eq!(pixels[1], 0xFF, "Should be white");
        assert_eq!(pixels[2], 0xFF, "Should be white");
        assert_eq!(pixels[3], 255, "Alpha should be 255");
    }

    // ── OAM Formatting Tests ───────────────────────────────────────────────

    #[test]
    fn test_format_oam_entries_returns_40_entries() {
        let oam = [0u8; OAM_SIZE];
        let entries = format_oam_entries(&oam, false);
        assert_eq!(entries.len(), 40, "Should return 40 OAM entries");
    }

    #[test]
    fn test_format_oam_entries_dmg_format() {
        let mut oam = [0u8; OAM_SIZE];
        // Sprite 0: Y=16, X=8, tile=5, attr=0x00
        oam[0] = 16;
        oam[1] = 8;
        oam[2] = 5;
        oam[3] = 0x00;

        let entries = format_oam_entries(&oam, false);

        // Format should be: "0 Y=16 X=8 tile=05 attr=00"
        assert!(entries[0].contains("16"), "Should contain Y value");
        assert!(entries[0].contains("8"), "Should contain X value");
        assert!(
            entries[0].contains("5") || entries[0].contains("05"),
            "Should contain tile"
        );
    }

    // ── Palette Formatting Tests ───────────────────────────────────────────

    #[test]
    fn test_format_palette_info_dmg_includes_registers() {
        let bgp = 0xE4;
        let obp0 = 0xE0;
        let obp1 = 0xE1;
        let bg_palette_ram = [0u8; PALETTE_RAM_SIZE];
        let obj_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let info = format_palette_info(bgp, obp0, obp1, &bg_palette_ram, &obj_palette_ram, false);

        assert!(!info.is_empty(), "Should return palette info");
        let text = info.join("\n");
        assert!(
            text.contains("BGP") || text.contains("E4"),
            "Should contain BGP info"
        );
    }

    #[test]
    fn test_format_palette_info_cgb_includes_rgb() {
        // CGB palette: first color of palette 0 is white (0x7FFF = 31,31,31)
        let mut bg_palette_ram = [0u8; PALETTE_RAM_SIZE];
        bg_palette_ram[0] = 0xFF;
        bg_palette_ram[1] = 0x7F;

        let bgp = 0xE4;
        let obp0 = 0xE0;
        let obp1 = 0xE1;
        let obj_palette_ram = [0u8; PALETTE_RAM_SIZE];

        let info = format_palette_info(bgp, obp0, obp1, &bg_palette_ram, &obj_palette_ram, true);

        assert!(!info.is_empty(), "Should return palette info");
        let text = info.join("\n");
        // Should contain RGB values (255,255,255 for 31,31,31 with improved conversion)
        assert!(
            text.contains("255") || text.contains("RGB"),
            "Should contain RGB values"
        );
    }

    // ── Snapshot Tests ─────────────────────────────────────────────────────

    // Note: Snapshot tests will be added in GREEN phase when from_gb is implemented
}
