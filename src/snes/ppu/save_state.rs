//! PPU save-state capture and restore.
//!
//! The transient framebuffer is not serialized (it is regenerated each frame); on restore it is
//! cleared and redrawn over the following frame.

use super::{CGRAM_SIZE, OAM_SIZE, Ppu, ScanPosition, SnesVideoRegion, VRAM_SIZE};
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
        self.mosaic = state.mosaic;
        self.mosaic_vblock_size = state.mosaic_vblock_size;
        self.mosaic_vcount = state.mosaic_vcount;

        // Reset the transient OBJ runtime state so stale pipeline data from before the load can't
        // leak into the first restored frame (the line buffer is rebuilt as rendering resumes).
        self.obj_line = Default::default();
        self.obj_range_over_dot = None;
        self.obj_time_over_pending = false;

        // The framebuffer is transient; clear it and let the next frame redraw.
        self.framebuffer.iter_mut().for_each(|p| *p = 0);
        debug_assert_eq!(
            self.framebuffer.len(),
            super::SCREEN_WIDTH_MAX * super::SCREEN_HEIGHT_MAX
        );
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
    use super::super::Ppu;

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
}
