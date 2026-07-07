//! PPU save-state capture and restore.
//!
//! The transient framebuffer is not serialized (it is regenerated each frame); on restore it is
//! cleared and redrawn over the following frame.

use super::{
    CGRAM_SIZE, OAM_SIZE, Ppu, PpuLineTimingProfile, ScanPosition, SnesVideoRegion,
    VISIBLE_DOT_START, VISIBLE_LINE_START, VRAM_SIZE,
};
use crate::snes::console::save_state::SnesPpuState;

impl Ppu {
    pub(crate) fn capture_state(&self) -> SnesPpuState {
        SnesPpuState {
            vram: self.vram.clone(),
            cgram: self.cgram.clone(),
            oam: self.oam.clone(),
            scanline: self.position.scanline,
            dot: self.position.dot,
            master_cycle_accumulator: self.master_cycle_accumulator,
            line_clock: self.line_clock,
            last_hperiod: self.last_hperiod,
            total_master_clocks: self.total_master_clocks,
            dram_refresh_position: self.dram_refresh_position,
            line_timing_profile: self.line_timing_profile.as_u8(),
            inidisp: self.inidisp,
            nmi_enable: self.nmi_enable,
            nmi_flag: self.nmi_flag,
            vblank_active: self.vblank_active,
            nmi_line_prev: self.nmi_line_prev,
            nmi_edge: self.nmi_edge,
            frame_complete: self.frame_complete,
            vram_increment_after_high: self.vram_increment_after_high,
            vram_increment_step: self.vram_increment_step,
            vram_address: self.vram_address,
            vram_prefetch: self.vram_prefetch,
            cgram_address: self.cgram_address,
            cgram_latch: self.cgram_latch,
            oam_address: self.oam_address,
            oam_latch: self.oam_latch,
            ophct_latch: self.ophct_latch,
            opvct_latch: self.opvct_latch,
            counter_latch_flag: self.counter_latch_flag,
            ophct_read_high: self.ophct_read_high,
            opvct_read_high: self.opvct_read_high,
            wrio: self.wrio,
            irq_mode: self.irq_mode,
            htime: self.htime,
            vtime: self.vtime,
            timeup_flag: self.timeup_flag,
            irq_line: self.irq_line,
            irq_edge_age: self.irq_edge_age,
            interlace_field: self.interlace_field,
            video_region: match self.video_region {
                SnesVideoRegion::Ntsc => 0,
                SnesVideoRegion::Pal => 1,
            },
            bg_mode: self.bg_mode,
            bg3_priority: self.bg3_priority,
            bg_tile_size_16: self.bg_tile_size_16,
            bg_tilemap_base: self.bg_tilemap_base,
            bg_screen_size: self.bg_screen_size,
            bg_char_base: self.bg_char_base,
            bg_hofs: self.bg_hofs,
            bg_vofs: self.bg_vofs,
            bg_old: self.bg_old,
            tm: self.tm,
            ts: self.ts,
            tmw: self.tmw,
            tsw: self.tsw,
            cgwsel: self.cgwsel,
            cgadsub: self.cgadsub,
            coldata: self.coldata,
            w12sel: self.w12sel,
            w34sel: self.w34sel,
            wobjsel: self.wobjsel,
            wh: self.wh,
            wbglog: self.wbglog,
            wobjlog: self.wobjlog,
            setini: self.setini,
            m7a: self.m7a,
            m7b: self.m7b,
            m7c: self.m7c,
            m7d: self.m7d,
            m7x: self.m7x,
            m7y: self.m7y,
            m7hofs: self.m7hofs,
            m7vofs: self.m7vofs,
            m7sel: self.m7sel,
            m7_old: self.m7_old,
            obsel: self.obsel,
            oam_addr_reload: self.oam_addr_reload,
            oam_priority_rotation: self.oam_priority_rotation,
            stat77_range_over: self.stat77_range_over,
            stat77_time_over: self.stat77_time_over,
            obj_range_over_dot: self.obj_range_over_dot,
            obj_time_over_pending: self.obj_time_over_pending,
            obj_eval_dirty: self.obj_eval_dirty,
            mosaic: self.mosaic,
            mosaic_vblock_size: self.mosaic_vblock_size,
            mosaic_vcount: self.mosaic_vcount,
        }
    }

