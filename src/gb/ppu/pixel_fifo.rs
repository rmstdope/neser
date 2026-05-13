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
    pending_obj_stall_dots: u16,
    #[serde(default)]
    obj_stall_events: Vec<sprites::ObjPenaltyEvent>,
    #[serde(default)]
    next_obj_stall_event: usize,
    #[serde(default)]
    sprite_indices: Vec<usize>,
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
            pending_obj_stall_dots: 0,
            obj_stall_events: Vec::new(),
            next_obj_stall_event: 0,
            sprite_indices: Vec::new(),
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
        } else {
            self.sprite_indices.clear();
            self.obj_stall_events.clear();
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
        if elapsed < FETCHER_STARTUP_DOTS {
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
        let bg_window_enabled = lcdc & 0x01 != 0;
        let obj_enabled = lcdc & 0x02 != 0;
        let win_enabled = lcdc & 0x20 != 0;

        let bg_idx = if bg_window_enabled {
            background::fetch_bg_pixel(x, self.scanline, vram, lcdc, registers.scx, registers.scy)
        } else {
            0
        };

        let bw_idx = if win_enabled {
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
