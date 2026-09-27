//! Running the NES to a point in time: the end of the frame, the next frame or scanline,
//! or an interrupt handler's entry.
//!
//! These are the only NES loops that decide when running stops. The desktop and web
//! frontends, the debugger, autorun and `frame_bench` all call them, so a change to what
//! ends a frame (nr-3xu: a jammed CPU does not) is made here once.

use std::ops::ControlFlow;

use super::Nes;
use crate::nes::cpu::InterruptKind;

/// Upper bound on instructions for run-to-next-frame and run-to-interrupt, so a debugger
/// action can never hang the frontend. About 67 NTSC frames.
const MAX_FRAME_STEPS: usize = 2_000_000;
/// Upper bound on instructions for run-to-next-scanline.
const MAX_SCANLINE_STEPS: usize = 100_000;

impl Nes {
    /// The one NES frame loop: calls `step` until the PPU has finished a frame or `step`
    /// breaks. `step` executes at most one instruction per call. Returns whether the frame
    /// is ready; the ready flag is left for the caller to clear.
    pub fn run_until_frame_ready(
        &mut self,
        mut step: impl FnMut(&mut Nes) -> ControlFlow<()>,
    ) -> bool {
        while !self.is_ready_to_render() {
            if step(self).is_break() {
                break;
            }
        }
        self.is_ready_to_render()
    }

    /// Run one whole frame and clear the ready flag. Audio samples stay queued for the
    /// caller to drain.
    pub fn run_one_frame(&mut self) {
        self.run_until_frame_ready(|nes| {
            nes.run_cpu_tick();
            ControlFlow::Continue(())
        });
        self.clear_ready_to_render();
    }

    /// Run one whole frame and clear the ready flag, discarding every audio sample as it
    /// is produced. For headless playback and benchmarks, which play no sound.
    pub fn run_one_frame_discarding_audio(&mut self) {
        self.run_until_frame_ready(|nes| {
            nes.run_cpu_tick();
            while nes.get_sample().is_some() {}
            ControlFlow::Continue(())
        });
        self.clear_ready_to_render();
    }

    /// Debugger: run until the PPU's scanline wraps to the next frame.
    pub fn run_to_next_frame(&mut self) {
        let mut previous_scanline = self.ppu().borrow().scanline();
        for _ in 0..MAX_FRAME_STEPS {
            self.run_cpu_tick();
            let scanline = self.ppu().borrow().scanline();
            if scanline < previous_scanline {
                break;
            }
            previous_scanline = scanline;
        }
    }

    /// Debugger: run until the PPU moves to another scanline.
    pub fn run_to_next_scanline(&mut self) {
        let start_scanline = self.ppu().borrow().scanline();
        for _ in 0..MAX_SCANLINE_STEPS {
            self.run_cpu_tick();
            if self.ppu().borrow().scanline() != start_scanline {
                break;
            }
        }
    }

