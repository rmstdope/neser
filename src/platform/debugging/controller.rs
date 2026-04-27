//! Generic debugger controller components.
//!
//! This module provides shared types and logic for debugger controllers.
//! Platform-specific controllers (NES, GB) use these components while
//! implementing their own run-frame loop and instruction execution.

use super::breakpoints::{BreakpointKind, BreakpointList};

/// One-shot breakpoint used for stepping and run-to operations.
///
/// This struct tracks temporary breakpoints that are automatically
/// cleaned up when hit or when another stepping operation begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporaryBreakpoint<I: Copy + PartialEq + Eq> {
    /// Address where the temporary breakpoint is set.
    pub pc: u16,
    /// Whether a user-defined PC breakpoint already existed at this address.
    pub already_present: bool,
    /// Whether the pre-existing breakpoint was enabled before we took over.
    pub was_enabled_before: bool,
    /// If set, the breakpoint only fires when this interrupt is active.
    pub required_interrupt: Option<I>,
    /// Tracks whether we have exited the required interrupt at least once.
    pub has_exited_required_interrupt: bool,
    /// When true, other breakpoints are suppressed while this temp is active.
    pub ignore_other_breakpoints: bool,
}

impl<I: Copy + PartialEq + Eq> TemporaryBreakpoint<I> {
    /// Create a new temporary breakpoint at the given PC.
    pub fn new(pc: u16, already_present: bool, was_enabled_before: bool) -> Self {
        Self {
            pc,
            already_present,
            was_enabled_before,
            required_interrupt: None,
            has_exited_required_interrupt: true,
            ignore_other_breakpoints: false,
        }
    }

    /// Create a temporary breakpoint for interrupt-based run-to operations.
    pub fn new_for_interrupt(
        pc: u16,
        already_present: bool,
        was_enabled_before: bool,
        interrupt: I,
        currently_in_interrupt: bool,
    ) -> Self {
        Self {
            pc,
            already_present,
            was_enabled_before,
            required_interrupt: Some(interrupt),
            has_exited_required_interrupt: !currently_in_interrupt,
            ignore_other_breakpoints: true,
        }
    }
}

/// Core debugger state shared between platform-specific controllers.
///
/// This struct contains the state that is common to all debugger controllers
/// but doesn't include the view state or platform-specific execution logic.
#[derive(Debug)]
pub struct DebuggerControllerCore<I: Copy + PartialEq + Eq> {
    /// Whether emulation is paused.
    pub paused: bool,
    /// Whether the debugger window is open.
    pub debugger_open: bool,
    /// List of persistent breakpoints.
    pub breakpoints: BreakpointList,
    /// Temporary breakpoint for stepping operations.
    pub temporary_breakpoint: Option<TemporaryBreakpoint<I>>,
    /// PC to ignore once (for continuing from a breakpoint).
    pub breakpoint_ignore_once_at_pc: Option<u16>,
    /// CPU cycles after last instruction (for cycle/frame breakpoints).
    pub last_post_instruction_cycles: u64,
    /// Frame count after last instruction (for frame breakpoints).
    pub last_post_instruction_frame: u64,
}

impl<I: Copy + PartialEq + Eq> Default for DebuggerControllerCore<I> {
    fn default() -> Self {
        Self::new(&[], false)
    }
}

impl<I: Copy + PartialEq + Eq> DebuggerControllerCore<I> {
    /// Create a new controller core with optional pre-loaded breakpoints.
    pub fn new(config_breakpoints: &[BreakpointKind], debugger_enabled: bool) -> Self {
        let mut breakpoints = BreakpointList::new();
        for &kind in config_breakpoints {
            breakpoints.add(kind);
        }
        Self {
            paused: debugger_enabled,
            debugger_open: debugger_enabled,
            breakpoints,
            temporary_breakpoint: None,
            breakpoint_ignore_once_at_pc: None,
            last_post_instruction_cycles: 0,
            last_post_instruction_frame: 0,
        }
    }

    // ── State getters ──────────────────────────────────────────────────

    /// Check if emulation is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Check if the debugger window is open.
    pub fn is_debugger_open(&self) -> bool {
        self.debugger_open
    }

    /// Get reference to breakpoint list.
    pub fn breakpoints(&self) -> &BreakpointList {
        &self.breakpoints
    }

    /// Get mutable reference to breakpoint list.
    pub fn breakpoints_mut(&mut self) -> &mut BreakpointList {
        &mut self.breakpoints
    }

    // ── Debugger open/close ────────────────────────────────────────────

    /// Open the debugger and pause emulation.
    pub fn enter_debugger(&mut self) {
        self.paused = true;
        self.debugger_open = true;
    }

    /// Close the debugger and resume emulation.
    ///
    /// If there's a PC breakpoint at the given address, sets up to ignore it once.
    pub fn continue_from_debugger(&mut self, current_pc: u16, cycles: u64, frame: u64) {
        if self.breakpoints.has_enabled_pc_breakpoint_at(current_pc) {
            self.breakpoint_ignore_once_at_pc = Some(current_pc);
        }
        self.last_post_instruction_cycles = cycles;
        self.last_post_instruction_frame = frame;
        self.paused = false;
        self.debugger_open = false;
    }

