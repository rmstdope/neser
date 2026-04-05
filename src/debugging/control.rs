//! Shared debugger controller used by both SDL and native frontends.
//!
//! Owns all debugger state: breakpoints, stepping, pause/continue flags,
//! and view state. Frontends delegate to this controller for emulation
//! control flow and sync their own UI state from the controller's getters.

use crate::console::Nes;
use crate::cpu::InterruptKind;
use crate::debugging::breakpoints::{BreakpointKind, BreakpointList, EvalContext};
use crate::debugging::snapshot::DebuggerViewState;
use crate::debugging::tracing::Tracing;
use crate::debugging::ui::DebuggerUiAction;

/// One-shot breakpoint used for stepping and run-to-interrupt operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TemporaryBreakpoint {
    pc: u16,
    /// Whether a user-defined PC breakpoint already existed at this address
    /// before we added the temporary one (so we don't remove it on cleanup).
    already_present: bool,
    /// If set, the breakpoint only fires when the CPU is inside this interrupt.
    required_interrupt: Option<InterruptKind>,
    /// Tracks whether we have left the required interrupt at least once.
    has_exited_required_interrupt: bool,
    /// When true, other breakpoints are suppressed while this temp is active
    /// (used by run-to-NMI/IRQ to avoid stopping at unrelated breakpoints).
    ignore_other_breakpoints: bool,
}

const JSR_OPCODE: u8 = 0x20;

/// Central debugger state shared between frontends.
pub struct DebuggerController {
    paused: bool,
    debugger_open: bool,
    view_state: DebuggerViewState,
    breakpoints: BreakpointList,
    temporary_breakpoint: Option<TemporaryBreakpoint>,
    arm_temporary_breakpoint_after_next_instruction: bool,
    breakpoint_ignore_once_at_pc: Option<u16>,
    last_post_instruction_cycles: u64,
    last_post_instruction_frame: u64,
}

impl DebuggerController {
    /// Create a new controller, optionally pre-loaded with config breakpoints.
    pub fn new(config_breakpoints: &[BreakpointKind]) -> Self {
        let mut breakpoints = BreakpointList::new();
        for &kind in config_breakpoints {
            breakpoints.add(kind);
        }
        Self {
            paused: false,
            debugger_open: false,
            view_state: DebuggerViewState::default(),
            breakpoints,
            temporary_breakpoint: None,
            arm_temporary_breakpoint_after_next_instruction: false,
            breakpoint_ignore_once_at_pc: None,
            last_post_instruction_cycles: 0,
            last_post_instruction_frame: 0,
        }
    }

    // ── State getters ──────────────────────────────────────────────────

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_debugger_open(&self) -> bool {
        self.debugger_open
    }

    pub fn breakpoints(&self) -> &BreakpointList {
        &self.breakpoints
    }

    pub fn view_state_mut(&mut self) -> &mut DebuggerViewState {
        &mut self.view_state
    }

    // ── Debugger open/close ────────────────────────────────────────────

    /// Open the debugger and pause emulation.
    pub fn enter_debugger(&mut self) {
        self.paused = true;
        self.debugger_open = true;
    }

    /// Close the debugger and resume emulation.
    pub fn continue_from_debugger(&mut self, nes: &Nes) {
        if self
            .breakpoints
            .has_enabled_pc_breakpoint_at(nes.cpu_ref().pc())
        {
            self.breakpoint_ignore_once_at_pc = Some(nes.cpu_ref().pc());
        }

        self.last_post_instruction_cycles = nes.cpu_ref().get_total_cycles();
        self.last_post_instruction_frame = nes.ppu().borrow().frame_count();

        self.paused = false;
        self.debugger_open = false;
    }

    /// Toggle the debugger: open+pause if closed, continue if open.
    pub fn toggle_debugger(&mut self, nes: &Nes) {
        if self.debugger_open {
            self.continue_from_debugger(nes);
        } else {
            self.enter_debugger();
        }
    }

    // ── Breakpoint management ──────────────────────────────────────────

    fn add_pc_breakpoint(&mut self, addr: u16) {
        self.breakpoints.add(BreakpointKind::Pc(addr));
    }

    fn remove_pc_breakpoint(&mut self, addr: u16) {
        if let Some(idx) = self
            .breakpoints
            .iter()
            .position(|b| b.kind == BreakpointKind::Pc(addr))
        {
            self.breakpoints.remove(idx);
        }
    }