    /// The little-endian handler address stored at an interrupt vector ($FFFA NMI,
    /// $FFFE IRQ/BRK), read without side effects.
    pub fn read_vector_target(&self, vector_addr: u16) -> u16 {
        let memory = self.bus().borrow();
        let lo = memory.read_cpu_for_debugger(vector_addr);
        let hi = memory.read_cpu_for_debugger(vector_addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Debugger: run until the CPU enters a fresh `kind` interrupt at the handler named by
    /// `vector_addr`. If the CPU is already inside that interrupt, it must leave it first.
    /// A jammed CPU can never get there, so it stops at once.
    pub fn run_to_interrupt_entry(&mut self, vector_addr: u16, kind: InterruptKind) {
        let target_pc = self.read_vector_target(vector_addr);
        let mut has_exited_required_interrupt = self.cpu_ref().current_interrupt() != Some(kind);

        for _ in 0..MAX_FRAME_STEPS {
            if self.cpu_ref().is_halted() {
                break;
            }
            self.run_cpu_tick();
            if self.cpu_ref().current_interrupt() != Some(kind) {
                has_exited_required_interrupt = true;
                continue;
            }
            if has_exited_required_interrupt && self.cpu_ref().pc() == target_pc {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::{Cartridge, NametableLayout};
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;

    const NMI_HANDLER: u16 = 0x9000;

    /// A NES running `program` from $8000, with the NMI vector at `NMI_HANDLER` (an RTI).
    fn nes_running(program: &[u8]) -> Nes {
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        let mut prg_rom = vec![0xEAu8; 0x8000];
        prg_rom[..program.len()].copy_from_slice(program);
        prg_rom[0x1000] = 0x40; // $9000: RTI
        prg_rom[0x7FFA..0x7FFC].copy_from_slice(&NMI_HANDLER.to_le_bytes());
        prg_rom[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());
        prg_rom[0x7FFE..0x8000].copy_from_slice(&0x8000u16.to_le_bytes());
        let cart = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cart);
        nes.reset(false);
        nes
    }

    /// $8000: NOP; JMP $8000.
    fn nes_with_nop_loop() -> Nes {
        nes_running(&[0xEA, 0x4C, 0x00, 0x80])
    }

    /// A KIL at the reset target: the CPU jams on its first instruction.
    fn nes_jammed_at_reset() -> Nes {
        let mut nes = nes_running(&[0x02]);
        nes.run_cpu_tick();
        assert!(nes.cpu_ref().is_halted());
        nes
    }

    fn frame_count(nes: &Nes) -> u64 {
        nes.ppu().borrow().timing().frame_count()
    }

    fn scanline(nes: &Nes) -> u16 {
        nes.ppu().borrow().scanline()
    }

    #[test]
    fn run_one_frame_finishes_frames_with_a_jammed_cpu() {
        let mut nes = nes_jammed_at_reset();
        nes.run_one_frame(); // align with a frame boundary
        let frame = frame_count(&nes);

        nes.run_one_frame();
        nes.run_one_frame();

        assert_eq!(frame_count(&nes), frame + 2);
        assert!(!nes.is_ready_to_render(), "the ready flag is cleared");
        assert!(nes.cpu_ref().is_halted(), "the CPU stays jammed");
    }

    #[test]
    fn run_one_frame_keeps_audio_samples_queued() {
        let mut nes = nes_with_nop_loop();
        nes.run_one_frame();
        assert!(nes.sample_ready(), "the caller drains the frame's audio");
    }

    #[test]
    fn run_one_frame_discarding_audio_finishes_frames_with_a_jammed_cpu() {
        let mut nes = nes_jammed_at_reset();
        nes.run_one_frame_discarding_audio();
        let frame = frame_count(&nes);

        nes.run_one_frame_discarding_audio();
        nes.run_one_frame_discarding_audio();

        assert_eq!(frame_count(&nes), frame + 2);
        assert!(!nes.is_ready_to_render(), "the ready flag is cleared");
        assert!(!nes.sample_ready(), "every sample was discarded");
    }

    #[test]
    fn run_until_frame_ready_stops_when_the_step_breaks() {
        let mut nes = nes_with_nop_loop();
        let mut calls = 0;

        let ready = nes.run_until_frame_ready(|nes| {
            calls += 1;
            nes.run_cpu_tick();
            if calls == 10 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });

        assert!(!ready);
        assert_eq!(calls, 10);
    }

    #[test]
    fn run_until_frame_ready_leaves_the_ready_flag_set() {
        let mut nes = nes_jammed_at_reset();
        let ready = nes.run_until_frame_ready(|nes| {
            nes.run_cpu_tick();
            ControlFlow::Continue(())
        });
        assert!(ready);
        assert!(nes.is_ready_to_render());
    }

    #[test]
    fn run_to_next_scanline_advances_with_a_jammed_cpu() {
        let mut nes = nes_jammed_at_reset();
        let before = scanline(&nes);
        nes.run_to_next_scanline();
        assert_ne!(scanline(&nes), before);
    }

    #[test]
    fn run_to_next_frame_advances_with_a_jammed_cpu() {
        let mut nes = nes_jammed_at_reset();
        let before = frame_count(&nes);
        nes.run_to_next_frame();
        assert_ne!(frame_count(&nes), before);
    }

    #[test]
    fn read_vector_target_reads_the_little_endian_vector() {
        let nes = nes_with_nop_loop();
        assert_eq!(nes.read_vector_target(0xFFFA), NMI_HANDLER);
        assert_eq!(nes.read_vector_target(0xFFFC), 0x8000);
    }

    #[test]
    fn run_to_interrupt_entry_reaches_the_nmi_handler() {
        // $8000: LDA #$80; $8002: STA $2000; JMP $8002 — keeps asking for NMI at vblank.
        let mut nes = nes_running(&[0xA9, 0x80, 0x8D, 0x00, 0x20, 0x4C, 0x02, 0x80]);

        nes.run_to_interrupt_entry(0xFFFA, InterruptKind::Nmi);

        assert_eq!(nes.cpu_ref().pc(), NMI_HANDLER);
        assert_eq!(nes.cpu_ref().current_interrupt(), Some(InterruptKind::Nmi));
    }

    #[test]
    fn run_to_interrupt_entry_gives_up_on_a_jammed_cpu() {
        let mut nes = nes_jammed_at_reset();
        let pc = nes.cpu_ref().pc();

        nes.run_to_interrupt_entry(0xFFFA, InterruptKind::Nmi);

        assert!(nes.cpu_ref().is_halted());
        assert_eq!(
            nes.cpu_ref().pc(),
            pc,
            "a jammed CPU never reaches a handler"
        );
    }
}