    /// Toggle the debugger: open+pause if closed, continue if open.
    pub fn toggle_debugger(&mut self, current_pc: u16, cycles: u64, frame: u64) {
        if self.debugger_open {
            self.continue_from_debugger(current_pc, cycles, frame);
        } else {
            self.enter_debugger();
        }
    }

    // ── Temporary breakpoint management ────────────────────────────────

    /// Clear the temporary breakpoint and restore any pre-existing breakpoint state.
    pub fn clear_temporary_breakpoint(&mut self) {
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

    /// Set a simple temporary breakpoint at the given PC.
    pub fn set_temporary_breakpoint(&mut self, pc: u16) {
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

        self.temporary_breakpoint = Some(TemporaryBreakpoint::new(
            pc,
            already_present,
            was_enabled_before,
        ));
    }

    /// Set a temporary breakpoint for interrupt-based run-to operations.
    pub fn set_temporary_breakpoint_for_interrupt(
        &mut self,
        pc: u16,
        interrupt: I,
        currently_in_interrupt: bool,
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

        self.temporary_breakpoint = Some(TemporaryBreakpoint::new_for_interrupt(
            pc,
            already_present,
            was_enabled_before,
            interrupt,
            currently_in_interrupt,
        ));
    }

    /// Check if we should ignore the PC breakpoint at the given address.
    ///
    /// Returns true if the breakpoint should be ignored (and clears the ignore flag).
    pub fn should_ignore_pc_breakpoint(&mut self, pc: u16) -> bool {
        if self.breakpoint_ignore_once_at_pc == Some(pc) {
            self.breakpoint_ignore_once_at_pc = None;
            true
        } else {
            false
        }
    }

    /// Check if other breakpoints should be suppressed due to a run-to operation.
    pub fn should_suppress_other_breakpoints(&self, pc: u16) -> bool {
        if let Some(ref tb) = self.temporary_breakpoint {
            tb.ignore_other_breakpoints && pc != tb.pc
        } else {
            false
        }
    }

    /// Update interrupt exit tracking for run-to-interrupt operations.
    ///
    /// Call this at the start of each instruction to track when we've exited
    /// the required interrupt context.
    pub fn update_interrupt_exit_tracking(&mut self, current_interrupt: Option<I>) {
        if let Some(ref mut tb) = self.temporary_breakpoint
            && let Some(required) = tb.required_interrupt
            && !tb.has_exited_required_interrupt
            && current_interrupt != Some(required)
        {
            tb.has_exited_required_interrupt = true;
        }
    }

    /// Check if a temporary breakpoint is hit at the given PC.
    ///
    /// Returns `Some(should_trigger)` if there's a temporary breakpoint at this PC,
    /// with the bool indicating whether it should fire based on interrupt state.
    /// Returns `None` if there's no temporary breakpoint at this PC.
    pub fn check_temporary_breakpoint_hit(
        &mut self,
        pc: u16,
        current_interrupt: Option<I>,
    ) -> Option<bool> {
        let tb = self.temporary_breakpoint.as_ref()?;

        if tb.pc != pc {
            return None;
        }

        // Check interrupt requirements
        let should_trigger = if let Some(required) = tb.required_interrupt {
            tb.has_exited_required_interrupt && current_interrupt == Some(required)
        } else {
            true
        };

        Some(should_trigger)
    }

    /// Clean up the temporary breakpoint after it has been triggered.
    pub fn cleanup_triggered_temporary_breakpoint(&mut self) {
        if let Some(tb) = self.temporary_breakpoint.take() {
            if tb.already_present {
                if !tb.was_enabled_before {
                    self.breakpoints.set_pc_breakpoint_enabled(tb.pc, false);
                }
            } else {
                self.remove_pc_breakpoint(tb.pc);
            }
        }
    }

    // ── Breakpoint CRUD helpers ────────────────────────────────────────

    /// Add a PC breakpoint at the given address.
    pub fn add_pc_breakpoint(&mut self, addr: u16) {
        self.breakpoints.add(BreakpointKind::Pc(addr));
    }

    /// Remove a PC breakpoint at the given address.
    pub fn remove_pc_breakpoint(&mut self, addr: u16) {
        if let Some(idx) = self
            .breakpoints
            .iter()
            .position(|b| b.kind == BreakpointKind::Pc(addr))
        {
            self.breakpoints.remove(idx);
        }
    }

    /// Update post-instruction tracking values.
    pub fn update_post_instruction_state(&mut self, cycles: u64, frame: u64) {
        self.last_post_instruction_cycles = cycles;
        self.last_post_instruction_frame = frame;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use a simple unit type for interrupt in tests
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestInterrupt;

    #[test]
    fn test_controller_core_new_default_state() {
        let core: DebuggerControllerCore<TestInterrupt> = DebuggerControllerCore::new(&[], false);
        assert!(!core.is_paused());
        assert!(!core.is_debugger_open());
        assert!(core.breakpoints().is_empty());
    }

    #[test]
    fn test_controller_core_new_with_breakpoints() {
        let core: DebuggerControllerCore<TestInterrupt> = DebuggerControllerCore::new(
            &[BreakpointKind::Pc(0x8000), BreakpointKind::Cycle(1000)],
            false,
        );
        assert_eq!(core.breakpoints().len(), 2);
    }

    #[test]
    fn test_controller_core_debugger_enabled() {
        let core: DebuggerControllerCore<TestInterrupt> = DebuggerControllerCore::new(&[], true);
        assert!(core.is_paused());
        assert!(core.is_debugger_open());
    }

    #[test]
    fn test_enter_debugger() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);
        core.enter_debugger();
        assert!(core.is_paused());
        assert!(core.is_debugger_open());
    }

