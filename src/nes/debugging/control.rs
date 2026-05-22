//! Shared debugger controller used by both SDL and native frontends.
//!
//! Owns all debugger state: breakpoints, stepping, pause/continue flags,
//! and view state. Frontends delegate to this controller for emulation
//! control flow and sync their own UI state from the controller's getters.

use super::snapshot::DebuggerViewState;
use super::ui::DebuggerUiAction;
use crate::nes::console::Nes;
use crate::nes::cpu::InterruptKind;
use crate::platform::debugging::Tracing;
use crate::platform::debugging::breakpoints::{BreakpointKind, BreakpointList, EvalContext};
use crate::platform::debugging::controller::DebuggerControllerCore;

const JSR_OPCODE: u8 = 0x20;

/// Central debugger state shared between frontends.
pub struct DebuggerController {
    /// Core debugger state (breakpoints, pause/resume, temporary breakpoints).
    core: DebuggerControllerCore<InterruptKind>,
    #[allow(dead_code)] // Used once GlBackend is refactored to accept external view_state
    view_state: DebuggerViewState,
    /// NES-specific: arm a temporary breakpoint after the next instruction completes.
    arm_temporary_breakpoint_after_next_instruction: bool,
}

impl DebuggerController {
    /// Create a new controller, optionally pre-loaded with config breakpoints
    /// and optionally entering the debugger immediately.
    pub fn new(config_breakpoints: &[BreakpointKind], debugger_enabled: bool) -> Self {
        Self {
            core: DebuggerControllerCore::new(config_breakpoints, debugger_enabled),
            view_state: DebuggerViewState::default(),
            arm_temporary_breakpoint_after_next_instruction: false,
        }
    }

    // ── State getters ──────────────────────────────────────────────────

    #[allow(dead_code)] // Used by native frontend and tests
    pub fn is_paused(&self) -> bool {
        self.core.is_paused()
    }

    pub fn is_debugger_open(&self) -> bool {
        self.core.is_debugger_open()
    }

    pub fn breakpoints(&self) -> &BreakpointList {
        self.core.breakpoints()
    }

    pub fn breakpoints_mut(&mut self) -> &mut BreakpointList {
        self.core.breakpoints_mut()
    }

    #[allow(dead_code)] // Used once GlBackend is refactored to accept external view_state
    pub fn view_state_mut(&mut self) -> &mut DebuggerViewState {
        &mut self.view_state
    }

    // ── Debugger open/close ────────────────────────────────────────────

    /// Open the debugger and pause emulation.
    pub fn enter_debugger(&mut self) {
        self.core.enter_debugger();
    }

    /// Close the debugger and resume emulation.
    pub fn continue_from_debugger(&mut self, nes: &Nes) {
        self.core.continue_from_debugger(
            nes.cpu_ref().pc(),
            nes.cpu_ref().get_total_cycles(),
            nes.ppu().borrow().frame_count(),
        );
    }

    /// Toggle the debugger: open+pause if closed, continue if open.
    pub fn toggle_debugger(&mut self, nes: &Nes) {
        if self.core.is_debugger_open() {
            self.continue_from_debugger(nes);
        } else {
            self.enter_debugger();
        }
    }

    // ── Temporary breakpoints ──────────────────────────────────────────

    fn clear_temporary_breakpoint(&mut self) {
        self.core.clear_temporary_breakpoint();
    }

    fn set_temporary_breakpoint(&mut self, pc: u16) {
        self.core.set_temporary_breakpoint(pc);
    }

    fn set_temporary_breakpoint_for_interrupt(
        &mut self,
        nes: &Nes,
        pc: u16,
        required_interrupt: InterruptKind,
    ) {
        let currently_in_interrupt = nes.cpu_ref().current_interrupt() == Some(required_interrupt);
        self.core.set_temporary_breakpoint_for_interrupt(
            pc,
            required_interrupt,
            currently_in_interrupt,
        );
    }

    fn arm_temporary_breakpoint_after_next_instruction(&mut self) {
        self.arm_temporary_breakpoint_after_next_instruction = true;
    }

    fn maybe_arm_temporary_breakpoint_after_instruction(&mut self, nes: &Nes) {
        if !self.arm_temporary_breakpoint_after_next_instruction {
            return;
        }
        self.arm_temporary_breakpoint_after_next_instruction = false;
        self.set_temporary_breakpoint(nes.cpu_ref().pc());
    }

