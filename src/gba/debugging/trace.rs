//! ARM7TDMI execution trace.
//!
//! Captures a ring buffer of recently executed instructions for the GBA
//! debugger. Each [`TraceEntry`] records the PC, the raw instruction word
//! (16- or 32-bit), the disassembled mnemonic and a snapshot of the 16 ARM
//! general-purpose registers plus CPSR at the time the instruction retired.
//!
//! The ring-buffer mechanics live in the shared
//! [`crate::platform::debugging::trace_ring::TraceRing`]; [`CpuTrace`] is simply
//! `TraceRing<TraceEntry>`, so the bounded-buffer/enable/evict behaviour is
//! implemented and tested once in the platform layer and reused here (and by
//! future SNES debugging).

pub use crate::platform::debugging::trace_ring::{DEFAULT_TRACE_CAPACITY, TraceRing};

/// Snapshot of an executed ARM7TDMI instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    /// Address of the instruction that was executed.
    pub pc: u32,
    /// Raw instruction word (low 16 bits used in Thumb mode).
    pub instr: u32,
    /// `true` if the instruction was a 16-bit Thumb halfword.
    pub thumb: bool,
    /// Disassembled mnemonic (already-formatted).
    pub disasm: String,
    /// Snapshot of R0..R15 at the time the instruction retired.
    pub regs: [u32; 16],
    /// CPSR value at the time the instruction retired.
    pub cpsr: u32,
    /// Cumulative cycle counter at the time the instruction retired.
    pub cycles: u64,
}

/// Bounded ring buffer of executed ARM7TDMI instructions.
pub type CpuTrace = TraceRing<TraceEntry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_carries_register_snapshot() {
        let mut regs = [0u32; 16];
        regs[0] = 0xDEAD_BEEF;
        regs[15] = 0x0800_0000;
        let mut trace = CpuTrace::with_capacity(4);
        trace.set_enabled(true);
        trace.push(TraceEntry {
            pc: 0x0800_0000,
            instr: 0xE3A0_0001,
            thumb: false,
            disasm: "MOV R0, #0x1".to_string(),
            regs,
            cpsr: 0x6000_001F,
            cycles: 42,
        });
        let last = trace.last().unwrap();
        assert_eq!(last.regs[0], 0xDEAD_BEEF);
        assert_eq!(last.regs[15], 0x0800_0000);
        assert_eq!(last.cpsr, 0x6000_001F);
        assert_eq!(last.cycles, 42);
        assert!(!last.thumb);
    }

    #[test]
    fn default_uses_the_shared_default_capacity() {
        let trace = CpuTrace::default();
        assert_eq!(trace.capacity(), DEFAULT_TRACE_CAPACITY);
        assert!(trace.is_empty());
    }
}
