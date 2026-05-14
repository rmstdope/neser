use serde::{Deserialize, Serialize};

use super::background;
use super::registers::Registers;
use super::rendering::{self, cgb_palette_lookup, dmg_palette_index};
use super::screen_buffer::ScreenBuffer;
use super::sprites;
use super::window;
use crate::gb::model::CgbModel;

const FETCHER_STARTUP_DOTS: u16 = 16;
const INITIAL_BGP: u8 = 0xFC;
const LCDC_BG_WINDOW_ENABLE: u8 = 0x01;
const LCDC_BG_MAP: u8 = 0x08;

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
        previous_lcdc: u8,
        new_lcdc: u8,
        fetch_delay: LcdcBgMapFetchDelay,
    ) {
        let next_fetch_tile_start = next_x
            .next_multiple_of(8)
            .saturating_add(fetch_delay.pixels());
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelFifoRenderer {
    active: bool,
    scanline: u8,
    mode3_start_dot: u16,
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
    sprite_indices: Vec<usize>,
    #[serde(default)]
    leftmost_obj_oam_x: Option<u8>,
    #[serde(default)]
    lcdc_bg_enable_edge: LcdcBgEnableEdge,
    #[serde(default)]
    lcdc_bg_map_edge: LcdcBgMapEdge,
}

impl PixelFifoRenderer {
    pub fn new() -> Self {
        Self {
            active: false,
            scanline: 0,
            mode3_start_dot: 0,
            next_x: 0,
            window_active: false,
            bgp_edge_active: false,
            bgp_edge_x: 0,
            bgp_edge_value: INITIAL_BGP,
            fine_scroll_delay_dots: 0,
            pending_obj_stall_dots: 0,
            obj_stall_events: Vec::new(),
            next_obj_stall_event: 0,
            sprite_indices: Vec::new(),
            leftmost_obj_oam_x: None,
            lcdc_bg_enable_edge: LcdcBgEnableEdge::default(),
            lcdc_bg_map_edge: LcdcBgMapEdge::default(),
        }
    }

    pub fn begin_scanline(
        &mut self,
        scanline: u8,
        mode3_start_dot: u16,
        oam: &[u8; 0xA0],
        registers: &Registers,
    ) {
        self.active = true;
        self.scanline = scanline;
        self.mode3_start_dot = mode3_start_dot;
        self.next_x = 0;
        self.window_active = false;
        self.bgp_edge_active = false;
        self.bgp_edge_value = registers.bgp;
        self.lcdc_bg_enable_edge = LcdcBgEnableEdge::default();
        self.lcdc_bg_map_edge = LcdcBgMapEdge::default();
        self.fine_scroll_delay_dots = u16::from(registers.scx & 0x07);
        self.pending_obj_stall_dots = 0;
        self.next_obj_stall_event = 0;
        if registers.lcdc & 0x02 != 0 {
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
        self.queue_stall_events_for_next_pixel();
        if self.pending_obj_stall_dots > 0 {
            self.pending_obj_stall_dots -= 1;
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
        self.lcdc_bg_map_edge.clear_consumed(self.next_x);
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

    pub fn record_lcdc_write(&mut self, previous: u8, new: u8, cgb_mode: bool, dmg_compat: bool) {
        if !self.active || self.next_x as u32 >= ScreenBuffer::WIDTH {
            return;
        }

        let waiting_on_obj_fetch = self.is_waiting_on_obj_fetch();
        if !cgb_mode || dmg_compat {
            let fetch_delay = self.lcdc_bg_map_fetch_delay(
                previous,
                new,
                cgb_mode,
                dmg_compat,
                waiting_on_obj_fetch,
            );
            self.lcdc_bg_map_edge
                .record_delayed_write(self.next_x, previous, new, fetch_delay);
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
        let obj_enabled = lcdc & 0x02 != 0;
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
        let obj_enabled = lcdc & 0x02 != 0;
        let win_enabled = lcdc & 0x20 != 0;

        let bg_idx = if bg_window_enabled {
            background::fetch_bg_pixel(
                x,
                self.scanline,
                vram,
                bg_fetch_lcdc,
                registers.scx,
                registers.scy,
            )
        } else {
            0
        };

        let bw_idx = if bg_window_enabled && win_enabled {
            match window::fetch_window_pixel(
                x,
                self.scanline,
                vram,
                lcdc,
                registers.wx,
                registers.wy,
                window_line,
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

        let sprite_px = if obj_enabled {
            sprites::fetch_sprite_pixel(x, self.scanline, &self.sprite_indices, oam, vram, lcdc)
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

    fn queue_stall_events_for_next_pixel(&mut self) {
        while self.next_obj_stall_event < self.obj_stall_events.len() {
            let event = self.obj_stall_events[self.next_obj_stall_event];
            if event.x > self.next_x {
                break;
            }
            self.pending_obj_stall_dots += event.dots;
            self.next_obj_stall_event += 1;
        }
    }

    fn is_waiting_on_obj_fetch(&self) -> bool {
        self.pending_obj_stall_dots > 0
            || self
                .obj_stall_events
                .get(self.next_obj_stall_event)
                .is_some_and(|event| event.x <= self.next_x)
    }
}

impl Default for PixelFifoRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{LcdcBgEnableEdge, LcdcBgEnableEdgeTiming, LcdcBgMapEdge, PixelFifoRenderer};

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
    fn record_lcdc_write_delays_visible_left_edge_obj_bg_map_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 0;
        renderer.leftmost_obj_oam_x = Some(8);

        renderer.record_lcdc_write(0x83, 0x8B, false, false);

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
    fn record_lcdc_write_delays_cgb_dmg_compat_tile_phase_bg_map_fetch() {
        let mut renderer = PixelFifoRenderer::new();
        renderer.active = true;
        renderer.next_x = 15;

        renderer.record_lcdc_write(0xD3, 0xDB, true, true);

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

        renderer.record_lcdc_write(0x93, 0x92, true, true);

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

        renderer.record_lcdc_write(0x93, 0x92, true, true);

        assert!(
            !renderer
                .lcdc_bg_enable_edge
                .bg_window_enabled_for_pixel(0, 0x93)
        );
    }
}
