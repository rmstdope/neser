use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum DmgTileByteOverride {
    #[default]
    None,
    TileIndex,
    TileIndexAndCache {
        lcdc: u8,
    },
    CachedData,
    Data(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DmgLayer {
    Background,
    Window,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DmgPixelFetch {
    pub layer: DmgLayer,
    pub x: u32,
    pub scanline: u8,
    pub scx: u8,
    pub scy: u8,
    pub wx: u8,
    pub wy: u8,
    pub window_line: u8,
    pub map_lcdc: u8,
    pub low_lcdc: u8,
    pub high_lcdc: u8,
    pub low_override: DmgTileByteOverride,
    pub high_override: DmgTileByteOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DmgFetchedPixel {
    pub colour_index: u8,
    pub low_cache_data: Option<u8>,
    pub high_cache_data: Option<u8>,
}

#[cfg(test)]
pub(super) fn fetch_dmg_pixel(vram: &[u8; 0x2000], fetch: DmgPixelFetch) -> Option<u8> {
    fetch_dmg_pixel_with_data(vram, fetch).map(|pixel| pixel.colour_index)
}

pub(super) fn fetch_dmg_pixel_with_data(
    vram: &[u8; 0x2000],
    fetch: DmgPixelFetch,
) -> Option<DmgFetchedPixel> {
    let (map_base, tile_x, tile_y, pixel_x, pixel_y) = match fetch.layer {
        DmgLayer::Background => {
            let bg_x = fetch.scx.wrapping_add(fetch.x as u8);
            let bg_y = fetch.scy.wrapping_add(fetch.scanline);
            (
                tile_map_base(fetch.map_lcdc, 0x08),
                (bg_x / 8) as usize,
                (bg_y / 8) as usize,
                bg_x % 8,
                bg_y % 8,
            )
        }
        DmgLayer::Window => {
            if fetch.map_lcdc & 0x20 == 0 || fetch.scanline < fetch.wy {
                return None;
            }
            let win_x_start = i16::from(fetch.wx) - 7;
            let visible_win_x_start = win_x_start.max(0) as u32;
            if fetch.x < visible_win_x_start {
                return None;
            }
            let win_x = ((fetch.x as i16) - win_x_start) as u8;
            let win_y = fetch.window_line;
            (
                tile_map_base(fetch.map_lcdc, 0x40),
                (win_x / 8) as usize,
                (win_y / 8) as usize,
                win_x % 8,
                win_y % 8,
            )
        }
    };

    let tile_index = vram[map_base + tile_y * 32 + tile_x];
    let low_addr = tile_data_addr(tile_index, pixel_y, fetch.low_lcdc);
    let high_addr = tile_data_addr(tile_index, pixel_y, fetch.high_lcdc) + 1;
    let low_cache_data = fetch
        .low_override
        .cache_lcdc()
        .map(|lcdc| vram[tile_data_addr(tile_index, pixel_y, lcdc)]);
    let high_cache_data = fetch
        .high_override
        .cache_lcdc()
        .map(|lcdc| vram[tile_data_addr(tile_index, pixel_y, lcdc) + 1]);
    let low_data = resolve_tile_byte_override(vram, tile_index, low_addr, fetch.low_override);
    let high_data = resolve_tile_byte_override(vram, tile_index, high_addr, fetch.high_override);
    let bit = 7 - pixel_x;
    Some(DmgFetchedPixel {
        colour_index: ((high_data >> bit) & 1) << 1 | ((low_data >> bit) & 1),
        low_cache_data,
        high_cache_data,
    })
}

impl DmgTileByteOverride {
    fn cache_lcdc(self) -> Option<u8> {
        match self {
            DmgTileByteOverride::TileIndexAndCache { lcdc } => Some(lcdc),
            DmgTileByteOverride::None
            | DmgTileByteOverride::TileIndex
            | DmgTileByteOverride::CachedData
            | DmgTileByteOverride::Data(_) => None,
        }
    }
}

fn resolve_tile_byte_override(
    vram: &[u8; 0x2000],
    tile_index: u8,
    addr: usize,
    override_mode: DmgTileByteOverride,
) -> u8 {
    match override_mode {
        DmgTileByteOverride::None => vram[addr],
        DmgTileByteOverride::TileIndex | DmgTileByteOverride::TileIndexAndCache { .. } => {
            tile_index
        }
        DmgTileByteOverride::CachedData => {
            debug_assert!(
                false,
                "cached TILE_SEL data should be resolved before fetching"
            );
            vram[addr]
        }
        DmgTileByteOverride::Data(data) => data,
    }
}

fn tile_map_base(lcdc: u8, bit: u8) -> usize {
    if lcdc & bit != 0 { 0x1C00 } else { 0x1800 }
}

fn tile_data_addr(tile_index: u8, row: u8, lcdc: u8) -> usize {
    let tile_start = if lcdc & 0x10 != 0 {
        usize::from(tile_index) * 16
    } else {
        (0x1000i32 + i32::from(tile_index as i8) * 16) as usize
    };
    tile_start + usize::from(row) * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_vram() -> [u8; 0x2000] {
        [0; 0x2000]
    }

    fn vram_with_mixed_tile_select_sources() -> [u8; 0x2000] {
        let mut vram = blank_vram();
        vram[0x1800] = 0x01;
        vram[0x1010] = 0x80;
        vram[0x1011] = 0x00;
        vram[0x0010] = 0x00;
        vram[0x0011] = 0x80;
        vram
    }

    #[test]
    fn background_fetch_samples_tile_select_independently_for_low_and_high_bytes() {
        let vram = vram_with_mixed_tile_select_sources();

        let colour = fetch_dmg_pixel(
            &vram,
            DmgPixelFetch {
                layer: DmgLayer::Background,
                x: 0,
                scanline: 0,
                scx: 0,
                scy: 0,
                wx: 7,
                wy: 0,
                window_line: 0,
                map_lcdc: 0x81,
                low_lcdc: 0x81,
                high_lcdc: 0x91,
                low_override: DmgTileByteOverride::None,
                high_override: DmgTileByteOverride::None,
            },
        );

        assert_eq!(
            colour,
            Some(3),
            "low byte should come from signed tile data while high byte comes from unsigned tile data"
        );
    }

    #[test]
    fn window_fetch_samples_tile_select_independently_for_low_and_high_bytes() {
        let vram = vram_with_mixed_tile_select_sources();

        let colour = fetch_dmg_pixel(
            &vram,
            DmgPixelFetch {
                layer: DmgLayer::Window,
                x: 0,
                scanline: 0,
                scx: 0,
                scy: 0,
                wx: 7,
                wy: 0,
                window_line: 0,
                map_lcdc: 0xA1,
                low_lcdc: 0xA1,
                high_lcdc: 0xB1,
                low_override: DmgTileByteOverride::None,
                high_override: DmgTileByteOverride::None,
            },
        );

        assert_eq!(
            colour,
            Some(3),
            "window fetches should use the same independent low/high TILE_SEL sampling as BG fetches"
        );
    }
}
