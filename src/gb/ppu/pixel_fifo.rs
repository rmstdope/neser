use serde::{Deserialize, Serialize};

use super::background;
use super::bg_fifo::{self, DmgLayer, DmgPixelFetch, DmgTileByteOverride};
use super::obj_fifo::ObjFetchModel;
use super::registers::Registers;
use super::rendering::{self, cgb_palette_lookup, dmg_palette_index};
use super::screen_buffer::ScreenBuffer;
use super::sprites;
use super::window;
use crate::gb::model::CgbModel;

const FETCHER_STARTUP_DOTS: u16 = 16;
const INITIAL_BGP: u8 = 0xFC;
const LCDC_BG_WINDOW_ENABLE: u8 = 0x01;
const LCDC_OBJ_ENABLE: u8 = 0x02;
const LCDC_BG_MAP: u8 = 0x08;
const LCDC_TILE_DATA: u8 = 0x10;
const LCDC_WINDOW_ENABLE: u8 = 0x20;
const LCDC_WINDOW_MAP: u8 = 0x40;
const OBJ_PIXELS_PER_FETCH: u8 = 8;
const CGB_DMG_COMPAT_OBJ_ENABLE_EDGE_PIXELS: u8 = 2;
const DMG_OBJ_FETCH_ABORT_COMPLETES_WHEN_DOTS_REMAINING: u16 = 2;
const DMG_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 4;
const CGB_DMG_COMPAT_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 6;
const DMG_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 1;
const CGB_DMG_COMPAT_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 3;
const OFF_LEFT_WINDOW_DELAYED_OBJ_X_START: u8 = OBJ_PIXELS_PER_FETCH - 3;
const OFF_LEFT_WINDOW_DELAYED_OBJ_X_END: u8 = OBJ_PIXELS_PER_FETCH - 1;
const OFF_LEFT_WINDOW_EXTRA_STALL_DOTS: u16 = 2;
// Same-dot WX overwrites before x=95 occur at the off-left fetcher's reactivation
// decision point; from x=95 onward the later write can still cancel the pending
// off-left reactivation observed by the Mealybug WX=6 ROM.
const OFF_LEFT_WX_OVERWRITE_CANCEL_X: u8 = 95;
const WINDOW_STARTUP_STALL_DOTS: u8 = 6;
const STEADY_WINDOW_MAP_FETCH_DELAY_X: u8 = OBJ_PIXELS_PER_FETCH * 2;
const LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_SET_X: u8 = OBJ_PIXELS_PER_FETCH * 2 + 5;
const LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_RESTORE_X: u8 = OBJ_PIXELS_PER_FETCH * 3 + 5;
const SCX_FINE_MASK: u8 = 0x07;
const SCX_TILE_MASK: u8 = !SCX_FINE_MASK;
const SCX_LOW_BITS_SAMPLE_DOTS: u16 = 6;
/// DMG SCY write callback delay: aligns the tick-before-register-write callback with
/// the PPU's observed fetch-stage sampling point.
const DMG_SCY_WRITE_CALLBACK_DELAY_DOTS: u16 = 2;
/// CGB SCY write callback delay: 2 hardware T-cycles of propagation delay plus 2 dots
/// from the emulator's tick-before-write model. Both CGB-C and CGB-D share this value;
/// the difference in behaviour comes from which stages sample `effective_scy`
/// (`b_stage_only` flag), not from write timing.
const CGB_SCY_WRITE_CALLBACK_DELAY_DOTS: u16 = 4;
const DMG_OBJ_FETCH_EXTRA_SCY_ADVANCE_DOTS: u16 = 1;