    pub(crate) fn restore_state(&mut self, state: &SnesPpuState) -> Result<(), String> {
        restore_memory(&mut self.vram, &state.vram, VRAM_SIZE, "VRAM")?;
        restore_memory(&mut self.cgram, &state.cgram, CGRAM_SIZE, "CGRAM")?;
        restore_memory(&mut self.oam, &state.oam, OAM_SIZE, "OAM")?;

        self.position = ScanPosition {
            scanline: state.scanline,
            dot: state.dot,
        };
        self.master_cycle_accumulator = state.master_cycle_accumulator;
        self.line_clock = state.line_clock;
        self.last_hperiod = state.last_hperiod;
        self.total_master_clocks = state.total_master_clocks;
        self.dram_refresh_position = state.dram_refresh_position;
        self.line_timing_profile = PpuLineTimingProfile::from_u8(state.line_timing_profile);
        self.inidisp = state.inidisp;
        self.nmi_enable = state.nmi_enable;
        self.nmi_flag = state.nmi_flag;
        self.vblank_active = state.vblank_active;
        self.nmi_line_prev = state.nmi_line_prev;
        self.nmi_edge = state.nmi_edge;
        self.frame_complete = state.frame_complete;
        self.vram_increment_after_high = state.vram_increment_after_high;
        self.vram_increment_step = state.vram_increment_step;
        self.vram_address = state.vram_address;
        self.vram_prefetch = state.vram_prefetch;
        self.cgram_address = state.cgram_address;
        self.cgram_latch = state.cgram_latch;
        self.oam_address = state.oam_address;
        self.oam_latch = state.oam_latch;
        self.ophct_latch = state.ophct_latch;
        self.opvct_latch = state.opvct_latch;
        self.counter_latch_flag = state.counter_latch_flag;
        self.ophct_read_high = state.ophct_read_high;
        self.opvct_read_high = state.opvct_read_high;
        self.wrio = state.wrio;
        self.irq_mode = state.irq_mode & 0x03;
        self.htime = state.htime & 0x01FF;
        self.vtime = state.vtime & 0x01FF;
        self.timeup_flag = state.timeup_flag;
        self.irq_line = state.irq_line;
        self.irq_edge_age = state.irq_edge_age;
        self.interlace_field = state.interlace_field;
        self.video_region = match state.video_region {
            1 => SnesVideoRegion::Pal,
            _ => SnesVideoRegion::Ntsc,
        };
        self.bg_mode = state.bg_mode;
        self.bg3_priority = state.bg3_priority;
        self.bg_tile_size_16 = state.bg_tile_size_16;
        self.bg_tilemap_base = state.bg_tilemap_base;
        self.bg_screen_size = state.bg_screen_size;
        self.bg_char_base = state.bg_char_base;
        self.bg_hofs = state.bg_hofs;
        self.bg_vofs = state.bg_vofs;
        self.bg_old = state.bg_old;
        self.tm = state.tm;
        self.ts = state.ts;
        self.tmw = state.tmw;
        self.tsw = state.tsw;
        self.cgwsel = state.cgwsel;
        self.cgadsub = state.cgadsub;
        self.coldata = state.coldata;
        self.w12sel = state.w12sel;
        self.w34sel = state.w34sel;
        self.wobjsel = state.wobjsel;
        self.wh = state.wh;
        self.wbglog = state.wbglog;
        self.wobjlog = state.wobjlog;
        self.setini = state.setini;
        self.m7a = state.m7a;
        self.m7b = state.m7b;
        self.m7c = state.m7c;
        self.m7d = state.m7d;
        self.m7x = state.m7x;
        self.m7y = state.m7y;
        self.m7hofs = state.m7hofs;
        self.m7vofs = state.m7vofs;
        self.m7sel = state.m7sel;
        self.m7_old = state.m7_old;
        self.obsel = state.obsel;
        self.oam_addr_reload = state.oam_addr_reload;
        self.oam_priority_rotation = state.oam_priority_rotation;
        self.stat77_range_over = state.stat77_range_over;
        self.stat77_time_over = state.stat77_time_over;
        self.obj_range_over_dot = state.obj_range_over_dot;
        self.obj_time_over_pending = state.obj_time_over_pending;
        self.obj_eval_dirty = state.obj_eval_dirty;
        self.mosaic = state.mosaic;
        self.mosaic_vblock_size = state.mosaic_vblock_size;
        self.mosaic_vcount = state.mosaic_vcount;

        // The framebuffer is transient; clear it and let the next frame redraw.
        self.framebuffer.iter_mut().for_each(|p| *p = 0);
        debug_assert_eq!(
            self.framebuffer.len(),
            super::SCREEN_WIDTH_MAX * super::SCREEN_HEIGHT_MAX
        );
        // Per-scanline latched INIDISP is transient too; the next frame relatches it.
        self.line_inidisp.iter_mut().for_each(|v| *v = 0);
        debug_assert_eq!(self.line_inidisp.len(), super::SCREEN_HEIGHT_MAX);
        // The scanline currently being rendered already passed its own latch
        // point (VISIBLE_DOT_START) before this snapshot was taken, so it
        // won't be relatched by render_dot this frame -- restore it directly
        // from the (already-restored) live INIDISP value.
        if self.position.scanline >= VISIBLE_LINE_START
            && self.position.dot > VISIBLE_DOT_START
            && (self.position.scanline as usize)
                < VISIBLE_LINE_START as usize + self.active_screen_height()
        {
            let y = (self.position.scanline - VISIBLE_LINE_START) as usize;
            let row = self.framebuffer_row(y);
            self.line_inidisp[row] = self.inidisp;
        }
        Ok(())
    }
}

