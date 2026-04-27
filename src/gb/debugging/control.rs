//! Game Boy debugger controller.
//!
//! Manages debugger state: breakpoints, stepping, pause/continue flags, and view state.
//! Handles the run-frame loop with breakpoint evaluation.

use super::snapshot::GbDebuggerViewState;
use crate::gb::bus::GbBus;
use crate::gb::console::{CpuTraceLine, Gb};
use crate::platform::debugging::breakpoints::{
    BreakpointKind, BreakpointList, EvalContext, GbInterruptKind,
};

/// One-shot breakpoint used for stepping and run-to operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TemporaryBreakpoint {
    pc: u16,
    /// Whether a user-defined PC breakpoint already existed at this address.
    already_present: bool,
    /// Whether the pre-existing breakpoint was enabled before we took over.
    was_enabled_before: bool,
    /// If set, the breakpoint only fires when this interrupt is about to fire.
    required_interrupt: Option<GbInterruptKind>,
    /// Tracks whether we have exited the required interrupt at least once.
    has_exited_required_interrupt: bool,
    /// When true, other breakpoints are suppressed while this temp is active.
    ignore_other_breakpoints: bool,
}

// Opcodes for CALL and RST instructions (for step-over)
const CALL_OPCODE: u8 = 0xCD; // CALL n16
const CALL_NZ_OPCODE: u8 = 0xC4; // CALL NZ,n16
const CALL_Z_OPCODE: u8 = 0xCC; // CALL Z,n16
const CALL_NC_OPCODE: u8 = 0xD4; // CALL NC,n16
const CALL_C_OPCODE: u8 = 0xDC; // CALL C,n16
const RST_00_OPCODE: u8 = 0xC7;
const RST_08_OPCODE: u8 = 0xCF;
const RST_10_OPCODE: u8 = 0xD7;
const RST_18_OPCODE: u8 = 0xDF;
const RST_20_OPCODE: u8 = 0xE7;
const RST_28_OPCODE: u8 = 0xEF;
const RST_30_OPCODE: u8 = 0xF7;
const RST_38_OPCODE: u8 = 0xFF;

/// Central debugger state for Game Boy.
pub struct GbDebuggerController {
    paused: bool,
    debugger_open: bool,
    view_state: GbDebuggerViewState,
    breakpoints: BreakpointList,
    temporary_breakpoint: Option<TemporaryBreakpoint>,
    breakpoint_ignore_once_at_pc: Option<u16>,
    last_post_instruction_cycles: u64,
    last_post_instruction_frame: u64,
}

impl GbDebuggerController {
    /// Create a new controller with optional pre-loaded breakpoints.
    pub fn new(config_breakpoints: &[BreakpointKind], debugger_enabled: bool) -> Self {
        let mut breakpoints = BreakpointList::new();
        for &kind in config_breakpoints {
            breakpoints.add(kind);
        }
        Self {
            paused: debugger_enabled,
            debugger_open: debugger_enabled,
            view_state: GbDebuggerViewState::default(),
            breakpoints,
            temporary_breakpoint: None,
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

    pub fn breakpoints_mut(&mut self) -> &mut BreakpointList {
        &mut self.breakpoints
    }

    pub fn view_state_mut(&mut self) -> &mut GbDebuggerViewState {
        &mut self.view_state
    }

    // ── Debugger open/close ────────────────────────────────────────────

    /// Open the debugger and pause emulation.
    pub fn enter_debugger<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        gb.set_cpu_trace_enabled(true);
        self.paused = true;
        self.debugger_open = true;
    }

    /// Close the debugger and resume emulation.
    pub fn continue_from_debugger<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        if self
            .breakpoints
            .has_enabled_pc_breakpoint_at(gb.cpu.regs.pc)
        {
            self.breakpoint_ignore_once_at_pc = Some(gb.cpu.regs.pc);
        }

        self.last_post_instruction_cycles = gb.cpu.cycles();
        self.last_post_instruction_frame = gb.cpu.bus.ppu().frame_count();

        gb.set_cpu_trace_enabled(false);
        self.paused = false;
        self.debugger_open = false;
    }