fn scy_fetch_start_dots() -> u16 {
    SCX_LOW_BITS_SAMPLE_DOTS + 3
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LcdcBgEnableEdgeTiming {
    CurrentPixelUsesNew,
    CurrentPixelUsesPrevious,
    HoldPreviousForOneExtraPixel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LcdcBgMapFetchDelay {
    NextBgFetch,
    OneBgFetchLater,
}

impl LcdcBgMapFetchDelay {
    fn pixels(self) -> u8 {
        match self {
            Self::NextBgFetch => 8,
            Self::OneBgFetchLater => 16,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcBgEnableEdge {
    active: bool,
    start_x: u8,
    end_x: u8,
    enabled: bool,
}

impl LcdcBgEnableEdge {
    fn record_write(
        &mut self,
        next_x: u8,
        previous_lcdc: u8,
        new_lcdc: u8,
        timing: LcdcBgEnableEdgeTiming,
    ) {
        if previous_lcdc & LCDC_BG_WINDOW_ENABLE == new_lcdc & LCDC_BG_WINDOW_ENABLE {
            return;
        }

        self.active = true;
        self.start_x = next_x;
        self.end_x = match timing {
            LcdcBgEnableEdgeTiming::HoldPreviousForOneExtraPixel => next_x.saturating_add(1),
            LcdcBgEnableEdgeTiming::CurrentPixelUsesNew
            | LcdcBgEnableEdgeTiming::CurrentPixelUsesPrevious => next_x,
        };
        self.enabled = match timing {
            LcdcBgEnableEdgeTiming::CurrentPixelUsesNew => new_lcdc & LCDC_BG_WINDOW_ENABLE != 0,
            LcdcBgEnableEdgeTiming::CurrentPixelUsesPrevious
            | LcdcBgEnableEdgeTiming::HoldPreviousForOneExtraPixel => {
                previous_lcdc & LCDC_BG_WINDOW_ENABLE != 0
            }
        };
    }

    fn bg_window_enabled_for_pixel(&self, x: u32, current_lcdc: u8) -> bool {
        if self.active && u32::from(self.start_x) <= x && x <= u32::from(self.end_x) {
            self.enabled
        } else {
            current_lcdc & LCDC_BG_WINDOW_ENABLE != 0
        }
    }

    fn clear_consumed(&mut self, next_x: u8) {
        if self.active && self.end_x == next_x {
            self.active = false;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct LcdcBgMapEdgeRange {
    start_x: u8,
    end_x: u8,
    bg_map_bit: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcBgMapEdge {
    ranges: Vec<LcdcBgMapEdgeRange>,
}

impl LcdcBgMapEdge {
    fn bg_tile_boundary_at_or_after(next_x: u8, scx: u8) -> u8 {
        let bg_phase = scx.wrapping_add(next_x) & 0x07;
        let pixels_until_boundary = if bg_phase == 0 { 0 } else { 8 - bg_phase };
        next_x.saturating_add(pixels_until_boundary)
    }

    fn record_write(&mut self, start_x: u8, end_x: u8, previous_lcdc: u8, new_lcdc: u8) {
        if previous_lcdc & LCDC_BG_MAP == new_lcdc & LCDC_BG_MAP || start_x > end_x {
            return;
        }

        self.ranges.push(LcdcBgMapEdgeRange {
            start_x,
            end_x,
            bg_map_bit: previous_lcdc & LCDC_BG_MAP,
        });
    }

    fn record_delayed_write(
        &mut self,
        next_x: u8,
        scx: u8,
        previous_lcdc: u8,
        new_lcdc: u8,
        fetch_delay: LcdcBgMapFetchDelay,
    ) {
        let next_fetch_tile_start =
            Self::bg_tile_boundary_at_or_after(next_x, scx).saturating_add(fetch_delay.pixels());
        let end_x = next_fetch_tile_start.saturating_sub(1);
        self.record_write(next_x, end_x, previous_lcdc, new_lcdc);
    }

    fn lcdc_for_bg_fetch(&self, x: u32, current_lcdc: u8) -> u8 {
        if let Some(range) = self
            .ranges
            .iter()
            .find(|range| u32::from(range.start_x) <= x && x <= u32::from(range.end_x))
        {
            (current_lcdc & !LCDC_BG_MAP) | range.bg_map_bit
        } else {
            current_lcdc
        }
    }

    fn clear_consumed(&mut self, next_x: u8) {
        self.ranges.retain(|range| range.end_x != next_x);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct LcdcWindowMapEdgeRange {
    start_x: u8,
    end_x: u8,
    window_map_bit: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcWindowMapEdge {
    ranges: Vec<LcdcWindowMapEdgeRange>,
}

impl LcdcWindowMapEdge {
    fn window_tile_boundary_at_or_after(next_x: u8, wx: u8) -> u8 {
        let win_x_start = i16::from(wx) - 7;
        let window_x = i16::from(next_x) - win_x_start;
        let phase = window_x.rem_euclid(8) as u8;
        let pixels_until_boundary = if phase == 0 { 0 } else { 8 - phase };
        next_x.saturating_add(pixels_until_boundary)
    }

    fn record_write(&mut self, start_x: u8, end_x: u8, previous_lcdc: u8, new_lcdc: u8) {
        if previous_lcdc & LCDC_WINDOW_MAP == new_lcdc & LCDC_WINDOW_MAP || start_x > end_x {
            return;
        }

        self.ranges.push(LcdcWindowMapEdgeRange {
            start_x,
            end_x,
            window_map_bit: previous_lcdc & LCDC_WINDOW_MAP,
        });
    }

    fn lcdc_for_window_fetch(&self, x: u32, current_lcdc: u8) -> u8 {
        if let Some(range) = self
            .ranges
            .iter()
            .find(|range| u32::from(range.start_x) <= x && x <= u32::from(range.end_x))
        {
            (current_lcdc & !LCDC_WINDOW_MAP) | range.window_map_bit
        } else {
            current_lcdc
        }
    }

    fn clear_consumed(&mut self, next_x: u8) {
        self.ranges.retain(|range| range.end_x != next_x);
    }

    fn ranges_cover(&self, x: u8) -> bool {
        self.ranges
            .iter()
            .any(|range| range.start_x <= x && x <= range.end_x)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct LcdcTileDataRange {
    start_x: u8,
    end_x: u8,
    low_lcdc: u8,
    high_lcdc: u8,
    #[serde(default)]
    low_override: DmgTileByteOverride,
    #[serde(default)]
    high_override: DmgTileByteOverride,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcTileDataEdge {
    ranges: Vec<LcdcTileDataRange>,
}

impl LcdcTileDataEdge {
    fn record_write(&mut self, start_x: u8, end_x: u8, low_lcdc: u8, high_lcdc: u8) {
        self.record_write_with_overrides(
            start_x,
            end_x,
            low_lcdc,
            high_lcdc,
            DmgTileByteOverride::None,
            DmgTileByteOverride::None,
        );
    }

    fn record_write_with_overrides(
        &mut self,
        start_x: u8,
        end_x: u8,
        low_lcdc: u8,
        high_lcdc: u8,
        low_override: DmgTileByteOverride,
        high_override: DmgTileByteOverride,
    ) {
        if start_x > end_x {
            return;
        }

        self.ranges.push(LcdcTileDataRange {
            start_x,
            end_x,
            low_lcdc,
            high_lcdc,
            low_override,
            high_override,
        });
    }

    fn lcdc_for_tile_data(
        &self,
        x: u32,
        current_lcdc: u8,
    ) -> (u8, u8, DmgTileByteOverride, DmgTileByteOverride) {
        self.ranges
            .iter()
            .rev()
            .find(|range| u32::from(range.start_x) <= x && x <= u32::from(range.end_x))
            .map_or(
                (
                    current_lcdc,
                    current_lcdc,
                    DmgTileByteOverride::None,
                    DmgTileByteOverride::None,
                ),
                |range| {
                    (
                        (current_lcdc & !LCDC_TILE_DATA) | (range.low_lcdc & LCDC_TILE_DATA),
                        (current_lcdc & !LCDC_TILE_DATA) | (range.high_lcdc & LCDC_TILE_DATA),
                        range.low_override,
                        range.high_override,
                    )
                },
            )
    }

    fn has_latched_range(&self, start_x: u8, end_x: u8) -> bool {
        self.ranges.iter().any(|range| {
            range.start_x == start_x
                && range.end_x == end_x
                && range.low_lcdc & LCDC_TILE_DATA == range.high_lcdc & LCDC_TILE_DATA
        })
    }

    fn has_range(&self, start_x: u8, end_x: u8) -> bool {
        self.ranges
            .iter()
            .any(|range| range.start_x == start_x && range.end_x == end_x)
    }

    fn tile_select_bits_for_range(&self, start_x: u8, end_x: u8) -> Option<(u8, u8)> {
        self.ranges
            .iter()
            .rev()
            .find(|range| range.start_x == start_x && range.end_x == end_x)
            .map(|range| {
                (
                    range.low_lcdc & LCDC_TILE_DATA,
                    range.high_lcdc & LCDC_TILE_DATA,
                )
            })
    }

    fn clear_consumed(&mut self, next_x: u8) {
        self.ranges.retain(|range| range.end_x != next_x);
    }

    fn ranges_cover(&self, x: u8) -> bool {
        self.ranges
            .iter()
            .any(|range| range.start_x <= x && x <= range.end_x)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcObjEnableEdge {
    active: bool,
    start_x: u8,
    end_x: u8,
    enabled: bool,
}

impl LcdcObjEnableEdge {
    fn record_write(&mut self, next_x: u8, end_x: u8, previous_lcdc: u8, new_lcdc: u8) {
        if previous_lcdc & LCDC_OBJ_ENABLE == new_lcdc & LCDC_OBJ_ENABLE {
            return;
        }

        self.active = true;
        self.start_x = next_x;
        self.end_x = end_x;
        self.enabled = previous_lcdc & LCDC_OBJ_ENABLE != 0;
    }

    fn obj_enabled_for_pixel(&self, x: u32, current_lcdc: u8) -> bool {
        if self.active && u32::from(self.start_x) <= x && x <= u32::from(self.end_x) {
            self.enabled
        } else {
            current_lcdc & LCDC_OBJ_ENABLE != 0
        }
    }

    fn clear_consumed(&mut self, next_x: u8) {
        if self.active && self.end_x == next_x {
            self.active = false;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ObjFetchLcdcRange {
    start_x: u8,
    end_x: u8,
    low_lcdc: u8,
    high_lcdc: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct WindowEnableEdge {
    start_x: u8,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct WindowXEdge {
    start_x: u8,
    wx: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum BgScxFetchStep {
    #[default]
    GetTile,
    TileDataLow,
    TileDataHigh,
    Sleep,
    Push,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BgScxSampler {
    config: BgScxSamplerConfig,
    step: BgScxFetchStep,
    step_dots: u8,
    latched_scx: u8,
    visible_tile_scx: u8,
    visible_pixels_remaining: u8,
    queued_tile_scx: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct BgScxSamplerConfig {
    fetch_start_dots: u16,
    queue_prefetched_tile: bool,
    pause_on_obj_fetch: bool,
}

impl BgScxSampler {
    fn new(scx: u8, config: BgScxSamplerConfig) -> Self {
        let scx = scx & SCX_TILE_MASK;
        Self {
            config,
            step: BgScxFetchStep::GetTile,
            step_dots: 0,
            latched_scx: scx,
            visible_tile_scx: scx,
            visible_pixels_remaining: 0,
            queued_tile_scx: None,
        }
    }

    fn reset(&mut self, scx: u8, config: BgScxSamplerConfig) {
        *self = Self::new(scx, config);
    }

    fn tick(&mut self, elapsed: u16, scx: u8) {
        if elapsed < self.config.fetch_start_dots {
            return;
        }

        match self.step {
            BgScxFetchStep::GetTile => {
                if self.step_dots == 0 {
                    self.latched_scx = scx & SCX_TILE_MASK;
                }
                self.advance_step(BgScxFetchStep::TileDataLow);
            }
            BgScxFetchStep::TileDataLow => self.advance_step(BgScxFetchStep::TileDataHigh),
            BgScxFetchStep::TileDataHigh => self.advance_step(BgScxFetchStep::Sleep),
            BgScxFetchStep::Sleep => {
                self.step_dots += 1;
                if self.step_dots == 2 {
                    self.step = BgScxFetchStep::Push;
                    self.step_dots = 0;
                    self.push_latched_tile();
                }
            }
            BgScxFetchStep::Push => self.push_latched_tile(),
        }
    }

    fn will_sample_scx(&self, elapsed: u16) -> bool {
        elapsed >= self.config.fetch_start_dots
            && self.step == BgScxFetchStep::GetTile
            && self.step_dots == 0
    }

    fn push_latched_tile(&mut self) {
        if self.visible_pixels_remaining == 0 && self.queued_tile_scx.is_none() {
            self.visible_tile_scx = self.latched_scx;
            self.visible_pixels_remaining = OBJ_PIXELS_PER_FETCH;
            self.step = BgScxFetchStep::GetTile;
            self.step_dots = 0;
            return;
        }

        if self.config.queue_prefetched_tile && self.queued_tile_scx.is_none() {
            self.queued_tile_scx = Some(self.latched_scx);
            self.step = BgScxFetchStep::GetTile;
            self.step_dots = 0;
        }
    }

    fn advance_step(&mut self, next_step: BgScxFetchStep) {
        self.step_dots += 1;
        if self.step_dots == 2 {
            self.step = next_step;
            self.step_dots = 0;
        }
    }

    fn current_scx(&mut self) -> u8 {
        self.ensure_front_loaded();
        self.visible_tile_scx
    }

    fn consume_pixel(&mut self) {
        self.ensure_front_loaded();
        self.visible_pixels_remaining = self.visible_pixels_remaining.saturating_sub(1);
    }

    fn ensure_front_loaded(&mut self) {
        if self.visible_pixels_remaining == 0
            && let Some(scx) = self.queued_tile_scx.take()
        {
            self.visible_tile_scx = scx;
            self.visible_pixels_remaining = OBJ_PIXELS_PER_FETCH;
        }
    }

    fn pauses_on_obj_fetch(&self) -> bool {
        self.config.pause_on_obj_fetch
    }
}

impl Default for BgScxSampler {
    fn default() -> Self {
        Self::new(0, BgScxSamplerConfig::default())
    }
}

impl BgScxSamplerConfig {
    fn for_mode(cgb_mode: bool) -> Self {
        if cgb_mode {
            // Native CGB fetches one dot later and keeps a prefetched BG tile
            // available while OBJ fetches stall the visible pixel stream.
            Self {
                fetch_start_dots: SCX_LOW_BITS_SAMPLE_DOTS + 1,
                queue_prefetched_tile: true,
                pause_on_obj_fetch: true,
            }
        } else {
            Self::default()
        }
    }
}

impl Default for BgScxSamplerConfig {
    fn default() -> Self {
        Self {
            fetch_start_dots: SCX_LOW_BITS_SAMPLE_DOTS,
            queue_prefetched_tile: false,
            pause_on_obj_fetch: false,
        }
    }
}

/// Tracks the SCY register value sampled at specific background tile-fetch stages.
///
/// On DMG and CGB-C, SCY is read during the **B** (tile map), **0** (low byte), and **1**
/// (high byte) fetch stages. Changing SCY between stages allows mixing tile-map rows and
/// tile-data rows, producing the characteristic scanline-scroll glitch.
///
/// On CGB-D (and newer), SCY is only read at the **B** stage (`b_stage_only = true`), so
/// no inter-stage mixing can occur.
///
/// The sampler mirrors the `BgScxSampler` step machine. The `fetch_start_dots`
/// places the first B-stage three dots later than the SCX tile-column latch,
/// matching Mealybug's mid-scanline SCY timing on DMG and CGB revisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BgScySampler {
    step: BgScxFetchStep,
    step_dots: u8,
    fetch_start_dots: u16,
    /// When `true` (CGB-D), SCY is only latched at the B stage; all data latches equal
    /// `scy_map` for the same tile (no inter-stage mixing).
    b_stage_only: bool,
    latched_scy_map: u8,
    /// SCY latched at TileDataLow (stage 0); used for low bitplane row-within-tile.
    latched_scy_data_low: u8,
    /// SCY latched at TileDataHigh (stage 1); used for high bitplane row-within-tile.
    latched_scy_data_high: u8,
    visible_scy_map: u8,
    visible_scy_data_low: u8,
    visible_scy_data_high: u8,
    visible_pixels_remaining: u8,
}

impl BgScySampler {
    fn new(scy: u8, fetch_start_dots: u16, b_stage_only: bool) -> Self {
        Self {
            step: BgScxFetchStep::GetTile,
            step_dots: 0,
            fetch_start_dots,
            b_stage_only,
            latched_scy_map: scy,
            latched_scy_data_low: scy,
            latched_scy_data_high: scy,
            visible_scy_map: scy,
            visible_scy_data_low: scy,
            visible_scy_data_high: scy,
            visible_pixels_remaining: 0,
        }
    }

    fn reset(&mut self, scy: u8, fetch_start_dots: u16, b_stage_only: bool) {
        *self = Self::new(scy, fetch_start_dots, b_stage_only);
    }

    fn tick(&mut self, elapsed: u16, scy: u8) {
        if elapsed < self.fetch_start_dots {
            return;
        }
        match self.step {
            BgScxFetchStep::GetTile => {
                if self.step_dots == 0 {
                    self.latched_scy_map = scy;
                    if self.b_stage_only {
                        self.latched_scy_data_low = scy;
                        self.latched_scy_data_high = scy;
                    }
                }
                self.advance_step(BgScxFetchStep::TileDataLow);
            }
            BgScxFetchStep::TileDataLow => {
                if self.step_dots == 0 && !self.b_stage_only {
                    self.latched_scy_data_low = scy;
                    self.latched_scy_data_high = scy;
                }
                self.advance_step(BgScxFetchStep::TileDataHigh);
            }
            BgScxFetchStep::TileDataHigh => {
                if self.step_dots == 0 && !self.b_stage_only {
                    // Stage 1 independently re-samples SCY for the high tile byte.
                    self.latched_scy_data_high = scy;
                }
                self.advance_step(BgScxFetchStep::Sleep);
            }
            BgScxFetchStep::Sleep => {
                self.step_dots += 1;
                if self.step_dots == 2 {
                    self.step = BgScxFetchStep::Push;
                    self.step_dots = 0;
                    self.push_latched_tile();
                }
            }
            BgScxFetchStep::Push => self.push_latched_tile(),
        }
    }

    fn push_latched_tile(&mut self) {
        if self.visible_pixels_remaining == 0 {
            self.visible_scy_map = self.latched_scy_map;
            self.visible_scy_data_low = self.latched_scy_data_low;
            self.visible_scy_data_high = self.latched_scy_data_high;
            self.visible_pixels_remaining = OBJ_PIXELS_PER_FETCH;
            self.step = BgScxFetchStep::GetTile;
            self.step_dots = 0;
        }
    }

    fn advance_step(&mut self, next_step: BgScxFetchStep) {
        self.step_dots += 1;
        if self.step_dots == 2 {
            self.step = next_step;
            self.step_dots = 0;
        }
    }

    fn current_scy_map(&self) -> u8 {
        self.visible_scy_map
    }

    fn current_scy_data_low(&self) -> u8 {
        self.visible_scy_data_low
    }

    fn current_scy_data_high(&self) -> u8 {
        self.visible_scy_data_high
    }

    fn consume_pixel(&mut self) {
        self.visible_pixels_remaining = self.visible_pixels_remaining.saturating_sub(1);
    }
}

impl Default for BgScySampler {
    fn default() -> Self {
        Self::new(0, SCX_LOW_BITS_SAMPLE_DOTS, false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelFifoRenderer {
    active: bool,
    scanline: u8,
    mode3_start_dot: u16,
    #[serde(default)]
    scanline_start_lcdc: u8,
    #[serde(default)]
    scanline_start_wx: u8,
    next_x: u8,
    window_active: bool,
    #[serde(default)]
    window_triggered: bool,
    #[serde(default)]
    window_fetch_wx: u8,
    #[serde(default)]
    window_fetch_line: u8,
    #[serde(default)]
    pending_hidden_window_fetch_wx: Option<u8>,
    #[serde(default)]
    window_activation_count: u8,
    bgp_edge_active: bool,
    bgp_edge_x: u8,
    bgp_edge_value: u8,
    #[serde(default)]
    obp0_edge_active: bool,
    #[serde(default)]
    obp0_edge_x: u8,
    #[serde(default)]
    obp0_edge_value: u8,
    #[serde(default)]
    obp1_edge_active: bool,
    #[serde(default)]
    obp1_edge_x: u8,
    #[serde(default)]
    obp1_edge_value: u8,
    #[serde(default)]
    fine_scroll_delay_dots: u16,
    #[serde(default)]
    pending_window_startup_stall_dots: u8,
    #[serde(default)]
    window_startup_stall_consumed: u8,
    #[serde(default)]
    scanline_start_scx_low: u8,
    #[serde(default)]
    bg_fetch_scx: u8,
    #[serde(default)]
    scx_low_bits_sampled: bool,
    #[serde(default)]
    bg_scx_sampler: BgScxSampler,
    #[serde(default)]
    pending_scx_sample_override: Option<u8>,
    /// SCY value sampled at the B (tile-map) fetch stage for the current tile.
    #[serde(default)]
    bg_fetch_scy_map: u8,
    /// SCY value sampled at the 0 (TileDataLow) fetch stage; used for low-bitplane pixel row.
    /// Equals `bg_fetch_scy_map` on CGB-D (B-stage-only model).
    #[serde(default)]
    bg_fetch_scy_data_low: u8,
    /// SCY value sampled at the 1 (TileDataHigh) fetch stage; used for high-bitplane pixel row.
    /// Equals `bg_fetch_scy_map` on CGB-D (B-stage-only model).
    #[serde(default)]
    bg_fetch_scy_data_high: u8,
    #[serde(default)]
    bg_scy_sampler: BgScySampler,
    /// Effective SCY seen by the PPU tile fetcher after the write callback has
    /// reached the model-specific fetch sampling point.
    #[serde(default)]
    effective_scy: u8,
    /// Pending SCY write: `(dot_when_active, new_value)`.
    /// When `elapsed >= dot_when_active`, `effective_scy` is updated to `new_value`.
    #[serde(default, alias = "pending_cgb_scy")]
    pending_scy_write: Option<(u16, u8)>,
    /// When `true` (CGB-D and newer), SCY is only sampled at the B (GetTile) stage;
    /// data latches always equal `scy_map` for each tile (no inter-stage mixing).
    #[serde(default)]
    scy_b_stage_only: bool,
    #[serde(default)]
    pending_obj_stall_dots: u16,
    #[serde(default)]
    pending_obj_bg_fetch_wait_dots: u16,
    #[serde(default)]
    obj_stall_events: Vec<sprites::ObjPenaltyEvent>,
    #[serde(default)]
    next_obj_stall_event: usize,
    #[serde(default)]
    active_obj_stall_x: Option<u8>,
    #[serde(default)]
    canceled_obj_fetch_ranges: Vec<(u8, u8)>,
    #[serde(default)]
    obj_fetch_lcdc_ranges: Vec<ObjFetchLcdcRange>,
    #[serde(default)]
    active_obj_fetch_lcdc_range: Option<usize>,
    #[serde(default)]
    obj_fetch_ignores_lcdc: bool,
    #[serde(default)]
    sprite_indices: Vec<usize>,
    #[serde(default)]
    leftmost_obj_oam_x: Option<u8>,
    #[serde(default)]
    lcdc_bg_enable_edge: LcdcBgEnableEdge,
    #[serde(default)]
    lcdc_bg_map_edge: LcdcBgMapEdge,
    #[serde(default)]
    lcdc_window_map_edge: LcdcWindowMapEdge,
    #[serde(default)]
    lcdc_tile_data_edge: LcdcTileDataEdge,
    #[serde(default)]
    lcdc_obj_enable_edge: LcdcObjEnableEdge,
    #[serde(default)]
    window_enable_edges: Vec<WindowEnableEdge>,
    #[serde(default)]
    window_reset_edges: Vec<u8>,
    #[serde(default)]
    window_x_edges: Vec<WindowXEdge>,
    #[serde(default)]
    window_reactivation_edges: Vec<u8>,
    #[serde(default)]
    window_reactivation_shift: u8,
    #[serde(default)]
    off_left_window_reactivation_x: Option<u8>,
    #[serde(default)]
    bg_fetch_x_offset: u8,
    #[serde(default)]
    cgb_dmg_tile_sel_glitch_data: Option<u8>,
}

impl PixelFifoRenderer {
    pub fn new() -> Self {
        Self {
            active: false,
            scanline: 0,
            mode3_start_dot: 0,
            scanline_start_lcdc: 0,
            scanline_start_wx: 0,
            next_x: 0,
            window_active: false,
            window_triggered: false,
            window_fetch_wx: 7,
            window_fetch_line: 0,
            pending_hidden_window_fetch_wx: None,
            window_activation_count: 0,
            bgp_edge_active: false,
            bgp_edge_x: 0,
            bgp_edge_value: INITIAL_BGP,
            obp0_edge_active: false,
            obp0_edge_x: 0,
            obp0_edge_value: 0,
            obp1_edge_active: false,
            obp1_edge_x: 0,
            obp1_edge_value: 0,
            fine_scroll_delay_dots: 0,
            pending_window_startup_stall_dots: 0,
            window_startup_stall_consumed: 0,
            scanline_start_scx_low: 0,
            bg_fetch_scx: 0,
            scx_low_bits_sampled: true,
            bg_scx_sampler: BgScxSampler::default(),
            pending_scx_sample_override: None,
            bg_fetch_scy_map: 0,
            bg_fetch_scy_data_low: 0,
            bg_fetch_scy_data_high: 0,
            bg_scy_sampler: BgScySampler::default(),
            effective_scy: 0,
            pending_scy_write: None,
            scy_b_stage_only: false,
            pending_obj_stall_dots: 0,
            pending_obj_bg_fetch_wait_dots: 0,
            obj_stall_events: Vec::new(),
            next_obj_stall_event: 0,
            active_obj_stall_x: None,
            canceled_obj_fetch_ranges: Vec::new(),
            obj_fetch_lcdc_ranges: Vec::new(),
            active_obj_fetch_lcdc_range: None,
            obj_fetch_ignores_lcdc: false,
            sprite_indices: Vec::new(),
            leftmost_obj_oam_x: None,
            lcdc_bg_enable_edge: LcdcBgEnableEdge::default(),
            lcdc_bg_map_edge: LcdcBgMapEdge::default(),
            lcdc_window_map_edge: LcdcWindowMapEdge::default(),
            lcdc_tile_data_edge: LcdcTileDataEdge::default(),
            lcdc_obj_enable_edge: LcdcObjEnableEdge::default(),
            window_enable_edges: Vec::new(),
            window_reset_edges: Vec::new(),
            window_x_edges: Vec::new(),
            window_reactivation_edges: Vec::new(),
            window_reactivation_shift: 0,
            off_left_window_reactivation_x: None,
            bg_fetch_x_offset: 0,
            cgb_dmg_tile_sel_glitch_data: None,
        }
    }

    pub fn begin_scanline(
        &mut self,
        scanline: u8,
        mode3_start_dot: u16,
        oam: &[u8; 0xA0],
        registers: &Registers,
        cgb_mode: bool,
        dmg_compat: bool,
    ) {
        self.active = true;
        self.scanline = scanline;
        self.mode3_start_dot = mode3_start_dot;
        self.scanline_start_lcdc = registers.lcdc;
        self.scanline_start_wx = registers.wx;
        self.next_x = 0;
        self.window_active = false;
        self.window_triggered = false;
        self.window_fetch_wx = registers.wx;
        self.window_fetch_line = 0;
        self.pending_hidden_window_fetch_wx = None;
        self.window_activation_count = 0;
        self.bgp_edge_active = false;
        self.bgp_edge_value = registers.bgp;
        self.obp0_edge_active = false;
        self.obp0_edge_value = registers.obp0;
        self.obp1_edge_active = false;
        self.obp1_edge_value = registers.obp1;
        self.lcdc_bg_enable_edge = LcdcBgEnableEdge::default();
        self.lcdc_bg_map_edge = LcdcBgMapEdge::default();
        self.lcdc_window_map_edge = LcdcWindowMapEdge::default();
        self.lcdc_tile_data_edge = LcdcTileDataEdge::default();
        self.lcdc_obj_enable_edge = LcdcObjEnableEdge::default();
        self.scanline_start_scx_low = registers.scx & SCX_FINE_MASK;
        self.bg_fetch_scx = registers.scx;
        self.scx_low_bits_sampled = false;
        self.bg_scx_sampler
            .reset(registers.scx, BgScxSamplerConfig::for_mode(cgb_mode));
        self.pending_scx_sample_override = None;
        self.effective_scy = registers.scy;
        self.pending_scy_write = None;
        self.bg_scy_sampler
            .reset(registers.scy, scy_fetch_start_dots(), self.scy_b_stage_only);
        self.bg_fetch_scy_map = registers.scy;
        self.bg_fetch_scy_data_low = registers.scy;
        self.bg_fetch_scy_data_high = registers.scy;
        self.fine_scroll_delay_dots = u16::from(self.scanline_start_scx_low);
        self.pending_window_startup_stall_dots = 0;
        self.window_startup_stall_consumed = 0;
        self.pending_obj_stall_dots = 0;
        self.pending_obj_bg_fetch_wait_dots = 0;
        self.next_obj_stall_event = 0;
        self.active_obj_stall_x = None;
        self.canceled_obj_fetch_ranges.clear();
        self.obj_fetch_lcdc_ranges.clear();
        self.window_enable_edges.clear();
        self.window_reset_edges.clear();
        self.window_x_edges.clear();
        self.window_reactivation_edges.clear();
        self.window_reactivation_shift = 0;
        self.off_left_window_reactivation_x = None;
        self.bg_fetch_x_offset = 0;
        self.cgb_dmg_tile_sel_glitch_data = None;
        self.active_obj_fetch_lcdc_range = None;
        self.obj_fetch_ignores_lcdc = ObjFetchModel::for_dmg_render_path(cgb_mode, dmg_compat)
            .is_some_and(ObjFetchModel::ignores_lcdc_obj_enable);
        if self.obj_fetch_ignores_lcdc || registers.lcdc & LCDC_OBJ_ENABLE != 0 {
            sprites::scan_oam_line_into(scanline, oam, registers.lcdc, &mut self.sprite_indices);
            let bg_fetch_wait_extra_dots = if cgb_mode {
                0
            } else {
                DMG_OBJ_FETCH_EXTRA_SCY_ADVANCE_DOTS
            };
            sprites::schedule_obj_penalties_with_bg_fetch_wait_extra(
                &self.sprite_indices,
                oam,
                registers.scx,
                bg_fetch_wait_extra_dots,
                &mut self.obj_stall_events,
            );
            self.leftmost_obj_oam_x = self
                .sprite_indices
                .iter()
                .map(|&index| oam[index * 4 + 1])
                .min();
            self.extend_off_left_window_obj_stall(registers);
        } else {
            self.sprite_indices.clear();
            self.obj_stall_events.clear();
            self.leftmost_obj_oam_x = None;
        }
    }

    fn extend_off_left_window_obj_stall(&mut self, registers: &Registers) {
        let window_starts_at_left_edge =
            registers.lcdc & 0x20 != 0 && self.scanline >= registers.wy && registers.wx <= 7;
        if !window_starts_at_left_edge
            || !self
                .leftmost_obj_oam_x
                .is_some_and(is_off_left_window_delayed_obj_x)
        {
            return;
        }

        if let Some(event) = self
            .obj_stall_events
            .first_mut()
            .filter(|event| event.x == 0)
        {
            event.dots = event.dots.saturating_add(OFF_LEFT_WINDOW_EXTRA_STALL_DOTS);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        dot: u16,
        vram: &[u8; 0x2000],
        vram_bank1: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        bg_palette_ram: &[u8; 64],
        obj_palette_ram: &[u8; 64],
        window_line: u8,
        cgb_mode: bool,
        opri_dmg_mode: bool,
        dmg_compat: bool,
        screen_buffer: &mut ScreenBuffer,
    ) -> Option<u8> {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return None;
        }

        let elapsed = dot.saturating_sub(self.mode3_start_dot);
        self.update_effective_scy(elapsed);
        let effective_scy = self.effective_scy;
        self.sample_scanline_start_scx_low(elapsed, registers.scx);
        let will_sample_scx = self.bg_scx_sampler.will_sample_scx(elapsed);
        let fetch_scx = self.scx_for_fetch_sample(elapsed, registers.scx);
        if elapsed < FETCHER_STARTUP_DOTS {
            self.bg_scx_sampler.tick(elapsed, fetch_scx);
            self.clear_pending_scx_sample_override(will_sample_scx);
            self.bg_scy_sampler.tick(elapsed, effective_scy);
            return None;
        }
        if elapsed < FETCHER_STARTUP_DOTS + self.fine_scroll_delay_dots {
            self.bg_scx_sampler.tick(elapsed, fetch_scx);
            self.clear_pending_scx_sample_override(will_sample_scx);
            self.bg_scy_sampler.tick(elapsed, effective_scy);
            self.consume_bg_scx_pixel();
            self.bg_scy_sampler.consume_pixel();
            return None;
        }
        self.queue_stall_events_for_next_pixel(registers.lcdc);
        if self.pending_obj_stall_dots > 0 {
            if !self.bg_scx_sampler.pauses_on_obj_fetch() {
                self.bg_scx_sampler.tick(elapsed, fetch_scx);
                self.clear_pending_scx_sample_override(will_sample_scx);
            }
            if self.pending_obj_bg_fetch_wait_dots > 0 {
                // The leading OBJ tile-wait/fetch setup samples SCY timing without
                // advancing the CGB visible-tile SCX prefetch abstraction.
                self.bg_scy_sampler.tick(elapsed, effective_scy);
            }
            self.pending_obj_bg_fetch_wait_dots =
                self.pending_obj_bg_fetch_wait_dots.saturating_sub(1);
            self.pending_obj_stall_dots -= 1;
            if self.pending_obj_stall_dots == 0 {
                self.pending_obj_bg_fetch_wait_dots = 0;
                self.active_obj_stall_x = None;
                self.active_obj_fetch_lcdc_range = None;
            }
            return None;
        }

        if self.pending_window_startup_stall_dots > 0 {
            self.bg_scx_sampler.tick(elapsed, fetch_scx);
            self.clear_pending_scx_sample_override(will_sample_scx);
            self.bg_scy_sampler.tick(elapsed, effective_scy);
            self.pending_window_startup_stall_dots -= 1;
            self.window_startup_stall_consumed += 1;
            return None;
        }
        if self.window_startup_stall_consumed == 0 && self.window_would_trigger_at_next_x(registers)
        {
            self.bg_scx_sampler.tick(elapsed, fetch_scx);
            self.clear_pending_scx_sample_override(will_sample_scx);
            self.bg_scy_sampler.tick(elapsed, effective_scy);
            self.pending_window_startup_stall_dots =
                self.window_startup_stall_dots(registers).saturating_sub(1);
            self.window_startup_stall_consumed = 1;
            return None;
        }

        self.bg_scx_sampler.tick(elapsed, fetch_scx);
        self.clear_pending_scx_sample_override(will_sample_scx);
        self.bg_scy_sampler.tick(elapsed, effective_scy);
        let x = u32::from(self.next_x);
        self.update_bg_fetch_scx();
        self.update_bg_fetch_scy();
        if cgb_mode && !dmg_compat {
            self.render_cgb_pixel(
                x,
                vram,
                vram_bank1,
                oam,
                registers,
                bg_palette_ram,
                obj_palette_ram,
                window_line,
                opri_dmg_mode,
                screen_buffer,
            );
        } else if cgb_mode {
            self.render_cgb_dmg_compat_pixel(
                x,
                vram,
                oam,
                registers,
                bg_palette_ram,
                obj_palette_ram,
                window_line,
                screen_buffer,
            );
        } else {
            self.render_dmg_pixel(x, vram, oam, registers, window_line, screen_buffer);
        }
        self.clear_consumed_bgp_edge();
        self.clear_consumed_obp_edges();
        self.lcdc_bg_enable_edge.clear_consumed(self.next_x);
        self.lcdc_obj_enable_edge.clear_consumed(self.next_x);
        self.lcdc_bg_map_edge.clear_consumed(self.next_x);
        self.lcdc_window_map_edge.clear_consumed(self.next_x);
        self.lcdc_tile_data_edge.clear_consumed(self.next_x);
        self.clear_consumed_obj_fetch_lcdc_ranges();
        self.clear_consumed_window_edges();
        self.consume_bg_scx_pixel();
        self.next_x = self.next_x.saturating_add(1);
        if self.next_x as u32 >= ScreenBuffer::WIDTH {
            self.active = false;
            Some(self.window_activation_count)
        } else {
            None
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn fixup_after_state_load(
        &mut self,
        cgb_mode: bool,
        scy_b_stage_only: bool,
        current_scy: u8,
    ) {
        self.bg_scx_sampler.config = BgScxSamplerConfig::for_mode(cgb_mode);
        self.scy_b_stage_only = scy_b_stage_only;
        self.bg_scy_sampler.b_stage_only = scy_b_stage_only;
        self.bg_scy_sampler.fetch_start_dots = scy_fetch_start_dots();
        self.effective_scy = current_scy;
        self.pending_scy_write = None;
    }

    pub(super) fn set_scy_b_stage_only(&mut self, b_stage_only: bool) {
        self.scy_b_stage_only = b_stage_only;
        self.bg_scy_sampler.b_stage_only = b_stage_only;
    }

    pub fn record_scx_write(&mut self, previous: u8, dot: u16) {
        if self.active
            && self
                .bg_scx_sampler
                .will_sample_scx(dot.saturating_sub(self.mode3_start_dot))
        {
            // Register writes are visible to the PPU after the fetch phase that
            // is already sampling on the same dot.
            self.pending_scx_sample_override = Some(previous);
        }
    }

    /// Record a write to the SCY register (`$FF42`) during mode 3.
    ///
    /// - `_old_scy` — value of SCY before the write (kept for API symmetry with `record_scx_write`; unused)
    /// - `new_scy`  — new value being written
    /// - `dot`      — current absolute dot counter (used to compute the elapsed delay)
    /// - `cgb_mode` — selects the model-specific callback delay constant
    pub fn record_scy_write(&mut self, _old_scy: u8, new_scy: u8, dot: u16, cgb_mode: bool) {
        if !self.active {
            return;
        }
        let elapsed = dot.saturating_sub(self.mode3_start_dot);
        let delay = if cgb_mode {
            CGB_SCY_WRITE_CALLBACK_DELAY_DOTS
        } else {
            DMG_SCY_WRITE_CALLBACK_DELAY_DOTS
        };
        self.pending_scy_write = Some((elapsed.saturating_add(delay), new_scy));
    }

    fn scx_for_fetch_sample(&self, elapsed: u16, current_scx: u8) -> u8 {
        if self.bg_scx_sampler.will_sample_scx(elapsed) {
            self.pending_scx_sample_override.unwrap_or(current_scx)
        } else {
            current_scx
        }
    }

    fn clear_pending_scx_sample_override(&mut self, sampled: bool) {
        if sampled {
            self.pending_scx_sample_override = None;
        }
    }

    pub fn record_bgp_write(
        &mut self,
        previous: u8,
        new: u8,
        registers: &Registers,
        cgb_mode: bool,
        dmg_compat: bool,
        cgb_model: CgbModel,
    ) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }

        self.bgp_edge_active = true;
        self.bgp_edge_x = self.next_x;
        // If an OBJ fetch is delaying this pixel, the BGP write lands before
        // the pixel is pushed to LCD, so the delayed pixel sees the new value.
        self.bgp_edge_value = if self.next_x == 0
            || self.is_waiting_on_obj_fetch()
            || self.is_waiting_on_window_startup()
            || self.window_startup_would_delay_next_pixel(registers)
        {
            new
        } else if cgb_mode && dmg_compat {
            match cgb_model {
                CgbModel::CgbD | CgbModel::CgbE => new,
                CgbModel::Cgb0 | CgbModel::CgbA | CgbModel::CgbB | CgbModel::CgbC => previous,
            }
        } else {
            previous | new
        };
    }

    pub fn record_obp0_write(
        &mut self,
        previous: u8,
        new: u8,
        cgb_mode: bool,
        dmg_compat: bool,
        cgb_model: CgbModel,
    ) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }
        if !(cgb_mode && dmg_compat) {
            return;
        }

        self.obp0_edge_active = true;
        self.obp0_edge_x = self.next_x;
        // Unlike BGP (which affects background pixels), OBP0 affects sprite pixels.
        // If an OBJ fetch is delaying this pixel, the OBP write lands before
        // the pixel is pushed to LCD, so the delayed pixel sees the new value.
        // Otherwise CGB-D picks up the new value immediately; CGB-C latches it
        // one pixel late (uses the previous value).
        // At pixel 0 the write always takes effect immediately on all models.
        self.obp0_edge_value = if self.next_x == 0 || self.is_waiting_on_obj_fetch() {
            new
        } else {
            match cgb_model {
                CgbModel::CgbD | CgbModel::CgbE => new,
                CgbModel::Cgb0 | CgbModel::CgbA | CgbModel::CgbB | CgbModel::CgbC => previous,
            }
        };
    }

    pub fn record_obp1_write(
        &mut self,
        previous: u8,
        new: u8,
        cgb_mode: bool,
        dmg_compat: bool,
        cgb_model: CgbModel,
    ) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }
        if !(cgb_mode && dmg_compat) {
            return;
        }

        self.obp1_edge_active = true;
        self.obp1_edge_x = self.next_x;
        self.obp1_edge_value = if self.next_x == 0 || self.is_waiting_on_obj_fetch() {
            new
        } else {
            match cgb_model {
                CgbModel::CgbD | CgbModel::CgbE => new,
                CgbModel::Cgb0 | CgbModel::CgbA | CgbModel::CgbB | CgbModel::CgbC => previous,
            }
        };
    }

    #[cfg(test)]
    pub fn record_lcdc_write(
        &mut self,
        previous: u8,
        new: u8,
        scx: u8,
        cgb_mode: bool,
        dmg_compat: bool,
    ) {
        self.record_lcdc_write_with_window(previous, new, scx, cgb_mode, dmg_compat, 166, 144);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_lcdc_write_with_window(
        &mut self,
        previous: u8,
        new: u8,
        scx: u8,
        cgb_mode: bool,
        dmg_compat: bool,
        wx: u8,
        wy: u8,
    ) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }

        let consumed = self.window_startup_stall_consumed;
        let actual_next_x = self.next_x;
        let actual_pend = self.pending_obj_stall_dots;
        let actual_window_active = self.window_active;
        if consumed > 0 {
            let (effective_next_x, effective_pend) =
                self.compute_effective_state_for_window_stall(consumed);
            // Window is active in pre-PR once its first pixel (at window_visible_start_x)
            // would have been rendered — i.e. effective_next_x is past the trigger point.
            let effective_window_active =
                self.window_active || effective_next_x > window_visible_start_x(wx);
            self.next_x = effective_next_x;
            self.pending_obj_stall_dots = effective_pend;
            self.window_active = effective_window_active;
        }

        let waiting_on_obj_fetch = self.is_waiting_on_obj_fetch();
        self.record_lcdc_window_enable_write(previous, new, wx);
        let window_fetch_active = previous & LCDC_WINDOW_ENABLE != 0
            && self.scanline >= wy
            && self.next_x == wx.saturating_sub(7);
        self.record_lcdc_window_map_write(previous, new, wx, window_fetch_active, cgb_mode);
        self.record_window_startup_gap_window_map(previous, new, consumed, actual_next_x);
        let tile_data_fetch_phase = if window_fetch_active || self.window_active {
            self.next_x.saturating_sub(wx.saturating_sub(7)) & 0x07
        } else {
            scx.wrapping_add(self.next_x) & 0x07
        };
        self.record_lcdc_obj_size_write(previous, new, cgb_mode, dmg_compat);
        self.record_lcdc_tile_data_write(
            previous,
            new,
            tile_data_fetch_phase,
            window_fetch_active,
            cgb_mode && dmg_compat,
            waiting_on_obj_fetch,
        );
        // During the startup stall, the effective pre-stall fetch position can
        // be ahead of the visible renderer. Fill only uncovered pixels in that
        // gap so already-latched mixed bitplane ranges keep priority.
        if consumed > 0 && previous & LCDC_TILE_DATA != new & LCDC_TILE_DATA {
            let gap_start = actual_next_x;
            // self.next_x is eff_nx here (effective state still applied)
            let gap_end = self.next_x.saturating_sub(1);
            let mut start = gap_start;
            while start <= gap_end {
                if self.lcdc_tile_data_edge.ranges_cover(start) {
                    start = start.saturating_add(1);
                    continue;
                }
                let mut end = start;
                while end < gap_end && !self.lcdc_tile_data_edge.ranges_cover(end.saturating_add(1))
                {
                    end = end.saturating_add(1);
                }
                self.lcdc_tile_data_edge
                    .record_write(start, end, previous, previous);
                start = end.saturating_add(1);
            }
        }
        if let Some(model) = ObjFetchModel::for_dmg_render_path(cgb_mode, dmg_compat) {
            self.record_lcdc_obj_enable_write(model, previous, new);
        }
        if !cgb_mode || dmg_compat {
            let fetch_delay = self.lcdc_bg_map_fetch_delay(
                previous,
                new,
                cgb_mode,
                dmg_compat,
                waiting_on_obj_fetch,
            );
            self.lcdc_bg_map_edge.record_delayed_write(
                self.next_x,
                scx,
                previous,
                new,
                fetch_delay,
            );
        }
        let timing = if cgb_mode && dmg_compat {
            if waiting_on_obj_fetch && self.next_x == 0 && self.pending_obj_stall_dots == 1 {
                LcdcBgEnableEdgeTiming::CurrentPixelUsesPrevious
            } else if waiting_on_obj_fetch {
                LcdcBgEnableEdgeTiming::CurrentPixelUsesNew
            } else {
                LcdcBgEnableEdgeTiming::HoldPreviousForOneExtraPixel
            }
        } else if self.next_x == 0 || waiting_on_obj_fetch {
            LcdcBgEnableEdgeTiming::CurrentPixelUsesNew
        } else {
            LcdcBgEnableEdgeTiming::CurrentPixelUsesPrevious
        };
        self.lcdc_bg_enable_edge
            .record_write(self.next_x, previous, new, timing);
        if consumed > 0 {
            self.next_x = actual_next_x;
            self.pending_obj_stall_dots = actual_pend;
            self.window_active = actual_window_active;
        }
    }

    pub fn record_wx_write(&mut self, wx: u8, wy: u8) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }

        let reactivation_x = window_visible_start_x(wx);
        if !self.window_triggered
            && self.next_x == 0
            && self.scanline >= wy
            && self.scanline_start_lcdc & LCDC_WINDOW_ENABLE != 0
            // WX=0..6 starts fetching before the first visible dot; WX=7
            // still uses the regular visible trigger and should not survive
            // an immediate Mode 3 WX write as a hidden off-left fetch.
            && self.scanline_start_wx < 7
            && window_triggers_at_x(self.scanline_start_wx, 0)
        {
            let fetch_wx = if self.scanline_start_wx == 6 {
                7
            } else {
                self.scanline_start_wx
            };
            self.pending_hidden_window_fetch_wx = Some(fetch_wx);
        }
        if self.scanline_start_wx == 6 && self.next_x == 0 && wx > 6 && wx < 167 {
            self.off_left_window_reactivation_x = Some(reactivation_x);
        } else if self.scanline_start_wx == 6
            && self.off_left_window_reactivation_x.is_some()
            && !self.window_triggered
        {
            let old_reactivation_x = self.off_left_window_reactivation_x.unwrap();
            let same_fetch_decision_dot = old_reactivation_x == self.next_x
                || old_reactivation_x == self.next_x.saturating_add(1);
            if same_fetch_decision_dot && old_reactivation_x < OFF_LEFT_WX_OVERWRITE_CANCEL_X {
                // The off-left window fetch has already reached its reactivation
                // decision point; a same-dot WX overwrite cannot cancel it yet.
            } else if reactivation_x > self.next_x {
                self.off_left_window_reactivation_x = Some(reactivation_x);
            } else {
                self.off_left_window_reactivation_x = None;
                self.pending_hidden_window_fetch_wx = None;
            }
        }
        self.window_reactivation_edges.retain(|&x| x < self.next_x);
        if self.next_x == 0
            && self.scanline_start_wx < 6
            && reactivation_x > self.next_x
            // WX=4/5 off-left fetches are already one visible dot past their
            // original trigger phase when the ROM writes WX=LY at x=0. The
            // reactivation zero pixel appears only when the new trigger lands
            // on that next fetch decision phase.
            && reactivation_x & 0x07 == self.scanline_start_wx.wrapping_add(1) & 0x07
            && u32::from(reactivation_x) < ScreenBuffer::WIDTH
            && self.scanline >= wy
        {
            self.window_reactivation_edges.push(reactivation_x);
        }
        self.window_x_edges.push(WindowXEdge {
            start_x: self.next_x,
            wx,
        });
    }

    fn record_lcdc_window_enable_write(&mut self, previous: u8, new: u8, current_wx: u8) {
        if previous & LCDC_WINDOW_ENABLE == new & LCDC_WINDOW_ENABLE {
            return;
        }

        let enabled = new & LCDC_WINDOW_ENABLE != 0;
        let start_x = if enabled && !self.window_active && !self.window_triggered {
            self.next_x
        } else {
            // Once the window fetcher is active, enable transitions take effect
            // at fetch boundaries rather than at the CPU write pixel.
            self.next_window_fetch_boundary(current_wx)
        };
        if enabled
            && !self.window_active
            && !self.window_triggered
            && self.next_x == window_visible_start_x(current_wx)
        {
            // The WX trigger decision for this dot has already been missed; a
            // same-dot LCDC.5 enable can only start the window on the next dot.
            self.window_x_edges.push(WindowXEdge {
                start_x,
                wx: current_wx.saturating_add(1),
            });
        }
        self.window_enable_edges
            .push(WindowEnableEdge { start_x, enabled });
        if !enabled {
            if self.next_x == 0 && !self.window_triggered {
                self.pending_hidden_window_fetch_wx = None;
            }
            self.window_reset_edges.push(start_x);
        }
    }

    fn record_lcdc_window_map_write(
        &mut self,
        previous: u8,
        new: u8,
        wx: u8,
        window_fetch_active: bool,
        cgb_mode: bool,
    ) {
        if !window_fetch_active && !self.window_active {
            return;
        }

        if previous & LCDC_WINDOW_MAP == new & LCDC_WINDOW_MAP {
            return;
        }

        let boundary = LcdcWindowMapEdge::window_tile_boundary_at_or_after(self.next_x, wx);
        let mut start_x = self.next_x;
        let mut end_x = boundary.saturating_sub(1);
        let turning_on = previous & LCDC_WINDOW_MAP == 0 && new & LCDC_WINDOW_MAP != 0;
        let turning_off = previous & LCDC_WINDOW_MAP != 0 && new & LCDC_WINDOW_MAP == 0;
        let left_obj = self.leftmost_obj_oam_x;
        let delayed_by_left_edge_obj = left_obj.is_some_and(|oam_x| oam_x != 0);
        let delayed_by_visible_left_edge_obj = left_obj.is_some_and(|oam_x| oam_x >= 8);

        if turning_on && boundary == self.next_x {
            let delay_current_tile = delayed_by_visible_left_edge_obj
                || (cgb_mode && left_obj.is_some_and(|oam_x| (1..=2).contains(&oam_x)));
            if window_fetch_active && delay_current_tile {
                end_x = if delayed_by_visible_left_edge_obj {
                    OBJ_PIXELS_PER_FETCH * 2 - 1
                } else {
                    self.next_x.saturating_add(OBJ_PIXELS_PER_FETCH - 1)
                };
            } else {
                return;
            }
        }
        if turning_on && delayed_by_visible_left_edge_obj && self.next_x < OBJ_PIXELS_PER_FETCH {
            end_x = OBJ_PIXELS_PER_FETCH * 2 - 1;
        }
        if self.should_delay_steady_window_map_fetch(left_obj, turning_on, turning_off) {
            end_x = boundary.saturating_add(OBJ_PIXELS_PER_FETCH - 1);
        }

        let delay_restore_to_next_tile = if cgb_mode {
            left_obj.is_some_and(|oam_x| oam_x <= 2)
        } else {
            left_obj.is_some_and(|oam_x| (1..=2).contains(&oam_x))
        };
        if turning_off
            && delay_restore_to_next_tile
            && boundary <= OBJ_PIXELS_PER_FETCH
            && self.next_x <= OBJ_PIXELS_PER_FETCH
        {
            end_x = OBJ_PIXELS_PER_FETCH * 2 - 1;
            if cgb_mode && delayed_by_left_edge_obj {
                start_x = OBJ_PIXELS_PER_FETCH;
            }
        }
        if turning_off && left_obj.is_some_and(|oam_x| oam_x >= OBJ_PIXELS_PER_FETCH * 2) {
            start_x = OBJ_PIXELS_PER_FETCH * 2;
            end_x = start_x.saturating_add(OBJ_PIXELS_PER_FETCH - 1);
        }
        self.lcdc_window_map_edge
            .record_write(start_x, end_x, previous, new);
    }

    fn record_window_startup_gap_window_map(
        &mut self,
        previous: u8,
        new: u8,
        consumed: u8,
        actual_next_x: u8,
    ) {
        if consumed == 0 || previous & LCDC_WINDOW_MAP == new & LCDC_WINDOW_MAP {
            return;
        }

        let gap_start = actual_next_x;
        let gap_end = self.next_x.saturating_sub(1);
        let mut start = gap_start;
        while start <= gap_end {
            if self.lcdc_window_map_edge.ranges_cover(start) {
                start = start.saturating_add(1);
                continue;
            }
            let mut end = start;
            while end < gap_end
                && !self
                    .lcdc_window_map_edge
                    .ranges_cover(end.saturating_add(1))
            {
                end = end.saturating_add(1);
            }
            self.lcdc_window_map_edge
                .record_write(start, end, previous, new);
            start = end.saturating_add(1);
        }
    }

    fn should_delay_steady_window_map_fetch(
        &self,
        left_obj: Option<u8>,
        turning_on: bool,
        turning_off: bool,
    ) -> bool {
        if left_obj == Some(1) {
            return (turning_on && self.next_x >= LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_SET_X)
                || (turning_off && self.next_x >= LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_RESTORE_X);
        }

        self.next_x >= STEADY_WINDOW_MAP_FETCH_DELAY_X
    }

    fn next_window_fetch_boundary(&self, current_wx: u8) -> u8 {
        if !self.window_triggered && !self.window_active {
            return self.next_x;
        }

        let wx = if self.window_triggered {
            self.window_fetch_wx
        } else {
            current_wx
        };
        let start_x = window_visible_start_x(wx);
        if start_x == 0 && self.next_x != 0 && wx <= 7 {
            return if wx <= 1 {
                wx.saturating_add(9)
            } else {
                wx.saturating_add(1)
            };
        }

        let phase = self.next_x.saturating_sub(start_x) & 0x07;
        if phase == 0 {
            self.next_x.saturating_add(8)
        } else if phase == 7 {
            self.next_x.saturating_add(9)
        } else {
            self.next_x.saturating_add(8 - phase)
        }
    }

    fn record_lcdc_tile_data_write(
        &mut self,
        previous: u8,
        new: u8,
        bg_phase: u8,
        window_fetch_active: bool,
        cgb_dmg_compat: bool,
        waiting_on_obj_fetch: bool,
    ) {
        if previous & LCDC_TILE_DATA == new & LCDC_TILE_DATA {
            return;
        }

        let current_fetch_start = self.next_x.saturating_sub(bg_phase);
        let current_fetch_end = current_fetch_start.saturating_add(7);
        let next_fetch_start = current_fetch_start.saturating_add(8);
        if self.should_cancel_delayed_lcdc_tile_data_fetch(previous, new) {
            self.record_visible_left_edge_delayed_lcdc_tile_data_fetch(new);
        }

        if cgb_dmg_compat && !window_fetch_active {
            let handled = if previous & LCDC_TILE_DATA == 0 && new & LCDC_TILE_DATA != 0 {
                self.record_cgb_dmg_compat_tile_select_rising_edge(
                    current_fetch_start,
                    current_fetch_end,
                    next_fetch_start,
                    previous,
                    new,
                    bg_phase,
                    waiting_on_obj_fetch,
                )
            } else {
                self.record_cgb_dmg_compat_tile_select_falling_edge(
                    current_fetch_start,
                    current_fetch_end,
                    next_fetch_start,
                    previous,
                    new,
                    bg_phase,
                    waiting_on_obj_fetch,
                )
            };
            if handled {
                return;
            }
        }

        if !cgb_dmg_compat
            && self.should_restore_lcdc_tile_data_at_window_obj_delayed_boundary(
                previous, new, bg_phase,
            )
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                new,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if !cgb_dmg_compat
            && self.should_restore_lcdc_tile_data_at_window_obj_stalled_boundary(
                previous, new, bg_phase,
            )
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                new,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
            );
        } else if !window_fetch_active
            && self.should_restore_lcdc_tile_data_at_obj_stalled_boundary(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                previous,
            );
        } else if !(window_fetch_active || cgb_dmg_compat && self.window_active)
            && self.should_restore_lcdc_tile_data_in_delayed_fetch(previous, new)
        {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.record_next_lcdc_tile_data_write(next_fetch_start, bg_phase, previous, new);
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_phase4_window_tile_data(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write_with_overrides(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
                DmgTileByteOverride::CachedData,
                DmgTileByteOverride::None,
            );
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_phase6_window_tile_data(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write_with_overrides(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
                DmgTileByteOverride::None,
                DmgTileByteOverride::CachedData,
            );
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_later_window_tile_data(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_tile_data_at_early_window_obj(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                new,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if cgb_dmg_compat
            && self
                .should_set_cgb_dmg_compat_tile_data_after_window_start_obj(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_tile_data_at_visible_window_obj_x8(
                previous, new, bg_phase,
            )
        {
            let high_override = if self.scanline & 0x07 == 0 {
                DmgTileByteOverride::Data(0xFF)
            } else {
                DmgTileByteOverride::CachedData
            };
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write_with_overrides(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
                DmgTileByteOverride::None,
                high_override,
            );
        } else if cgb_dmg_compat
            && self.should_set_cgb_dmg_compat_tile_data_at_late_visible_window_obj(
                previous, new, bg_phase,
            )
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                previous,
            );
        } else if cgb_dmg_compat
            && self.should_ignore_cgb_dmg_compat_visible_window_obj_set(previous, new)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                previous,
            );
        } else if window_fetch_active
            && !cgb_dmg_compat
            && self.should_set_lcdc_tile_data_at_visible_window_obj_x8(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
            );
        } else if window_fetch_active
            && !cgb_dmg_compat
            && self.should_set_lcdc_tile_data_at_off_left_window_start(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                new,
            );
        } else if !cgb_dmg_compat
            && self.should_set_lcdc_tile_data_in_delayed_window_obj_fetch(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
            );
        } else if !cgb_dmg_compat
            && self.should_set_lcdc_tile_data_at_window_obj_late_stall(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
            );
            self.record_visible_left_edge_delayed_lcdc_tile_data_fetch(new);
        } else if !cgb_dmg_compat
            && self
                .should_set_lcdc_tile_data_at_window_obj_stalled_boundary(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                new,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if !cgb_dmg_compat
            && self.should_set_lcdc_tile_data_after_off_left_window_obj_boundary(
                previous, new, bg_phase,
            )
        {
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                previous,
            );
        } else if !(window_fetch_active || cgb_dmg_compat && self.window_active)
            && self.should_delay_lcdc_tile_data_by_extra_fetch(previous, new)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                previous,
            );
            self.record_visible_left_edge_delayed_lcdc_tile_data_fetch(new);
        } else if cgb_dmg_compat
            && self.should_restore_cgb_dmg_compat_phase4_window_tile_data(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write_with_overrides(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
                DmgTileByteOverride::TileIndexAndCache { lcdc: previous },
                DmgTileByteOverride::None,
            );
        } else if cgb_dmg_compat
            && self.should_restore_cgb_dmg_compat_phase6_window_tile_data(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write_with_overrides(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
                DmgTileByteOverride::None,
                DmgTileByteOverride::TileIndexAndCache { lcdc: previous },
            );
        } else if cgb_dmg_compat
            && (self.should_restore_cgb_dmg_compat_later_window_tile_data(previous, new, bg_phase)
                || self.should_restore_cgb_dmg_compat_tile_data_after_early_window_obj(
                    previous, new, bg_phase,
                ))
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if cgb_dmg_compat
            && self.should_restore_cgb_dmg_compat_tile_data_at_late_visible_window_obj(
                previous, new, bg_phase,
            )
        {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                previous,
                new,
            );
        } else if cgb_dmg_compat
            && self.should_ignore_cgb_dmg_compat_visible_window_obj_restore(previous, new)
        {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if self
            .should_restore_lcdc_tile_data_at_off_left_window_boundary(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if self.should_restore_lcdc_tile_data_at_window_boundary(previous, new, bg_phase) {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                new,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if self.should_restore_lcdc_tile_data_after_window_low_byte(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                previous,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if self.should_set_lcdc_tile_data_after_window_boundary(previous, new, bg_phase) {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if self.should_set_lcdc_tile_data_after_window_low_byte(previous, new, bg_phase) {
            self.lcdc_tile_data_edge.record_write(
                current_fetch_start,
                current_fetch_end,
                previous,
                new,
            );
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                new,
            );
        } else if !cgb_dmg_compat
            && self
                .should_restore_lcdc_tile_data_after_window_left_edge_obj(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                previous,
            );
        } else if self.next_x == current_fetch_start {
            if window_fetch_active {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    new,
                    new,
                );
            } else {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    new,
                );
            }
        } else if !self
            .lcdc_tile_data_edge
            .has_latched_range(current_fetch_start, current_fetch_end)
        {
            if !self
                .lcdc_tile_data_edge
                .has_range(current_fetch_start, current_fetch_end)
            {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
            }
            self.record_next_lcdc_tile_data_write(next_fetch_start, bg_phase, previous, new);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_cgb_dmg_compat_tile_select_falling_edge(
        &mut self,
        current_fetch_start: u8,
        current_fetch_end: u8,
        next_fetch_start: u8,
        previous: u8,
        new: u8,
        bg_phase: u8,
        waiting_on_obj_fetch: bool,
    ) -> bool {
        if self.window_active {
            if bg_phase == 7 && self.leftmost_obj_starts_before_screen() {
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    new,
                );
                return true;
            }
            return false;
        }

        let previous_tile_select = previous & LCDC_TILE_DATA;
        let new_tile_select = new & LCDC_TILE_DATA;
        let current_bits = self
            .lcdc_tile_data_edge
            .tile_select_bits_for_range(current_fetch_start, current_fetch_end);
        let current_sampled_previous =
            current_bits == Some((previous_tile_select, previous_tile_select));
        let current_sampled_new = current_bits == Some((new_tile_select, new_tile_select));
        let current_sampled_new_low_previous_high =
            current_bits == Some((new_tile_select, previous_tile_select));

        match (waiting_on_obj_fetch, bg_phase) {
            (false, 0) if current_sampled_previous => {
                if current_fetch_start == 8 {
                    self.lcdc_tile_data_edge.record_write_with_overrides(
                        current_fetch_start,
                        current_fetch_end,
                        new,
                        previous,
                        DmgTileByteOverride::TileIndexAndCache { lcdc: previous },
                        DmgTileByteOverride::None,
                    );
                }
                self.lcdc_tile_data_edge.record_write_with_overrides(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    new,
                    DmgTileByteOverride::TileIndexAndCache { lcdc: previous },
                    DmgTileByteOverride::None,
                );
                true
            }
            (false, 0) if current_sampled_new => {
                if self.leftmost_obj_oam_x == Some(3) {
                    self.lcdc_tile_data_edge.record_write(
                        current_fetch_start,
                        current_fetch_end,
                        new,
                        previous,
                    );
                }
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    new,
                );
                true
            }
            (false, 0) if current_sampled_new_low_previous_high => {
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    new,
                );
                true
            }
            (false, 1) if current_sampled_previous => {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    new,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    new,
                );
                true
            }
            (false, 1) if current_sampled_new => {
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    new,
                );
                true
            }
            (false, 1) if current_sampled_new_low_previous_high => {
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    new,
                );
                true
            }
            (false, 2) => {
                self.lcdc_tile_data_edge.record_write_with_overrides(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    new,
                    DmgTileByteOverride::None,
                    DmgTileByteOverride::TileIndexAndCache { lcdc: previous },
                );
                true
            }
            (true, 0) if current_sampled_new => {
                let (low_lcdc, high_lcdc) = if self.scanline & 0x07 == 0 {
                    (previous, previous)
                } else {
                    (new, previous)
                };
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    low_lcdc,
                    high_lcdc,
                );
                true
            }
            (true, 1) if current_sampled_new => {
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    previous,
                );
                true
            }
            (false, 3..=7) => {
                if self.window_active && bg_phase == 7 {
                    self.lcdc_tile_data_edge.record_write(
                        next_fetch_start,
                        next_fetch_start.saturating_add(7),
                        previous,
                        new,
                    );
                    return true;
                }
                let next_lcdc = if self.window_active
                    || self.leftmost_obj_oam_x.is_some_and(|oam_x| oam_x >= 8)
                {
                    new
                } else {
                    previous
                };
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    next_lcdc,
                    next_lcdc,
                );
                true
            }
            _ => false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_cgb_dmg_compat_tile_select_rising_edge(
        &mut self,
        current_fetch_start: u8,
        current_fetch_end: u8,
        next_fetch_start: u8,
        previous: u8,
        new: u8,
        bg_phase: u8,
        waiting_on_obj_fetch: bool,
    ) -> bool {
        if self.window_active {
            return false;
        }

        if waiting_on_obj_fetch {
            return false;
        }

        match bg_phase {
            0 => {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write_with_overrides(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    new,
                    new,
                    DmgTileByteOverride::CachedData,
                    DmgTileByteOverride::None,
                );
                true
            }
            1 => {
                let next_low_lcdc = if self.leftmost_obj_oam_x.is_some_and(|oam_x| oam_x >= 4) {
                    previous
                } else {
                    new
                };
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    next_low_lcdc,
                    new,
                );
                true
            }
            2 => {
                let next_lcdc = if current_fetch_start == 0 {
                    previous
                } else {
                    new
                };
                let high_override = if next_lcdc & LCDC_TILE_DATA != previous & LCDC_TILE_DATA {
                    DmgTileByteOverride::CachedData
                } else {
                    DmgTileByteOverride::None
                };
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write_with_overrides(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    next_lcdc,
                    DmgTileByteOverride::None,
                    high_override,
                );
                true
            }
            3..=7 => {
                self.lcdc_tile_data_edge.record_write(
                    current_fetch_start,
                    current_fetch_end,
                    previous,
                    previous,
                );
                self.lcdc_tile_data_edge.record_write(
                    next_fetch_start,
                    next_fetch_start.saturating_add(7),
                    previous,
                    previous,
                );
                true
            }
            _ => false,
        }
    }

    fn should_delay_lcdc_tile_data_by_extra_fetch(&self, previous_lcdc: u8, new_lcdc: u8) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self
                .left_edge_obj_tile_data_delay_start_x()
                .is_some_and(|start_x| self.next_x == start_x)
    }

    fn should_cancel_delayed_lcdc_tile_data_fetch(&self, previous_lcdc: u8, new_lcdc: u8) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.left_edge_obj_tile_data_delay_start_x().is_some()
            && self.next_x < 8
    }

    fn should_restore_lcdc_tile_data_in_delayed_fetch(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.left_edge_obj_tile_data_delay_start_x().is_some()
            && (OBJ_PIXELS_PER_FETCH..OBJ_PIXELS_PER_FETCH * 2).contains(&self.next_x)
    }

    fn should_set_cgb_dmg_compat_later_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| oam_x == 1 || (oam_x == 5 && bg_phase <= 4))
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && (2..=7).contains(&bg_phase)
    }

    fn should_set_cgb_dmg_compat_phase4_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(5)
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && bg_phase == 4
    }

    fn should_set_cgb_dmg_compat_phase6_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(5)
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && bg_phase == 6
    }

    fn should_restore_cgb_dmg_compat_later_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| oam_x == 1 || (oam_x == 5 && bg_phase <= 4))
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && (2..=7).contains(&bg_phase)
    }

    fn should_restore_cgb_dmg_compat_phase4_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self.leftmost_obj_oam_x == Some(5)
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && bg_phase == 4
    }

    fn should_restore_cgb_dmg_compat_phase6_window_tile_data(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self.leftmost_obj_oam_x == Some(5)
            && self.next_x.saturating_sub(bg_phase) >= OBJ_PIXELS_PER_FETCH
            && bg_phase == 6
    }

    fn should_restore_lcdc_tile_data_at_obj_stalled_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && !self.window_active
            && bg_phase == 0
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH * 2)
            && self.next_x == OBJ_PIXELS_PER_FETCH
            && self.pending_obj_stall_dots > 0
    }

    fn should_restore_lcdc_tile_data_at_window_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && bg_phase <= 1
            && self.next_x == OBJ_PIXELS_PER_FETCH.saturating_add(bg_phase)
    }

    fn should_restore_lcdc_tile_data_at_off_left_window_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(is_off_left_window_delayed_obj_x)
            && self.next_x == OBJ_PIXELS_PER_FETCH
            && bg_phase == 0
    }

    fn should_restore_cgb_dmg_compat_tile_data_after_early_window_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self.leftmost_obj_oam_x == Some(4)
            && self.next_x == OBJ_PIXELS_PER_FETCH + 1
            && bg_phase == 1
    }

    fn should_restore_cgb_dmg_compat_tile_data_at_late_visible_window_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| (16..=17).contains(&oam_x))
            && (OBJ_PIXELS_PER_FETCH..=OBJ_PIXELS_PER_FETCH + 1).contains(&self.next_x)
            && bg_phase <= 1
    }

    fn should_ignore_cgb_dmg_compat_visible_window_obj_restore(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| (9..=15).contains(&oam_x))
    }

    fn should_restore_lcdc_tile_data_after_window_low_byte(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        tile_data_turning_off
            && self.window_active
            && (2..=3).contains(&bg_phase)
            && self.next_x == OBJ_PIXELS_PER_FETCH.saturating_add(bg_phase)
    }

    fn should_set_lcdc_tile_data_after_window_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on && self.window_active && bg_phase == 1 && self.next_x == bg_phase
    }

    fn should_set_lcdc_tile_data_after_window_low_byte(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on && self.window_active && bg_phase == 2 && self.next_x == bg_phase
    }

    fn should_set_lcdc_tile_data_at_window_obj_late_stall(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH + 7)
            && self.next_x == OBJ_PIXELS_PER_FETCH - 1
            && bg_phase == OBJ_PIXELS_PER_FETCH - 1
            && self.pending_obj_stall_dots > 0
    }

    fn should_set_cgb_dmg_compat_tile_data_at_early_window_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        let early_window_obj_phase = self
            .leftmost_obj_oam_x
            .filter(|&oam_x| (3..=4).contains(&oam_x))
            .map(|oam_x| oam_x - 3);
        tile_data_turning_on
            && (self.window_active || self.next_x == 0)
            && early_window_obj_phase.is_some_and(|phase| self.next_x == phase && bg_phase == phase)
    }

    fn should_set_cgb_dmg_compat_tile_data_after_window_start_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.next_x == 0
            && bg_phase == 0
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| (5..=7).contains(&oam_x))
    }

    fn should_set_cgb_dmg_compat_tile_data_at_visible_window_obj_x8(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH)
            && self.next_x == 0
            && bg_phase == 0
    }

    fn should_set_lcdc_tile_data_at_visible_window_obj_x8(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH)
            && self.next_x == 0
            && bg_phase == 0
            && self.pending_obj_stall_dots > 0
    }

    fn should_set_cgb_dmg_compat_tile_data_at_late_visible_window_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| (16..=17).contains(&oam_x))
            && self.next_x == OBJ_PIXELS_PER_FETCH
            && bg_phase == 0
    }

    fn should_ignore_cgb_dmg_compat_visible_window_obj_set(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self
                .leftmost_obj_oam_x
                .is_some_and(|oam_x| (9..=15).contains(&oam_x))
    }

    fn should_set_lcdc_tile_data_at_window_obj_stalled_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH * 2 + 1)
            && self.next_x == OBJ_PIXELS_PER_FETCH
            && bg_phase == 0
            && self.pending_obj_stall_dots == 0
    }

    fn should_set_lcdc_tile_data_at_off_left_window_start(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self
                .leftmost_obj_oam_x
                .is_some_and(is_off_left_window_delayed_obj_x)
            && self.next_x == 0
            && bg_phase == 0
    }

    fn should_set_lcdc_tile_data_in_delayed_window_obj_fetch(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        let delayed_fetch_phase = self
            .leftmost_obj_oam_x
            .filter(|&oam_x| (OBJ_PIXELS_PER_FETCH + 4..=OBJ_PIXELS_PER_FETCH + 6).contains(&oam_x))
            .map(|oam_x| oam_x - OBJ_PIXELS_PER_FETCH);
        tile_data_turning_on
            && self.window_active
            && delayed_fetch_phase.is_some_and(|phase| self.next_x == phase && bg_phase == phase)
            && self.pending_obj_stall_dots > 0
    }

    fn should_set_lcdc_tile_data_after_off_left_window_obj_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH / 2)
            && self.next_x == 1
            && bg_phase == 1
    }

    fn should_restore_lcdc_tile_data_after_window_left_edge_obj(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        let near_left_restore_phase = self
            .leftmost_obj_oam_x
            .filter(|&oam_x| (OBJ_PIXELS_PER_FETCH..=OBJ_PIXELS_PER_FETCH + 2).contains(&oam_x))
            .map(|oam_x| oam_x - 3);
        tile_data_turning_off
            && self.window_active
            && near_left_restore_phase
                .is_some_and(|phase| self.next_x == phase && bg_phase == phase)
    }

    fn should_restore_lcdc_tile_data_at_window_obj_delayed_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        let delayed_boundary_phase = self
            .leftmost_obj_oam_x
            .filter(|&oam_x| (OBJ_PIXELS_PER_FETCH + 3..=OBJ_PIXELS_PER_FETCH + 7).contains(&oam_x))
            .map(|oam_x| (oam_x - (OBJ_PIXELS_PER_FETCH + 3)).min(2));
        tile_data_turning_off
            && self.window_active
            && delayed_boundary_phase.is_some_and(|phase| {
                self.next_x == OBJ_PIXELS_PER_FETCH.saturating_add(phase) && bg_phase == phase
            })
    }

    fn should_restore_lcdc_tile_data_at_window_obj_stalled_boundary(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_off =
            previous_lcdc & LCDC_TILE_DATA != 0 && new_lcdc & LCDC_TILE_DATA == 0;
        let stalled_boundary_phase = self
            .leftmost_obj_oam_x
            .filter(|&oam_x| {
                (OBJ_PIXELS_PER_FETCH * 2..=OBJ_PIXELS_PER_FETCH * 2 + 1).contains(&oam_x)
            })
            .map(|oam_x| oam_x - OBJ_PIXELS_PER_FETCH * 2);
        tile_data_turning_off
            && self.window_active
            && stalled_boundary_phase.is_some_and(|phase| {
                self.next_x == OBJ_PIXELS_PER_FETCH.saturating_add(phase) && bg_phase == phase
            })
            && self.pending_obj_stall_dots > 0
    }

    fn left_edge_obj_tile_data_delay_start_x(&self) -> Option<u8> {
        self.leftmost_obj_oam_x
            .filter(|&oam_x| (8..=15).contains(&oam_x))
            .map(|oam_x| oam_x - 8)
    }

    fn record_visible_left_edge_delayed_lcdc_tile_data_fetch(&mut self, lcdc: u8) {
        let delayed_fetch_start = OBJ_PIXELS_PER_FETCH * 2;
        self.lcdc_tile_data_edge.record_write(
            delayed_fetch_start,
            delayed_fetch_start.saturating_add(OBJ_PIXELS_PER_FETCH - 1),
            lcdc,
            lcdc,
        );
    }

    fn record_next_lcdc_tile_data_write(
        &mut self,
        next_fetch_start: u8,
        bg_phase: u8,
        previous: u8,
        new: u8,
    ) {
        let (low_lcdc, high_lcdc) = if bg_phase <= 1 {
            (new, new)
        } else {
            (previous, new)
        };
        self.lcdc_tile_data_edge.record_write(
            next_fetch_start,
            next_fetch_start.saturating_add(7),
            low_lcdc,
            high_lcdc,
        );
    }

    fn record_lcdc_obj_enable_write(&mut self, model: ObjFetchModel, previous: u8, new: u8) {
        let obj_turning_off = previous & LCDC_OBJ_ENABLE != 0 && new & LCDC_OBJ_ENABLE == 0;
        let waiting_on_obj_fetch = self.is_waiting_on_obj_fetch();
        if model == ObjFetchModel::CgbDmgCompat
            && (!waiting_on_obj_fetch
                || self.pending_obj_stall_dots <= DMG_OBJ_FETCH_ABORT_COMPLETES_WHEN_DOTS_REMAINING)
        {
            self.lcdc_obj_enable_edge.record_write(
                self.next_x,
                self.next_x
                    .saturating_add(CGB_DMG_COMPAT_OBJ_ENABLE_EDGE_PIXELS - 1),
                previous,
                new,
            );
        } else if model == ObjFetchModel::Dmg && self.next_x != 0 && !waiting_on_obj_fetch {
            self.lcdc_obj_enable_edge
                .record_write(self.next_x, self.next_x, previous, new);
        }
        if model != ObjFetchModel::Dmg || !obj_turning_off || !self.is_waiting_on_obj_fetch() {
            return;
        }

        if let Some(fetch_x) = self.active_obj_stall_x {
            self.cancel_obj_fetch_at(fetch_x);
            if self.pending_obj_stall_dots > DMG_OBJ_FETCH_ABORT_COMPLETES_WHEN_DOTS_REMAINING {
                self.pending_obj_stall_dots = 0;
                self.active_obj_stall_x = None;
            }
        } else if let Some(event) = self
            .obj_stall_events
            .get(self.next_obj_stall_event)
            .copied()
            && event.x <= self.next_x
        {
            self.cancel_obj_fetch_at(event.x);
        }
    }

    fn cancel_obj_fetch_at(&mut self, x: u8) {
        self.canceled_obj_fetch_ranges
            .push((x, x.saturating_add(OBJ_PIXELS_PER_FETCH - 1)));
    }

    fn record_lcdc_obj_size_write(
        &mut self,
        previous: u8,
        new: u8,
        cgb_mode: bool,
        dmg_compat: bool,
    ) {
        if previous & 0x04 == new & 0x04 {
            return;
        }

        let Some(range_index) = self.active_obj_fetch_lcdc_range else {
            return;
        };
        let leftmost_obj_starts_before_screen = self.leftmost_obj_starts_before_screen();
        let Some(range) = self.obj_fetch_lcdc_ranges.get_mut(range_index) else {
            return;
        };
        // The first off-left OBJ fetch is sampled before visible Mode 3 writes.
        if range.start_x == 0 && leftmost_obj_starts_before_screen {
            return;
        }

        let (low_threshold, high_threshold) = if cgb_mode && dmg_compat {
            (
                CGB_DMG_COMPAT_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING,
                CGB_DMG_COMPAT_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING,
            )
        } else {
            (
                DMG_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING,
                DMG_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING,
            )
        };
        if self.pending_obj_stall_dots >= high_threshold {
            range.high_lcdc = new;
        }
        if self.pending_obj_stall_dots >= low_threshold {
            range.low_lcdc = new;
        }
    }

    fn lcdc_bg_map_fetch_delay(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        cgb_mode: bool,
        dmg_compat: bool,
        waiting_on_obj_fetch: bool,
    ) -> LcdcBgMapFetchDelay {
        if self.should_delay_lcdc_bg_map_by_extra_fetch(
            previous_lcdc,
            new_lcdc,
            cgb_mode,
            dmg_compat,
            waiting_on_obj_fetch,
        ) {
            LcdcBgMapFetchDelay::OneBgFetchLater
        } else {
            LcdcBgMapFetchDelay::NextBgFetch
        }
    }

    fn should_delay_lcdc_bg_map_by_extra_fetch(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        cgb_mode: bool,
        dmg_compat: bool,
        waiting_on_obj_fetch: bool,
    ) -> bool {
        let bg_map_turning_on = previous_lcdc & LCDC_BG_MAP == 0 && new_lcdc & LCDC_BG_MAP != 0;
        let bg_map_turning_off = previous_lcdc & LCDC_BG_MAP != 0 && new_lcdc & LCDC_BG_MAP == 0;
        let visible_left_edge_obj = self.leftmost_obj_oam_x == Some(8);
        let cgb_dmg_compat = cgb_mode && dmg_compat;

        // Mealybug's DMG-compat references show these edge phases keep the previous
        // BG map latched through one additional 8-pixel fetch.
        (bg_map_turning_on
            && (visible_left_edge_obj
                || self.cgb_dmg_compat_delays_bg_map_start(cgb_dmg_compat, waiting_on_obj_fetch)))
            || (bg_map_turning_off
                && self.cgb_dmg_compat_delays_bg_map_restore(cgb_dmg_compat, waiting_on_obj_fetch))
    }

    fn cgb_dmg_compat_delays_bg_map_start(
        &self,
        cgb_dmg_compat: bool,
        waiting_on_obj_fetch: bool,
    ) -> bool {
        if !cgb_dmg_compat {
            return false;
        }

        let off_left_edge_obj = self.leftmost_obj_oam_x == Some(0);
        (self.next_x == 0 && !off_left_edge_obj)
            || self.is_cgb_dmg_compat_no_obj_tile_phase(waiting_on_obj_fetch)
    }

    fn cgb_dmg_compat_delays_bg_map_restore(
        &self,
        cgb_dmg_compat: bool,
        waiting_on_obj_fetch: bool,
    ) -> bool {
        cgb_dmg_compat
            && ((matches!(self.next_x, 7 | 8) && !waiting_on_obj_fetch)
                || self.is_cgb_dmg_compat_no_obj_tile_phase(waiting_on_obj_fetch))
    }

    fn is_cgb_dmg_compat_no_obj_tile_phase(&self, waiting_on_obj_fetch: bool) -> bool {
        !waiting_on_obj_fetch && matches!(self.next_x & 0x07, 0 | 7)
    }

    fn render_dmg_pixel(
        &mut self,
        x: u32,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        window_line: u8,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let (colour_index, is_sprite, sprite_palette) =
            self.dmg_pixel_layers(x, vram, oam, registers, window_line);
        let palette = if is_sprite {
            if sprite_palette == 0 {
                registers.obp0
            } else {
                registers.obp1
            }
        } else {
            self.bgp_for_pixel(x, registers.bgp)
        };
        let mapped_index = dmg_palette_index(palette, colour_index);
        let grey = rendering::dmg_grey(mapped_index);
        screen_buffer.set_pixel(x, u32::from(self.scanline), grey, grey, grey);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cgb_dmg_compat_pixel(
        &mut self,
        x: u32,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        bg_palette_ram: &[u8; 64],
        obj_palette_ram: &[u8; 64],
        window_line: u8,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let (colour_index, is_sprite, sprite_palette) =
            self.dmg_pixel_layers_with_options(x, vram, oam, registers, window_line, false);
        let (r, g, b) = if is_sprite {
            let palette_reg = if sprite_palette == 0 {
                self.obp0_for_pixel(x, registers.obp0)
            } else {
                self.obp1_for_pixel(x, registers.obp1)
            };
            let mapped_index = dmg_palette_index(palette_reg, colour_index);
            cgb_palette_lookup(obj_palette_ram, sprite_palette, mapped_index)
        } else {
            let mapped_index =
                dmg_palette_index(self.bgp_for_pixel(x, registers.bgp), colour_index);
            cgb_palette_lookup(bg_palette_ram, 0, mapped_index)
        };
        screen_buffer.set_pixel(x, u32::from(self.scanline), r, g, b);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_cgb_pixel(
        &mut self,
        x: u32,
        vram: &[u8; 0x2000],
        vram_bank1: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        bg_palette_ram: &[u8; 64],
        obj_palette_ram: &[u8; 64],
        window_line: u8,
        opri_dmg_mode: bool,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let lcdc = registers.lcdc;
        let obj_enabled = lcdc & LCDC_OBJ_ENABLE != 0;
        let master_priority = lcdc & 0x01 != 0;

        let bg_px = background::fetch_bg_pixel_cgb_with_fine_scx(
            vram,
            vram_bank1,
            background::CgbPixelFetch {
                x,
                scanline: self.scanline,
                scx: self.bg_fetch_scx,
                fine_scx: self.scanline_start_scx_low,
                scy_map: self.bg_fetch_scy_map,
                scy_data_low: self.bg_fetch_scy_data_low,
                scy_data_high: self.bg_fetch_scy_data_high,
                lcdc,
            },
        );
        let window_fetch = self.window_fetch_for_pixel(x, registers, window_line);
        let bw_px = if let Some((window_wx, window_line)) = window_fetch {
            let window_lcdc = self.lcdc_window_map_edge.lcdc_for_window_fetch(x, lcdc);
            let window_sample_x = self.window_sample_x_for_pixel(x);
            match window::fetch_window_pixel_cgb(
                window_sample_x,
                self.scanline,
                vram,
                vram_bank1,
                window_lcdc,
                window_wx,
                registers.wy,
                window_line,
            ) {
                Some(mut win_px) => {
                    if self.window_reactivation_zero_for_pixel(x) {
                        win_px.colour_index = 0;
                        win_px.palette_num = 0;
                        win_px.vram_bank = 0;
                        win_px.bg_priority = false;
                    }
                    win_px
                }
                None => bg_px,
            }
        } else {
            self.window_triggered = false;
            bg_px
        };

        let sprite_px = if obj_enabled {
            sprites::fetch_sprite_pixel_cgb(
                x,
                self.scanline,
                &self.sprite_indices,
                oam,
                vram,
                vram_bank1,
                lcdc,
                opri_dmg_mode,
            )
        } else {
            None
        };

        let (r, g, b) = if let Some(sp) = sprite_px {
            let bg_wins =
                bw_px.colour_index != 0 && master_priority && (bw_px.bg_priority || sp.bg_priority);
            if bg_wins {
                cgb_palette_lookup(bg_palette_ram, bw_px.palette_num, bw_px.colour_index)
            } else {
                cgb_palette_lookup(obj_palette_ram, sp.cgb_palette, sp.colour_index)
            }
        } else {
            cgb_palette_lookup(bg_palette_ram, bw_px.palette_num, bw_px.colour_index)
        };

        screen_buffer.set_pixel(x, u32::from(self.scanline), r, g, b);
    }

    fn dmg_pixel_layers(
        &mut self,
        x: u32,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        window_line: u8,
    ) -> (u8, bool, u8) {
        self.dmg_pixel_layers_with_options(x, vram, oam, registers, window_line, true)
    }

    fn resolve_cgb_dmg_tile_sel_overrides(
        &mut self,
        low_override: DmgTileByteOverride,
        high_override: DmgTileByteOverride,
        oam: &[u8; 0xA0],
        vram: &[u8; 0x2000],
        registers: &Registers,
    ) -> (DmgTileByteOverride, DmgTileByteOverride) {
        if !matches!(low_override, DmgTileByteOverride::CachedData)
            && !matches!(high_override, DmgTileByteOverride::CachedData)
        {
            return (low_override, high_override);
        }

        let cached_data = self.cgb_dmg_tile_sel_glitch_data(oam, vram, registers);
        (
            Self::resolve_cgb_dmg_tile_sel_override(low_override, cached_data),
            Self::resolve_cgb_dmg_tile_sel_override(high_override, cached_data),
        )
    }

    fn resolve_cgb_dmg_tile_sel_override(
        override_mode: DmgTileByteOverride,
        cached_data: u8,
    ) -> DmgTileByteOverride {
        match override_mode {
            DmgTileByteOverride::CachedData => DmgTileByteOverride::Data(cached_data),
            DmgTileByteOverride::None
            | DmgTileByteOverride::TileIndex
            | DmgTileByteOverride::TileIndexAndCache { .. }
            | DmgTileByteOverride::Data(_) => override_mode,
        }
    }

    fn cgb_dmg_tile_sel_glitch_data(
        &mut self,
        oam: &[u8; 0xA0],
        vram: &[u8; 0x2000],
        registers: &Registers,
    ) -> u8 {
        if let Some(data) = self.cgb_dmg_tile_sel_glitch_data {
            return data;
        }

        let data = self
            .initial_cgb_dmg_tile_sel_glitch_data(oam, vram, registers)
            .unwrap_or(0);
        self.cgb_dmg_tile_sel_glitch_data = Some(data);
        data
    }

    fn initial_cgb_dmg_tile_sel_glitch_data(
        &self,
        oam: &[u8; 0xA0],
        vram: &[u8; 0x2000],
        registers: &Registers,
    ) -> Option<u8> {
        self.sprite_indices
            .iter()
            .copied()
            .filter(|&index| oam[index * 4 + 1] <= 16)
            .max_by_key(|&index| (oam[index * 4 + 1], index))
            .map(|index| {
                sprites::sprite_tile_row_high_byte(self.scanline, index, oam, vram, registers.lcdc)
            })
    }

    fn update_cgb_dmg_tile_sel_glitch_data(&mut self, pixel: bg_fifo::DmgFetchedPixel) {
        if let Some(data) = pixel.low_cache_data {
            self.cgb_dmg_tile_sel_glitch_data = Some(data);
        }
        if let Some(data) = pixel.high_cache_data {
            self.cgb_dmg_tile_sel_glitch_data = Some(data);
        }
    }

    fn window_first_pixel_tile_select_toggle_changes_colour(
        &self,
        current_colour: u8,
        x: u32,
        vram: &[u8; 0x2000],
        registers: &Registers,
    ) -> bool {
        if x != 0 || !self.window_active || self.leftmost_obj_oam_x != Some(4) {
            return false;
        }

        let toggled_lcdc = registers.lcdc ^ LCDC_TILE_DATA;
        bg_fifo::fetch_dmg_pixel_with_data(
            vram,
            DmgPixelFetch {
                layer: DmgLayer::Window,
                x,
                scanline: self.scanline,
                scx: registers.scx,
                fine_scx: registers.scx & SCX_FINE_MASK,
                scy_map: registers.scy,
                scy_data_low: registers.scy,
                scy_data_high: registers.scy,
                wx: self.window_fetch_wx,
                wy: registers.wy,
                window_line: self.window_fetch_line,
                map_lcdc: registers.lcdc | LCDC_WINDOW_ENABLE,
                low_lcdc: toggled_lcdc,
                high_lcdc: toggled_lcdc,
                low_override: DmgTileByteOverride::None,
                high_override: DmgTileByteOverride::None,
            },
        )
        .is_some_and(|pixel| pixel.colour_index != current_colour)
    }

    fn dmg_pixel_layers_with_options(
        &mut self,
        x: u32,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        window_line: u8,
        dmg_window_glitch: bool,
    ) -> (u8, bool, u8) {
        let lcdc = registers.lcdc;
        let bg_fetch_lcdc = self.lcdc_bg_map_edge.lcdc_for_bg_fetch(x, lcdc);
        let bg_window_enabled = self
            .lcdc_bg_enable_edge
            .bg_window_enabled_for_pixel(x, lcdc);
        let obj_enabled = self.obj_enabled_for_pixel(lcdc);
        let window_fetch = self.window_fetch_for_pixel(x, registers, window_line);

        let bg_idx = if bg_window_enabled {
            let bg_x = x.saturating_sub(u32::from(self.bg_fetch_x_offset));
            let (low_lcdc, high_lcdc, low_override, high_override) = self
                .lcdc_tile_data_edge
                .lcdc_for_tile_data(x, bg_fetch_lcdc);
            let (low_override, high_override) = self.resolve_cgb_dmg_tile_sel_overrides(
                low_override,
                high_override,
                oam,
                vram,
                registers,
            );
            match bg_fifo::fetch_dmg_pixel_with_data(
                vram,
                DmgPixelFetch {
                    layer: DmgLayer::Background,
                    x: bg_x,
                    scanline: self.scanline,
                    scx: self.bg_fetch_scx,
                    fine_scx: self.scanline_start_scx_low,
                    scy_map: self.bg_fetch_scy_map,
                    scy_data_low: self.bg_fetch_scy_data_low,
                    scy_data_high: self.bg_fetch_scy_data_high,
                    wx: registers.wx,
                    wy: registers.wy,
                    window_line,
                    map_lcdc: bg_fetch_lcdc,
                    low_lcdc,
                    high_lcdc,
                    low_override,
                    high_override,
                },
            ) {
                Some(pixel) => {
                    self.update_cgb_dmg_tile_sel_glitch_data(pixel);
                    pixel.colour_index
                }
                None => 0,
            }
        } else {
            0
        };

        let mut window_tile_select_latched = false;
        let mut bw_idx = if let Some((window_wx, window_line)) =
            window_fetch.filter(|_| bg_window_enabled)
        {
            let window_sample_x = self.window_sample_x_for_pixel(x);
            let (low_lcdc, high_lcdc, low_override, high_override) =
                self.lcdc_tile_data_edge.lcdc_for_tile_data(x, lcdc);
            window_tile_select_latched = low_lcdc & LCDC_TILE_DATA != lcdc & LCDC_TILE_DATA
                || high_lcdc & LCDC_TILE_DATA != lcdc & LCDC_TILE_DATA
                || low_override != DmgTileByteOverride::None
                || high_override != DmgTileByteOverride::None;
            let (low_override, high_override) = self.resolve_cgb_dmg_tile_sel_overrides(
                low_override,
                high_override,
                oam,
                vram,
                registers,
            );
            let window_map_lcdc = self.lcdc_window_map_edge.lcdc_for_window_fetch(x, lcdc);
            match bg_fifo::fetch_dmg_pixel_with_data(
                vram,
                DmgPixelFetch {
                    layer: DmgLayer::Window,
                    x: window_sample_x,
                    scanline: self.scanline,
                    scx: registers.scx,
                    fine_scx: registers.scx & SCX_FINE_MASK,
                    scy_map: registers.scy,
                    scy_data_low: registers.scy,
                    scy_data_high: registers.scy,
                    wx: window_wx,
                    wy: registers.wy,
                    window_line,
                    map_lcdc: window_map_lcdc | LCDC_WINDOW_ENABLE,
                    low_lcdc,
                    high_lcdc,
                    low_override,
                    high_override,
                },
            ) {
                Some(pixel) => {
                    self.update_cgb_dmg_tile_sel_glitch_data(pixel);
                    if self.window_reactivation_zero_for_pixel(x) {
                        0
                    } else if dmg_window_glitch && self.dmg_window_startup_gap_uses_bg_pixel(x) {
                        bg_idx
                    } else {
                        pixel.colour_index
                    }
                }
                None => bg_idx,
            }
        } else if dmg_window_glitch && self.dmg_disabled_window_color0_pixel(x, registers) {
            0
        } else {
            bg_idx
        };

        let sprite_px = if obj_enabled && !self.is_obj_fetch_canceled_for_pixel(x) {
            let (low_lcdc, high_lcdc) = self.obj_fetch_lcdc_for_pixel(x, lcdc);
            sprites::fetch_sprite_pixel_with_lcdc_samples(
                x,
                self.scanline,
                &self.sprite_indices,
                oam,
                vram,
                low_lcdc,
                high_lcdc,
            )
        } else {
            None
        };

        let tile_select_first_pixel_glitch = window_tile_select_latched
            || self
                .window_first_pixel_tile_select_toggle_changes_colour(bw_idx, x, vram, registers);
        if sprite_px.is_none()
            && tile_select_first_pixel_glitch
            && x == 0
            && !self.dmg_window_startup_gap_uses_bg_pixel(x)
            && self.window_active
            && self.leftmost_obj_oam_x == Some(4)
        {
            bw_idx = if dmg_window_glitch { 3 } else { 2 };
        }

        if let Some(sp) = sprite_px {
            if sp.bg_priority && bw_idx != 0 {
                (bw_idx, false, 0)
            } else {
                (sp.colour_index, true, sp.palette)
            }
        } else {
            (bw_idx, false, 0)
        }
    }

    fn bgp_for_pixel(&self, x: u32, current: u8) -> u8 {
        if self.bgp_edge_active && u32::from(self.bgp_edge_x) == x {
            self.bgp_edge_value
        } else {
            current
        }
    }

    fn obp0_for_pixel(&self, x: u32, current: u8) -> u8 {
        if self.obp0_edge_active && u32::from(self.obp0_edge_x) == x {
            self.obp0_edge_value
        } else {
            current
        }
    }

    fn obp1_for_pixel(&self, x: u32, current: u8) -> u8 {
        if self.obp1_edge_active && u32::from(self.obp1_edge_x) == x {
            self.obp1_edge_value
        } else {
            current
        }
    }

    fn update_bg_fetch_scx(&mut self) {
        self.bg_fetch_scx = self.bg_scx_sampler.current_scx() | self.scanline_start_scx_low;
    }

    fn update_bg_fetch_scy(&mut self) {
        self.bg_fetch_scy_map = self.bg_scy_sampler.current_scy_map();
        self.bg_fetch_scy_data_low = self.bg_scy_sampler.current_scy_data_low();
        self.bg_fetch_scy_data_high = self.bg_scy_sampler.current_scy_data_high();
        self.bg_scy_sampler.consume_pixel();
    }

    /// Apply any matured SCY write to `effective_scy`.
    fn update_effective_scy(&mut self, elapsed: u16) {
        if matches!(self.pending_scy_write, Some((effective_at, _)) if elapsed >= effective_at) {
            let (_, new_scy) = self.pending_scy_write.take().unwrap();
            self.effective_scy = new_scy;
        }
    }

    fn consume_bg_scx_pixel(&mut self) {
        self.bg_scx_sampler.consume_pixel();
    }

    fn sample_scanline_start_scx_low(&mut self, elapsed: u16, current_scx: u8) {
        if self.scx_low_bits_sampled || elapsed < SCX_LOW_BITS_SAMPLE_DOTS {
            return;
        }

        self.scanline_start_scx_low = current_scx & SCX_FINE_MASK;
        self.bg_fetch_scx = (self.bg_fetch_scx & SCX_TILE_MASK) | self.scanline_start_scx_low;
        self.fine_scroll_delay_dots = u16::from(self.scanline_start_scx_low);
        self.scx_low_bits_sampled = true;
    }

    fn dmg_disabled_window_color0_pixel(&self, x: u32, registers: &Registers) -> bool {
        let x_u8 = x as u8;
        let disabled_by_mid_scanline_edge = self
            .window_enable_edges
            .iter()
            .any(|edge| !edge.enabled && u32::from(edge.start_x) <= x);
        self.scanline >= registers.wy
            && disabled_by_mid_scanline_edge
            && !self.window_enabled_for_pixel(x, registers.lcdc)
            && window_triggers_at_x(self.wx_for_pixel(x, registers.wx), x_u8)
            && x_u8 & 0x07 == 0
    }

    fn dmg_window_startup_gap_uses_bg_pixel(&self, x: u32) -> bool {
        // On DMG, a WIN_MAP write in the startup gap can leave the first visible
        // dot sourced from the BG FIFO even though the window fetch has triggered.
        x == 0
            && self.window_startup_stall_consumed > 0
            && self
                .lcdc_window_map_edge
                .ranges
                .iter()
                .any(|range| range.start_x == 0 && range.window_map_bit == 0)
    }

    fn clear_consumed_bgp_edge(&mut self) {
        if self.bgp_edge_active && self.bgp_edge_x == self.next_x {
            self.bgp_edge_active = false;
        }
    }

    fn clear_consumed_obp_edges(&mut self) {
        if self.obp0_edge_active && self.obp0_edge_x == self.next_x {
            self.obp0_edge_active = false;
        }
        if self.obp1_edge_active && self.obp1_edge_x == self.next_x {
            self.obp1_edge_active = false;
        }
    }

    fn window_fetch_for_pixel(
        &mut self,
        x: u32,
        registers: &Registers,
        window_line: u8,
    ) -> Option<(u8, u8)> {
        let x_u8 = x as u8;
        if self.window_reset_edges.contains(&x_u8) {
            self.window_triggered = false;
            self.bg_fetch_x_offset = u8::from(x_u8 != 0);
        }

        if self.scanline < registers.wy
            || self.off_left_window_warmup_scanline(registers.wy)
            || !self.window_enabled_for_pixel(x, registers.lcdc)
        {
            self.window_triggered = false;
            return None;
        }
        if let Some(reactivation_x) = self.off_left_window_reactivation_x()
            && x_u8 < reactivation_x
        {
            self.window_triggered = false;
            return None;
        }

        if !self.window_triggered {
            let pending_hidden_wx = self.pending_hidden_window_fetch_wx.take();
            let wx = pending_hidden_wx.unwrap_or_else(|| self.wx_for_pixel(x, registers.wx));
            let off_left_reactivation = self
                .off_left_window_reactivation_x()
                .is_some_and(|reactivation_x| x_u8 == reactivation_x);
            if pending_hidden_wx.is_none()
                && !off_left_reactivation
                && !window_triggers_at_x(wx, x_u8)
            {
                return None;
            }

            self.window_triggered = true;
            self.window_active = true;
            self.bg_fetch_x_offset = 0;
            self.window_fetch_wx = wx;
            self.window_fetch_line = window_line.wrapping_add(self.window_activation_count);
            self.window_activation_count = self.window_activation_count.wrapping_add(1);
        }

        Some((self.window_fetch_wx, self.window_fetch_line))
    }

    fn window_enabled_for_pixel(&self, x: u32, current_lcdc: u8) -> bool {
        let base_lcdc = if self.window_enable_edges.is_empty() {
            current_lcdc
        } else {
            self.scanline_start_lcdc
        };
        let mut enabled = base_lcdc & LCDC_WINDOW_ENABLE != 0;
        for edge in &self.window_enable_edges {
            if u32::from(edge.start_x) <= x {
                enabled = edge.enabled;
            }
        }
        enabled
    }

    fn wx_for_pixel(&self, x: u32, current_wx: u8) -> u8 {
        let mut wx = if self.window_x_edges.is_empty() {
            current_wx
        } else {
            self.scanline_start_wx
        };
        for edge in &self.window_x_edges {
            if u32::from(edge.start_x) <= x {
                wx = edge.wx;
            }
        }
        wx
    }

    fn is_obj_fetch_canceled_for_pixel(&self, x: u32) -> bool {
        self.canceled_obj_fetch_ranges
            .iter()
            .any(|&(start, end)| u32::from(start) <= x && x <= u32::from(end))
    }

    fn obj_fetch_lcdc_for_pixel(&self, x: u32, current_lcdc: u8) -> (u8, u8) {
        self.obj_fetch_lcdc_ranges
            .iter()
            .find(|range| u32::from(range.start_x) <= x && x <= u32::from(range.end_x))
            .map_or((current_lcdc, current_lcdc), |range| {
                (range.low_lcdc, range.high_lcdc)
            })
    }

    fn obj_enabled_for_pixel(&self, lcdc: u8) -> bool {
        self.lcdc_obj_enable_edge
            .obj_enabled_for_pixel(self.next_x.into(), lcdc)
    }

    fn queue_stall_events_for_next_pixel(&mut self, lcdc: u8) {
        while self.next_obj_stall_event < self.obj_stall_events.len() {
            let event = self.obj_stall_events[self.next_obj_stall_event];
            if event.x > self.next_x {
                break;
            }
            if self.pending_obj_stall_dots == 0 {
                self.active_obj_stall_x = Some(event.x);
                self.start_obj_fetch_lcdc_range(event.x, lcdc);
            }
            self.pending_obj_stall_dots += event.dots;
            self.pending_obj_bg_fetch_wait_dots += event.bg_fetch_wait_dots;
            self.next_obj_stall_event += 1;
        }
    }

    fn start_obj_fetch_lcdc_range(&mut self, x: u8, lcdc: u8) {
        let range_index = self.obj_fetch_lcdc_ranges.len();
        // Off-left OBJ fetches have already selected tile bytes before the first
        // visible pixel, even though their stall still delays x=0.
        let sampled_lcdc = if x == 0 && self.leftmost_obj_starts_before_screen() {
            self.scanline_start_lcdc
        } else {
            lcdc
        };
        self.obj_fetch_lcdc_ranges.push(ObjFetchLcdcRange {
            start_x: x,
            end_x: x.saturating_add(OBJ_PIXELS_PER_FETCH - 1),
            low_lcdc: sampled_lcdc,
            high_lcdc: sampled_lcdc,
        });
        self.active_obj_fetch_lcdc_range = Some(range_index);
    }

    fn clear_consumed_obj_fetch_lcdc_ranges(&mut self) {
        self.obj_fetch_lcdc_ranges
            .retain(|range| range.end_x != self.next_x);
    }

    fn clear_consumed_window_edges(&mut self) {
        self.window_reset_edges.retain(|&x| x != self.next_x);
        prune_consumed_window_enable_edges(&mut self.window_enable_edges, self.next_x);
        prune_consumed_window_x_edges(&mut self.window_x_edges, self.next_x);
        if self.window_reactivation_edges.contains(&self.next_x) {
            self.window_reactivation_shift = self.window_reactivation_shift.saturating_add(1);
        }
        self.window_reactivation_edges.retain(|&x| x != self.next_x);
    }

    fn window_reactivation_zero_for_pixel(&self, x: u32) -> bool {
        self.window_reactivation_edges
            .iter()
            .any(|&edge_x| u32::from(edge_x) == x)
    }

    fn is_waiting_on_obj_fetch(&self) -> bool {
        self.pending_obj_stall_dots > 0
            || self
                .obj_stall_events
                .get(self.next_obj_stall_event)
                .is_some_and(|event| event.x <= self.next_x)
    }

    fn window_would_trigger_at_next_x(&self, registers: &Registers) -> bool {
        self.window_trigger_condition_at_next_x(registers)
    }

    fn window_trigger_condition_at_next_x(&self, registers: &Registers) -> bool {
        !self.window_triggered
            && self.scanline >= registers.wy
            && !self.off_left_window_warmup_scanline(registers.wy)
            && !self.window_reset_edges.contains(&self.next_x)
            && self.window_enabled_for_pixel(u32::from(self.next_x), registers.lcdc)
            && window_triggers_at_x(
                self.wx_for_pixel(u32::from(self.next_x), registers.wx),
                self.next_x,
            )
    }

    fn window_startup_stall_dots(&self, registers: &Registers) -> u8 {
        let wx = self.wx_for_pixel(u32::from(self.next_x), registers.wx);
        // The mealybug m3_window_timing_wx_0 ROM documents that WX=0 activates
        // one T-cycle later when SCX has nonzero fine scroll.
        WINDOW_STARTUP_STALL_DOTS + u8::from(wx == 0 && self.scanline_start_scx_low != 0)
    }

    fn is_waiting_on_window_startup(&self) -> bool {
        self.pending_window_startup_stall_dots > 0
    }

    fn off_left_window_warmup_scanline(&self, wy: u8) -> bool {
        self.scanline_start_wx == 6 && self.scanline.wrapping_sub(wy) < 2
    }

    fn off_left_window_reactivation_x(&self) -> Option<u8> {
        if self.scanline_start_wx != 6 {
            return None;
        }
        self.off_left_window_reactivation_x
    }

    fn window_sample_x_for_pixel(&self, x: u32) -> u32 {
        let reactivation_offset = self.off_left_window_reactivation_x().map_or(0, u32::from);
        x.saturating_sub(reactivation_offset + u32::from(self.window_reactivation_shift))
    }

    fn window_startup_would_delay_next_pixel(&self, registers: &Registers) -> bool {
        self.window_startup_stall_consumed == 0
            && self.window_trigger_condition_at_next_x(registers)
    }

    /// Computes the effective `(next_x, pending_obj_stall_dots)` as pre-window-stall code
    /// would have seen them. Called only when `window_startup_stall_consumed > 0`.
    fn compute_effective_state_for_window_stall(&self, consumed: u8) -> (u8, u16) {
        let actual_next_x = self.next_x;
        let actual_pend = self.pending_obj_stall_dots;
        let actual_waiting = self.is_waiting_on_obj_fetch();

        if actual_waiting {
            let (stall_total, stall_x) = if actual_pend > 0 {
                // In-stall: stall has been running for `consumed` fewer ticks in pre-PR.
                (actual_pend, actual_next_x)
            } else {
                // Queued: event.x <= next_x but stall not yet started (pend=0).
                // In pre-PR, the stall started `consumed` ticks ago.
                let total = self
                    .obj_stall_events
                    .get(self.next_obj_stall_event)
                    .map_or(0, |e| e.dots);
                (total, actual_next_x)
            };
            let consumed_u16 = u16::from(consumed);
            let effective_pend = stall_total.saturating_sub(consumed_u16);
            // If stall ended early in pre-PR, `excess` additional pixels were rendered.
            let excess = consumed_u16.saturating_sub(stall_total) as u8;
            let effective_next_x = stall_x.saturating_add(excess);
            (effective_next_x, effective_pend)
        } else if let Some(event) = self.obj_stall_events.get(self.next_obj_stall_event) {
            let stall_x = event.x;
            let consumed_i = i32::from(consumed);
            let next_x_i = i32::from(actual_next_x);
            let stall_x_i = i32::from(stall_x);

            if stall_x_i > next_x_i && stall_x_i <= next_x_i + consumed_i {
                // Not-yet-queued: the OBJ stall at stall_x falls within the consumed window.
                // In pre-PR, the stall at stall_x had already started by write time.
                let elapsed_pre = next_x_i + consumed_i - stall_x_i;
                let stall_total = event.dots as i32;
                if elapsed_pre >= stall_total {
                    // Stall ended in pre-PR; `elapsed_pre - stall_total` extra pixels rendered.
                    let extra = (elapsed_pre - stall_total) as u8;
                    (stall_x.saturating_add(extra), 0u16)
                } else if elapsed_pre > 0 {
                    // Stall in progress in pre-PR.
                    (stall_x, (stall_total - elapsed_pre) as u16)
                } else {
                    // elapsed_pre == 0: stall queued (fires next tick) in pre-PR.
                    (stall_x, 0u16)
                }
            } else {
                // OBJ stall is beyond the consumed window; pre-PR had more pixels rendered.
                (actual_next_x.saturating_add(consumed), 0)
            }
        } else {
            // No pending OBJ events; pre-PR had rendered `consumed` more pixels.
            (actual_next_x.saturating_add(consumed), 0)
        }
    }

    fn leftmost_obj_starts_before_screen(&self) -> bool {
        self.leftmost_obj_oam_x.is_some_and(|oam_x| oam_x < 8)
    }
}

impl Default for PixelFifoRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn is_off_left_window_delayed_obj_x(oam_x: u8) -> bool {
    (OFF_LEFT_WINDOW_DELAYED_OBJ_X_START..=OFF_LEFT_WINDOW_DELAYED_OBJ_X_END).contains(&oam_x)
}

fn window_visible_start_x(wx: u8) -> u8 {
    wx.saturating_sub(7)
}

fn window_triggers_at_x(wx: u8, x: u8) -> bool {
    wx < 167 && x == window_visible_start_x(wx)
}

fn prune_consumed_window_enable_edges(edges: &mut Vec<WindowEnableEdge>, x: u8) {
    let Some(latest_consumed_index) = edges.iter().rposition(|edge| edge.start_x <= x) else {
        return;
    };

    let mut index = 0;
    edges.retain(|edge| {
        let keep = index == latest_consumed_index || edge.start_x > x;
        index += 1;
        keep
    });
}

fn prune_consumed_window_x_edges(edges: &mut Vec<WindowXEdge>, x: u8) {
    let Some(latest_consumed_index) = edges.iter().rposition(|edge| edge.start_x <= x) else {
        return;
    };

    let mut index = 0;
    edges.retain(|edge| {
        let keep = index == latest_consumed_index || edge.start_x > x;
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::super::{registers::Registers, screen_buffer::ScreenBuffer};
    use super::{
        BgScxSamplerConfig, LCDC_BG_WINDOW_ENABLE, LCDC_OBJ_ENABLE, LCDC_TILE_DATA,
        LCDC_WINDOW_ENABLE, LCDC_WINDOW_MAP, LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_RESTORE_X,
        LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_SET_X, LcdcBgEnableEdge, LcdcBgEnableEdgeTiming,
        LcdcBgMapEdge, LcdcBgMapFetchDelay, OFF_LEFT_WX_OVERWRITE_CANCEL_X, PixelFifoRenderer,
        SCX_LOW_BITS_SAMPLE_DOTS, WindowEnableEdge, WindowXEdge, scy_fetch_start_dots,
    };
    use crate::gb::model::CgbModel;

    fn oam_with_sprite_at(oam_y: u8, oam_x: u8, tile: u8, attrs: u8) -> [u8; 0xA0] {
        let mut oam = [0u8; 0xA0];
        oam[0] = oam_y;
        oam[1] = oam_x;
        oam[2] = tile;
        oam[3] = attrs;
        oam
    }

    fn vram_with_mixed_bg_tile_select_sources() -> [u8; 0x2000] {
        let mut vram = [0u8; 0x2000];
        vram[0x1800] = 0x01;
        vram[0x1010] = 0x80;
        vram[0x0011] = 0x80;
        vram
    }

    fn vram_with_blank_signed_and_solid_unsigned_tiles() -> [u8; 0x2000] {
        let mut vram = [0u8; 0x2000];
        vram[0x1800] = 0x00;
        vram[0x1801] = 0x00;
        vram[0x0000] = 0xFF;
        vram[0x0001] = 0xFF;
        vram
    }

    fn vram_with_blank_9800_window_and_solid_9c00_window() -> [u8; 0x2000] {
        let mut vram = [0u8; 0x2000];
        vram[0x1800] = 0x00;
        for tile in &mut vram[0x1C00..0x1C20] {
            *tile = 0x01;
        }
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0xFF;
        vram
    }

    fn vram_with_solid_window_and_sprite_tiles() -> [u8; 0x2000] {
        let mut vram = [0u8; 0x2000];
        for tile in &mut vram[0x1C00..0x1C20] {
            *tile = 0x01;
        }
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0xFF;
        vram[0x0020] = 0xFF;
        vram
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_renderer(
        renderer: &mut PixelFifoRenderer,
        dot: u16,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        cgb_mode: bool,
        dmg_compat: bool,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let vram_bank1 = [0u8; 0x2000];
        let bg_palette_ram = [0u8; 64];
        let mut obj_palette_ram = [0u8; 64];
        obj_palette_ram[2] = 0xFF;
        obj_palette_ram[3] = 0x7F;
        renderer.tick(
            dot,
            vram,
            &vram_bank1,
            oam,
            registers,
            &bg_palette_ram,
            &obj_palette_ram,
            0,
            cgb_mode,
            false,
            dmg_compat,
            screen_buffer,
        );
    }

    #[test]
    fn native_cgb_window_activation_reports_window_line_increment() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x80 | LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE;
        registers.wx = 7;
        registers.wy = 0;
        let vram = [0u8; 0x2000];
        let vram_bank1 = [0u8; 0x2000];
        let oam = [0u8; 0xA0];
        let bg_palette_ram = [0u8; 64];
        let obj_palette_ram = [0u8; 64];
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, false);
        let mut activation_count = None;
        for dot in 80..400 {
            if let Some(count) = renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                false,
                &mut screen_buffer,
            ) {
                activation_count = Some(count);
                break;
            }
        }

        assert_eq!(
            activation_count,
            Some(1),
            "native CGB rendering should report one window activation so PPU window_line advances"
        );
    }

    #[test]
    fn fixup_after_state_load_restores_cgb_scx_sampler_timing() {
        let mut renderer = PixelFifoRenderer::new();
        let registers = Registers::new();
        let oam = [0u8; 0xA0];

        renderer.begin_scanline(0, 80, &oam, &registers, true, false);
        renderer.bg_scx_sampler.config = BgScxSamplerConfig::default();
        renderer.bg_scy_sampler.fetch_start_dots = SCX_LOW_BITS_SAMPLE_DOTS;
        renderer.effective_scy = 0;

        renderer.fixup_after_state_load(true, false, 0x5a);

        assert_eq!(
            renderer.bg_scx_sampler.config,
            BgScxSamplerConfig::for_mode(true),
            "legacy CGB save-states should not resume with DMG SCX fetch timing"
        );
        assert!(!renderer.bg_scx_sampler.will_sample_scx(6));
        assert!(renderer.bg_scx_sampler.will_sample_scx(7));
        assert_eq!(
            renderer.bg_scy_sampler.fetch_start_dots,
            scy_fetch_start_dots()
        );
        assert_eq!(renderer.effective_scy, 0x5a);
    }

    #[test]
    fn fixup_after_state_load_restores_dmg_scy_sampler_timing() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.bg_scy_sampler.fetch_start_dots = SCX_LOW_BITS_SAMPLE_DOTS;

        renderer.fixup_after_state_load(false, false, 0x81);

        assert_eq!(
            renderer.bg_scy_sampler.fetch_start_dots,
            scy_fetch_start_dots()
        );
        assert_eq!(renderer.effective_scy, 0x81);
    }

    #[test]
    fn clear_consumed_window_edges_keeps_latest_state_and_future_writes() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.next_x = 5;
        renderer.window_enable_edges = vec![
            WindowEnableEdge {
                start_x: 8,
                enabled: false,
            },
            WindowEnableEdge {
                start_x: 2,
                enabled: false,
            },
            WindowEnableEdge {
                start_x: 5,
                enabled: true,
            },
            WindowEnableEdge {
                start_x: 10,
                enabled: false,
            },
        ];
        renderer.window_x_edges = vec![
            WindowXEdge { start_x: 8, wx: 8 },
            WindowXEdge { start_x: 2, wx: 2 },
            WindowXEdge { start_x: 5, wx: 5 },
            WindowXEdge {
                start_x: 10,
                wx: 10,
            },
        ];

        renderer.clear_consumed_window_edges();

        assert_eq!(
            renderer
                .window_enable_edges
                .iter()
                .map(|edge| (edge.start_x, edge.enabled))
                .collect::<Vec<_>>(),
            vec![(8, false), (5, true), (10, false)]
        );
        assert_eq!(
            renderer
                .window_x_edges
                .iter()
                .map(|edge| (edge.start_x, edge.wx))
                .collect::<Vec<_>>(),
            vec![(8, 8), (5, 5), (10, 10)]
        );
    }

    #[test]
    fn wx_write_at_x0_preserves_only_off_left_hidden_window_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 12;
        renderer.scanline_start_lcdc = LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE;
        renderer.scanline_start_wx = 6;
        renderer.next_x = 0;

        renderer.record_wx_write(12, 0);

        assert_eq!(renderer.pending_hidden_window_fetch_wx, Some(7));

        renderer.pending_hidden_window_fetch_wx = None;
        renderer.scanline_start_wx = 7;

        renderer.record_wx_write(12, 0);

        assert_eq!(renderer.pending_hidden_window_fetch_wx, None);
    }

    #[test]
    fn wx_reactivation_edge_uses_next_fetch_decision_phase() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 12;
        renderer.scanline_start_wx = 4;
        renderer.next_x = 0;

        renderer.record_wx_write(12, 0);

        assert_eq!(renderer.window_reactivation_edges, vec![5]);

        renderer.window_reactivation_edges.clear();

        renderer.record_wx_write(11, 0);

        assert!(
            renderer.window_reactivation_edges.is_empty(),
            "WX=11 reactivates at x=4, the original phase rather than the next fetch decision phase"
        );

        renderer.scanline_start_wx = 120;
        renderer.record_wx_write(24, 0);

        assert!(
            renderer.window_reactivation_edges.is_empty(),
            "ordinary on-screen WX starts must not create off-left reactivation edges"
        );
    }

    #[test]
    fn wx6_same_dot_overwrite_cancels_at_threshold() {
        fn renderer_with_pending_reactivation(next_x: u8, reactivation_x: u8) -> PixelFifoRenderer {
            let mut renderer = PixelFifoRenderer::new();
            renderer.active = true;
            renderer.scanline_start_wx = 6;
            renderer.next_x = next_x;
            renderer.off_left_window_reactivation_x = Some(reactivation_x);
            renderer
        }

        let mut before_threshold = renderer_with_pending_reactivation(93, 94);

        before_threshold.record_wx_write(80, 0);

        assert_eq!(before_threshold.off_left_window_reactivation_x, Some(94));

        let mut at_threshold =
            renderer_with_pending_reactivation(94, OFF_LEFT_WX_OVERWRITE_CANCEL_X);

        at_threshold.record_wx_write(80, 0);

        assert_eq!(at_threshold.off_left_window_reactivation_x, None);
    }

    #[test]
    fn same_dot_lcdc_window_enable_delays_inactive_window_trigger_one_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 16;
        renderer.scanline_start_lcdc = LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA;
        renderer.next_x = 9;

        renderer.record_lcdc_window_enable_write(
            LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA,
            LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE,
            16,
        );

        assert_eq!(
            renderer
                .window_x_edges
                .iter()
                .map(|edge| (edge.start_x, edge.wx))
                .collect::<Vec<_>>(),
            vec![(9, 17)]
        );

        let mut registers = Registers::new();
        registers.lcdc = LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE;
        registers.wx = 16;
        registers.wy = 0;

        assert!(!renderer.window_trigger_condition_at_next_x(&registers));
        renderer.next_x = 10;
        assert!(renderer.window_trigger_condition_at_next_x(&registers));
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_dmg_until_next_x(
        renderer: &mut PixelFifoRenderer,
        dot: &mut u16,
        target_next_x: u8,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let deadline = dot.saturating_add(256);
        while renderer.next_x < target_next_x {
            tick_renderer(
                renderer,
                *dot,
                vram,
                oam,
                registers,
                false,
                false,
                screen_buffer,
            );
            *dot = dot.saturating_add(1);
            assert!(
                *dot < deadline,
                "renderer did not reach next_x={target_next_x}"
            );
        }
    }

    #[test]
    fn wx_write_after_off_left_window_fetch_keeps_initial_window_stream() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 12;
        renderer.scanline_start_lcdc = LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE;
        renderer.scanline_start_wx = 4;
        renderer.next_x = 0;

        renderer.record_wx_write(12, 4);

        let mut registers = Registers::new();
        registers.lcdc = LCDC_BG_WINDOW_ENABLE | LCDC_TILE_DATA | LCDC_WINDOW_ENABLE;
        registers.wx = 12;
        registers.wy = 4;

        assert_eq!(
            renderer.window_fetch_for_pixel(0, &registers, 0),
            Some((4, 0)),
            "a WX write after the off-left window fetch has started must not replace the active window stream"
        );
    }

    #[test]
    fn wx_reactivation_zero_pixel_allows_priority_sprite_to_show() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.window_triggered = true;
        renderer.window_fetch_wx = 7;
        renderer.window_reactivation_edges = vec![5];
        renderer.sprite_indices.push(0);
        let mut registers = Registers::new();
        registers.lcdc = LCDC_BG_WINDOW_ENABLE
            | LCDC_OBJ_ENABLE
            | LCDC_TILE_DATA
            | LCDC_WINDOW_ENABLE
            | LCDC_WINDOW_MAP;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_solid_window_and_sprite_tiles();
        let oam = oam_with_sprite_at(16, 13, 2, 0x80);

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(5, &vram, &oam, &registers, 0);

        assert_eq!(colour_index, 1);
        assert!(is_sprite);
    }

    #[test]
    fn lcdc_bg_enable_edge_defaults_to_current_lcdc_bit() {
        // Given: no recorded mid-Mode-3 LCDC write.
        let edge = LcdcBgEnableEdge::default();

        // When/Then: the helper follows the live LCDC bit 0 value.
        assert!(edge.bg_window_enabled_for_pixel(12, 0x93));
        assert!(!edge.bg_window_enabled_for_pixel(12, 0x92));
    }

    #[test]
    fn lcdc_bg_map_edge_defaults_to_current_lcdc_bit_3() {
        let edge = LcdcBgMapEdge::default();

        assert_eq!(edge.lcdc_for_bg_fetch(8, 0x83) & 0x08, 0x00);
        assert_eq!(edge.lcdc_for_bg_fetch(8, 0x8B) & 0x08, 0x08);
    }

    #[test]
    fn lcdc_bg_map_edge_holds_previous_map_through_recorded_range() {
        let mut edge = LcdcBgMapEdge::default();
        edge.record_write(8, 15, 0x83, 0x8B);

        assert_eq!(edge.lcdc_for_bg_fetch(8, 0x8B) & 0x08, 0x00);
        assert_eq!(edge.lcdc_for_bg_fetch(15, 0x8B) & 0x08, 0x00);
        assert_eq!(edge.lcdc_for_bg_fetch(16, 0x8B) & 0x08, 0x08);
    }

    #[test]
    fn lcdc_bg_map_edge_replaces_only_bg_map_bit() {
        let mut edge = LcdcBgMapEdge::default();
        edge.record_write(8, 15, 0x83, 0x8B);

        assert_eq!(edge.lcdc_for_bg_fetch(8, 0xDB), 0xD3);
    }

    #[test]
    fn lcdc_bg_map_edge_preserves_older_overlap_until_tile_latch_expires() {
        let mut edge = LcdcBgMapEdge::default();
        edge.record_write(0, 7, 0x83, 0x8B);
        edge.record_write(6, 15, 0x8B, 0x83);

        assert_eq!(edge.lcdc_for_bg_fetch(6, 0x83) & 0x08, 0x00);
        assert_eq!(edge.lcdc_for_bg_fetch(8, 0x83) & 0x08, 0x08);
        assert_eq!(edge.lcdc_for_bg_fetch(16, 0x83) & 0x08, 0x00);
    }

    #[test]
    fn lcdc_bg_map_edge_uses_scx_fine_scroll_for_tile_latch_boundary() {
        let mut edge = LcdcBgMapEdge::default();
        edge.record_delayed_write(0, 7, 0x83, 0x8B, LcdcBgMapFetchDelay::NextBgFetch);

        assert_eq!(LcdcBgMapEdge::bg_tile_boundary_at_or_after(0, 7), 1);
        assert_eq!(edge.lcdc_for_bg_fetch(8, 0x8B) & 0x08, 0x00);
        assert_eq!(edge.lcdc_for_bg_fetch(9, 0x8B) & 0x08, 0x08);
    }

    #[test]
    fn window_map_write_after_tile_boundary_keeps_current_window_tile_on_previous_map() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 1;
        renderer.window_active = true;
        renderer.window_triggered = true;
        renderer.window_fetch_wx = 7;
        renderer.record_lcdc_write_with_window(0xB1, 0xF1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xF1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_9800_window_and_solid_9c00_window();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(1, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "the window tile already selected when LCDC.6 changes should stay on the previous map"
        );
        assert_eq!(
            next_tile_colour, 3,
            "the following window tile fetch should use the new map"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_map_write_at_tile_boundary_uses_new_map_without_left_edge_obj_delay() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xF1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xF1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_9800_window_and_solid_9c00_window();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(current_tile_colour, 3);
        assert_eq!(next_tile_colour, 3);
        assert!(!is_sprite);
    }

    #[test]
    fn window_map_write_at_tile_boundary_with_left_edge_obj_delay_keeps_current_tile() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.leftmost_obj_oam_x = Some(1);
        renderer.record_lcdc_write_with_window(0xB1, 0xF1, 0, true, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xF1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_9800_window_and_solid_9c00_window();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(current_tile_colour, 0);
        assert_eq!(next_tile_colour, 3);
        assert!(!is_sprite);
    }

    #[test]
    fn window_obj_x4_first_pixel_uses_window_map_without_tile_select_latch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.leftmost_obj_oam_x = Some(4);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_9800_window_and_solid_9c00_window();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "the TILE_SEL-only first-pixel quirk must not override plain window-map fetches"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_obj_x4_first_pixel_uses_tile_select_glitch_when_alternate_data_is_visible() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.leftmost_obj_oam_x = Some(4);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let mut vram = vram_with_blank_9800_window_and_solid_9c00_window();
        vram[0x0000] = 0xFF;
        vram[0x0001] = 0xFF;
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 3,
            "the OAM X=4 first-pixel quirk applies only to TILE_SEL writes"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn steady_window_map_writes_apply_one_fetch_after_next_tile_boundary() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.window_triggered = true;
        renderer.window_fetch_wx = 7;
        renderer.next_x = LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_SET_X;
        renderer.record_lcdc_write_with_window(0xB1, 0xF1, 0, true, false, 7, 0);
        renderer.next_x = LEFT_EDGE_OBJ_X1_STEADY_WINDOW_MAP_RESTORE_X;
        renderer.record_lcdc_write_with_window(0xF1, 0xB1, 0, true, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.wx = 7;
        registers.wy = 0;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_9800_window_and_solid_9c00_window();
        let oam = [0u8; 0xA0];

        let (tile_after_set_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(24, &vram, &oam, &registers, 0);
        let (tile_after_restore_colour, _, _) =
            renderer.dmg_pixel_layers(32, &vram, &oam, &registers, 0);
        let (live_restored_colour, _, _) =
            renderer.dmg_pixel_layers(40, &vram, &oam, &registers, 0);

        assert_eq!(tile_after_set_colour, 0);
        assert_eq!(tile_after_restore_colour, 3);
        assert_eq!(live_restored_colour, 0);
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_delays_visible_left_edge_obj_bg_map_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 0;
        renderer.leftmost_obj_oam_x = Some(8);

        renderer.record_lcdc_write(0x83, 0x8B, 0, false, false);

        assert_eq!(
            renderer.lcdc_bg_map_edge.lcdc_for_bg_fetch(8, 0x8B) & 0x08,
            0x00
        );
        assert_eq!(
            renderer.lcdc_bg_map_edge.lcdc_for_bg_fetch(16, 0x8B) & 0x08,
            0x08
        );
    }

    #[test]
    fn visible_left_edge_obj_delays_tile_select_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(8);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (next_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            next_tile_colour, 0,
            "a visible-left-edge OBJ fetch should keep the following BG tile on the previous TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn visible_left_edge_obj_restore_cancels_delayed_tile_select_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(8);
        let scx = 3;
        renderer.record_lcdc_write(0x81, 0x91, scx, false, false);
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, scx, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.scx = scx;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(5, &vram, &oam, &registers, 0);
        let (next_fetch_colour, _, _) = renderer.dmg_pixel_layers(13, &vram, &oam, &registers, 0);
        let (delayed_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(21, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 3,
            "a restore at the SCX-shifted boundary should still let that fetch use the previous TILE_SEL sample"
        );
        assert_eq!(
            next_fetch_colour, 0,
            "the fetch after the restore should use the restored TILE_SEL sample"
        );
        assert_eq!(
            delayed_fetch_colour, 0,
            "restoring TILE_SEL before the delayed fetch starts should keep that fetch on the previous sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_delays_tile_select_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 1;
        renderer.pending_obj_stall_dots = 10;
        renderer.leftmost_obj_oam_x = Some(9);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 6;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (next_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            next_tile_colour, 0,
            "an OBJ fetch starting one pixel in should keep the following BG tile on the previous TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_delays_tile_select_even_after_low_byte_phase() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 2;
        renderer.pending_obj_stall_dots = 9;
        renderer.leftmost_obj_oam_x = Some(10);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 6;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (next_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            next_tile_colour, 0,
            "an OBJ fetch starting two pixels in should keep both bitplanes of the following BG tile on the previous TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_delays_tile_select_even_after_high_byte_phase() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 3;
        renderer.pending_obj_stall_dots = 8;
        renderer.leftmost_obj_oam_x = Some(11);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 6;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (next_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            next_tile_colour, 0,
            "an OBJ fetch starting three pixels in should keep both bitplanes of the following BG tile on the previous TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_restore_at_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 3;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(11);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (delayed_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore at the delayed BG boundary should update the current fetch to the restored TILE_SEL sample"
        );
        assert_eq!(
            delayed_fetch_colour, 0,
            "the delayed following fetch should also stay on the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_restore_after_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 4;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(12);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 9;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(10, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore after the delayed BG boundary should update the in-progress fetch to the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_restore_two_pixels_after_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(13);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 10;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(11, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore two pixels after the delayed BG boundary should update the in-progress fetch to the restored TILE_SEL sample"
        );
        assert_eq!(
            following_fetch_colour, 1,
            "the following fetch should keep the delayed low-byte TILE_SEL sample and use the restored high-byte sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_restore_three_pixels_after_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 6;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(14);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 11;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(12, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore three pixels after the delayed BG boundary should update the in-progress fetch to the restored TILE_SEL sample"
        );
        assert_eq!(
            following_fetch_colour, 1,
            "the following fetch should keep the delayed low-byte TILE_SEL sample and use the restored high-byte sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_restore_four_pixels_after_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 7;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(15);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 12;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(13, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore four pixels after the delayed BG boundary should update the in-progress fetch to the restored TILE_SEL sample"
        );
        assert_eq!(
            following_fetch_colour, 1,
            "the following fetch should keep the delayed low-byte TILE_SEL sample and use the restored high-byte sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn on_screen_obj_restore_at_stalled_boundary_preserves_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.leftmost_obj_oam_x = Some(16);
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.pending_obj_stall_dots = 3;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 0,
            "a restore at an OBJ-stalled BG boundary should update the in-progress fetch to the restored TILE_SEL sample"
        );
        assert_eq!(
            following_fetch_colour, 3,
            "the following fetch should keep the TILE_SEL sample latched before the OBJ stall"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_delays_cgb_dmg_compat_tile_phase_bg_map_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 15;

        renderer.record_lcdc_write(0xD3, 0xDB, 0, true, true);

        assert_eq!(
            renderer.lcdc_bg_map_edge.lcdc_for_bg_fetch(24, 0xDB) & 0x08,
            0x00
        );
        assert_eq!(
            renderer.lcdc_bg_map_edge.lcdc_for_bg_fetch(32, 0xDB) & 0x08,
            0x08
        );
    }

    #[test]
    fn recorded_tile_data_samples_mix_bg_low_and_high_bitplanes() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.scanline = 0;
        renderer.lcdc_tile_data_edge.record_write(0, 7, 0x81, 0x91);
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.bgp = 0xE4;
        let vram = vram_with_mixed_bg_tile_select_sources();
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);

        assert_eq!(colour_index, 3);
        assert!(!is_sprite);
    }

    #[test]
    fn recorded_tile_data_samples_mix_window_low_and_high_bitplanes() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.lcdc_tile_data_edge.record_write(0, 7, 0xA1, 0xB1);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_mixed_bg_tile_select_sources();
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);

        assert_eq!(colour_index, 3);
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_after_visible_bg_tile_latch_updates_next_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "a TILE_SEL write after the visible tile has been latched must not alter that tile"
        );
        assert_eq!(
            next_tile_colour, 3,
            "the next BG fetch should sample the new TILE_SEL value for both bitplanes"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_at_left_window_boundary_updates_current_window_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);

        assert_eq!(
            colour_index, 3,
            "a TILE_SEL write at WX=7 should apply to the first window tile fetch, not a preceding BG tile"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_at_scrolled_window_boundary_uses_window_phase() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 8;
        renderer.lcdc_tile_data_edge.record_write(0, 7, 0xA1, 0xA1);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 7, false, false, 15, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.scx = 7;
        registers.bgp = 0xE4;
        registers.wx = 15;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (bg_colour, is_sprite, _) = renderer.dmg_pixel_layers(7, &vram, &oam, &registers, 0);
        let (window_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            bg_colour, 0,
            "window TILE_SEL sampling must not bleed into the preceding BG pixel"
        );
        assert_eq!(
            window_colour, 3,
            "a TILE_SEL write at a WX>7 window boundary should use window phase, not SCX-shifted BG phase"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_after_window_boundary_updates_following_window_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 8;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "after the window has started, a boundary write must not alter the already-latched window tile"
        );
        assert_eq!(
            next_tile_colour, 3,
            "the following window fetch should sample the new TILE_SEL value"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_tile_select_set_one_pixel_after_boundary_updates_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 1;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 3,
            "a window TILE_SEL set one pixel after the boundary should update the current fetch"
        );
        assert_eq!(
            following_tile_colour, 3,
            "the following window fetch should also use the new TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_tile_select_set_two_pixels_after_boundary_mixes_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 2;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(0, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 2,
            "a window TILE_SEL set two pixels after the boundary should keep the previous low byte and sample the new high byte"
        );
        assert_eq!(
            following_tile_colour, 3,
            "the following window fetch should use the new TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_restore_at_tile_boundary_mixes_current_fetch_bitplanes() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.window_active = true;
        renderer.next_x = 8;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 1,
            "a window TILE_SEL restore at the tile boundary should keep the previous low byte and sample the restored high byte"
        );
        assert_eq!(
            following_tile_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_restore_one_pixel_after_boundary_mixes_current_fetch_bitplanes() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 1;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 9;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 1,
            "a window TILE_SEL restore one pixel after the boundary should keep the previous low byte and sample the restored high byte"
        );
        assert_eq!(
            following_tile_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_restore_two_pixels_after_boundary_keeps_current_fetch_latched() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 2;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 10;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 3,
            "a window TILE_SEL restore two pixels after the boundary should not alter the current fetch"
        );
        assert_eq!(
            following_tile_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn window_restore_three_pixels_after_boundary_keeps_current_fetch_latched() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 3;
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 11;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 3,
            "a window TILE_SEL restore three pixels after the boundary should not alter the current fetch"
        );
        assert_eq!(
            following_tile_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn visible_left_edge_obj_window_restore_mixes_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(8);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.window_active = true;
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (following_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            following_tile_colour, 2,
            "a visible-left-edge OBJ stall should leave the following window fetch with the restored low byte and delayed high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn off_left_obj_window_restore_after_boundary_keeps_delayed_high_byte() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0xE4;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 4, 0, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        tick_dmg_until_next_x(
            &mut renderer,
            &mut dot,
            1,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;
        tick_dmg_until_next_x(
            &mut renderer,
            &mut dot,
            9,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (85, 85, 85),
            "with the window startup stall applied, the first following window pixel keeps the delayed high-byte sample"
        );
    }

    #[test]
    fn off_left_obj_window_set_delays_first_visible_pixels() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0x6C;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 5, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        while dot < 104 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;
        while dot < 112 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xA3;
        let deadline = dot.saturating_add(64);
        while renderer.next_x <= 8 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not output the first following window tile pixel"
            );
        }

        assert_eq!(
            screen_buffer.get_pixel(0, 0),
            (85, 85, 85),
            "the first visible window pixel should wait long enough to mix the previous low byte with the set high byte"
        );
        assert_eq!(
            screen_buffer.get_pixel(1, 0),
            (85, 85, 85),
            "the second visible window pixel should wait long enough to keep the set high byte"
        );
        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (170, 170, 170),
            "the first following window tile should remain on the set TILE_SEL sample after the restore at its boundary"
        );
    }

    #[test]
    fn later_off_left_obj_window_set_delays_first_visible_pixels() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0x6C;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 6, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        while dot < 104 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;
        while dot < 112 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xA3;
        let deadline = dot.saturating_add(64);
        while renderer.next_x <= 8 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not output the first following window tile pixel"
            );
        }

        assert_eq!(
            screen_buffer.get_pixel(0, 0),
            (85, 85, 85),
            "the first visible window pixel should wait through the later off-left OBJ stall"
        );
        assert_eq!(
            screen_buffer.get_pixel(1, 0),
            (85, 85, 85),
            "the second visible window pixel should wait through the later off-left OBJ stall"
        );
        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (170, 170, 170),
            "the following tile should stay on the set TILE_SEL sample after the restore"
        );
    }

    #[test]
    fn final_off_left_obj_window_set_delays_first_visible_pixels() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0x6C;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 7, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        while dot < 104 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;
        while dot < 112 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xA3;
        let deadline = dot.saturating_add(64);
        while renderer.next_x <= 8 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not output the first following window tile pixel"
            );
        }

        assert_eq!(
            screen_buffer.get_pixel(0, 0),
            (85, 85, 85),
            "the first visible window pixel should wait through the final off-left OBJ stall"
        );
        assert_eq!(
            screen_buffer.get_pixel(1, 0),
            (85, 85, 85),
            "the second visible window pixel should wait through the final off-left OBJ stall"
        );
        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (170, 170, 170),
            "the following tile should stay on the set TILE_SEL sample after the restore"
        );
    }

    #[test]
    fn delayed_obj_window_set_mixes_first_stalled_fetch_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0xE4;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 12, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 4 && renderer.pending_obj_stall_dots == 3) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach the OAM X=12 stall");
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;

        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 9 && renderer.pending_obj_stall_dots == 0) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not reach the OAM X=12 restore point"
            );
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (85, 85, 85),
            "the first delayed window pixel should mix the previous low byte with the set high byte"
        );
    }

    #[test]
    fn delayed_obj_window_set_later_keeps_first_stalled_fetch_pixels_latched() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0xE4;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 13, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 5 && renderer.pending_obj_stall_dots == 3) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach the OAM X=13 stall");
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;

        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 10 && renderer.pending_obj_stall_dots == 0) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not reach the OAM X=13 restore point"
            );
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (255, 255, 255),
            "with the window startup stall applied, the first later delayed window pixel remains on the previous tile-data sample"
        );
        assert_eq!(
            screen_buffer.get_pixel(9, 0),
            (255, 255, 255),
            "with the window startup stall applied, the second later delayed window pixel remains on the previous tile-data sample"
        );
    }

    #[test]
    fn delayed_obj_window_set_after_high_phase_keeps_first_stalled_fetch_pixels_latched() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0xA3;
        registers.bgp = 0xE4;
        registers.obp0 = 0xFF;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = oam_with_sprite_at(16, 14, 1, 0);
        let mut screen_buffer = ScreenBuffer::new();
        let mut dot: u16 = 80;

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 6 && renderer.pending_obj_stall_dots == 4) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach the OAM X=14 stall");
        }
        renderer.record_lcdc_write_with_window(
            0xA3,
            0xB3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );
        registers.lcdc = 0xB3;

        let deadline = dot.saturating_add(256);
        while !(renderer.next_x == 10 && renderer.pending_obj_stall_dots == 0) {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(
                dot < deadline,
                "renderer did not reach the OAM X=14 restore point"
            );
        }
        renderer.record_lcdc_write_with_window(
            0xB3,
            0xA3,
            registers.scx,
            false,
            false,
            registers.wx,
            registers.wy,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (255, 255, 255),
            "with the window startup stall applied, the first high-phase delayed window pixel remains on the previous tile-data sample"
        );
        assert_eq!(
            screen_buffer.get_pixel(9, 0),
            (255, 255, 255),
            "with the window startup stall applied, the second high-phase delayed window pixel remains on the previous tile-data sample"
        );
    }

    #[test]
    fn near_left_edge_obj_window_restore_mixes_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 1;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(9);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 6;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (following_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            following_tile_colour, 2,
            "a near-left-edge OBJ stall should leave the following window fetch with the restored low byte and delayed high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_after_low_byte_mixes_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 2;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(10);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 7;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (following_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            following_tile_colour, 2,
            "a near-left-edge OBJ restore after the low-byte phase should leave the following window fetch with the restored low byte and delayed high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_at_delayed_boundary_mixes_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 3;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(11);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(9, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a near-left-edge OBJ restore at the delayed window boundary should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_after_delayed_boundary_mixes_current_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 4;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(12);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 9;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(10, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a near-left-edge OBJ restore after the delayed window boundary should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_later_after_boundary_updates_current_and_following_fetches()
     {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 5;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(13);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 10;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(11, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a near-left-edge OBJ restore later after the delayed window boundary should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert_eq!(
            following_fetch_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_after_high_phase_updates_current_and_following_fetches() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 6;
        renderer.pending_obj_stall_dots = 3;
        renderer.leftmost_obj_oam_x = Some(14);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 10;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(12, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a near-left-edge OBJ restore after the high-byte phase should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert_eq!(
            following_fetch_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_restore_at_late_stall_updates_current_and_following_fetches() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 7;
        renderer.pending_obj_stall_dots = 5;
        renderer.leftmost_obj_oam_x = Some(15);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 10;
        renderer.pending_obj_stall_dots = 0;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(14, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a late near-left-edge OBJ restore should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert_eq!(
            following_fetch_colour, 0,
            "the following window fetch should use the restored TILE_SEL sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn near_left_edge_obj_window_set_at_late_stall_mixes_current_fetch_first_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 7;
        renderer.pending_obj_stall_dots = 5;
        renderer.leftmost_obj_oam_x = Some(15);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a late near-left-edge OBJ TILE_SEL set should leave the first current-window pixel with the previous low byte and new high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn on_screen_obj_window_restore_at_stalled_boundary_mixes_current_and_following_fetches() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.leftmost_obj_oam_x = Some(16);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.pending_obj_stall_dots = 3;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "a stalled on-screen OBJ restore should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert_eq!(
            following_fetch_colour, 1,
            "the following window fetch should keep the delayed low byte and restored high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn on_screen_obj_window_restore_after_stalled_boundary_mixes_current_and_following_fetches() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.leftmost_obj_oam_x = Some(17);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        renderer.next_x = 9;
        renderer.pending_obj_stall_dots = 3;
        renderer.record_lcdc_write_with_window(0xB1, 0xA1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xA1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_fetch_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "an on-screen OBJ restore after the stalled boundary should leave the current window fetch with the restored low byte and delayed high byte"
        );
        assert_eq!(
            following_fetch_colour, 1,
            "the following window fetch should keep the delayed low byte and restored high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn on_screen_obj_window_set_at_stalled_boundary_mixes_current_fetch_first_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.window_active = true;
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 0;
        renderer.leftmost_obj_oam_x = Some(17);
        renderer.record_lcdc_write_with_window(0xA1, 0xB1, 0, false, false, 7, 0);
        let mut registers = Registers::new();
        registers.lcdc = 0xB1;
        registers.bgp = 0xE4;
        registers.wx = 7;
        registers.wy = 0;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_fetch_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_fetch_colour, 2,
            "an on-screen OBJ TILE_SEL set at the stalled boundary should leave the first current-window pixel with the previous low byte and new high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn paired_lcdc_writes_while_visible_tile_is_latched_update_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 0;
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 5;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(5, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "later TILE_SEL writes during output of an already-latched tile must not alter that visible tile"
        );
        assert_eq!(
            next_tile_colour, 3,
            "the following fetch should retain the TILE_SEL sample from the earlier write until its bitplanes are latched"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_mid_visible_tile_updates_following_bg_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 1;
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(2, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "a TILE_SEL write after the visible tile has started output must not alter the rest of that tile"
        );
        assert_eq!(
            next_tile_colour, 3,
            "the following BG fetch should sample the new TILE_SEL value"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn paired_lcdc_writes_late_in_visible_tile_mix_following_fetches() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 2;
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        renderer.next_x = 10;
        renderer.record_lcdc_write(0x91, 0x81, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let vram = vram_with_blank_signed_and_solid_unsigned_tiles();
        let oam = [0u8; 0xA0];

        let (current_tile_colour, is_sprite, _) =
            renderer.dmg_pixel_layers(3, &vram, &oam, &registers, 0);
        let (next_tile_colour, _, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);
        let (following_tile_colour, _, _) =
            renderer.dmg_pixel_layers(16, &vram, &oam, &registers, 0);

        assert_eq!(
            current_tile_colour, 0,
            "late TILE_SEL writes must not alter the rest of the visible tile"
        );
        assert_eq!(
            next_tile_colour, 2,
            "the following fetch should keep the previous low byte and sample the new high byte"
        );
        assert_eq!(
            following_tile_colour, 1,
            "the restore write should let the next fetch keep the new low byte and sample the previous high byte"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn record_lcdc_write_mid_tile_does_not_bleed_tile_data_samples_into_next_tile() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 1;
        renderer.record_lcdc_write(0x81, 0x91, 0, false, false);
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.bgp = 0xE4;
        let mut vram = vram_with_mixed_bg_tile_select_sources();
        vram[0x1801] = 0x02;
        vram[0x1020] = 0x80;
        vram[0x0020] = 0x00;
        vram[0x0021] = 0x80;
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            colour_index, 2,
            "the next tile should use the current TILE_SEL for both bitplanes"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn cgb_dmg_compat_tile_select_set_keeps_normal_bg_fetch_phase() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.scanline = 0;
        renderer.next_x = 1;
        renderer.record_lcdc_write(0x81, 0x91, 0, true, true);
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.bgp = 0xE4;
        let mut vram = vram_with_mixed_bg_tile_select_sources();
        vram[0x1801] = 0x02;
        vram[0x1020] = 0x80;
        vram[0x0020] = 0x00;
        vram[0x0021] = 0x80;
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(8, &vram, &oam, &registers, 0);

        assert_eq!(
            colour_index, 2,
            "CGB DMG-compat TILE_SEL set should not use the falling-edge glitch phase"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn cgb_dmg_compat_tile_select_set_phase0_uses_cached_low_byte() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 8;

        renderer.record_lcdc_write(0x81, 0x91, 0, true, true);

        assert_eq!(
            renderer.lcdc_tile_data_edge.lcdc_for_tile_data(16, 0x91),
            (
                0x91,
                0x91,
                super::DmgTileByteOverride::CachedData,
                super::DmgTileByteOverride::None
            ),
            "CGB DMG-compat TILE_SEL set in phase 0 should substitute cached data for the next low byte"
        );
    }

    #[test]
    fn cgb_dmg_compat_tile_select_set_phase2_uses_cached_high_byte() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 10;

        renderer.record_lcdc_write(0x81, 0x91, 0, true, true);

        assert_eq!(
            renderer.lcdc_tile_data_edge.lcdc_for_tile_data(16, 0x91),
            (
                0x81,
                0x91,
                super::DmgTileByteOverride::None,
                super::DmgTileByteOverride::CachedData
            ),
            "CGB DMG-compat TILE_SEL set in phase 2 should keep the previous low byte and substitute cached data for the high byte"
        );
    }

    #[test]
    fn cgb_dmg_compat_tile_select_set_phase2_at_left_edge_delays_following_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 2;

        renderer.record_lcdc_write(0x81, 0x91, 0, true, true);

        assert_eq!(
            renderer.lcdc_tile_data_edge.lcdc_for_tile_data(8, 0x91),
            (
                0x81,
                0x81,
                super::DmgTileByteOverride::None,
                super::DmgTileByteOverride::None
            ),
            "the first visible CGB DMG-compat fetch should not sample a phase-2 set until the later fetch"
        );
    }

    #[test]
    fn cgb_dmg_compat_tile_select_restore_phase2_keeps_cached_current_fetch_mix() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 10;
        renderer.record_lcdc_write(0x81, 0x91, 0, true, true);
        renderer.next_x = 18;

        renderer.record_lcdc_write(0x91, 0x81, 0, true, true);

        assert_eq!(
            renderer.lcdc_tile_data_edge.lcdc_for_tile_data(16, 0x81),
            (
                0x81,
                0x91,
                super::DmgTileByteOverride::None,
                super::DmgTileByteOverride::CachedData
            ),
            "the falling-edge glitch in phase 2 must not drop the cached high-byte sample from the already-sampled fetch"
        );
    }

    #[test]
    fn cgb_dmg_compat_tile_select_restore_during_obj_stall_keeps_previous_fetch_on_tile_row_start()
    {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 8;
        renderer.pending_obj_stall_dots = 3;
        renderer.lcdc_tile_data_edge.record_write(8, 15, 0x81, 0x81);
        renderer
            .lcdc_tile_data_edge
            .record_write(16, 23, 0x91, 0x91);

        renderer.record_lcdc_write(0x91, 0x81, 0, true, true);

        assert_eq!(
            renderer.lcdc_tile_data_edge.lcdc_for_tile_data(16, 0x81),
            (
                0x91,
                0x91,
                super::DmgTileByteOverride::None,
                super::DmgTileByteOverride::None
            ),
            "a restore during the OBJ stall at the tile row start should keep the following fetch on the previous TILE_SEL sample"
        );
    }

    #[test]
    fn recorded_tile_data_samples_use_latest_overlapping_range() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.scanline = 0;
        renderer.lcdc_tile_data_edge.record_write(0, 7, 0x81, 0x91);
        renderer.lcdc_tile_data_edge.record_write(3, 7, 0x91, 0x81);
        let mut registers = Registers::new();
        registers.lcdc = 0x81;
        registers.bgp = 0xE4;
        let mut vram = [0u8; 0x2000];
        vram[0x1800] = 0x01;
        vram[0x1010] = 0x04;
        vram[0x1011] = 0x04;
        vram[0x0010] = 0x00;
        vram[0x0011] = 0x04;
        let oam = [0u8; 0xA0];

        let (colour_index, is_sprite, _) = renderer.dmg_pixel_layers(5, &vram, &oam, &registers, 0);

        assert_eq!(
            colour_index, 2,
            "overlapping tile-data ranges should use the most recent recorded sample"
        );
        assert!(!is_sprite);
    }

    #[test]
    fn lcdc_bg_enable_edge_uses_new_value_for_delayed_current_pixel() {
        // Given: an LCDC bit 0 write lands before the current pixel has been pushed
        // because an object fetch is still delaying that pixel.
        let mut edge = LcdcBgEnableEdge::default();
        edge.record_write(12, 0x93, 0x92, LcdcBgEnableEdgeTiming::CurrentPixelUsesNew);

        // When/Then: the delayed current pixel observes the new BG/window-enable bit.
        assert!(!edge.bg_window_enabled_for_pixel(12, 0x93));
    }

    #[test]
    fn lcdc_bg_enable_edge_can_keep_previous_value_for_current_pixel() {
        let mut edge = LcdcBgEnableEdge::default();
        edge.record_write(
            0,
            0x93,
            0x92,
            LcdcBgEnableEdgeTiming::CurrentPixelUsesPrevious,
        );

        assert!(edge.bg_window_enabled_for_pixel(0, 0x92));
        assert!(!edge.bg_window_enabled_for_pixel(1, 0x92));
    }

    #[test]
    fn lcdc_bg_enable_edge_holds_previous_value_until_cgb_dmg_compat_delay_ends() {
        // Given: CGB DMG-compat observes this LCDC bit 0 edge one output pixel
        // later than DMG on the mealybug timing ROMs.
        let mut edge = LcdcBgEnableEdge::default();
        edge.record_write(
            12,
            0x93,
            0x92,
            LcdcBgEnableEdgeTiming::HoldPreviousForOneExtraPixel,
        );

        // When/Then: the previous value is held through the delayed boundary.
        assert!(edge.bg_window_enabled_for_pixel(12, 0x93));
        assert!(edge.bg_window_enabled_for_pixel(13, 0x92));
        assert!(!edge.bg_window_enabled_for_pixel(14, 0x92));
    }

    #[test]
    fn lcdc_bg_enable_edge_clears_after_consumed_pixel() {
        // Given: an edge was recorded for the current output pixel.
        let mut edge = LcdcBgEnableEdge::default();
        edge.record_write(12, 0x93, 0x92, LcdcBgEnableEdgeTiming::CurrentPixelUsesNew);

        // When: that pixel is consumed.
        edge.clear_consumed(12);

        // Then: later resolution falls back to the current LCDC bit.
        assert!(edge.bg_window_enabled_for_pixel(12, 0x93));
    }

    #[test]
    fn record_lcdc_write_keeps_previous_on_cgb_left_edge_final_obj_stall_dot() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 0;
        renderer.obj_stall_events.clear();
        renderer.pending_obj_stall_dots = 1;

        renderer.record_lcdc_write(0x93, 0x92, 0, true, true);

        assert!(
            renderer
                .lcdc_bg_enable_edge
                .bg_window_enabled_for_pixel(0, 0x92)
        );
        assert!(
            !renderer
                .lcdc_bg_enable_edge
                .bg_window_enabled_for_pixel(1, 0x92)
        );
    }

    #[test]
    fn record_lcdc_write_uses_new_on_cgb_left_edge_before_final_obj_stall_dot() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 0;
        renderer.pending_obj_stall_dots = 2;

        renderer.record_lcdc_write(0x93, 0x92, 0, true, true);

        assert!(
            !renderer
                .lcdc_bg_enable_edge
                .bg_window_enabled_for_pixel(0, 0x93)
        );
    }

    #[test]
    fn cgb_dmg_compat_fifo_stalls_for_sprite_even_when_lcdc_obj_enable_is_clear() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x91;
        registers.obp0 = 0xE4;
        let oam = oam_with_sprite_at(16, 8, 1, 0);
        let mut vram = [0u8; 0x2000];
        vram[0x0010] = 0x80;
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        tick_renderer(
            &mut renderer,
            96,
            &vram,
            &oam,
            &registers,
            true,
            true,
            &mut screen_buffer,
        );

        assert_eq!(
            renderer.next_x, 0,
            "CGB DMG-compat object fetches should delay the production FIFO even when LCDC.1 is clear"
        );

        renderer.record_lcdc_write(0x91, 0x93, registers.scx, true, true);
        registers.lcdc = 0x93;
        for dot in 97..112 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                true,
                true,
                &mut screen_buffer,
            );
        }

        assert_eq!(
            screen_buffer.get_pixel(0, 0),
            (255, 255, 255),
            "CGB DMG-compat should fetch the sprite while LCDC.1 is clear and render it if LCDC.1 is enabled before the pixel is mixed"
        );
    }

    #[test]
    fn dmg_fifo_suppresses_sprite_pixels_when_lcdc_obj_enable_turns_off_mid_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x93;
        registers.bgp = 0xE4;
        registers.obp0 = 0xE4;
        let oam = oam_with_sprite_at(16, 8, 1, 0);
        let mut vram = [0u8; 0x2000];
        vram[0x0010] = 0x80;
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, false, false);
        tick_renderer(
            &mut renderer,
            96,
            &vram,
            &oam,
            &registers,
            false,
            false,
            &mut screen_buffer,
        );
        renderer.record_lcdc_write(0x93, 0x91, registers.scx, false, false);
        registers.lcdc = 0x91;
        for dot in 97..112 {
            tick_renderer(
                &mut renderer,
                dot,
                &vram,
                &oam,
                &registers,
                false,
                false,
                &mut screen_buffer,
            );
        }

        assert_eq!(
            screen_buffer.get_pixel(0, 0),
            (255, 255, 255),
            "DMG object fetch cancellation should suppress the fetched sprite pixel in the production FIFO"
        );
    }

    // ── OBP0 / OBP1 mid-Mode-3 write edge tests ──────────────────────────────
    //
    // The mealybug m3_obp0_change test shows that CGB-C uses the *previous*
    // OBP0 value at the write-boundary pixel while CGB-D uses the *new* value
    // (analogous to the existing BGP edge-tracking behaviour).
    //
    // Scenario: sprite at screen X=8 with all pixels having colour_index=1.
    // obj_palette_ram (from tick_renderer helper):
    //   palette 0, entry 0 (mapped_index=0) → 0x0000 → (0, 0, 0) black
    //   palette 0, entry 1 (mapped_index=1) → 0x7FFF → (255,255,255) white
    //
    // OBP0 prev=0xE4: dmg_palette_index(0xE4, 1) = 1 → white
    // OBP0 new =0x00: dmg_palette_index(0x00, 1) = 0 → black
    //
    // So at the write-boundary pixel (screen X=8):
    //   CGB-C must render white  (previous value)
    //   CGB-D must render black  (new value)

    fn vram_with_solid_obj_tile() -> [u8; 0x2000] {
        // Tile 1 (at 0x0010): all pixels have colour_index=1
        // lo-byte=0xFF, hi-byte=0x00 → colour = (0<<1)|1 = 1 for every pixel
        let mut vram = [0u8; 0x2000];
        vram[0x0010] = 0xFF;
        vram[0x0011] = 0x00;
        vram
    }

    fn tick_cgb_compat_until_next_x(
        renderer: &mut PixelFifoRenderer,
        dot: &mut u16,
        target_next_x: u8,
        vram: &[u8; 0x2000],
        oam: &[u8; 0xA0],
        registers: &Registers,
        screen_buffer: &mut ScreenBuffer,
    ) {
        let deadline = dot.saturating_add(512);
        while renderer.next_x < target_next_x {
            tick_renderer(
                renderer,
                *dot,
                vram,
                oam,
                registers,
                true,
                true,
                screen_buffer,
            );
            *dot = dot.saturating_add(1);
            assert!(
                *dot < deadline,
                "renderer did not reach next_x={target_next_x}"
            );
        }
    }

    #[test]
    fn cgb_c_obp0_write_mid_mode3_uses_previous_value_at_write_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83; // LCD on, BG+OBJ enabled, obj tile data at 0x8000
        registers.bgp = 0x00; // bg = all white (transparent background)
        registers.obp0 = 0xE4; // initial: colour_index 1 → mapped 1 → white in obj_palette_ram
        let oam = oam_with_sprite_at(16, 16, 1, 0); // renders at screen X=8
        let vram = vram_with_solid_obj_tile();
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        // Advance to exactly before screen X=8 is output
        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            8,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        // Record OBP0 write with no OBJ fetch delay (prev=0xE4→white, new=0x00→black).
        renderer.pending_obj_stall_dots = 0;
        renderer.obj_stall_events.clear();
        renderer.record_obp0_write(0xE4, 0x00, true, true, CgbModel::CgbC);
        registers.obp0 = 0x00;

        // Advance through sprite fetch stall and render pixel at screen X=8
        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            9,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (255, 255, 255),
            "CGB-C: OBP0 write at the pixel boundary should use the *previous* palette value"
        );
    }

    #[test]
    fn cgb_c_obp0_write_during_obj_fetch_delay_uses_new_value_at_delayed_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83;
        registers.bgp = 0x00;
        registers.obp0 = 0xE4;
        let oam = oam_with_sprite_at(16, 16, 1, 0);
        let vram = vram_with_solid_obj_tile();
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            8,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        renderer.obj_stall_events.clear();
        renderer.pending_obj_stall_dots = 1;
        renderer.record_obp0_write(0xE4, 0x00, true, true, CgbModel::CgbC);
        registers.obp0 = 0x00;

        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            9,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (0, 0, 0),
            "CGB-C: OBP0 writes during OBJ fetch delay should affect the delayed pixel"
        );
    }

    #[test]
    fn cgb_d_obp0_write_mid_mode3_uses_new_value_at_write_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83;
        registers.bgp = 0x00;
        registers.obp0 = 0xE4;
        let oam = oam_with_sprite_at(16, 16, 1, 0);
        let vram = vram_with_solid_obj_tile();
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            8,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        // Record OBP0 write — CGB-D model
        renderer.record_obp0_write(0xE4, 0x00, true, true, CgbModel::CgbD);
        registers.obp0 = 0x00;

        // Advance through sprite fetch stall and render pixel at screen X=8
        tick_cgb_compat_until_next_x(
            &mut renderer,
            &mut dot,
            9,
            &vram,
            &oam,
            &registers,
            &mut screen_buffer,
        );

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (0, 0, 0),
            "CGB-D: OBP0 write at the pixel boundary should use the *new* palette value"
        );
    }

    #[test]
    fn cgb_c_obp1_write_mid_mode3_uses_previous_value_at_write_pixel() {
        // Same as OBP0 test but with OBP1 (OAM attr bit 4 = 1 selects OBP1)
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83;
        registers.bgp = 0x00;
        registers.obp0 = 0x00; // not used
        registers.obp1 = 0xE4; // initial: colour_index 1 → mapped 1 → white via obj_palette_ram[8..9]
        // OBP1 sprites use palette_ram slot 1 (bytes 8..15).
        // Build custom obj_palette_ram: palette 1, entry 1 = 0x7FFF (white).
        let mut obj_palette_ram = [0u8; 64];
        obj_palette_ram[10] = 0xFF; // palette 1, entry 1, lo
        obj_palette_ram[11] = 0x7F; // palette 1, entry 1, hi → 0x7FFF = white

        let oam = oam_with_sprite_at(16, 16, 1, 0x10); // attr bit4=1 → OBP1
        let vram = vram_with_solid_obj_tile();
        let vram_bank1 = [0u8; 0x2000];
        let bg_palette_ram = [0u8; 64];
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        // Advance until next_x=8 (sprite at screen X=8 is about to render)
        let deadline = dot.saturating_add(512);
        while renderer.next_x < 8 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=8");
        }

        // Record OBP1 write with no OBJ fetch delay (prev=0xE4→white, new=0x00→black).
        renderer.pending_obj_stall_dots = 0;
        renderer.obj_stall_events.clear();
        renderer.record_obp1_write(0xE4, 0x00, true, true, CgbModel::CgbC);
        registers.obp1 = 0x00;

        // Advance through sprite fetch stall and render pixel at screen X=8
        let deadline = dot.saturating_add(512);
        while renderer.next_x < 9 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=9");
        }

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (255, 255, 255),
            "CGB-C: OBP1 write at the pixel boundary should use the *previous* palette value"
        );
    }

    #[test]
    fn cgb_c_obp1_write_during_obj_fetch_delay_uses_new_value_at_delayed_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83;
        registers.bgp = 0x00;
        registers.obp0 = 0x00;
        registers.obp1 = 0xE4;
        let mut obj_palette_ram = [0u8; 64];
        obj_palette_ram[10] = 0xFF;
        obj_palette_ram[11] = 0x7F;

        let oam = oam_with_sprite_at(16, 16, 1, 0x10);
        let vram = vram_with_solid_obj_tile();
        let vram_bank1 = [0u8; 0x2000];
        let bg_palette_ram = [0u8; 64];
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        let deadline = dot.saturating_add(512);
        while renderer.next_x < 8 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=8");
        }

        renderer.pending_obj_stall_dots = 1;
        renderer.record_obp1_write(0xE4, 0x00, true, true, CgbModel::CgbC);
        registers.obp1 = 0x00;

        let deadline = dot.saturating_add(512);
        while renderer.next_x < 9 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=9");
        }

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (0, 0, 0),
            "CGB-C: OBP1 writes during OBJ fetch delay should affect the delayed pixel"
        );
    }

    #[test]
    fn cgb_d_obp1_write_mid_mode3_uses_new_value_at_write_pixel() {
        let mut renderer = PixelFifoRenderer::new();
        let mut registers = Registers::new();
        registers.lcdc = 0x83;
        registers.bgp = 0x00;
        registers.obp0 = 0x00;
        registers.obp1 = 0xE4;
        let mut obj_palette_ram = [0u8; 64];
        obj_palette_ram[10] = 0xFF;
        obj_palette_ram[11] = 0x7F;

        let oam = oam_with_sprite_at(16, 16, 1, 0x10); // OBP1
        let vram = vram_with_solid_obj_tile();
        let vram_bank1 = [0u8; 0x2000];
        let bg_palette_ram = [0u8; 64];
        let mut screen_buffer = ScreenBuffer::new();

        renderer.begin_scanline(0, 80, &oam, &registers, true, true);
        let mut dot = 80u16;

        // Advance until next_x=8
        let deadline = dot.saturating_add(512);
        while renderer.next_x < 8 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=8");
        }

        // Record OBP1 write — CGB-D model
        renderer.record_obp1_write(0xE4, 0x00, true, true, CgbModel::CgbD);
        registers.obp1 = 0x00;

        // Advance through sprite fetch stall and render pixel at screen X=8
        let deadline = dot.saturating_add(512);
        while renderer.next_x < 9 {
            renderer.tick(
                dot,
                &vram,
                &vram_bank1,
                &oam,
                &registers,
                &bg_palette_ram,
                &obj_palette_ram,
                0,
                true,
                false,
                true,
                &mut screen_buffer,
            );
            dot = dot.saturating_add(1);
            assert!(dot < deadline, "renderer did not reach next_x=9");
        }

        assert_eq!(
            screen_buffer.get_pixel(8, 0),
            (0, 0, 0),
            "CGB-D: OBP1 write at the pixel boundary should use the *new* palette value"
        );
    }
}