    // ── Breakpoint evaluation ──────────────────────────────────────────

    /// Check PC-based breakpoints before an instruction executes.
    /// Returns `true` if a breakpoint was hit and the debugger was entered.
    fn check_breakpoint_hit(&mut self, pc: u16, current_interrupt: Option<InterruptKind>) -> bool {
        self.core.update_interrupt_exit_tracking(current_interrupt);

        // Allow executing the instruction we just continued from.
        if self.core.should_ignore_pc_breakpoint(pc) {
            return false;
        }

        if !self.core.breakpoints().has_enabled_pc_breakpoint_at(pc) {
            return false;
        }

        // While a run-to temp breakpoint is active, suppress other breakpoints.
        if self.core.should_suppress_other_breakpoints(pc) {
            return false;
        }

        if !self.resolve_temporary_breakpoint_at_hit(pc, current_interrupt) {
            return false;
        }

        self.enter_debugger();
        true
    }

    /// Resolve temporary breakpoint state when a PC breakpoint is hit.
    /// Returns `true` if the breakpoint should fire, `false` to suppress it.
    fn resolve_temporary_breakpoint_at_hit(
        &mut self,
        pc: u16,
        current_interrupt: Option<InterruptKind>,
    ) -> bool {
        match self
            .core
            .check_temporary_breakpoint_hit(pc, current_interrupt)
        {
            Some(true) => {
                // Temporary breakpoint should fire - clean it up
                self.core.cleanup_triggered_temporary_breakpoint();
                true
            }
            Some(false) => {
                // Temporary breakpoint at this PC but conditions not met
                false
            }
            None => {
                // No temporary breakpoint at this PC
                // Check if we should cancel a pending step (hit different breakpoint)
                if self.core.temporary_breakpoint.is_some() {
                    let tb = self.core.temporary_breakpoint.as_ref().unwrap();
                    if !tb.ignore_other_breakpoints {
                        // Hit a different breakpoint while a step is pending — cancel the step.
                        self.clear_temporary_breakpoint();
                    }
                }
                true
            }
        }
    }

    /// Check cycle, frame, and write-address breakpoints after instruction execution.
    fn check_post_instruction_breakpoints(&mut self, nes: &Nes) {
        if self.core.is_paused() {
            return;
        }
        let prev_cycles = self.core.last_post_instruction_cycles;
        let current_cycles = nes.cpu_ref().get_total_cycles();
        let prev_frame = self.core.last_post_instruction_frame;
        let current_frame = nes.ppu().borrow().frame_count();
        let ctx = EvalContext {
            pc: nes.cpu_ref().pc(),
            prev_cpu_cycles: prev_cycles,
            cpu_cycles: current_cycles,
            prev_frame,
            frame: current_frame,
            write_addr: nes.cpu_ref().last_cpu_write_addr(),
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
        };
        self.core
            .update_post_instruction_state(current_cycles, current_frame);
        let hit =
            self.core.breakpoints().iter().any(|bp| {
                bp.enabled && !matches!(bp.kind, BreakpointKind::Pc(_)) && bp.is_hit(&ctx)
            });
        if hit {
            self.enter_debugger();
        }
    }

    // ── Stepping ───────────────────────────────────────────────────────

    /// Step into: execute exactly one CPU instruction (blocking).
    pub fn step_into(&mut self, nes: &mut Nes) {
        self.enter_debugger();
        nes.run_cpu_tick();
    }

    /// Step over: if the current instruction is JSR, use a temporary breakpoint
    /// at the return address so intermediate breakpoints are respected.
    /// For non-JSR instructions, executes one instruction (blocking).
    pub fn step_over(&mut self, nes: &mut Nes) {
        let pc = nes.cpu_ref().pc();
        let opcode = nes.bus().borrow().read_cpu_for_debugger(pc);

        if opcode == JSR_OPCODE {
            let return_pc = pc.wrapping_add(3);
            self.set_temporary_breakpoint(return_pc);
            self.continue_from_debugger(nes);
        } else {
            self.enter_debugger();
            nes.run_cpu_tick();
        }
    }

    // ── Blocking run-to helpers ────────────────────────────────────────