    /// Toggle the debugger: open+pause if closed, continue if open.
    pub fn toggle_debugger<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        if self.debugger_open {
            self.continue_from_debugger(gb);
        } else {
            self.enter_debugger(gb);
        }
    }

    // ── Stepping ───────────────────────────────────────────────────────

    /// Execute one instruction and re-enter the debugger.
    pub fn step_into<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        if self.paused {
            self.paused = false;
            self.run_one_instruction(gb);
            self.paused = true;
        }
    }

    /// Step over CALL/RST instructions (break at return address).
    pub fn step_over<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        if !self.paused {
            return;
        }

        let pc = gb.cpu.regs.pc;
        let opcode = gb.read_for_debugger(pc);

        // Check if it's a CALL or RST instruction
        let is_call_or_rst = matches!(
            opcode,
            CALL_OPCODE
                | CALL_NZ_OPCODE
                | CALL_Z_OPCODE
                | CALL_NC_OPCODE
                | CALL_C_OPCODE
                | RST_00_OPCODE
                | RST_08_OPCODE
                | RST_10_OPCODE
                | RST_18_OPCODE
                | RST_20_OPCODE
                | RST_28_OPCODE
                | RST_30_OPCODE
                | RST_38_OPCODE
        );

        if is_call_or_rst {
            // Calculate return address (CALL is 3 bytes, RST is 1 byte)
            let return_addr = if opcode == CALL_OPCODE
                || opcode == CALL_NZ_OPCODE
                || opcode == CALL_Z_OPCODE
                || opcode == CALL_NC_OPCODE
                || opcode == CALL_C_OPCODE
            {
                pc.wrapping_add(3)
            } else {
                pc.wrapping_add(1)
            };

            self.set_temporary_breakpoint(return_addr);
            self.paused = false;
            self.debugger_open = false;
        } else {
            // Not a call - just step into
            self.step_into(gb);
        }
    }

    /// Run until next frame boundary.
    pub fn run_to_next_frame<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        if !self.paused {
            return;
        }

        let target_frame = gb.cpu.bus.ppu().frame_count() + 1;
        self.breakpoints.add(BreakpointKind::Frame(target_frame));
        self.set_temporary_breakpoint(gb.cpu.regs.pc);
        self.paused = false;
        self.debugger_open = false;
    }

    /// Run until next scanline.
    /// Run to next scanline (not yet implemented).
    ///
    /// Scanline stepping is not yet implemented for the Game Boy debugger.
    /// This method intentionally leaves the debugger paused rather than
    /// approximating with a frame breakpoint, which would be misleading.
    /// True scanline stepping support requires scanline polling with a
    /// max-steps guard or a dedicated breakpoint type.
    pub fn run_to_next_scanline<B: GbBus>(&mut self, _gb: &mut Gb<B>) {
        // Intentionally left as a no-op until true scanline stepping support
        // is implemented. Approximating with run_to_next_frame() would confuse
        // debugger users expecting single-scanline behavior.
    }

    /// Run until specific interrupt is about to fire.
    pub fn run_to_interrupt<B: GbBus>(&mut self, gb: &mut Gb<B>, kind: GbInterruptKind) {
        if !self.paused {
            return;
        }

        self.breakpoints.add(BreakpointKind::GbInterrupt(kind));
        self.set_temporary_breakpoint_for_interrupt(gb.cpu.regs.pc, kind);
        self.paused = false;
        self.debugger_open = false;
    }

    /// Apply UI actions from the debugger interface.
    ///
    /// Handles step over, step into, continue, run-to commands, and breakpoint management.
    pub fn apply_ui_action<B: GbBus>(
        &mut self,
        gb: &mut Gb<B>,
        action: super::ui::GbDebuggerUiAction,
    ) {
        if !self.debugger_open {
            return;
        }

        let mut should_continue = action.continue_run;

        if action.step_over {
            self.step_over(gb);
            should_continue = false; // step_over unpauses internally
        }

        if action.step_into {
            self.step_into(gb);
            should_continue = false; // step_into unpauses internally
        }

        if action.run_to_next_frame {
            self.run_to_next_frame(gb);
            should_continue = false; // run_to methods unpause internally
        }

        if action.run_to_next_scanline {
            self.run_to_next_scanline(gb);
            // Note: currently a no-op, leaves debugger paused
        }

        if action.run_to_vblank {
            self.run_to_interrupt(gb, GbInterruptKind::VBlank);
            should_continue = false;
        }

        if action.run_to_stat {
            self.run_to_interrupt(gb, GbInterruptKind::Stat);
            should_continue = false;
        }

        if action.run_to_timer {
            self.run_to_interrupt(gb, GbInterruptKind::Timer);
            should_continue = false;
        }

        if should_continue {
            self.paused = false;
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

    // ── Main execution loop ────────────────────────────────────────────

    /// Run the emulator until frame ready or debugger pause.
    ///
    /// audio_drain: callback to drain audio samples during execution.
    pub fn run_frame<B: GbBus, F>(&mut self, gb: &mut Gb<B>, audio_drain: &mut F)
    where
        F: FnMut(&mut Gb<B>),
    {
        if self.paused {
            return;
        }

        while !gb.is_frame_ready() {
            // Check pre-instruction breakpoints (PC, interrupt)
            if self.check_breakpoint_hit_pre_instruction(gb) {
                self.enter_debugger(gb);
                return;
            }

            // Execute one instruction
            self.run_one_instruction(gb);

            // Check post-instruction breakpoints (cycle, frame, write)
            if self.check_post_instruction_breakpoints(gb) {
                self.enter_debugger(gb);
                return;
            }

            // Drain audio
            audio_drain(gb);
        }
    }

    // ── Internal helpers ───────────────────────────────────────────────

    fn run_one_instruction<B: GbBus>(&mut self, gb: &mut Gb<B>) {
        // Capture trace before execution
        if gb.cpu_trace_enabled() {
            let pc = gb.cpu.regs.pc;
            let opcode = gb.read_for_debugger(pc);

            // Determine instruction length
            let len = if opcode == 0xCB {
                2
            } else {
                crate::gb::cpu::opcode::lookup(opcode).bytes() as usize
            };

            let mut bytes = Vec::with_capacity(len);
            for i in 0..len {
                bytes.push(gb.read_for_debugger(pc.wrapping_add(i as u16)));
            }

            let actual_op = if opcode == 0xCB {
                bytes.get(1).copied().unwrap_or(0)
            } else {
                opcode
            };
            let text = crate::gb::debugging::disasm::format_instruction(actual_op, pc, &bytes);

            gb.push_cpu_trace_line(CpuTraceLine {
                addr: pc,
                bytes,
                text,
            });
        }

        // Execute instruction
        gb.step();

        // Clear last write address after checking post-instruction breakpoints
        // (We need it for write-address breakpoint evaluation)
    }

    fn check_breakpoint_hit_pre_instruction<B: GbBus>(&mut self, gb: &Gb<B>) -> bool {
        let pc = gb.cpu.regs.pc;

        // Skip if we're ignoring this PC once
        if self.breakpoint_ignore_once_at_pc == Some(pc) {
            self.breakpoint_ignore_once_at_pc = None;
            return false;
        }

        // Build eval context for pre-instruction checks (PC, interrupt)
        let ie = gb.read_for_debugger(0xFFFF);
        let if_reg = gb.read_for_debugger(0xFF0F);
        let ime = gb.cpu.ime;

        let ctx = EvalContext {
            pc,
            prev_cpu_cycles: gb.cpu.cycles(),
            cpu_cycles: gb.cpu.cycles(),
            prev_frame: gb.cpu.bus.ppu().frame_count(),
            frame: gb.cpu.bus.ppu().frame_count(),
            write_addr: None,
            gb_ie: Some(ie),
            gb_if: Some(if_reg),
            gb_ime: Some(ime),
        };

        // Check temporary breakpoint first
        if let Some(ref mut tb) = self.temporary_breakpoint {
            // Check if we've exited the required interrupt
            if let Some(required) = tb.required_interrupt {
                let pending = (ie & if_reg & required.bit_mask()) != 0;
                if !pending && !tb.has_exited_required_interrupt {
                    tb.has_exited_required_interrupt = true;
                }
            }

            // Check if temporary breakpoint is hit
            if tb.pc == pc {
                let should_trigger = if let Some(required) = tb.required_interrupt {
                    tb.has_exited_required_interrupt
                        && (ie & if_reg & required.bit_mask()) != 0
                        && ime
                } else {
                    true
                };

                if should_trigger {
                    self.clear_temporary_breakpoint();
                    return true;
                }
            }

            // If ignoring other breakpoints, skip regular evaluation
            if tb.ignore_other_breakpoints {
                return false;
            }
        }

        // Check regular breakpoints
        self.breakpoints
            .iter()
            .any(|bp| bp.enabled && bp.is_hit(&ctx))
    }

    fn check_post_instruction_breakpoints<B: GbBus>(&mut self, gb: &mut Gb<B>) -> bool {
        let cycles = gb.cpu.cycles();
        let frame = gb.cpu.bus.ppu().frame_count();
        let write_addr = gb.cpu.last_cpu_write_addr();

        // Skip temporary breakpoint interference
        if let Some(ref tb) = self.temporary_breakpoint
            && tb.ignore_other_breakpoints
        {
            self.last_post_instruction_cycles = cycles;
            self.last_post_instruction_frame = frame;
            return false;
        }

        let ctx = EvalContext {
            pc: gb.cpu.regs.pc,
            prev_cpu_cycles: self.last_post_instruction_cycles,
            cpu_cycles: cycles,
            prev_frame: self.last_post_instruction_frame,
            frame,
            write_addr,
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
        };

        self.last_post_instruction_cycles = cycles;
        self.last_post_instruction_frame = frame;

        let hit = self
            .breakpoints
            .iter()
            .any(|bp| bp.enabled && bp.is_hit(&ctx));

        // Clear last_write_addr after evaluating post-instruction breakpoints
        // to enforce strict "this instruction only" semantics for write-address
        // breakpoints. Even though the CPU clears at instruction boundary,
        // this extra clear ensures the debugger doesn't hold stale write info.
        gb.cpu.clear_last_write_addr();

        hit
    }

    // ── Temporary breakpoint management ────────────────────────────────

    fn clear_temporary_breakpoint(&mut self) {
        if let Some(tb) = self.temporary_breakpoint.take() {
            if tb.already_present {
                // Restore original enabled state
                if !tb.was_enabled_before {
                    self.breakpoints.set_pc_breakpoint_enabled(tb.pc, false);
                }
            } else {
                self.remove_pc_breakpoint(tb.pc);
            }
        }
    }

    fn set_temporary_breakpoint(&mut self, pc: u16) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
        let was_enabled_before = if already_present {
            self.breakpoints
                .force_enable_pc_breakpoint_at(pc)
                .unwrap_or(false)
        } else {
            self.add_pc_breakpoint(pc);
            true
        };

        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            was_enabled_before,
            required_interrupt: None,
            has_exited_required_interrupt: true,
            ignore_other_breakpoints: false,
        });
    }

    fn set_temporary_breakpoint_for_interrupt(
        &mut self,
        pc: u16,
        required_interrupt: GbInterruptKind,
    ) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
        let was_enabled_before = if already_present {
            self.breakpoints
                .force_enable_pc_breakpoint_at(pc)
                .unwrap_or(false)
        } else {
            self.add_pc_breakpoint(pc);
            true
        };

        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            was_enabled_before,
            required_interrupt: Some(required_interrupt),
            has_exited_required_interrupt: false,
            ignore_other_breakpoints: true,
        });
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::DmgBus;
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::console::Gb;
    use crate::gb::model::DmgModel;

    // ── Test helpers ───────────────────────────────────────────────────

    fn minimal_cart_with_nop_loop() -> Box<dyn crate::gb::cartridge::GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        // Write NOP loop at $0000: NOP; JP $0000
        rom[0x0000] = 0x00; // NOP
        rom[0x0001] = 0xC3; // JP $0000
        rom[0x0002] = 0x00; // low byte
        rom[0x0003] = 0x00; // high byte
        // Cartridge header
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn default_controller() -> GbDebuggerController {
        GbDebuggerController::new(&[], false)
    }

    fn gb_with_nop_loop() -> Gb<DmgBus> {
        let bus = DmgBus::new(minimal_cart_with_nop_loop(), DmgModel::DmgB);
        let mut gb = Gb::new(bus);

        // Disable boot ROM so we can execute code from $0000
        gb.cpu.bus.write(0xFF50, 0x01);
        gb.cpu.regs.pc = 0x0000;
        gb
    }

    // ── State management tests ─────────────────────────────────────────

    #[test]
    fn test_new_controller_is_not_paused_and_debugger_closed() {
        let ctrl = default_controller();
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    #[test]
    fn test_new_controller_with_debugger_enabled() {
        let ctrl = GbDebuggerController::new(&[], true);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_new_controller_with_config_breakpoints() {
        let ctrl = GbDebuggerController::new(
            &[BreakpointKind::Pc(0xC000), BreakpointKind::Cycle(1000)],
            false,
        );
        assert_eq!(ctrl.breakpoints().len(), 2);
    }

    #[test]
    fn test_enter_debugger_sets_paused_and_open() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();
        ctrl.enter_debugger(&mut gb);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_continue_from_debugger_clears_paused_and_open() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);
        ctrl.continue_from_debugger(&mut gb);

        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    #[test]
    fn test_toggle_debugger_opens_when_closed() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.toggle_debugger(&mut gb);
        assert!(ctrl.is_paused());
        assert!(ctrl.is_debugger_open());
    }

    #[test]
    fn test_toggle_debugger_closes_when_open() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);
        ctrl.toggle_debugger(&mut gb);

        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    // ── Stepping tests ─────────────────────────────────────────────────

    #[test]
    fn test_step_into_executes_one_instruction() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);
        let initial_pc = gb.cpu.regs.pc;

        ctrl.step_into(&mut gb);

        // NOP should advance PC by 1
        assert_eq!(gb.cpu.regs.pc, initial_pc.wrapping_add(1));
        assert!(ctrl.is_paused());
    }

    #[test]
    fn test_step_over_on_nop_behaves_like_step_into() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);
        let initial_pc = gb.cpu.regs.pc;

        ctrl.step_over(&mut gb);

        // NOP is not a CALL/RST, should just step
        assert_eq!(gb.cpu.regs.pc, initial_pc.wrapping_add(1));
        assert!(ctrl.is_paused());
    }

    // ── Breakpoint tests ───────────────────────────────────────────────

    #[test]
    fn test_pc_breakpoint_pauses_execution() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        // Add breakpoint at PC+1
        ctrl.breakpoints_mut().add(BreakpointKind::Pc(0x0001));

        let mut audio_drain = |_: &mut Gb<DmgBus>| {};

        // Run should stop at breakpoint
        ctrl.run_frame(&mut gb, &mut audio_drain);

        assert_eq!(gb.cpu.regs.pc, 0x0001);
        assert!(ctrl.is_paused());
        assert!(
            ctrl.is_debugger_open(),
            "Debugger should open when breakpoint is hit"
        );
    }

    #[test]
    fn test_breakpoint_ignore_once_on_continue() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        // Set PC to where we have a breakpoint
        gb.cpu.regs.pc = 0x0001;
        ctrl.breakpoints_mut().add(BreakpointKind::Pc(0x0001));

        ctrl.enter_debugger(&mut gb);
        ctrl.continue_from_debugger(&mut gb);

        // Should set ignore flag so we don't immediately re-trigger
        assert!(ctrl.breakpoint_ignore_once_at_pc.is_some());
    }

    #[test]
    fn test_step_over_on_call_sets_temporary_breakpoint() {
        let mut ctrl = default_controller();

        // Create ROM with CALL instruction
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xCD; // CALL n16
        rom[0x0001] = 0x10; // target low
        rom[0x0002] = 0xC0; // target high -> CALL $C010
        rom[0x0010] = 0xC9; // RET at target
        // Cartridge header
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;

        let cart = load_cartridge(&rom).expect("valid ROM");
        let bus = DmgBus::new(cart, DmgModel::DmgB);
        let mut gb = Gb::new(bus);

        gb.cpu.bus.write(0xFF50, 0x01); // Disable boot ROM
        gb.cpu.regs.pc = 0x0000;

        ctrl.enter_debugger(&mut gb);
        ctrl.step_over(&mut gb);

        // Should have set temporary breakpoint at return address (PC + 3)
        assert!(ctrl.temporary_breakpoint.is_some());
        let temp_bp = ctrl.temporary_breakpoint.as_ref().unwrap();
        assert_eq!(temp_bp.pc, 0x0003);
        // Should unpause and close debugger UI to run until breakpoint
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    #[test]
    fn test_step_over_on_rst_sets_temporary_breakpoint() {
        let mut ctrl = default_controller();

        // Create ROM with RST instruction
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xC7; // RST $00
        rom[0x0001] = 0x00; // next instruction
        // Cartridge header
        rom[0x0147] = 0x00;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;

        let cart = load_cartridge(&rom).expect("valid ROM");
        let bus = DmgBus::new(cart, DmgModel::DmgB);
        let mut gb = Gb::new(bus);

        gb.cpu.bus.write(0xFF50, 0x01);
        gb.cpu.regs.pc = 0x0000;

        ctrl.enter_debugger(&mut gb);
        ctrl.step_over(&mut gb);

        // Should have set temporary breakpoint at return address (PC + 1)
        assert!(ctrl.temporary_breakpoint.is_some());
        let temp_bp = ctrl.temporary_breakpoint.as_ref().unwrap();
        assert_eq!(temp_bp.pc, 0x0001);
        // Should unpause and close debugger UI to run until breakpoint
        assert!(!ctrl.is_paused());
        assert!(!ctrl.is_debugger_open());
    }

    // ── UI Action tests ────────────────────────────────────────────────

    #[test]
    fn test_apply_ui_action_run_to_next_scanline() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            run_to_next_scanline: true,
            ..Default::default()
        };

        ctrl.apply_ui_action(&mut gb, action);

        // Currently run_to_next_scanline is a no-op, so should stay paused
        assert!(ctrl.is_paused());
    }

    #[test]
    fn test_apply_ui_action_run_to_next_frame() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            run_to_next_frame: true,
            ..Default::default()
        };

        ctrl.apply_ui_action(&mut gb, action);

        // Should have set temporary breakpoint for next frame
        // Note: The temporary breakpoint is stored as PC-based, not frame-based
        // We just check that a temporary breakpoint was set
        assert!(ctrl.temporary_breakpoint.is_some());
        assert!(!ctrl.is_paused()); // Should be running
    }

    #[test]
    fn test_apply_ui_action_run_to_vblank() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb);

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            run_to_vblank: true,
            ..Default::default()
        };

        ctrl.apply_ui_action(&mut gb, action);

        // Should have set temporary breakpoint for VBlank interrupt
        assert!(ctrl.temporary_breakpoint.is_some());
        let temp_bp = ctrl.temporary_breakpoint.as_ref().unwrap();
        assert_eq!(temp_bp.required_interrupt, Some(GbInterruptKind::VBlank));
    }

    #[test]
    fn test_apply_ui_action_add_breakpoint() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb); // Open debugger

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            add_breakpoint: Some(BreakpointKind::Pc(0xC000)),
            ..Default::default()
        };

        ctrl.apply_ui_action(&mut gb, action);

        // Should have added breakpoint
        assert_eq!(ctrl.breakpoints().len(), 1);
        assert!(
            ctrl.breakpoints()
                .iter()
                .any(|b| b.kind == BreakpointKind::Pc(0xC000))
        );
    }

    #[test]
    fn test_apply_ui_action_remove_breakpoint() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb); // Open debugger

        // Add a breakpoint first
        ctrl.breakpoints_mut().add(BreakpointKind::Pc(0xC000));
        ctrl.breakpoints_mut().add(BreakpointKind::Cycle(1000));

        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            remove_breakpoint: Some(0),
            ..Default::default()
        };

        ctrl.apply_ui_action(&mut gb, action);

        // Should have removed first breakpoint
        assert_eq!(ctrl.breakpoints().len(), 1);
        assert!(
            ctrl.breakpoints()
                .iter()
                .any(|b| b.kind == BreakpointKind::Cycle(1000))
        );
    }

    #[test]
    fn test_apply_ui_action_enable_disable_breakpoint() {
        let mut ctrl = default_controller();
        let mut gb = gb_with_nop_loop();

        ctrl.enter_debugger(&mut gb); // Open debugger

        // Add a breakpoint
        ctrl.breakpoints_mut().add(BreakpointKind::Pc(0xC000));
        assert!(ctrl.breakpoints().iter().next().unwrap().enabled);

        // Disable it
        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            disable_breakpoint: Some(0),
            ..Default::default()
        };
        ctrl.apply_ui_action(&mut gb, action);

        // Should have disabled breakpoint
        assert!(!ctrl.breakpoints().iter().next().unwrap().enabled);

        // Enable it again
        let action = crate::gb::debugging::ui::GbDebuggerUiAction {
            enable_breakpoint: Some(0),
            ..Default::default()
        };
        ctrl.apply_ui_action(&mut gb, action);

        // Should be enabled again
        assert!(ctrl.breakpoints().iter().next().unwrap().enabled);
    }

    // ── PC advancement tests ───────────────────────────────────────────

    #[test]
    fn test_step_into_advances_pc_by_one_instruction() {
        let mut gb = gb_with_nop_loop();
        let mut ctrl = default_controller();
        ctrl.enter_debugger(&mut gb);

        // Set PC to 0x0000 (NOP instruction, 1 byte)
        gb.cpu.regs.pc = 0x0000;
        let pc_before = gb.cpu.regs.pc;

        // Step one instruction
        ctrl.step_into(&mut gb);

        // Should advance by exactly 1 byte (NOP is 1 byte)
        assert_eq!(
            gb.cpu.regs.pc,
            pc_before + 1,
            "step_into should execute exactly one NOP instruction"
        );
    }

    #[test]
    fn test_step_over_non_call_advances_pc_by_one_instruction() {
        let mut gb = gb_with_nop_loop();
        let mut ctrl = default_controller();
        ctrl.enter_debugger(&mut gb);

        // Set PC to 0x0000 (NOP instruction, 1 byte)
        gb.cpu.regs.pc = 0x0000;
        let pc_before = gb.cpu.regs.pc;

        // Step over (NOP is not a CALL, so should behave like step_into)
        ctrl.step_over(&mut gb);

        // Should advance by exactly 1 byte
        assert_eq!(
            gb.cpu.regs.pc,
            pc_before + 1,
            "step_over on NOP should execute exactly one instruction"
        );
    }

    // ── View state tests ───────────────────────────────────────────────

    #[test]
    fn test_view_state_is_accessible() {
        let mut ctrl = default_controller();
        let view_state = ctrl.view_state_mut();

        // Should be able to modify view state
        view_state.set_wram_hexdump_base(0xC100);
        assert_eq!(ctrl.view_state_mut().wram_hexdump_base(), Some(0xC100));
    }
}
