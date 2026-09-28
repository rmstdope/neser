//! PPU save-state capture and restore.
//!
//! The transient framebuffer is not serialized (it is regenerated each frame); on restore it is
//! cleared and redrawn over the following frame.

use super::{
    CGRAM_SIZE, OAM_SIZE, Ppu, PpuLineTimingProfile, ScanPosition, SnesVideoRegion, VRAM_SIZE,
    VramAddressTranslation,
};
use crate::snes::console::save_state::SnesPpuState;

impl Ppu {
    pub(crate) fn capture_state(&self) -> SnesPpuState {
        // Every field is named, so a new one fails to build until it is saved or marked transient.
        let &Self {
            ref vram,
            ref cgram,
            ref oam,
            ref position,
            master_cycle_accumulator,
            line_clock,
            total_master_clocks,
            dram_refresh_position,
            hdma_init_position,
            ref line_timing_profile,
            inidisp,
            nmi_enable,
            nmi_flag,
            vblank_active,
            nmi_line_prev,
            nmi_edge,
            nmi_edge_arm,
            vram_increment_after_high,
            vram_increment_step,
            ref vram_address_translation,
            vram_address,
            vram_prefetch,
            cgram_address,
            cgram_latch,
            ref cgram_render_index,
            oam_address,
            oam_latch,
            ophct_latch,
            opvct_latch,
            counter_latch_flag,
            ophct_read_high,
            opvct_read_high,
            location_latch_request,
            location_latch_x,
            location_latch_y,
            frame_start_edge: _, // one-shot: raised and consumed within one master clock
            ppu2_open_bus,
            wrio,
            irq_mode,
            htime,
            vtime,
            timeup_flag,
            irq_line,
            hv_h_counter,
            hv_v_counter,
            irq_level,
            need_irq,
            interlace_field,
            frame_has_extra_scanline,
            ref video_region,
            framebuffer: _, // transient: cleared on restore, redrawn next frame
            use_high_res_output,
            line_main: _,       // transient: per-scanline buffer, reset on restore
            line_sub: _,        // transient: per-scanline buffer, reset on restore
            line_main_final: _, // transient: per-scanline buffer, reset on restore
            pixel_inidisp: _,   // transient: rewritten with each pixel drawn
            pending_completed_frames,
            auto_joypad_latch: _, // one-shot: raised and consumed within one master clock
            bg_mode,
            bg3_priority,
            bg_tile_size_16,
            bg_tilemap_base,
            bg_screen_size,
            bg_char_base,
            bg_hofs,
            bg_vofs,
            bg_old,
            bg_old_hofs,
            tm,
            ts,
            tmw,
            tsw,
            cgwsel,
            cgadsub,
            coldata,
            w12sel,
            w34sel,
            wobjsel,
            wh,
            wbglog,
            wobjlog,
            windows: _,    // derived: rebuilt from the window registers on restore
            mask_logic: _, // derived: rebuilt from the window registers on restore
            setini,
            m7a,
            m7b,
            m7c,
            m7d,
            m7x,
            m7y,
            m7hofs,
            m7vofs,
            m7sel,
            m7_old,
            obsel,
            oam_addr_reload,
            oam_priority_rotation,
            stat77_range_over,
            stat77_time_over,
            obj_pipeline: _, // transient: reset on restore, rebuilt by the next scanline
            mosaic,
            mosaic_vblock_size,
            mosaic_vcount,
        } = self;
        SnesPpuState {
            vram: vram.clone(),
            cgram: cgram.clone(),
            oam: oam.clone(),
            scanline: position.scanline,
            dot: position.dot,
            master_cycle_accumulator,
            line_clock,
            total_master_clocks,
            dram_refresh_position,
            hdma_init_position,
            line_timing_profile: line_timing_profile.as_u8(),
            inidisp,
            nmi_enable,
            nmi_flag,
            vblank_active,
            nmi_line_prev,
            nmi_edge,
            nmi_edge_arm,
            pending_completed_frames,
            vram_increment_after_high,
            vram_increment_step,
            vram_address_translation: vram_address_translation.as_u8(),
            vram_address,
            vram_prefetch,
            cgram_address,
            cgram_latch,
            cgram_render_index: cgram_render_index.get(),
            oam_address,
            oam_latch,
            ophct_latch,
            opvct_latch,
            counter_latch_flag,
            ophct_read_high,
            opvct_read_high,
            location_latch_request,
            location_latch_x,
            location_latch_y,
            ppu2_open_bus,
            wrio,
            irq_mode,
            htime,
            vtime,
            timeup_flag,
            irq_line,
            hv_h_counter: Some(hv_h_counter),
            hv_v_counter: Some(hv_v_counter),
            irq_level: Some(irq_level),
            need_irq: Some(need_irq),
            interlace_field,
            frame_has_extra_scanline,
            video_region: video_region.to_state_byte(),
            bg_mode,
            bg3_priority,
            bg_tile_size_16,
            bg_tilemap_base,
            bg_screen_size,
            bg_char_base,
            bg_hofs,
            bg_vofs,
            bg_old,
            bg_old_hofs,
            tm,
            ts,
            tmw,
            tsw,
            cgwsel,
            cgadsub,
            coldata,
            w12sel,
            w34sel,
            wobjsel,
            wh,
            wbglog,
            wobjlog,
            setini,
            m7a,
            m7b,
            m7c,
            m7d,
            m7x,
            m7y,
            m7hofs,
            m7vofs,
            m7sel,
            m7_old,
            obsel,
            oam_addr_reload,
            oam_priority_rotation,
            stat77_range_over,
            stat77_time_over,
            mosaic,
            mosaic_vblock_size,
            mosaic_vcount,
            use_high_res_output: Some(use_high_res_output),
        }
    }

