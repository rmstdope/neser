//! Bounded execution-trace ring buffer shared by per-console debuggers.
//!
//! [`TraceRing`] is a generic, console-agnostic ring buffer over an entry type
//! `E`. It bounds itself to a configurable capacity; when full, pushing a new
//! entry evicts the oldest. Capture can be toggled on and off, and pushing
//! while disabled is a no-op.
//!
//! Each console defines its own entry type (for example the GBA records a
//! `TraceEntry` with the ARM7TDMI register file) and aliases
//! `TraceRing<ConsoleEntry>` for its debugger, so the ring-buffer mechanics are
//! implemented and tested once here.

use std::collections::VecDeque;

/// Default ring-buffer capacity (number of entries).
pub const DEFAULT_TRACE_CAPACITY: usize = 1024;

/// Bounded ring buffer of recently executed instructions (or any entry type).
#[derive(Debug, Clone)]
pub struct TraceRing<E> {
    entries: VecDeque<E>,
    capacity: usize,
    enabled: bool,
}

impl<E> Default for TraceRing<E> {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_TRACE_CAPACITY)
    }
}

impl<E> TraceRing<E> {
    /// Create a new trace buffer with the given capacity.
    ///
    /// A capacity of zero is clamped to 1 so that the ring-buffer bounding
    /// remains meaningful — otherwise `len() == capacity` would never hold and
    /// the buffer would grow without bound.
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
    pub fn push(&mut self, entry: E) {
        if !self.enabled {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Iterate over retained entries from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &E> {
        self.entries.iter()
    }

    /// Most recently retired entry, if any.
    pub fn last(&self) -> Option<&E> {
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

    #[test]
    fn default_capacity_is_used_when_constructed_via_default() {
        let trace: TraceRing<u32> = TraceRing::default();
        assert_eq!(trace.capacity(), DEFAULT_TRACE_CAPACITY);
        assert!(trace.is_empty());
    }

    #[test]
    fn capacity_zero_is_clamped_to_one() {
        let trace: TraceRing<u32> = TraceRing::with_capacity(0);
        assert_eq!(trace.capacity(), 1);
    }

    #[test]
    fn push_is_noop_when_disabled() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(4);
        trace.push(0x1000);
        assert!(trace.is_empty());
    }

    #[test]
    fn push_records_entries_when_enabled() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(4);
        trace.set_enabled(true);
        trace.push(0x1000);
        trace.push(0x1004);
        assert_eq!(trace.len(), 2);
        assert_eq!(*trace.last().unwrap(), 0x1004);
    }

    #[test]
    fn ring_buffer_evicts_oldest_when_full() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(3);
        trace.set_enabled(true);
        for pc in [1u32, 2, 3, 4, 5] {
            trace.push(pc);
        }
        let pcs: Vec<u32> = trace.iter().copied().collect();
        assert_eq!(pcs, vec![3, 4, 5]);
    }

    #[test]
    fn iter_yields_oldest_to_newest() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(4);
        trace.set_enabled(true);
        trace.push(0x10);
        trace.push(0x20);
        trace.push(0x30);
        let pcs: Vec<u32> = trace.iter().copied().collect();
        assert_eq!(pcs, vec![0x10, 0x20, 0x30]);
    }

    #[test]
    fn clear_drops_entries_but_preserves_capacity() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(8);
        trace.set_enabled(true);
        trace.push(0x1);
        trace.push(0x2);
        trace.clear();
        assert!(trace.is_empty());
        assert_eq!(trace.capacity(), 8);
    }

    #[test]
    fn shrinking_capacity_drops_oldest_entries() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(8);
        trace.set_enabled(true);
        for pc in 0u32..8 {
            trace.push(pc);
        }
        trace.set_capacity(3);
        let pcs: Vec<u32> = trace.iter().copied().collect();
        assert_eq!(pcs, vec![5, 6, 7]);
    }

    #[test]
    fn growing_capacity_does_not_disturb_entries() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(2);
        trace.set_enabled(true);
        trace.push(0x1);
        trace.push(0x2);
        trace.set_capacity(16);
        trace.push(0x3);
        let pcs: Vec<u32> = trace.iter().copied().collect();
        assert_eq!(pcs, vec![0x1, 0x2, 0x3]);
        assert_eq!(trace.capacity(), 16);
    }

    #[test]
    fn set_enabled_does_not_clear_existing_entries() {
        let mut trace: TraceRing<u32> = TraceRing::with_capacity(4);
        trace.set_enabled(true);
        trace.push(0x10);
        trace.set_enabled(false);
        // Pushing while disabled is a no-op…
        trace.push(0x20);
        assert_eq!(trace.len(), 1);
        // …but existing entries remain.
        assert_eq!(*trace.last().unwrap(), 0x10);
    }
}