/// Restore a memory region from a snapshot: an empty snapshot zeroes it; a mismatched non-empty
/// snapshot is an error.
fn restore_memory(dst: &mut [u8], src: &[u8], expected: usize, name: &str) -> Result<(), String> {
    if src.is_empty() {
        dst.iter_mut().for_each(|b| *b = 0);
        return Ok(());
    }
    if src.len() != expected {
        return Err(format!(
            "PPU {name} size mismatch (expected {expected}, found {})",
            src.len()
        ));
    }
    dst.copy_from_slice(src);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{Ppu, PpuLineTimingProfile};

    #[test]
    fn capture_restore_round_trips_ppu_state() {
        let mut ppu = Ppu::new();
        // Mutate a representative slice of state.
        ppu.write_register(0x2121, 0x00);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12); // cgram[0] = 0x1234
        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0xAB);
        ppu.write_register(0x2119, 0xCD);
        ppu.write_register(0x2100, 0x0F);
        ppu.write_register(0x4200, 0x80);
        ppu.line_timing_profile = PpuLineTimingProfile::Long;
        // BG register state (added in #2760).
        ppu.write_register(0x2105, 0x11); // BGMODE: mode 1, BG1 16x16
        ppu.write_register(0x2107, 0x14); // BG1SC
        ppu.write_register(0x210B, 0x23); // BG12NBA
        ppu.write_register(0x210D, 0x1F); // BG1HOFS (write-twice)
        ppu.write_register(0x210D, 0x02);
        ppu.write_register(0x210E, 0x33); // BG1VOFS
        ppu.write_register(0x212C, 0x13); // TM
        for _ in 0..2000 {
            ppu.tick();
        }

        let snapshot = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&snapshot).unwrap();

        assert_eq!(restored.capture_state(), snapshot);
    }

    #[test]
    fn restore_rejects_wrong_vram_size() {
        let mut ppu = Ppu::new();
        let mut snapshot = ppu.capture_state();
        snapshot.vram = vec![0; 16]; // wrong size

        assert!(ppu.restore_state(&snapshot).is_err());
    }

    #[test]
    fn restore_with_empty_memory_zeroes_it() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2121, 0x00);
        ppu.write_register(0x2122, 0xFF);
        ppu.write_register(0x2122, 0x7F);

        let mut snapshot = ppu.capture_state();
        snapshot.cgram = Vec::new(); // default / missing

        ppu.restore_state(&snapshot).unwrap();
        assert_eq!(ppu.cgram_byte(0x00), 0x00);
    }

    #[test]
    fn restore_mid_scanline_preserves_per_dot_obj_continuity() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // visible output
        ppu.write_register(0x2101, 0x00); // 8x8 OBJ
        ppu.write_register(0x212C, 0x10); // enable OBJ on main screen

        // Backdrop red, OBJ color green.
        ppu.write_register(0x2121, 0);
        ppu.write_register(0x2122, 0x1F);
        ppu.write_register(0x2122, 0x00);
        ppu.write_register(0x2121, 129);
        ppu.write_register(0x2122, 0xE0);
        ppu.write_register(0x2122, 0x03);

        // Tile 0: solid color index 1.
        for row in 0..8usize {
            let r = row * 2;
            ppu.set_vram_byte(r, 0xFF);
            ppu.set_vram_byte(r + 1, 0x00);
            ppu.set_vram_byte(r + 16, 0x00);
            ppu.set_vram_byte(r + 17, 0x00);
        }

        // OBJ0 at x=0,y=0,tile=0,attr=0.
        ppu.set_oam_byte(0x00, 0);
        ppu.set_oam_byte(0x01, 0);
        ppu.set_oam_byte(0x02, 0);
        ppu.set_oam_byte(0x03, 0);
        ppu.set_oam_byte(0x200, 0);

        // Enter active display and stop mid-scanline.
        for _ in 0..((341 + 30) * 4) {
            ppu.tick();
        }

        let snapshot = ppu.capture_state();
        let mut restored = Ppu::new();
        restored.restore_state(&snapshot).unwrap();

        // Advance both PPUs by the same number of dots and require identical visible output.
        for _ in 0..(40 * 4) {
            ppu.tick();
            restored.tick();
        }

        let rgb_original = ppu.screen_snapshot_rgb();
        let rgb_restored = restored.screen_snapshot_rgb();
        let pixel = |rgb: &[u8], x: usize, y: usize| -> [u8; 3] {
            let i = (y * 256 + x) * 3;
            [rgb[i], rgb[i + 1], rgb[i + 2]]
        };

        assert_eq!(restored.position(), ppu.position());
        assert_eq!(pixel(&rgb_restored, 20, 0), pixel(&rgb_original, 20, 0));
        assert_eq!(pixel(&rgb_restored, 40, 0), pixel(&rgb_original, 40, 0));
        assert_eq!(restored.read_register(0x213E), ppu.read_register(0x213E));
    }
}
