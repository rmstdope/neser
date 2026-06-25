//! GBA debugger controller — glues breakpoints, the trace ring buffer and the
//! disassembler to the [`Arm7tdmi`] CPU.
//!
//! The controller is intentionally bus-generic: it accepts any
//! [`crate::gba::cpu::Bus`] implementation, which makes it usable both with
//! the production [`crate::gba::GbaBus`] and with the [`RamBus`] used in the
//! CPU unit tests. Frontends drive emulation via [`run_until_breakpoint`] /
//! [`step`], and consume captured [`TraceEntry`] values via
//! [`GbaDebuggerController::trace`].
//!
//! Optional file-trace logging is provided via
//! [`GbaDebuggerController::set_trace_file`]: every retired instruction is
//! appended to the file as a single line of disassembly when set. Errors
//! while writing are silently ignored — the debugger never aborts the
//! emulation if the log device disappears.
//!
//! [`Arm7tdmi`]: crate::gba::cpu::Arm7tdmi
//! [`RamBus`]: crate::gba::cpu::RamBus
//! [`run_until_breakpoint`]: GbaDebuggerController::run_until_breakpoint
//! [`step`]: GbaDebuggerController::step

// Test literals deliberately mirror Thumb instruction-encoding bit fields.
#![allow(clippy::unusual_byte_groupings)]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::disasm::{disasm_arm, disasm_thumb};
use super::trace::{CpuTrace, TraceEntry};
use crate::gba::cpu::{Arm7tdmi, Bus};
use crate::platform::debugging::breakpoints::{BreakpointKind, BreakpointList};

/// Result of attempting to advance emulation under debugger control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointHit {
    /// No breakpoint was hit during the requested run.
    None,
    /// Execution halted because the program counter reached `addr`.
    At(u32),
}

/// Combined breakpoint + trace state for the GBA debugger.
pub struct GbaDebuggerController {
    breakpoints: BreakpointList<u32>,
    trace: CpuTrace,
    trace_file: Option<BufWriter<File>>,
}

impl Default for GbaDebuggerController {
    fn default() -> Self {
        Self::new()
    }
}

impl GbaDebuggerController {
    /// Create a new controller with default trace capacity and no breakpoints.
    pub fn new() -> Self {
        Self {
            breakpoints: BreakpointList::new(),
            trace: CpuTrace::default(),
            trace_file: None,
        }
    }

    // ── Breakpoints ────────────────────────────────────────────────────────

    /// Mutable access to the breakpoint set.
    pub fn breakpoints_mut(&mut self) -> &mut BreakpointList<u32> {
        &mut self.breakpoints
    }

    /// Read-only access to the breakpoint set.
    pub fn breakpoints(&self) -> &BreakpointList<u32> {
        &self.breakpoints
    }

    /// Convenience: set a PC breakpoint at `addr`. Returns `true` if it was
    /// newly added, `false` if one already existed there.
    ///
    /// If a breakpoint already exists at `addr` it is (re-)enabled: the previous
    /// `BTreeSet`-based set had no disabled state, so an existing GBA breakpoint
    /// must remain active even if it had been disabled via the shared list APIs
    /// or loaded disabled from a `.debug` file.
    pub fn add_breakpoint(&mut self, addr: u32) -> bool {
        if self.breakpoints.has_pc_breakpoint_at(addr) {
            self.breakpoints.set_pc_breakpoint_enabled(addr, true);
            return false;
        }
        self.breakpoints.add(BreakpointKind::Pc(addr));
        true
    }

    /// Convenience: clear the PC breakpoint at `addr`. Returns `true` if one
    /// was present.
    pub fn remove_breakpoint(&mut self, addr: u32) -> bool {
        if !self.breakpoints.has_pc_breakpoint_at(addr) {
            return false;
        }
        self.breakpoints
            .remove_first_matching(&BreakpointKind::Pc(addr));
        true
    }

    // ── Trace buffer ──────────────────────────────────────────────────────

    /// Mutable access to the trace ring buffer (capacity, enable, ...).
    pub fn trace_mut(&mut self) -> &mut CpuTrace {
        &mut self.trace
    }

