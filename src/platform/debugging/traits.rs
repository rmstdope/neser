//! Core traits for platform-agnostic debugger components.
//!
//! These traits define the interface that emulators (NES, GB) must implement
//! to use the generic debugger controller, snapshot generation, and disassembly.

use super::breakpoints::EvalContext;

/// Trait for emulators that support debugging operations.
///
/// Implemented by `Nes` and `Gb<B>` to provide the generic debugger controller
/// with access to CPU state, memory, and execution control.
pub trait DebuggableEmulator {
    /// The type of interrupt this platform supports.
    type InterruptKind: Copy + PartialEq + Eq + core::fmt::Debug;

    // ── CPU State ──────────────────────────────────────────────────────

    /// Get the current program counter.
    fn pc(&self) -> u16;

    /// Get the total CPU cycles elapsed.
    fn cycles(&self) -> u64;

    /// Get the current PPU frame count.
    fn frame_count(&self) -> u64;

    /// Get the current interrupt context (if any).
    fn current_interrupt(&self) -> Option<Self::InterruptKind>;

    // ── Memory Access ──────────────────────────────────────────────────

    /// Read a byte from memory for debugger purposes (non-side-effecting).
    fn read_for_debugger(&self, addr: u16) -> u8;

    /// Read the opcode at the given address.
    fn read_opcode(&self, addr: u16) -> u8 {
        self.read_for_debugger(addr)
    }

    // ── Instruction Classification ─────────────────────────────────────

    /// Check if the opcode at the given address is a call/subroutine instruction.
    /// Used by step-over to determine when to set a breakpoint after the call.
    fn is_call_instruction(&self, addr: u16) -> bool;

    /// Get the instruction length at the given address.
    /// Used by step-over to calculate the return address.
    fn instruction_length(&self, addr: u16) -> u16;

    // ── Execution Control ──────────────────────────────────────────────

    /// Execute a single CPU instruction and return the new PC.
    fn run_one_instruction(&mut self) -> u16;

    /// Enable or disable CPU trace recording.
    fn set_cpu_trace_enabled(&mut self, enabled: bool);

    /// Check if CPU trace is currently enabled.
    fn is_cpu_trace_enabled(&self) -> bool;

    // ── Breakpoint Context ─────────────────────────────────────────────

    /// Build the evaluation context for breakpoint checking after an instruction.
    fn build_eval_context(
        &self,
        prev_cycles: u64,
        prev_frame: u64,
        write_addr: Option<u16>,
    ) -> EvalContext;
}

/// Trait for CPU register snapshots.
///
/// Each platform implements this for its specific CPU register set.
/// Used by the generic `DebuggerSnapshot` to store register state.
pub trait CpuSnapshot: Clone + Default + PartialEq + Eq + core::fmt::Debug {
    /// Format the CPU state as a multi-line string for display.
    fn format_display(&self) -> String;
}

/// Describes a memory region for hexdump display in the debugger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    /// Human-readable name for the region (e.g., "WRAM", "VRAM", "PRG").
    pub name: &'static str,
    /// Minimum valid address for this region.
    pub min_addr: u16,
    /// Maximum valid address for this region (inclusive).
    pub max_addr: u16,
    /// Default starting address for hexdump display.
    pub default_base: u16,
}

impl MemoryRegion {
    /// Normalize an address to a 16-byte aligned value within this region.
    pub fn normalize_base(&self, base: u16) -> u16 {
        let aligned = base & 0xFFF0;
        // Ensure base + 0xFF stays within region
        let max_base = self.max_addr.saturating_sub(0xFF);
        aligned.clamp(self.min_addr, max_base)
    }

    /// Calculate a hexdump base address centered around the given PC.
    pub fn base_from_pc(&self, pc: u16) -> u16 {
        if (self.min_addr..=self.max_addr).contains(&pc) {
            let centered = pc & 0xFFF0;
            let base = centered.saturating_sub(0x80);
            self.normalize_base(base)
        } else {
            self.default_base
        }
    }
}

/// Trait for memory region configuration.
///
/// Each platform implements this to define which memory regions are available
/// for hexdump display in the debugger.
pub trait MemoryRegions: Default + Clone + PartialEq + Eq + core::fmt::Debug {
    /// Get the list of memory regions available for hexdump display.
    fn regions() -> &'static [MemoryRegion];

    /// Get the primary memory region (used when only one region is shown).
    fn primary_region() -> &'static MemoryRegion {
        Self::regions()
            .first()
            .expect("at least one region required")
    }
}

/// Marker trait for disassembly line snapshots.
///
/// Implemented by platform-specific disassembly line types.
pub trait DisasmLine: Clone + PartialEq + Eq + core::fmt::Debug {
    /// Get the address of this instruction.
    fn addr(&self) -> u16;

    /// Get the raw instruction bytes.
    fn bytes(&self) -> &[u8];

    /// Get the formatted instruction text.
    fn text(&self) -> &str;

    /// Check if this is the current instruction (at PC).
    fn is_current(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_region_normalize_base() {
        let region = MemoryRegion {
            name: "WRAM",
            min_addr: 0xC000,
            max_addr: 0xDFFF,
            default_base: 0xC000,
        };

        // Already aligned and within range
        assert_eq!(region.normalize_base(0xC000), 0xC000);
        assert_eq!(region.normalize_base(0xC100), 0xC100);

        // Unaligned - should align to 16-byte boundary
        assert_eq!(region.normalize_base(0xC005), 0xC000);
        assert_eq!(region.normalize_base(0xC01F), 0xC010);

        // Below min - should clamp to min
        assert_eq!(region.normalize_base(0xB000), 0xC000);

        // Above max (would overflow with +0xFF) - should clamp
        assert_eq!(region.normalize_base(0xDF10), 0xDF00);
    }

    #[test]
    fn test_memory_region_base_from_pc() {
        let region = MemoryRegion {
            name: "WRAM",
            min_addr: 0xC000,
            max_addr: 0xDFFF,
            default_base: 0xC000,
        };

        // PC within region - center around PC
        // 0xC100 & 0xFFF0 = 0xC100, then -0x80 = 0xC080
        assert_eq!(region.base_from_pc(0xC100), 0xC080);

        // PC outside region - use default
        assert_eq!(region.base_from_pc(0x8000), 0xC000);
    }
}