    // ── Temporary breakpoints ──────────────────────────────────────────

    fn clear_temporary_breakpoint(&mut self) {
        if let Some(tb) = self.temporary_breakpoint.take()
            && !tb.already_present
        {
            self.remove_pc_breakpoint(tb.pc);
        }
    }

    fn set_temporary_breakpoint(&mut self, pc: u16) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
        if !already_present {
            self.add_pc_breakpoint(pc);
        }

        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            required_interrupt: None,
            has_exited_required_interrupt: true,
            ignore_other_breakpoints: false,
        });
    }

    fn set_temporary_breakpoint_for_interrupt(
        &mut self,
        nes: &Nes,
        pc: u16,
        required_interrupt: InterruptKind,
    ) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
        if !already_present {
            self.add_pc_breakpoint(pc);
        }

        let currently_in_interrupt = nes.cpu_ref().current_interrupt() == Some(required_interrupt);
        let has_exited_required_interrupt = !currently_in_interrupt;
        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            required_interrupt: Some(required_interrupt),
            has_exited_required_interrupt,
            ignore_other_breakpoints: true,
        });
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
        self.update_interrupt_exit_tracking(current_interrupt);

        // Allow executing the instruction we just continued from.
        if self.breakpoint_ignore_once_at_pc == Some(pc) {
            self.breakpoint_ignore_once_at_pc = None;
            return false;
        }

        if !self.breakpoints.has_enabled_pc_breakpoint_at(pc) {
            return false;
        }

        // While a run-to temp breakpoint is active, suppress other breakpoints.
        if let Some(tb) = self.temporary_breakpoint
            && tb.ignore_other_breakpoints
            && pc != tb.pc
        {
            return false;
        }

        if !self.resolve_temporary_breakpoint_at_hit(pc, current_interrupt) {
            return false;
        }

        self.enter_debugger();
        true
    }

    /// Track whether we've exited the required interrupt for run-to-interrupt breakpoints.
    fn update_interrupt_exit_tracking(&mut self, current_interrupt: Option<InterruptKind>) {
        if let Some(tb) = self.temporary_breakpoint.as_mut()
            && let Some(required_interrupt) = tb.required_interrupt
            && !tb.has_exited_required_interrupt
            && current_interrupt != Some(required_interrupt)
        {
            tb.has_exited_required_interrupt = true;
        }
    }

    /// Resolve temporary breakpoint state when a PC breakpoint is hit.
    /// Returns `true` if the breakpoint should fire, `false` to suppress it.
    fn resolve_temporary_breakpoint_at_hit(
        &mut self,
        pc: u16,
        current_interrupt: Option<InterruptKind>,
    ) -> bool {
        let Some(tb) = self.temporary_breakpoint else {
            return true;
        };

        if tb.pc == pc {
            // Temp breakpoint is at this exact address.
            if let Some(required_interrupt) = tb.required_interrupt {
                // Run-to-interrupt: only fire when we've re-entered the target interrupt.
                if current_interrupt == Some(required_interrupt) && tb.has_exited_required_interrupt
                {
                    self.cleanup_temporary_breakpoint(tb);
                    return true;
                }
                return false;
            }
            // Plain one-shot breakpoint (stepping).
            self.cleanup_temporary_breakpoint(tb);
            true
        } else if !tb.ignore_other_breakpoints {
            // Hit a different breakpoint while a step is pending — cancel the step.
            self.clear_temporary_breakpoint();
            true
        } else {
            true
        }
    }

    /// Remove the temporary breakpoint and clean up the underlying PC breakpoint if
    /// it was added by the controller (not a pre-existing user breakpoint).
    fn cleanup_temporary_breakpoint(&mut self, tb: TemporaryBreakpoint) {
        self.temporary_breakpoint = None;
        if !tb.already_present {
            self.remove_pc_breakpoint(tb.pc);
        }
    }

    /// Check cycle, frame, and write-address breakpoints after instruction execution.
    fn check_post_instruction_breakpoints(&mut self, nes: &Nes) {
        if self.paused {
            return;
        }
        let prev_cycles = self.last_post_instruction_cycles;
        let current_cycles = nes.cpu_ref().get_total_cycles();
        let prev_frame = self.last_post_instruction_frame;
        let current_frame = nes.ppu().borrow().frame_count();
        let ctx = EvalContext {
            pc: nes.cpu_ref().pc(),
            prev_cpu_cycles: prev_cycles,
            cpu_cycles: current_cycles,
            prev_frame,
            frame: current_frame,
            write_addr: nes.cpu_ref().last_cpu_write_addr(),
        };
        self.last_post_instruction_cycles = current_cycles;
        self.last_post_instruction_frame = current_frame;
        let hit = self
            .breakpoints
            .iter()
            .any(|bp| bp.enabled && !matches!(bp.kind, BreakpointKind::Pc(_)) && bp.is_hit(&ctx));
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

    /// Process a `DebuggerUiAction` returned from the ImGui render pass.
    pub fn apply_ui_action(&mut self, nes: &mut Nes, action: DebuggerUiAction) {
        if !self.debugger_open {
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
            self.breakpoints.add(kind);
        }
        if let Some(index) = action.remove_breakpoint {
            self.breakpoints.remove(index);
        }
        if let Some(index) = action.enable_breakpoint {
            self.breakpoints.enable(index);
        }
        if let Some(index) = action.disable_breakpoint {
            self.breakpoints.disable(index);
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
        if self.paused {
            return;
        }

        while !nes.is_ready_to_render() && !nes.cpu_ref().is_halted() {
            if self.check_breakpoint_hit(nes.cpu_ref().pc(), nes.cpu_ref().current_interrupt()) {
                break;
            }

            nes.run(tracing);
            self.maybe_arm_temporary_breakpoint_after_instruction(nes);
            self.check_post_instruction_breakpoints(nes);

            if self.paused {
                break;
            }

            audio_drain(nes);
        }
    }

    /// Run a single CPU instruction with breakpoint evaluation (for headless testing).
    pub fn tick_once(&mut self, nes: &mut Nes) {
        if self.paused {
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
        self.breakpoints = BreakpointList::load_from_str(&text);
    }

    /// Save breakpoints to a `.debug` file next to the ROM.
    pub fn save_breakpoints_to_debug_file(&self, nes: &Nes) {
        let Some(path) = nes.debug_path() else {
            return;
        };
        if self.breakpoints.is_empty() {
            if path.exists()
                && let Err(err) = std::fs::remove_file(&path)
            {
                crate::debugging::log_info(format!("Failed to remove .debug file: {err}"));
            }
            return;
        }
        let content = self.breakpoints.save_to_string();
        if let Err(err) = std::fs::write(&path, content) {
            crate::debugging::log_info(format!("Failed to save breakpoints: {err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::cartridge::{Cartridge, NametableLayout};
    use crate::console::Config;

    // ── Test helpers ───────────────────────────────────────────────────

    fn default_controller() -> DebuggerController {
        DebuggerController::new(&[])
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
        let ctrl =
            DebuggerController::new(&[BreakpointKind::Pc(0x8000), BreakpointKind::Cycle(1000)]);
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
        ctrl.breakpoints.add(BreakpointKind::Pc(pc));
        ctrl.enter_debugger();
        ctrl.continue_from_debugger(&nes);
        assert_eq!(ctrl.breakpoint_ignore_once_at_pc, Some(pc));
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
        ctrl.breakpoints.add(BreakpointKind::Pc(0x8000));
        let hit = ctrl.check_breakpoint_hit(0x8000, None);
        assert!(hit);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_pc_breakpoint_miss_does_not_enter_debugger() {
        let mut ctrl = default_controller();
        ctrl.breakpoints.add(BreakpointKind::Pc(0x8000));
        let hit = ctrl.check_breakpoint_hit(0x9000, None);
        assert!(!hit);
        assert!(!ctrl.is_paused());
    }

    #[test]
    fn test_ignore_once_skips_breakpoint_then_clears() {
        let mut ctrl = default_controller();
        ctrl.breakpoints.add(BreakpointKind::Pc(0x8000));
        ctrl.breakpoint_ignore_once_at_pc = Some(0x8000);

        let hit1 = ctrl.check_breakpoint_hit(0x8000, None);
        assert!(!hit1, "first check should be ignored");
        assert!(
            ctrl.breakpoint_ignore_once_at_pc.is_none(),
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
        ctrl.breakpoints.add(BreakpointKind::Cycle(target));

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
        ctrl.breakpoints.add(BreakpointKind::Frame(target_frame));

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
        let mut ctrl = DebuggerController::new(&[BreakpointKind::Pc(0xC000)]);
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
        ctrl.breakpoints.add(BreakpointKind::Pc(pc));
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
        ctrl.breakpoints.add(BreakpointKind::Cycle(target));

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
}