    fn run_to_next_frame(nes: &mut Nes) {
        const MAX_STEPS: usize = 2_000_000;
        let mut previous_scanline = nes.ppu().borrow().scanline();

        for _ in 0..MAX_STEPS {
            if nes.cpu_ref().is_halted() {
                break;
            }
            nes.run_cpu_tick();
            let scanline = nes.ppu().borrow().scanline();
            if scanline < previous_scanline {
                break;
            }
            previous_scanline = scanline;
        }
    }

    fn run_to_next_scanline(nes: &mut Nes) {
        const MAX_STEPS: usize = 100_000;
        let start_scanline = nes.ppu().borrow().scanline();

        for _ in 0..MAX_STEPS {
            if nes.cpu_ref().is_halted() {
                break;
            }
            nes.run_cpu_tick();
            let scanline = nes.ppu().borrow().scanline();
            if scanline != start_scanline {
                break;
            }
        }
    }

    fn read_vector_target(nes: &Nes, vector_addr: u16) -> u16 {
        let memory = nes.bus().borrow();
        let lo = memory.read_cpu_for_debugger(vector_addr);
        let hi = memory.read_cpu_for_debugger(vector_addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    fn arm_run_to_interrupt(&mut self, nes: &Nes, vector_addr: u16, kind: InterruptKind) -> bool {
        let target = Self::read_vector_target(nes, vector_addr);
        self.set_temporary_breakpoint_for_interrupt(nes, target, kind);
        true
    }

    // ── UI action handling ─────────────────────────────────────────────

    /// Process a `DebuggerUiAction` returned from the native debugger UI render pass.
    ///
    /// This handles control-flow actions (stepping, continue, breakpoint CRUD,
    /// run-to-interrupt). View-state actions (toggle PPU viewer, opacity,
    /// hexdump base, watch addresses) are currently applied by `GlBackend`
    /// which still owns `DebuggerViewState`. Once GlBackend is refactored
    /// (Sub-issues B/C), those actions will move here.
    pub fn apply_ui_action(&mut self, nes: &mut Nes, action: DebuggerUiAction) {
        if !self.core.is_debugger_open() {
            return;
        }

        let mut should_continue = action.continue_run;

        if action.step_over {
            let pc = nes.cpu_ref().pc();
            let opcode = nes.bus().borrow().read_cpu_for_debugger(pc);

            if opcode == JSR_OPCODE {
                let return_pc = pc.wrapping_add(3);
                self.set_temporary_breakpoint(return_pc);
            } else {
                self.arm_temporary_breakpoint_after_next_instruction();
            }
            should_continue = true;
        }

        if action.step_into {
            self.arm_temporary_breakpoint_after_next_instruction();
            should_continue = true;
        }

        if action.run_to_next_frame {
            Self::run_to_next_frame(nes);
        }
        if action.run_to_next_scanline {
            Self::run_to_next_scanline(nes);
        }
        if action.run_to_nmi {
            should_continue |= self.arm_run_to_interrupt(nes, 0xFFFA, InterruptKind::Nmi);
        }
        if action.run_to_irq {
            should_continue |= self.arm_run_to_interrupt(nes, 0xFFFE, InterruptKind::Irq);
        }

        if should_continue {
            self.continue_from_debugger(nes);
        }

        if let Some(kind) = action.add_breakpoint {
            self.core.breakpoints_mut().add(kind);
        }
        if let Some(index) = action.remove_breakpoint {
            self.core.breakpoints_mut().remove(index);
        }
        if let Some(index) = action.enable_breakpoint {
            self.core.breakpoints_mut().enable(index);
        }
        if let Some(index) = action.disable_breakpoint {
            self.core.breakpoints_mut().disable(index);
        }
    }

    // ── Emulation frame loop ───────────────────────────────────────────

    /// Run one emulation frame with breakpoint evaluation.
    ///
    /// The `audio_drain` callback is invoked after each instruction to drain
    /// audio samples from the NES APU — this keeps audio handling frontend-specific.
    pub fn run_frame(
        &mut self,
        nes: &mut Nes,
        tracing: &Tracing,
        audio_drain: &mut dyn FnMut(&mut Nes),
    ) {
        if self.core.is_paused() {
            return;
        }

        while !nes.is_ready_to_render() && !nes.cpu_ref().is_halted() {
            if self.check_breakpoint_hit(nes.cpu_ref().pc(), nes.cpu_ref().current_interrupt()) {
                break;
            }

            nes.run(tracing);
            self.maybe_arm_temporary_breakpoint_after_instruction(nes);
            self.check_post_instruction_breakpoints(nes);

            if self.core.is_paused() {
                break;
            }

            audio_drain(nes);
        }
    }

    /// Run a single CPU instruction with breakpoint evaluation (for headless testing).
    pub fn tick_once(&mut self, nes: &mut Nes) {
        if self.core.is_paused() {
            return;
        }

        if self.check_breakpoint_hit(nes.cpu_ref().pc(), nes.cpu_ref().current_interrupt()) {
            return;
        }

        nes.run_cpu_tick();
        self.maybe_arm_temporary_breakpoint_after_instruction(nes);
        self.check_post_instruction_breakpoints(nes);
    }

    // ── Debug file persistence ─────────────────────────────────────────

    /// Load breakpoints from a `.debug` file next to the ROM.
    pub fn load_breakpoints_from_debug_file(&mut self, nes: &Nes) {
        let Some(path) = nes.debug_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        self.core.breakpoints = BreakpointList::load_from_str(&text);
    }

    /// Save breakpoints to a `.debug` file next to the ROM.
    pub fn save_breakpoints_to_debug_file(&self, nes: &Nes) {
        self.save_debug_state_to_file(nes, &[]);
    }

    /// Load breakpoints **and** watch addresses from the `.debug` file next to the ROM.
    ///
    /// Returns the watch addresses found in the file (may be empty).
    pub fn load_debug_state_from_file(&mut self, nes: &Nes) -> Vec<u16> {
        let Some(path) = nes.debug_path() else {
            return vec![];
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return vec![];
        };
        self.core.breakpoints = BreakpointList::load_from_str(&text);
        crate::platform::debugging::breakpoints::parse_watch_addresses(&text)
    }

    /// Save breakpoints **and** watch addresses to the `.debug` file next to the ROM.
    ///
    /// Removes the file when both are empty.
    pub fn save_debug_state_to_file(&self, nes: &Nes, watch_addresses: &[u16]) {
        let Some(path) = nes.debug_path() else {
            return;
        };
        let bp_str = self.core.breakpoints().save_to_string();
        let watch_str =
            crate::platform::debugging::breakpoints::serialize_watch_addresses(watch_addresses);
        if bp_str.is_empty() && watch_str.is_empty() {
            if path.exists()
                && let Err(err) = std::fs::remove_file(&path)
            {
                crate::platform::debugging::log_info(format!(
                    "Failed to remove .debug file: {err}"
                ));
            }
            return;
        }
        let content = [bp_str, watch_str]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if let Err(err) = std::fs::write(&path, content) {
            crate::platform::debugging::log_info(format!("Failed to save debug state: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::{Cartridge, NametableLayout};
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;

    // ── Test helpers ───────────────────────────────────────────────────

    fn default_controller() -> DebuggerController {
        DebuggerController::new(&[], false)
    }

    fn nes_with_nop_loop() -> Nes {
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));

        // $8000: NOP; $8001: JMP $8000
        let mut prg_rom = vec![0xEAu8; 0x8000];
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFA] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFC] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;
        prg_rom[0x0000] = 0xEA; // NOP
        prg_rom[0x0001] = 0x4C; // JMP $8000
        prg_rom[0x0002] = 0x00;
        prg_rom[0x0003] = 0x80;

        let cart = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cart);
        nes.reset(false);
        nes
    }

    fn nes_with_jsr_program() -> Nes {
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));

