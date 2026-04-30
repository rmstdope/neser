//! ARM7TDMI breakpoint management.
//!
//! Provides a simple address-based breakpoint set that the GBA debugger can
//! query after every executed instruction to decide whether to halt or invoke
//! a callback. The implementation is intentionally lightweight — it stores a
//! sorted set of 32-bit ARM addresses and offers O(log n) `contains` lookups.
//!
//! Conditional breakpoints are explicitly out of scope for this issue (see
//! issue #2218); only address breakpoints are supported here.

use std::collections::BTreeSet;

/// Set of ARM7TDMI program-counter values that, when reached, should halt or
/// notify the debugger.
#[derive(Debug, Default, Clone)]
pub struct Breakpoints {
    /// Sorted set of breakpoint addresses.
    addrs: BTreeSet<u32>,
}

impl Breakpoints {
    /// Create an empty breakpoint set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a breakpoint at `addr`. Returns `true` if the address was newly
    /// inserted, `false` if it was already present.
    pub fn insert(&mut self, addr: u32) -> bool {
        self.addrs.insert(addr)
    }

    /// Remove the breakpoint at `addr`. Returns `true` if it was present.
    pub fn remove(&mut self, addr: u32) -> bool {
        self.addrs.remove(&addr)
    }

    /// Check whether a breakpoint is set at `addr`.
    pub fn contains(&self, addr: u32) -> bool {
        self.addrs.contains(&addr)
    }

    /// Number of active breakpoints.
    pub fn len(&self) -> usize {
        self.addrs.len()
    }

    /// `true` if no breakpoints are set.
    pub fn is_empty(&self) -> bool {
        self.addrs.is_empty()
    }

    /// Remove every breakpoint.
    pub fn clear(&mut self) {
        self.addrs.clear();
    }

    /// Iterator over breakpoint addresses in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.addrs.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_breakpoint_set_is_empty() {
        let bp = Breakpoints::new();
        assert!(bp.is_empty());
        assert_eq!(bp.len(), 0);
    }

    #[test]
    fn insert_returns_true_for_new_address_and_false_for_duplicate() {
        let mut bp = Breakpoints::new();
        assert!(bp.insert(0x0800_0000));
        assert!(!bp.insert(0x0800_0000));
        assert_eq!(bp.len(), 1);
    }

    #[test]
    fn contains_reports_inserted_addresses() {
        let mut bp = Breakpoints::new();
        bp.insert(0x0800_0000);
        bp.insert(0x0800_0004);
        assert!(bp.contains(0x0800_0000));
        assert!(bp.contains(0x0800_0004));
        assert!(!bp.contains(0x0800_0008));
    }

    #[test]
    fn remove_returns_whether_address_was_present() {
        let mut bp = Breakpoints::new();
        bp.insert(0x0300_1234);
        assert!(bp.remove(0x0300_1234));
        assert!(!bp.remove(0x0300_1234));
        assert!(!bp.contains(0x0300_1234));
    }

    #[test]
    fn clear_removes_all_breakpoints() {
        let mut bp = Breakpoints::new();
        bp.insert(0x1);
        bp.insert(0x2);
        bp.insert(0x3);
        bp.clear();
        assert!(bp.is_empty());
    }

    #[test]
    fn iter_yields_addresses_sorted() {
        let mut bp = Breakpoints::new();
        bp.insert(0x30);
        bp.insert(0x10);
        bp.insert(0x20);
        let collected: Vec<u32> = bp.iter().collect();
        assert_eq!(collected, vec![0x10, 0x20, 0x30]);
    }

    #[test]
    fn supports_arbitrary_32bit_addresses() {
        let mut bp = Breakpoints::new();
        // GBA address space is 32-bit; ensure full range works.
        bp.insert(0x0000_0000);
        bp.insert(0xFFFF_FFFC);
        assert!(bp.contains(0x0000_0000));
        assert!(bp.contains(0xFFFF_FFFC));
    }
}