    #[test]
    fn test_continue_from_debugger() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], true);
        core.continue_from_debugger(0x8000, 100, 5);
        assert!(!core.is_paused());
        assert!(!core.is_debugger_open());
        assert_eq!(core.last_post_instruction_cycles, 100);
        assert_eq!(core.last_post_instruction_frame, 5);
    }

    #[test]
    fn test_continue_from_debugger_with_breakpoint_at_pc() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[BreakpointKind::Pc(0x8000)], true);
        core.continue_from_debugger(0x8000, 100, 5);
        assert_eq!(core.breakpoint_ignore_once_at_pc, Some(0x8000));
    }

    #[test]
    fn test_toggle_debugger() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);

        // Toggle on
        core.toggle_debugger(0x8000, 0, 0);
        assert!(core.is_paused());
        assert!(core.is_debugger_open());

        // Toggle off
        core.toggle_debugger(0x8000, 0, 0);
        assert!(!core.is_paused());
        assert!(!core.is_debugger_open());
    }

    #[test]
    fn test_temporary_breakpoint_new() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);
        core.set_temporary_breakpoint(0xC000);

        assert!(core.temporary_breakpoint.is_some());
        let tb = core.temporary_breakpoint.as_ref().unwrap();
        assert_eq!(tb.pc, 0xC000);
        assert!(!tb.already_present);
        assert!(tb.was_enabled_before);
        assert!(core.breakpoints.has_pc_breakpoint_at(0xC000));
    }

    #[test]
    fn test_temporary_breakpoint_with_existing() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[BreakpointKind::Pc(0xC000)], false);

        // Disable the existing breakpoint
        core.breakpoints.disable(0);

        core.set_temporary_breakpoint(0xC000);

        let tb = core.temporary_breakpoint.as_ref().unwrap();
        assert!(tb.already_present);
        assert!(!tb.was_enabled_before);
    }

    #[test]
    fn test_clear_temporary_breakpoint_removes_added() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);
        core.set_temporary_breakpoint(0xC000);
        assert!(core.breakpoints.has_pc_breakpoint_at(0xC000));

        core.clear_temporary_breakpoint();
        assert!(!core.breakpoints.has_pc_breakpoint_at(0xC000));
        assert!(core.temporary_breakpoint.is_none());
    }

    #[test]
    fn test_clear_temporary_breakpoint_restores_disabled() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[BreakpointKind::Pc(0xC000)], false);
        core.breakpoints.disable(0);

        core.set_temporary_breakpoint(0xC000);
        // Should be force-enabled now
        assert!(core.breakpoints.iter().next().unwrap().enabled);

        core.clear_temporary_breakpoint();
        // Should be restored to disabled
        assert!(!core.breakpoints.iter().next().unwrap().enabled);
    }

    #[test]
    fn test_should_ignore_pc_breakpoint() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);
        core.breakpoint_ignore_once_at_pc = Some(0x8000);

        // First check should return true and clear
        assert!(core.should_ignore_pc_breakpoint(0x8000));
        assert!(core.breakpoint_ignore_once_at_pc.is_none());

        // Second check should return false
        assert!(!core.should_ignore_pc_breakpoint(0x8000));

        // Different address should return false
        core.breakpoint_ignore_once_at_pc = Some(0x8000);
        assert!(!core.should_ignore_pc_breakpoint(0x9000));
    }

    #[test]
    fn test_should_suppress_other_breakpoints() {
        let mut core: DebuggerControllerCore<TestInterrupt> =
            DebuggerControllerCore::new(&[], false);

        // No temporary breakpoint - no suppression
        assert!(!core.should_suppress_other_breakpoints(0x8000));

        // Regular temporary breakpoint - no suppression
        core.set_temporary_breakpoint(0xC000);
        assert!(!core.should_suppress_other_breakpoints(0x8000));

        // Interrupt-based temporary breakpoint - should suppress
        core.set_temporary_breakpoint_for_interrupt(0xC000, TestInterrupt, false);
        assert!(core.should_suppress_other_breakpoints(0x8000));
        assert!(!core.should_suppress_other_breakpoints(0xC000)); // Not at the temp bp addr
    }
}