        // $8000: JSR $8006; $8003: LDA #$01; $8005: BRK
        // $8006: INX; $8007: RTS
        let mut prg_rom = vec![0xEAu8; 0x8000];
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFA] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFC] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;

        prg_rom[0x0000] = 0x20; // JSR $8006
        prg_rom[0x0001] = 0x06;
        prg_rom[0x0002] = 0x80;
        prg_rom[0x0003] = 0xA9; // LDA #$01
        prg_rom[0x0004] = 0x01;
        prg_rom[0x0005] = 0x00; // BRK
        prg_rom[0x0006] = 0xE8; // INX
        prg_rom[0x0007] = 0x60; // RTS

        let cart = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cart);
        nes.reset(false);
        nes
    }

    fn insert_nop_cartridge(nes: &mut Nes, reset_vector: u16) {
        let mut prg_rom = vec![0xEAu8; 0x8000];
        prg_rom[0x7FFC] = (reset_vector & 0xFF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = 0x00;
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x80;

        let cart = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        nes.insert_cartridge(cart);
        nes.reset(false);
    }

    // ── State management tests ─────────────────────────────────────────

    #[test]
    fn test_new_controller_is_not_paused_and_debugger_closed() {
        let ctrl = default_controller();
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    #[test]
    fn test_new_controller_with_config_breakpoints() {
        let ctrl = DebuggerController::new(
            &[BreakpointKind::Pc(0x8000), BreakpointKind::Cycle(1000)],
            false,
        );
        assert_eq!(ctrl.breakpoints().len(), 2);
    }

    #[test]
    fn test_enter_debugger_sets_paused_and_open() {
        let mut ctrl = default_controller();
        ctrl.enter_debugger();
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_continue_from_debugger_clears_paused_and_open() {
        let mut ctrl = default_controller();
        let nes = nes_with_nop_loop();
        ctrl.enter_debugger();
        ctrl.continue_from_debugger(&nes);
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    #[test]
    fn test_continue_sets_ignore_once_when_breakpoint_at_current_pc() {
        let mut ctrl = default_controller();
        let nes = nes_with_nop_loop();
        let pc = nes.cpu_ref().pc();
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(pc));
        ctrl.enter_debugger();
        ctrl.continue_from_debugger(&nes);
        assert_eq!(ctrl.core.breakpoint_ignore_once_at_pc, Some(pc));
    }

    #[test]
    fn test_toggle_debugger_opens_when_closed() {
        let mut ctrl = default_controller();
        let nes = nes_with_nop_loop();
        ctrl.toggle_debugger(&nes);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_toggle_debugger_closes_when_open() {
        let mut ctrl = default_controller();
        let nes = nes_with_nop_loop();
        ctrl.enter_debugger();
        ctrl.toggle_debugger(&nes);
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    // ── Breakpoint hit tests ───────────────────────────────────────────

    #[test]
    fn test_pc_breakpoint_hit_enters_debugger() {
        let mut ctrl = default_controller();
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8000));
        let hit = ctrl.check_breakpoint_hit(0x8000, None);
        assert!(hit);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_pc_breakpoint_miss_does_not_enter_debugger() {
        let mut ctrl = default_controller();
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8000));
        let hit = ctrl.check_breakpoint_hit(0x9000, None);
        assert!(!hit);
        assert!(!ctrl.is_paused());
    }

    #[test]
    fn test_ignore_once_skips_breakpoint_then_clears() {
        let mut ctrl = default_controller();
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8000));
        ctrl.core.breakpoint_ignore_once_at_pc = Some(0x8000);

        let hit1 = ctrl.check_breakpoint_hit(0x8000, None);
        assert!(!hit1, "first check should be ignored");
        assert!(
            ctrl.core.breakpoint_ignore_once_at_pc.is_none(),
            "flag should be cleared"
        );

        let hit2 = ctrl.check_breakpoint_hit(0x8000, None);
        assert!(hit2, "second check should hit");
    }

    // ── Cycle breakpoint tests ─────────────────────────────────────────

    #[test]
    fn test_cycle_breakpoint_stops_emulation() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        let cycles_before = nes.cpu_ref().get_total_cycles();
        let target = cycles_before + 100;
        ctrl.core
            .breakpoints_mut()
            .add(BreakpointKind::Cycle(target));

        let tracing = Tracing::default();
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {});

        assert!(ctrl.is_paused(), "should be paused after cycle breakpoint");
        let cycles_after = nes.cpu_ref().get_total_cycles();
        assert!(
            cycles_after < cycles_before + 200,
            "should stop near target ({target}), got {cycles_after}"
        );
    }

    #[test]
    fn test_frame_breakpoint_pauses_at_target_frame() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        let target_frame = nes.ppu().borrow().frame_count() + 1;
        ctrl.core
            .breakpoints_mut()
            .add(BreakpointKind::Frame(target_frame));

        for _ in 0..2_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        assert!(ctrl.is_paused(), "frame breakpoint should pause");
    }

    // ── Step tests ─────────────────────────────────────────────────────

    #[test]
    fn test_step_into_executes_one_instruction() {
        let mut ctrl = default_controller();
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        let pc_before = nes.cpu_ref().pc();
        assert_eq!(pc_before, 0x8000);

        ctrl.step_into(&mut nes);

        assert_eq!(nes.cpu_ref().pc(), 0x8001, "should advance by one NOP");
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_step_over_non_jsr_executes_one_instruction() {
        let mut ctrl = default_controller();
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        ctrl.step_over(&mut nes);

        assert_eq!(nes.cpu_ref().pc(), 0x8001, "should advance by one NOP");
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_step_over_jsr_uses_temporary_breakpoint() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_jsr_program();
        nes.cpu_mut().set_x(0);

        ctrl.enter_debugger();
        ctrl.step_over(&mut nes);

        // Step over JSR: should continue (unpause) with a temp breakpoint at $8003
        assert!(!ctrl.is_paused(), "step-over JSR should continue running");

        // Run until the temp breakpoint hits
        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        assert_eq!(nes.cpu_ref().pc(), 0x8003, "should stop at return address");
        assert_eq!(
            nes.cpu_ref().x(),
            1,
            "subroutine should have executed (INX)"
        );
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_step_over_jsr_with_disabled_breakpoint_at_return_addr() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_jsr_program();
        nes.cpu_mut().set_x(0);

        // Add a *disabled* breakpoint at the return address ($8003).
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8003));
        ctrl.core.breakpoints_mut().disable(0);
        assert!(!ctrl.core.breakpoints().has_enabled_pc_breakpoint_at(0x8003));

        ctrl.enter_debugger();
        ctrl.step_over(&mut nes);

        // The disabled breakpoint should be force-enabled for the step-over.
        assert!(!ctrl.is_paused(), "step-over JSR should continue running");

        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        assert_eq!(
            nes.cpu_ref().pc(),
            0x8003,
            "should stop at return address despite disabled breakpoint"
        );
        assert!(ctrl.is_paused());
    }

    #[test]
    fn test_step_over_cleanup_restores_disabled_state() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_jsr_program();

        // Add a *disabled* breakpoint at the return address ($8003).
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8003));
        ctrl.core.breakpoints_mut().disable(0);

        ctrl.enter_debugger();
        ctrl.step_over(&mut nes);

        // Run until breakpoint hits.
        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        // After cleanup, the user's breakpoint should be restored to disabled.
        assert!(
            ctrl.core.breakpoints().has_pc_breakpoint_at(0x8003),
            "user breakpoint should still exist"
        );
        assert!(
            !ctrl.core.breakpoints().has_enabled_pc_breakpoint_at(0x8003),
            "user breakpoint should be restored to disabled"
        );
    }

    #[test]
    fn test_step_over_cleanup_keeps_enabled_breakpoint_enabled() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_jsr_program();

        // Add an *enabled* breakpoint at the return address ($8003).
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8003));

        ctrl.enter_debugger();
        ctrl.step_over(&mut nes);

        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        // After cleanup, the user's breakpoint should remain enabled.
        assert!(ctrl.core.breakpoints().has_enabled_pc_breakpoint_at(0x8003));
    }

    // ── UI action tests ────────────────────────────────────────────────

    #[test]
    fn test_apply_ui_action_step_into_arms_temporary_breakpoint() {
        let mut ctrl = default_controller();
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        ctrl.enter_debugger();
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                step_into: true,
                ..Default::default()
            },
        );

        assert!(
            !ctrl.is_paused(),
            "step-into action should continue running"
        );

        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        assert_eq!(nes.cpu_ref().pc(), 0x8001);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_apply_ui_action_step_over_jsr() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_jsr_program();
        nes.cpu_mut().set_x(0);

        ctrl.enter_debugger();
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                step_over: true,
                ..Default::default()
            },
        );

        assert!(!ctrl.is_paused(), "step-over should continue running");

        for _ in 0..1_000_000 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }

        assert_eq!(nes.cpu_ref().pc(), 0x8003);
        assert_eq!(nes.cpu_ref().x(), 1);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_apply_ui_action_continue_resumes() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        ctrl.enter_debugger();
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                continue_run: true,
                ..Default::default()
            },
        );

        assert!(!ctrl.is_paused(), "continue should unpause");
        assert!(!ctrl.is_debugger_open(), "continue should close debugger");
    }

    #[test]
    fn test_apply_ui_action_add_breakpoint() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        ctrl.enter_debugger();
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                add_breakpoint: Some(BreakpointKind::Pc(0xC000)),
                ..Default::default()
            },
        );

        assert_eq!(ctrl.breakpoints().len(), 1);
        assert!(ctrl.breakpoints().has_pc_breakpoint_at(0xC000));
    }

    #[test]
    fn test_apply_ui_action_remove_breakpoint() {
        let mut ctrl = DebuggerController::new(&[BreakpointKind::Pc(0xC000)], false);
        let mut nes = nes_with_nop_loop();

        ctrl.enter_debugger();
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                remove_breakpoint: Some(0),
                ..Default::default()
            },
        );

        assert!(ctrl.breakpoints().is_empty());
    }

    #[test]
    fn test_apply_ui_action_ignored_when_debugger_closed() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        // Debugger is NOT open
        ctrl.apply_ui_action(
            &mut nes,
            DebuggerUiAction {
                add_breakpoint: Some(BreakpointKind::Pc(0xC000)),
                ..Default::default()
            },
        );

        assert!(
            ctrl.breakpoints().is_empty(),
            "actions should be ignored when debugger is closed"
        );
    }

    // ── run_frame tests ────────────────────────────────────────────────

    #[test]
    fn test_run_frame_does_nothing_when_paused() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();
        ctrl.enter_debugger(); // paused

        let pc_before = nes.cpu_ref().pc();
        let tracing = Tracing::default();
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {});

        assert_eq!(
            nes.cpu_ref().pc(),
            pc_before,
            "should not advance when paused"
        );
    }

    #[test]
    fn test_run_frame_advances_to_frame_boundary() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();

        let tracing = Tracing::default();
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {});

        assert!(
            nes.is_ready_to_render() || nes.cpu_ref().is_halted(),
            "run_frame should reach frame boundary"
        );
    }

    #[test]
    fn test_run_frame_calls_audio_drain() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();
        let tracing = Tracing::default();

        let mut drain_count = 0u32;
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {
            drain_count += 1;
        });

        assert!(drain_count > 0, "audio drain should be called during frame");
    }

    #[test]
    fn test_continue_skips_breakpoint_once_on_same_pc() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();
        let pc = nes.cpu_ref().pc();

        // Set a breakpoint and enter debugger at the breakpoint PC.
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(pc));
        ctrl.enter_debugger();

        // Continue should set ignore_once.
        ctrl.continue_from_debugger(&nes);

        // The first tick should NOT re-break immediately.
        ctrl.tick_once(&mut nes);
        assert!(
            !ctrl.is_paused(),
            "should not re-break immediately on continue"
        );

        // NOP loop goes 8000 -> 8001 (JMP) -> 8000, so after another tick we're back.
        // The next time we hit $8000, the breakpoint should fire.
        for _ in 0..100 {
            ctrl.tick_once(&mut nes);
            if ctrl.is_paused() {
                break;
            }
        }
        assert!(ctrl.is_paused(), "breakpoint should fire on second visit");
    }

    // ── Cycle breakpoint threshold tracking ────────────────────────────

    #[test]
    fn test_cycle_breakpoint_does_not_fire_spuriously_after_debugger_actions() {
        let mut ctrl = default_controller();
        let mut nes = nes_with_nop_loop();
        let tracing = Tracing::default();

        // Run a frame to advance cycles.
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {});
        nes.clear_ready_to_render();

        let cycles_now = nes.cpu_ref().get_total_cycles();
        // Set a cycle breakpoint far in the future.
        let target = cycles_now + 100_000;
        ctrl.core
            .breakpoints_mut()
            .add(BreakpointKind::Cycle(target));

        // Enter debugger (simulating a step or other action that advances cycles).
        ctrl.enter_debugger();
        // Step a few instructions manually.
        nes.run_cpu_tick();
        nes.run_cpu_tick();
        // Continue — this should sync the cycle tracker.
        ctrl.continue_from_debugger(&nes);

        // Run another frame — the cycle breakpoint should NOT fire prematurely.
        ctrl.run_frame(&mut nes, &tracing, &mut |_| {});
        let cycles_after = nes.cpu_ref().get_total_cycles();

        if cycles_after < target {
            assert!(
                !ctrl.is_paused(),
                "cycle breakpoint should not fire before target"
            );
        }
    }

    #[test]
    fn test_new_with_debugger_enabled_starts_paused_and_open() {
        let ctrl = DebuggerController::new(&[], true);
        assert!(ctrl.is_paused(), "controller should start paused");
        assert!(
            ctrl.is_debugger_open(),
            "controller should start with debugger open"
        );
    }

    // ── Watch address persistence (#1860) ─────────────────────────────────────

    /// Returns a NES with a cartridge whose `rom_path` is set to a temp file,
    /// so that `debug_path()` returns something useful in tests.
    fn nes_with_rom_path(rom_path: &std::path::Path) -> Nes {
        let mut prg_rom = vec![0xEAu8; 0x8000];
        let reset: u16 = 0x8000;
        for off in [0x7FFAusize, 0x7FFC, 0x7FFE] {
            prg_rom[off] = (reset & 0xFF) as u8;
            prg_rom[off + 1] = (reset >> 8) as u8;
        }
        let mut cart = Cartridge::from_parts(prg_rom, vec![], NametableLayout::Horizontal);
        cart.set_rom_path_for_test(rom_path.to_path_buf());
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        nes.insert_cartridge(cart);
        nes
    }

    #[test]
    fn test_save_debug_state_includes_watch_addresses() {
        let dir = tempfile::tempdir().expect("temp dir must be created");
        let rom_path = dir.path().join("test.nes");
        let debug_path = dir.path().join("test.debug");

        let nes = nes_with_rom_path(&rom_path);
        let mut ctrl = default_controller();
        ctrl.core.breakpoints_mut().add(BreakpointKind::Pc(0x8000));

        ctrl.save_debug_state_to_file(&nes, &[0x0300, 0x00FF]);

        let content = std::fs::read_to_string(&debug_path).expect("debug file must exist");
        assert!(
            content.contains("watch 0x0300"),
            "debug file must contain watch 0x0300; got:\n{content}"
        );
        assert!(
            content.contains("watch 0x00FF"),
            "debug file must contain watch 0x00FF; got:\n{content}"
        );
        assert!(
            content.contains("pc 0x8000"),
            "debug file must still contain breakpoint; got:\n{content}"
        );
    }

    #[test]
    fn test_load_debug_state_returns_watch_addresses() {
        let dir = tempfile::tempdir().expect("temp dir must be created");
        let rom_path = dir.path().join("test.nes");
        let debug_path = dir.path().join("test.debug");

        std::fs::write(
            &debug_path,
            "pc 0x8000 enabled\nwatch 0x0300\nwatch 0x00FF\n",
        )
        .expect("write debug file");

        let nes = nes_with_rom_path(&rom_path);
        let mut ctrl = default_controller();

        let watches = ctrl.load_debug_state_from_file(&nes);

        assert_eq!(
            watches,
            vec![0x0300u16, 0x00FF],
            "loaded watches must match the debug file"
        );
        assert!(
            ctrl.core.breakpoints().has_enabled_pc_breakpoint_at(0x8000),
            "breakpoints must also be loaded"
        );
    }

    #[test]
    fn test_save_debug_state_watch_only_creates_file() {
        // Even when there are no breakpoints, a file should be created if
        // there are watch addresses to persist.
        let dir = tempfile::tempdir().expect("temp dir must be created");
        let rom_path = dir.path().join("test.nes");
        let debug_path = dir.path().join("test.debug");

        let nes = nes_with_rom_path(&rom_path);
        let ctrl = default_controller();
        ctrl.save_debug_state_to_file(&nes, &[0x0400]);

        assert!(
            debug_path.exists(),
            "debug file should exist with watch-only content"
        );
        let content = std::fs::read_to_string(&debug_path).unwrap();
        assert!(content.contains("watch 0x0400"));
    }

    #[test]
    fn test_save_debug_state_empty_removes_file() {
        // Both breakpoints and watches are empty → existing file must be removed.
        let dir = tempfile::tempdir().expect("temp dir must be created");
        let rom_path = dir.path().join("test.nes");
        let debug_path = dir.path().join("test.debug");
        std::fs::write(&debug_path, "pc 0x8000 enabled\nwatch 0x0300\n").unwrap();

        let nes = nes_with_rom_path(&rom_path);
        let ctrl = default_controller();
        ctrl.save_debug_state_to_file(&nes, &[]);

        assert!(
            !debug_path.exists(),
            "empty state should remove the debug file"
        );
    }
}