    /// Read-only access to the trace ring buffer.
    pub fn trace(&self) -> &CpuTrace {
        &self.trace
    }

    /// Enable trace capture.
    pub fn enable_trace(&mut self, enabled: bool) {
        self.trace.set_enabled(enabled);
    }

    /// Begin appending every retired instruction (as disassembly) to a file.
    /// Replaces any previous trace file.
    pub fn set_trace_file<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let file = File::create(path)?;
        self.trace_file = Some(BufWriter::new(file));
        Ok(())
    }

    /// Stop file logging. Flushes the writer best-effort before dropping it
    /// so that buffered lines are persisted; flush errors are ignored
    /// because trace logging is non-critical.
    pub fn clear_trace_file(&mut self) {
        if let Some(mut w) = self.trace_file.take() {
            let _ = w.flush();
        }
    }

    // ── Execution ─────────────────────────────────────────────────────────

    /// Execute exactly one instruction, capturing it into the trace and the
    /// optional file logger. Returns the new PC.
    pub fn step<B: Bus>(&mut self, cpu: &mut Arm7tdmi, bus: &mut B) -> u32 {
        let pc = cpu.regs.r[15];
        let thumb = cpu.thumb();
        let raw_instr = if thumb {
            bus.read16(pc & !1) as u32
        } else {
            bus.read32(pc & !3)
        };
        let disasm = if thumb {
            disasm_thumb(raw_instr as u16, pc)
        } else {
            disasm_arm(raw_instr, pc)
        };

        cpu.step(bus);

        let new_pc = cpu.regs.r[15];
        let entry = TraceEntry {
            pc,
            instr: raw_instr,
            thumb,
            disasm,
            regs: cpu.regs.r,
            cpsr: cpu.regs.cpsr,
            cycles: cpu.cycles,
        };
        if let Some(w) = self.trace_file.as_mut() {
            // Best-effort logging; ignore I/O errors so the emulator keeps running.
            let _ = writeln!(
                w,
                "{:08X}: {:08X} {}{}",
                entry.pc,
                entry.instr,
                if entry.thumb { "T " } else { "" },
                entry.disasm
            );
        }
        self.trace.push(entry);
        new_pc
    }

    /// Run up to `max_steps` instructions, halting early if PC matches an
    /// active breakpoint after a step.
    pub fn run_until_breakpoint<B: Bus>(
        &mut self,
        cpu: &mut Arm7tdmi,
        bus: &mut B,
        max_steps: u32,
    ) -> BreakpointHit {
        for _ in 0..max_steps {
            let new_pc = self.step(cpu, bus);
            if self.breakpoints.has_enabled_pc_breakpoint_at(new_pc) {
                return BreakpointHit::At(new_pc);
            }
        }
        BreakpointHit::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cpu::registers::CpuMode;
    use crate::gba::cpu::{Arm7tdmi, RamBus};

    /// Build a CPU and RAM ready to execute Thumb code at PC=0.
    fn make_thumb_cpu(words: &[u16]) -> (Arm7tdmi, RamBus) {
        let mut cpu = Arm7tdmi::new();
        // Drop privileges so we don't perturb banked SP/LR during stepping
        // (mirrors how the executor unit tests construct their CPU).
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.cpsr &= !crate::gba::cpu::registers::FLAG_I;
        cpu.regs.cpsr &= !crate::gba::cpu::registers::FLAG_F;
        cpu.regs.r[15] = 0;

        let mut bus = RamBus::new(0x1000);
        for (i, w) in words.iter().enumerate() {
            bus.write16(i as u32 * 2, *w);
        }
        (cpu, bus)
    }

    #[test]
    fn step_records_disassembly_and_register_snapshot() {
        // MOV R0, #42 (Thumb format 3)
        let (mut cpu, mut bus) = make_thumb_cpu(&[0b00100_000_00101010u16]);
        let mut dbg = GbaDebuggerController::new();
        dbg.enable_trace(true);

        dbg.step(&mut cpu, &mut bus);

        let last = dbg.trace().last().expect("trace entry recorded");
        assert_eq!(last.pc, 0);
        assert_eq!(last.disasm, "MOV R0, #42");
        assert!(last.thumb);
        assert_eq!(last.regs[0], 42);
    }

    #[test]
    fn run_until_breakpoint_halts_at_breakpoint_address() {
        // 4 × MOV R0, #0  (each is 2 bytes; PC advances 0,2,4,6,...).
        let (mut cpu, mut bus) =
            make_thumb_cpu(&[0x2000u16, 0x2000u16, 0x2000u16, 0x2000u16, 0x2000u16]);
        let mut dbg = GbaDebuggerController::new();
        dbg.add_breakpoint(0x4);

        let hit = dbg.run_until_breakpoint(&mut cpu, &mut bus, 16);

        assert_eq!(hit, BreakpointHit::At(0x4));
        assert_eq!(cpu.regs.r[15], 0x4);
    }

    #[test]
    fn run_until_breakpoint_returns_none_when_no_breakpoint_hit() {
        let (mut cpu, mut bus) = make_thumb_cpu(&[0x2000u16; 4]);
        let mut dbg = GbaDebuggerController::new();
        // No breakpoints; just run a small number of steps.
        let hit = dbg.run_until_breakpoint(&mut cpu, &mut bus, 3);
        assert_eq!(hit, BreakpointHit::None);
        assert_eq!(cpu.regs.r[15], 6);
    }

    #[test]
    fn run_until_breakpoint_ignores_a_disabled_breakpoint() {
        let (mut cpu, mut bus) =
            make_thumb_cpu(&[0x2000u16, 0x2000u16, 0x2000u16, 0x2000u16, 0x2000u16]);
        let mut dbg = GbaDebuggerController::new();
        dbg.add_breakpoint(0x4);
        // Disabling it via the shared list API must prevent the run loop from
        // halting (it only stops on active breakpoints).
        dbg.breakpoints_mut().set_pc_breakpoint_enabled(0x4, false);
        let hit = dbg.run_until_breakpoint(&mut cpu, &mut bus, 3);
        assert_eq!(hit, BreakpointHit::None);
        assert_eq!(cpu.regs.r[15], 6);
    }

    #[test]
    fn add_breakpoint_re_enables_an_existing_disabled_breakpoint() {
        let mut dbg = GbaDebuggerController::new();
        assert!(dbg.add_breakpoint(0x100));
        dbg.breakpoints_mut()
            .set_pc_breakpoint_enabled(0x100, false);
        assert!(!dbg.breakpoints().has_enabled_pc_breakpoint_at(0x100));
        // Re-adding does not insert a duplicate, but must re-enable it so the
        // breakpoint stays active (matching the old set's present == active).
        assert!(!dbg.add_breakpoint(0x100));
        assert!(dbg.breakpoints().has_enabled_pc_breakpoint_at(0x100));
    }

    #[test]
    fn trace_disabled_by_default_so_step_does_not_record() {
        let (mut cpu, mut bus) = make_thumb_cpu(&[0x2000u16]);
        let mut dbg = GbaDebuggerController::new();
        dbg.step(&mut cpu, &mut bus);
        assert!(dbg.trace().is_empty());
    }

    #[test]
    fn breakpoint_helpers_delegate_to_set() {
        let mut dbg = GbaDebuggerController::new();
        assert!(dbg.add_breakpoint(0x100));
        assert!(dbg.breakpoints().has_pc_breakpoint_at(0x100));
        assert!(dbg.remove_breakpoint(0x100));
        assert!(!dbg.breakpoints().has_pc_breakpoint_at(0x100));
    }

    #[test]
    fn trace_file_logging_writes_one_line_per_step() {
        let (mut cpu, mut bus) = make_thumb_cpu(&[0x2000u16, 0x2000u16]);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("neser-gba-trace-{}.log", std::process::id()));
        let mut dbg = GbaDebuggerController::new();
        dbg.set_trace_file(&path).unwrap();
        // Note: file logging works regardless of whether the in-memory ring
        // buffer is enabled.
        dbg.step(&mut cpu, &mut bus);
        dbg.step(&mut cpu, &mut bus);
        dbg.clear_trace_file();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("MOV R0, #0"));
        assert!(lines[1].contains("MOV R0, #0"));
        let _ = std::fs::remove_file(&path);
    }
}
