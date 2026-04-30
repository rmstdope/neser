//! ARM7TDMI execution trace.
//!
//! Captures a configurable ring buffer of recently executed instructions for
//! the GBA debugger. Each [`TraceEntry`] records the PC, the raw instruction
//! word (16- or 32-bit), the disassembled mnemonic and a snapshot of the
//! 16 ARM general-purpose registers plus CPSR at the time the instruction
//! retired.
//!
//! The [`CpuTrace`] type is a thin wrapper around [`std::collections::VecDeque`]
//! that bounds itself to a configurable capacity. When full, pushing a new
//! entry evicts the oldest.

use std::collections::VecDeque;

/// Default ring-buffer capacity (number of entries).
pub const DEFAULT_TRACE_CAPACITY: usize = 1024;

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

/// Bounded ring buffer of executed instructions.
#[derive(Debug, Clone)]
pub struct CpuTrace {
    entries: VecDeque<TraceEntry>,
    capacity: usize,
    enabled: bool,
}

impl Default for CpuTrace {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_TRACE_CAPACITY)
    }
}

impl CpuTrace {
    /// Create a new trace buffer with the given capacity.
    ///
    /// A capacity of zero is treated as 1 to avoid divide-by-zero / panic
    /// when pushing.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            enabled: false,
        }
    }

    /// Configured capacity (max number of retained entries).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Resize the ring buffer. Existing oldest entries are dropped if the new
    /// capacity is smaller than the current entry count.
    pub fn set_capacity(&mut self, capacity: usize) {
        let capacity = capacity.max(1);
        self.capacity = capacity;
        while self.entries.len() > capacity {
            self.entries.pop_front();
        }
        // VecDeque doesn't shrink automatically; that's fine for a debug buffer.
    }

    /// Whether the trace is currently capturing.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable trace capture. Disabling does not clear existing
    /// entries.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Number of entries currently retained.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if no entries are retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Append an entry. Drops the oldest if the buffer is full.
    ///
    /// No-op when the trace is disabled.
    pub fn push(&mut self, entry: TraceEntry) {
        if !self.enabled {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Iterate over retained entries from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &TraceEntry> {
        self.entries.iter()
    }

    /// Most recently retired instruction, if any.
    pub fn last(&self) -> Option<&TraceEntry> {
        self.entries.back()
    }

    /// Drop all retained entries (capacity unchanged).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pc: u32) -> TraceEntry {
        TraceEntry {
            pc,
            instr: 0,
            thumb: false,
            disasm: format!("NOP @ {:08X}", pc),
            regs: [0; 16],
            cpsr: 0,
            cycles: pc as u64,
        }
    }

    #[test]
    fn default_capacity_is_used_when_constructed_via_default() {
        let trace = CpuTrace::default();
        assert_eq!(trace.capacity(), DEFAULT_TRACE_CAPACITY);
        assert!(trace.is_empty());
    }

    #[test]
    fn capacity_zero_is_clamped_to_one() {
        let trace = CpuTrace::with_capacity(0);
        assert_eq!(trace.capacity(), 1);
    }

    #[test]
    fn push_is_noop_when_disabled() {
        let mut trace = CpuTrace::with_capacity(4);
        trace.push(entry(0x1000));
        assert!(trace.is_empty());
    }

    #[test]
    fn push_records_entries_when_enabled() {
        let mut trace = CpuTrace::with_capacity(4);
        trace.set_enabled(true);
        trace.push(entry(0x1000));
        trace.push(entry(0x1004));
        assert_eq!(trace.len(), 2);
        assert_eq!(trace.last().unwrap().pc, 0x1004);
    }

    #[test]
    fn ring_buffer_evicts_oldest_when_full() {
        let mut trace = CpuTrace::with_capacity(3);
        trace.set_enabled(true);
        for pc in [1u32, 2, 3, 4, 5] {
            trace.push(entry(pc));
        }
        let pcs: Vec<u32> = trace.iter().map(|e| e.pc).collect();
        assert_eq!(pcs, vec![3, 4, 5]);
    }

    #[test]
    fn iter_yields_oldest_to_newest() {
        let mut trace = CpuTrace::with_capacity(4);
        trace.set_enabled(true);
        trace.push(entry(0x10));
        trace.push(entry(0x20));
        trace.push(entry(0x30));
        let pcs: Vec<u32> = trace.iter().map(|e| e.pc).collect();
        assert_eq!(pcs, vec![0x10, 0x20, 0x30]);
    }

    #[test]
    fn clear_drops_entries_but_preserves_capacity() {
        let mut trace = CpuTrace::with_capacity(8);
        trace.set_enabled(true);
        trace.push(entry(0x1));
        trace.push(entry(0x2));
        trace.clear();
        assert!(trace.is_empty());
        assert_eq!(trace.capacity(), 8);
    }

    #[test]
    fn shrinking_capacity_drops_oldest_entries() {
        let mut trace = CpuTrace::with_capacity(8);
        trace.set_enabled(true);
        for pc in 0u32..8 {
            trace.push(entry(pc));
        }
        trace.set_capacity(3);
        let pcs: Vec<u32> = trace.iter().map(|e| e.pc).collect();
        assert_eq!(pcs, vec![5, 6, 7]);
    }

    #[test]
    fn growing_capacity_does_not_disturb_entries() {
        let mut trace = CpuTrace::with_capacity(2);
        trace.set_enabled(true);
        trace.push(entry(0x1));
        trace.push(entry(0x2));
        trace.set_capacity(16);
        trace.push(entry(0x3));
        let pcs: Vec<u32> = trace.iter().map(|e| e.pc).collect();
        assert_eq!(pcs, vec![0x1, 0x2, 0x3]);
        assert_eq!(trace.capacity(), 16);
    }

    #[test]
    fn set_enabled_does_not_clear_existing_entries() {
        let mut trace = CpuTrace::with_capacity(4);
        trace.set_enabled(true);
        trace.push(entry(0x10));
        trace.set_enabled(false);
        // Pushing while disabled is a no-op…
        trace.push(entry(0x20));
        assert_eq!(trace.len(), 1);
        // …but existing entries remain.
        assert_eq!(trace.last().unwrap().pc, 0x10);
    }

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
}
