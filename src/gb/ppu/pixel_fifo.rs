use serde::{Deserialize, Serialize};

use super::background;
use super::bg_fifo::{self, DmgLayer, DmgPixelFetch};
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
const OBJ_PIXELS_PER_FETCH: u8 = 8;
const CGB_DMG_COMPAT_OBJ_ENABLE_EDGE_PIXELS: u8 = 2;
const DMG_OBJ_FETCH_ABORT_COMPLETES_WHEN_DOTS_REMAINING: u16 = 2;
const DMG_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 4;
const CGB_DMG_COMPAT_OBJ_FETCH_LOW_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 6;
const DMG_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 1;
const CGB_DMG_COMPAT_OBJ_FETCH_HIGH_BYTE_SAMPLE_MIN_DOTS_REMAINING: u16 = 3;

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
struct LcdcTileDataRange {
    start_x: u8,
    end_x: u8,
    low_lcdc: u8,
    high_lcdc: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LcdcTileDataEdge {
    ranges: Vec<LcdcTileDataRange>,
}

impl LcdcTileDataEdge {
    fn record_write(&mut self, start_x: u8, end_x: u8, low_lcdc: u8, high_lcdc: u8) {
        if start_x > end_x {
            return;
        }

        self.ranges.push(LcdcTileDataRange {
            start_x,
            end_x,
            low_lcdc,
            high_lcdc,
        });
    }