    pub(crate) fn restore_state(&mut self, state: &SnesPpuState) -> Result<(), String> {
        // Every saved field is named; one never restored is an unused binding the gate rejects.
        let &SnesPpuState {
            ref vram,
            ref cgram,
            ref oam,
            scanline,
            dot,
            master_cycle_accumulator,
            line_clock,
            total_master_clocks,
            dram_refresh_position,
            hdma_init_position,
            line_timing_profile,
            inidisp,
            nmi_enable,
            nmi_flag,
            vblank_active,
            nmi_line_prev,
            nmi_edge,
            nmi_edge_arm,
            pending_completed_frames,
            vram_increment_after_high,
            vram_increment_step,
            vram_address_translation,
            vram_address,
            vram_prefetch,
            cgram_address,
            cgram_latch,
            cgram_render_index,
            oam_address,
            oam_latch,
            ophct_latch,
            opvct_latch,
            counter_latch_flag,
            ophct_read_high,
            opvct_read_high,
            location_latch_request,
            location_latch_x,
            location_latch_y,
            ppu2_open_bus,
            wrio,
            irq_mode,
            htime,
            vtime,
            timeup_flag,
            irq_line,
            ref hv_h_counter,
            ref hv_v_counter,
            ref irq_level,
            ref need_irq,
            interlace_field,
            frame_has_extra_scanline,
            video_region,
            bg_mode,
            bg3_priority,
            bg_tile_size_16,
            bg_tilemap_base,
            bg_screen_size,
            bg_char_base,
            bg_hofs,
            bg_vofs,
            bg_old,
            bg_old_hofs,
            tm,
            ts,
            tmw,
            tsw,
            cgwsel,
            cgadsub,
            coldata,
            w12sel,
            w34sel,
            wobjsel,
            wh,
            wbglog,
            wobjlog,
            setini,
            m7a,
            m7b,
            m7c,
            m7d,
            m7x,
            m7y,
            m7hofs,
            m7vofs,
            m7sel,
            m7_old,
            obsel,
            oam_addr_reload,
            oam_priority_rotation,
            stat77_range_over,
            stat77_time_over,
            mosaic,
            mosaic_vblock_size,
            mosaic_vcount,
            use_high_res_output,
        } = state;
        restore_memory(&mut self.vram, vram, VRAM_SIZE, "VRAM")?;
        restore_memory(&mut self.cgram, cgram, CGRAM_SIZE, "CGRAM")?;
        restore_memory(&mut self.oam, oam, OAM_SIZE, "OAM")?;

        self.position = ScanPosition { scanline, dot };
        self.master_cycle_accumulator = master_cycle_accumulator;
        self.line_clock = line_clock;
        self.total_master_clocks = total_master_clocks;
        self.dram_refresh_position = dram_refresh_position;
        self.hdma_init_position = hdma_init_position;
        self.line_timing_profile = PpuLineTimingProfile::from_u8(line_timing_profile);
        self.inidisp = inidisp;
        self.nmi_enable = nmi_enable;
        self.nmi_flag = nmi_flag;
        self.vblank_active = vblank_active;
        self.nmi_line_prev = nmi_line_prev;
        self.nmi_edge = nmi_edge;
        self.nmi_edge_arm = nmi_edge_arm;
        self.pending_completed_frames = pending_completed_frames;
        self.vram_increment_after_high = vram_increment_after_high;
        self.vram_increment_step = vram_increment_step;
        self.vram_address_translation = VramAddressTranslation::from_u8(vram_address_translation);
        self.vram_address = vram_address;
        self.vram_prefetch = vram_prefetch;
        self.cgram_address = cgram_address;
        self.cgram_latch = cgram_latch;
        self.cgram_render_index.set(cgram_render_index);
        self.oam_address = oam_address;
        self.oam_latch = oam_latch;
        self.ophct_latch = ophct_latch;
        self.opvct_latch = opvct_latch;
        self.counter_latch_flag = counter_latch_flag;
        self.ophct_read_high = ophct_read_high;
        self.opvct_read_high = opvct_read_high;
        self.location_latch_request = location_latch_request;
        self.location_latch_x = location_latch_x;
        self.location_latch_y = location_latch_y;
        self.ppu2_open_bus = ppu2_open_bus;
        self.wrio = wrio;
        self.irq_mode = irq_mode & 0x03;
        self.htime = htime & 0x01FF;
        self.vtime = vtime & 0x01FF;
        self.timeup_flag = timeup_flag;
        self.irq_line = irq_line;
        // IRQ counter circuit (#3144): states written before the circuit
        // existed carry `None` here (their `irq_edge_age` key is ignored).
        // Re-derive the counters the circuit would hold at the restored scan
        // position. H is reset on the ticks at clocks 6 and 10 and counts
        // 4-clock ticks from there; before that it still holds the previous
        // line's wrap value (338, bumped to 339 by the clock-2 increment) --
        // exact for the normal 1364-clock line, one off across a short/long
        // line, the only approximation left. V increments on the clock-6 tick
        // (resets on the clock-2 tick of scanline 0), so before that tick it
        // still holds the previous line -- on scanline 0, the previous frame's
        // last. The level is re-derived from the counters so a legacy state
        // resumed mid-matching-line doesn't see a spurious rising edge on its
        // first circuit tick (nor, with HTIME=0, a swallowed clock-6 edge);
        // the idle countdown means a legacy state captured mid-countdown
        // loses at most that one in-flight IRQ.
        let lc = self.line_clock;
        self.hv_h_counter = hv_h_counter.unwrap_or(if lc >= 10 {
            (lc - 10) / 4
        } else if lc >= 6 {
            0
        } else if lc >= 2 {
            339
        } else {
            338
        });
        self.hv_v_counter = hv_v_counter.unwrap_or_else(|| {
            if self.position.scanline == 0 {
                if lc >= 2 {
                    0
                } else {
                    self.effective_scanlines_per_frame() - 1
                }
            } else if lc >= 6 {
                self.position.scanline
            } else {
                self.position.scanline - 1
            }
        });
        self.irq_level = irq_level.unwrap_or_else(|| self.compute_irq_level());
        self.need_irq = need_irq.unwrap_or(0);
        self.interlace_field = interlace_field;
        self.frame_has_extra_scanline = frame_has_extra_scanline;
        self.video_region = SnesVideoRegion::from_state_byte(video_region);
        self.bg_mode = bg_mode;
        self.bg3_priority = bg3_priority;
        self.bg_tile_size_16 = bg_tile_size_16;
        self.bg_tilemap_base = bg_tilemap_base;
        self.bg_screen_size = bg_screen_size;
        self.bg_char_base = bg_char_base;
        self.bg_hofs = bg_hofs;
        self.bg_vofs = bg_vofs;
        self.bg_old = bg_old;
        self.bg_old_hofs = bg_old_hofs;
        self.tm = tm;
        self.ts = ts;
        self.tmw = tmw;
        self.tsw = tsw;
        self.cgwsel = cgwsel;
        self.cgadsub = cgadsub;
        self.coldata = coldata;
        self.w12sel = w12sel;
        self.w34sel = w34sel;
        self.wobjsel = wobjsel;
        self.wh = wh;
        self.wbglog = wbglog;
        self.wobjlog = wobjlog;
        // The decoded window cache is derived from the raw registers above, so
        // it must be rebuilt rather than persisted (#3011).
        self.decode_window_registers();
        self.setini = setini;
        self.m7a = m7a;
        self.m7b = m7b;
        self.m7c = m7c;
        self.m7d = m7d;
        self.m7x = m7x;
        self.m7y = m7y;
        self.m7hofs = m7hofs;
        self.m7vofs = m7vofs;
        self.m7sel = m7sel;
        self.m7_old = m7_old;
        self.obsel = obsel;
        self.oam_addr_reload = oam_addr_reload;
        self.oam_priority_rotation = oam_priority_rotation;
        self.stat77_range_over = stat77_range_over;
        self.stat77_time_over = stat77_time_over;
        // The OBJ pipeline is transient like the framebuffer: reset it and let the next
        // scanline's eval/fetch windows rebuild it. After a mid-active-scanline load the
        // row in progress renders without OBJ pixels (its presented line and the partially
        // run eval window are lost); rows from the next scanline onward are correct. This
        // one-line artifact matches the transient-framebuffer policy above.
        self.obj_pipeline = super::sprites::ObjPipeline::default();
        self.mosaic = mosaic;
        self.mosaic_vblock_size = mosaic_vblock_size;
        self.mosaic_vcount = mosaic_vcount;

        // The framebuffer is transient; clear it and let the next frame redraw.
        self.framebuffer.iter_mut().for_each(|p| *p = 0);
        // The hires layout latch is NOT derivable from the restored registers, even
        // though the framebuffer it describes is transient: it never clears mid-frame,
        // so a state captured after a hires -> native switch has it set while the
        // registers read native. Deriving there would shrink the resumed frame to
        // 256x224 and change every remaining dot's pixel addressing. States written
        // before the field existed carry `None` and fall back to deriving, which is the
        // best guess available for them.
        self.use_high_res_output = use_high_res_output
            .unwrap_or_else(|| self.hires_output_enabled() || self.interlace_enabled());
        debug_assert_eq!(
            self.framebuffer.len(),
            super::SCREEN_WIDTH_MAX * super::SCREEN_HEIGHT_MAX
        );
        // The per-scanline resolve/finalize buffers are transient like the framebuffer;
        // the next rendered line refills them left-to-right before any read.
        self.line_main = [super::ScreenPixel::default(); super::SCREEN_WIDTH];
        self.line_sub = [super::ScreenPixel::default(); super::SCREEN_WIDTH];
        self.line_main_final = [0; super::SCREEN_WIDTH];
        // The INIDISP each pixel was drawn with is transient like the pixel itself.
        self.pixel_inidisp.iter_mut().for_each(|v| *v = 0);
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
    use super::super::{
        DOTS_PER_SCANLINE, HDMA_INIT_BASE_POSITION, MASTER_CYCLES_PER_DOT, Ppu,
        PpuLineTimingProfile,
    };

    fn tick_scanlines(ppu: &mut Ppu, lines: u32) {
        for _ in 0..(u32::from(DOTS_PER_SCANLINE) * MASTER_CYCLES_PER_DOT * lines) {
            ppu.tick();
        }
    }

    #[test]
    fn save_state_round_trips_the_hdma_init_jitter() {
        // The once-per-frame HDMA reload fires at `12 + (total master clocks & 7)`,
        // recomputed at every scanline wrap. `dram_refresh_position` is captured but
        // this one was not, so a state restored on scanline 0 before the trigger
        // would reload HDMA up to 7 master clocks early, on the unjittered base.
        // The jitter is `(total master clocks & 7)`, and a 1364-clock scanline advances that
        // phase by 4 -- so it alternates between 4 and 0 from one line to the next, and only
        // every other line carries a value distinguishable from the unjittered default. Tick
        // until one does instead of hard-coding a line count, which would turn any change to
        // the scanline length or the start phase into a spurious failure here rather than in
        // whatever actually broke.
        let mut ppu = Ppu::new();
        let mut lines = 0;
        while ppu.hdma_init_position == HDMA_INIT_BASE_POSITION {
            lines += 1;
            assert!(
                lines <= 8,
                "no jittered HDMA init position within 8 scanlines -- the trigger clock is no \
                 longer phase-dependent, so this test can no longer tell a restored value from \
                 the default"
            );
            tick_scanlines(&mut ppu, 1);
        }
        let expected = ppu.hdma_init_position;
        let state = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");
        assert_eq!(
            restored.hdma_init_position, expected,
            "the HDMA init trigger clock must survive a save-state round trip"
        );
    }

    #[test]
    fn save_state_round_trips_the_hofs_low_bits_latch() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x210F, 0x05); // BG2HOFS loads the HOFS-only latch
        let state = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");
        restored.write_register(0x210D, 0x00); // BG1HOFS takes bits 0-2 from it
        assert_eq!(restored.bg_hofs[0] & 0x07, 0x05);
    }

    #[test]
    fn save_state_round_trips_the_cgram_render_index() {
        // The CGRAM render cursor decides where mid-render CGRAM writes land, so a
        // state saved mid-frame must restore it for deterministic resume.
        let ppu = Ppu::new();
        ppu.cgram_render_index.set(0x42);
        let snapshot = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&snapshot).expect("restore");
        assert_eq!(restored.cgram_render_index.get(), 0x42);
    }

    #[test]
    fn restore_keeps_the_hires_latch_of_a_frame_that_already_left_hires() {
        // The latch is deliberately NOT a function of the live registers: it never
        // clears mid-frame, so a frame that started hires and switched back to a
        // native mode is still a hires frame. Re-deriving it from the restored
        // registers would shrink the resumed frame to 256x224 and change every
        // remaining dot's pixel addressing -- save/restore has to reproduce the
        // frame the run would have produced, not a narrower one.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2105, 0x05); // mode 5 before the frame starts
        tick_scanlines(&mut ppu, 20);
        assert!(ppu.use_high_res_output, "the frame latched hires");

        ppu.write_register(0x2105, 0x01); // back to a native mode, mid-frame
        assert!(
            ppu.use_high_res_output,
            "the latch must not clear mid-frame -- precondition for this test"
        );
        let state = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");
        assert!(
            restored.use_high_res_output,
            "a mid-frame state captured after a hires->native switch must resume \
             in the hires layout, even though its registers now read native"
        );
        assert_eq!(restored.frame_dimensions(), (512, 448));
    }

    #[test]
    fn the_hires_layout_latch_round_trips_in_both_directions() {
        let latched = {
            let mut ppu = Ppu::new();
            ppu.write_register(0x2133, 0x08); // pseudo-hires before the frame starts
            tick_scanlines(&mut ppu, 3);
            assert!(ppu.use_high_res_output);
            ppu.capture_state()
        };
        let native = Ppu::new().capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&latched).expect("restore");
        assert!(
            restored.use_high_res_output,
            "a latched frame resumes hires"
        );

        // ...and the converse, so this isn't just "always true".
        restored.restore_state(&native).expect("restore");
        assert!(
            !restored.use_high_res_output,
            "an unlatched frame resumes native"
        );
    }

    #[test]
    fn a_state_written_before_the_latch_field_existed_derives_it_from_the_registers() {
        // `use_high_res_output` is `None` in states saved before #3034 added it.
        // Deriving from the registers is wrong only for the mid-frame-downgrade case
        // covered above -- for every other state it reproduces the right value, and it
        // is the best guess available for a state that never recorded one.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2105, 0x05); // mode 5
        let mut legacy = ppu.capture_state();
        legacy.use_high_res_output = None;

        let mut restored = Ppu::new();
        restored.restore_state(&legacy).expect("restore");
        assert!(
            restored.use_high_res_output,
            "a legacy state whose registers say mode 5 resumes hires"
        );

        let mut legacy_native = Ppu::new().capture_state();
        legacy_native.use_high_res_output = None;
        restored.restore_state(&legacy_native).expect("restore");
        assert!(
            !restored.use_high_res_output,
            "a legacy state whose registers say native resumes native"
        );
    }

    #[test]
    fn hv_irq_circuit_state_round_trips_mid_countdown() {
        // Capture between TIMEUP setting (clock 10 of the VTIME line) and the
        // CPU line rising (clock 14): the restored circuit must finish the
        // in-flight `need_irq` countdown on schedule.
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0x02); // VTIME = 2, V-only IRQ
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x20);
        tick_scanlines(&mut ppu, 2);
        for _ in 0..11 {
            ppu.tick();
        }
        assert_ne!(ppu.read_register(0x4211) & 0x80, 0, "TIMEUP set at capture");
        assert!(!ppu.poll_irq_dispatch(), "CPU line not yet up at capture");
        let state = ppu.capture_state();

        let mut restored = Ppu::new();
        restored.restore_state(&state).expect("restore");
        assert!(
            !restored.poll_irq_dispatch(),
            "line still down after restore"
        );
        for _ in 0..3 {
            restored.tick();
        }
        assert!(
            restored.poll_irq_dispatch(),
            "the restored countdown raises the CPU line at clock 14 on schedule"
        );
    }

    #[test]
    fn a_state_written_before_the_irq_circuit_existed_derives_its_counters() {
        // The circuit fields are `None` in states saved before #3144 added
        // them. The restore derives the V counter from the scan position and
        // the level from the derived counters -- so a legacy state resumed
        // mid-scanline neither re-fires an already-acknowledged IRQ (the
        // derived level is already high, no rising edge) nor misses the next
        // line's fresh one.
        let mut ppu = Ppu::new();
        ppu.write_register(0x4209, 0x05); // VTIME = 5, V-only IRQ
        ppu.write_register(0x420A, 0x00);
        ppu.write_register(0x4200, 0x20);
        tick_scanlines(&mut ppu, 5);
        for _ in 0..200 {
            ppu.tick();
        }
        // Well past the hold window, so this read also acknowledges.
        assert_ne!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "fired on the VTIME line"
        );
        assert_eq!(ppu.read_register(0x4211) & 0x80, 0, "acknowledged");
        let mut legacy = ppu.capture_state();
        legacy.hv_h_counter = None;
        legacy.hv_v_counter = None;
        legacy.irq_level = None;
        legacy.need_irq = None;

        let mut restored = Ppu::new();
        restored.restore_state(&legacy).expect("restore");
        // No spurious re-fire on the rest of the matching line: the derived
        // level is already high, so the first circuit tick sees no edge.
        for _ in 0..400 {
            restored.tick();
        }
        assert_eq!(
            restored.read_register(0x4211) & 0x80,
            0,
            "an acknowledged legacy IRQ must not re-fire mid-line after restore"
        );

        // ...and the derived V counter resumes normal operation: rewriting
        // VTIME to the next line fires there as usual.
        restored.write_register(0x4209, 0x06);
        tick_scanlines(&mut restored, 1);
        assert_ne!(
            restored.read_register(0x4211) & 0x80,
            0,
            "the next matching line still fires after a legacy restore"
        );
    }

    #[test]
    fn a_legacy_state_derives_the_h_counter_from_the_line_clock() {
        // H-only twin of the test above: HTIME=100 sets TIMEUP at clock
        // 100*4 + 14 = 414 of every line. Capture mid-line at clock 200
        // (real H counter (200-10)/4 = 47), drop the circuit fields, and the
        // restored run must still fire at exactly 414 -- a derivation off by
        // N ticks would shift the fire by 4*N clocks.
        let mut ppu = Ppu::new();
        ppu.write_register(0x4207, 0x64); // HTIME = 100
        ppu.write_register(0x4208, 0x00);
        ppu.write_register(0x4200, 0x10); // H-only IRQ
        tick_scanlines(&mut ppu, 1);
        ppu.read_register(0x4211); // acknowledge line 0's fire
        for _ in 0..200 {
            ppu.tick();
        }
        assert_eq!(
            ppu.read_register(0x4211) & 0x80,
            0,
            "line 1 has not fired yet at clock 200"
        );
        let mut legacy = ppu.capture_state();
        legacy.hv_h_counter = None;
        legacy.hv_v_counter = None;
        legacy.irq_level = None;
        legacy.need_irq = None;

        let mut restored = Ppu::new();
        restored.restore_state(&legacy).expect("restore");
        for _ in 0..213 {
            restored.tick();
        }
        assert_eq!(
            restored.read_register(0x4211) & 0x80,
            0,
            "no fire before clock 414 -- the derived H counter must not run fast"
        );
        restored.tick();
        assert_ne!(
            restored.read_register(0x4211) & 0x80,
            0,
            "the restored line still sets TIMEUP at clock HTIME*4 + 14 = 414"
        );
    }

    #[test]
    fn capture_restore_round_trips_ppu_state() {
        let mut ppu = Ppu::new();
        // Mutate a representative slice of state.
        ppu.write_register(0x2121, 0x00);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12); // cgram[0] = 0x1234
        ppu.write_register(0x2115, 0x8C); // increment-after-high + 10-bit translation (#2989)
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
    fn restore_mid_scanline_recovers_obj_pipeline_from_the_next_row() {
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

        // Enter active display and stop mid-scanline (scanline 1 dot 30, rendering row 0).
        for _ in 0..((341 + 30) * 4) {
            ppu.tick();
        }

        let snapshot = ppu.capture_state();
        let mut restored = Ppu::new();
        restored.restore_state(&snapshot).unwrap();

        // Advance both PPUs through the rest of scanline 1 and all of scanline 2, so row 1
        // (evaluated during the partially-restored scanline 1, fetched at its H=270..339
        // window) is fully rendered on both.
        for _ in 0..((2 * 341) * 4) {
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
        // Row 0 backdrop pixels rendered after the restore match (the OBJ pipeline is
        // transient, so row 0's sprite pixels are documented to drop for this one row).
        assert_eq!(pixel(&rgb_restored, 20, 0), pixel(&rgb_original, 20, 0));
        assert_eq!(pixel(&rgb_restored, 40, 0), pixel(&rgb_original, 40, 0));
        // Row 1 recovers full OBJ output: the sprite (rows 0..7) is drawn identically.
        assert_eq!(
            pixel(&rgb_restored, 4, 1),
            [0, 255, 0],
            "restored PPU redraws the OBJ from the next row on"
        );
        assert_eq!(pixel(&rgb_restored, 4, 1), pixel(&rgb_original, 4, 1));
        assert_eq!(restored.read_register(0x213E), ppu.read_register(0x213E));
    }
}