    fn lcdc_for_tile_data(&self, x: u32, current_lcdc: u8) -> (u8, u8) {
        self.ranges
            .iter()
            .rev()
            .find(|range| u32::from(range.start_x) <= x && x <= u32::from(range.end_x))
            .map_or((current_lcdc, current_lcdc), |range| {
                (
                    (current_lcdc & !LCDC_TILE_DATA) | (range.low_lcdc & LCDC_TILE_DATA),
                    (current_lcdc & !LCDC_TILE_DATA) | (range.high_lcdc & LCDC_TILE_DATA),
                )
            })
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

    fn clear_consumed(&mut self, next_x: u8) {
        self.ranges.retain(|range| range.end_x != next_x);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelFifoRenderer {
    active: bool,
    scanline: u8,
    mode3_start_dot: u16,
    #[serde(default)]
    scanline_start_lcdc: u8,
    next_x: u8,
    window_active: bool,
    bgp_edge_active: bool,
    bgp_edge_x: u8,
    bgp_edge_value: u8,
    #[serde(default)]
    fine_scroll_delay_dots: u16,
    #[serde(default)]
    pending_obj_stall_dots: u16,
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
    lcdc_tile_data_edge: LcdcTileDataEdge,
    #[serde(default)]
    lcdc_obj_enable_edge: LcdcObjEnableEdge,
}

impl PixelFifoRenderer {
    pub fn new() -> Self {
        Self {
            active: false,
            scanline: 0,
            mode3_start_dot: 0,
            scanline_start_lcdc: 0,
            next_x: 0,
            window_active: false,
            bgp_edge_active: false,
            bgp_edge_x: 0,
            bgp_edge_value: INITIAL_BGP,
            fine_scroll_delay_dots: 0,
            pending_obj_stall_dots: 0,
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
            lcdc_tile_data_edge: LcdcTileDataEdge::default(),
            lcdc_obj_enable_edge: LcdcObjEnableEdge::default(),
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
        self.next_x = 0;
        self.window_active = false;
        self.bgp_edge_active = false;
        self.bgp_edge_value = registers.bgp;
        self.lcdc_bg_enable_edge = LcdcBgEnableEdge::default();
        self.lcdc_bg_map_edge = LcdcBgMapEdge::default();
        self.lcdc_tile_data_edge = LcdcTileDataEdge::default();
        self.lcdc_obj_enable_edge = LcdcObjEnableEdge::default();
        self.fine_scroll_delay_dots = u16::from(registers.scx & 0x07);
        self.pending_obj_stall_dots = 0;
        self.next_obj_stall_event = 0;
        self.active_obj_stall_x = None;
        self.canceled_obj_fetch_ranges.clear();
        self.obj_fetch_lcdc_ranges.clear();
        self.active_obj_fetch_lcdc_range = None;
        self.obj_fetch_ignores_lcdc = ObjFetchModel::for_dmg_render_path(cgb_mode, dmg_compat)
            .is_some_and(ObjFetchModel::ignores_lcdc_obj_enable);
        if self.obj_fetch_ignores_lcdc || registers.lcdc & LCDC_OBJ_ENABLE != 0 {
            sprites::scan_oam_line_into(scanline, oam, registers.lcdc, &mut self.sprite_indices);
            sprites::schedule_obj_penalties(
                &self.sprite_indices,
                oam,
                registers.scx,
                &mut self.obj_stall_events,
            );
            self.leftmost_obj_oam_x = self
                .sprite_indices
                .iter()
                .map(|&index| oam[index * 4 + 1])
                .min();
        } else {
            self.sprite_indices.clear();
            self.obj_stall_events.clear();
            self.leftmost_obj_oam_x = None;
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
    ) -> Option<bool> {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return None;
        }

        let elapsed = dot.saturating_sub(self.mode3_start_dot);
        if elapsed < FETCHER_STARTUP_DOTS + self.fine_scroll_delay_dots {
            return None;
        }
        self.queue_stall_events_for_next_pixel(registers.lcdc);
        if self.pending_obj_stall_dots > 0 {
            self.pending_obj_stall_dots -= 1;
            if self.pending_obj_stall_dots == 0 {
                self.active_obj_stall_x = None;
                self.active_obj_fetch_lcdc_range = None;
            }
            return None;
        }

        let x = u32::from(self.next_x);
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
        self.lcdc_bg_enable_edge.clear_consumed(self.next_x);
        self.lcdc_obj_enable_edge.clear_consumed(self.next_x);
        self.lcdc_bg_map_edge.clear_consumed(self.next_x);
        self.lcdc_tile_data_edge.clear_consumed(self.next_x);
        self.clear_consumed_obj_fetch_lcdc_ranges();
        self.next_x = self.next_x.saturating_add(1);
        if self.next_x as u32 >= ScreenBuffer::WIDTH {
            self.active = false;
            Some(self.window_active)
        } else {
            None
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn record_bgp_write(
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

        self.bgp_edge_active = true;
        self.bgp_edge_x = self.next_x;
        // If an OBJ fetch is delaying this pixel, the BGP write lands before
        // the pixel is pushed to LCD, so the delayed pixel sees the new value.
        self.bgp_edge_value = if self.next_x == 0 || self.is_waiting_on_obj_fetch() {
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

        let waiting_on_obj_fetch = self.is_waiting_on_obj_fetch();
        let window_fetch_active =
            previous & 0x20 != 0 && self.scanline >= wy && self.next_x == wx.saturating_sub(7);
        self.record_lcdc_obj_size_write(previous, new, cgb_mode, dmg_compat);
        self.record_lcdc_tile_data_write(previous, new, scx, window_fetch_active);
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
    }

    fn record_lcdc_tile_data_write(
        &mut self,
        previous: u8,
        new: u8,
        scx: u8,
        window_fetch_active: bool,
    ) {
        if previous & LCDC_TILE_DATA == new & LCDC_TILE_DATA {
            return;
        }

        let bg_phase = scx.wrapping_add(self.next_x) & 0x07;
        let current_fetch_start = self.next_x.saturating_sub(bg_phase);
        let current_fetch_end = current_fetch_start.saturating_add(7);
        let next_fetch_start = current_fetch_start.saturating_add(8);
        if self.should_cancel_delayed_lcdc_tile_data_fetch(previous, new) {
            self.record_visible_left_edge_delayed_lcdc_tile_data_fetch(new);
        }

        if self
            .should_restore_lcdc_tile_data_at_window_obj_delayed_boundary(previous, new, bg_phase)
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
        } else if self
            .should_restore_lcdc_tile_data_at_window_obj_stalled_boundary(previous, new, bg_phase)
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
        } else if !window_fetch_active
            && self.should_restore_lcdc_tile_data_in_delayed_fetch(previous, new)
        {
            self.lcdc_tile_data_edge
                .record_write(current_fetch_start, current_fetch_end, new, new);
            self.record_next_lcdc_tile_data_write(next_fetch_start, bg_phase, previous, new);
        } else if self
            .should_set_lcdc_tile_data_in_delayed_window_obj_fetch(previous, new, bg_phase)
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
        } else if self.should_set_lcdc_tile_data_at_window_obj_late_stall(previous, new, bg_phase) {
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
        } else if self
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
        } else if self
            .should_set_lcdc_tile_data_after_off_left_window_obj_boundary(previous, new, bg_phase)
        {
            self.lcdc_tile_data_edge.record_write(
                next_fetch_start,
                next_fetch_start.saturating_add(7),
                new,
                previous,
            );
        } else if !window_fetch_active
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
        } else if self
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
        } else if self
            .lcdc_tile_data_edge
            .has_latched_range(current_fetch_start, current_fetch_end)
        {
            return;
        } else {
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

    fn should_set_lcdc_tile_data_in_delayed_window_obj_fetch(
        &self,
        previous_lcdc: u8,
        new_lcdc: u8,
        bg_phase: u8,
    ) -> bool {
        let tile_data_turning_on =
            previous_lcdc & LCDC_TILE_DATA == 0 && new_lcdc & LCDC_TILE_DATA != 0;
        tile_data_turning_on
            && self.window_active
            && self.leftmost_obj_oam_x == Some(OBJ_PIXELS_PER_FETCH + 4)
            && self.next_x == 4
            && bg_phase == 4
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
            self.dmg_pixel_layers(x, vram, oam, registers, window_line);
        let (r, g, b) = if is_sprite {
            let palette_reg = if sprite_palette == 0 {
                registers.obp0
            } else {
                registers.obp1
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
        let win_enabled = lcdc & 0x20 != 0;
        let master_priority = lcdc & 0x01 != 0;

        let bg_px = background::fetch_bg_pixel_cgb(
            x,
            self.scanline,
            vram,
            vram_bank1,
            lcdc,
            registers.scx,
            registers.scy,
        );
        let bw_px = if win_enabled {
            match window::fetch_window_pixel_cgb(
                x,
                self.scanline,
                vram,
                vram_bank1,
                lcdc,
                registers.wx,
                registers.wy,
                window_line,
            ) {
                Some(win_px) => {
                    self.window_active = true;
                    win_px
                }
                None => bg_px,
            }
        } else {
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
        let lcdc = registers.lcdc;
        let bg_fetch_lcdc = self.lcdc_bg_map_edge.lcdc_for_bg_fetch(x, lcdc);
        let bg_window_enabled = self
            .lcdc_bg_enable_edge
            .bg_window_enabled_for_pixel(x, lcdc);
        let obj_enabled = self.obj_enabled_for_pixel(lcdc);
        let win_enabled = lcdc & 0x20 != 0;

        let bg_idx = if bg_window_enabled {
            let (low_lcdc, high_lcdc) = self
                .lcdc_tile_data_edge
                .lcdc_for_tile_data(x, bg_fetch_lcdc);
            bg_fifo::fetch_dmg_pixel(
                vram,
                DmgPixelFetch {
                    layer: DmgLayer::Background,
                    x,
                    scanline: self.scanline,
                    scx: registers.scx,
                    scy: registers.scy,
                    wx: registers.wx,
                    wy: registers.wy,
                    window_line,
                    map_lcdc: bg_fetch_lcdc,
                    low_lcdc,
                    high_lcdc,
                },
            )
            .unwrap_or(0)
        } else {
            0
        };

        let bw_idx = if bg_window_enabled && win_enabled {
            let (low_lcdc, high_lcdc) = self.lcdc_tile_data_edge.lcdc_for_tile_data(x, lcdc);
            match bg_fifo::fetch_dmg_pixel(
                vram,
                DmgPixelFetch {
                    layer: DmgLayer::Window,
                    x,
                    scanline: self.scanline,
                    scx: registers.scx,
                    scy: registers.scy,
                    wx: registers.wx,
                    wy: registers.wy,
                    window_line,
                    map_lcdc: lcdc,
                    low_lcdc,
                    high_lcdc,
                },
            ) {
                Some(idx) => {
                    self.window_active = true;
                    idx
                }
                None => bg_idx,
            }
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

    fn clear_consumed_bgp_edge(&mut self) {
        if self.bgp_edge_active && self.bgp_edge_x == self.next_x {
            self.bgp_edge_active = false;
        }
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

    fn is_waiting_on_obj_fetch(&self) -> bool {
        self.pending_obj_stall_dots > 0
            || self
                .obj_stall_events
                .get(self.next_obj_stall_event)
                .is_some_and(|event| event.x <= self.next_x)
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

#[cfg(test)]
mod tests {
    use super::super::{registers::Registers, screen_buffer::ScreenBuffer};
    use super::{
        LcdcBgEnableEdge, LcdcBgEnableEdgeTiming, LcdcBgMapEdge, LcdcBgMapFetchDelay,
        PixelFifoRenderer,
    };

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
    fn off_left_obj_window_restore_after_boundary_mixes_first_following_pixel() {
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
            (170, 170, 170),
            "the first following window pixel should mix the set low byte with the restored high byte"
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
}
