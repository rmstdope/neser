//! Breakpoint engine for the NES debugger.
//!
//! Supports four condition types: PC match, CPU cycle count, CPU frame count, and CPU write-address.

use std::fmt;

/// The condition that triggers a breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointKind {
    /// Break when the program counter reaches the given address.
    Pc(u16),
    /// Break when the CPU cycle count equals the given value.
    Cycle(u64),
    /// Break when execution reaches the first instruction boundary on the given frame.
    Frame(u64),
    /// Break when the CPU writes to the given address (non-dummy write).
    WriteAddress(u16),
}

impl fmt::Display for BreakpointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BreakpointKind::Pc(addr) => write!(f, "PC={:04X}", addr),
            BreakpointKind::Cycle(n) => write!(f, "CYC={}", n),
            BreakpointKind::Frame(n) => write!(f, "FRM={}", n),
            BreakpointKind::WriteAddress(addr) => write!(f, "WR={:04X}", addr),
        }
    }
}

/// A single breakpoint with a condition and an enabled state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub kind: BreakpointKind,
    pub enabled: bool,
}

impl Breakpoint {
    pub fn new(kind: BreakpointKind) -> Self {
        Self {
            kind,
            enabled: true,
        }
    }

    /// Returns true if this breakpoint is enabled and its condition matches `ctx`.
    pub fn is_hit(&self, ctx: &EvalContext) -> bool {
        if !self.enabled {
            return false;
        }
        match self.kind {
            BreakpointKind::Pc(addr) => ctx.pc == addr,
            BreakpointKind::Cycle(target) => {
                ctx.prev_cpu_cycles < target && ctx.cpu_cycles >= target
            }
            BreakpointKind::Frame(target) => ctx.prev_frame < target && ctx.frame >= target,
            BreakpointKind::WriteAddress(addr) => ctx.write_addr == Some(addr),
        }
    }

    /// Serialize to a plain-text line suitable for a `.debug` file.
    pub fn serialize(&self) -> String {
        let state = if self.enabled { "enabled" } else { "disabled" };
        match self.kind {
            BreakpointKind::Pc(addr) => format!("pc {:#06X} {}", addr, state),
            BreakpointKind::Cycle(n) => format!("cycle {} {}", n, state),
            BreakpointKind::Frame(n) => format!("frame {} {}", n, state),
            BreakpointKind::WriteAddress(addr) => format!("write {:#06X} {}", addr, state),
        }
    }

    /// Parse a plain-text line from a `.debug` file. Returns `None` for comments/blank lines.
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return None;
        }
        let enabled = parts[2] != "disabled";
        let kind = match parts[0] {
            "pc" => {
                let addr = parse_u16(parts[1])?;
                BreakpointKind::Pc(addr)
            }
            "cycle" => {
                let n: u64 = parts[1].parse().ok()?;
                BreakpointKind::Cycle(n)
            }
            "frame" => {
                let n: u64 = parts[1].parse().ok()?;
                BreakpointKind::Frame(n)
            }
            "write" => {
                let addr = parse_u16(parts[1])?;
                BreakpointKind::WriteAddress(addr)
            }
            _ => return None,
        };
        Some(Self { kind, enabled })
    }
}

/// Parses a `u16` from either a hex string (`0x`/`0X` prefix) or a decimal string.
fn parse_u16(s: &str) -> Option<u16> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// Execution context passed to breakpoint evaluation after each CPU instruction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EvalContext {
    /// Current program counter value.
    pub pc: u16,
    /// Total CPU cycles before this instruction executed.
    pub prev_cpu_cycles: u64,
    /// Total CPU cycles after this instruction executed.
    pub cpu_cycles: u64,
    /// PPU frame counter value before this instruction executed.
    pub prev_frame: u64,
    /// PPU frame counter value after this instruction executed.
    pub frame: u64,
    /// Address written during this instruction (non-dummy write), if any.
    pub write_addr: Option<u16>,
}

/// An ordered list of persistent breakpoints.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BreakpointList {
    items: Vec<Breakpoint>,
}

impl BreakpointList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of breakpoints.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if there are no breakpoints.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Add a breakpoint. Deduplicates by kind — if the kind already exists, the add is a no-op.
    pub fn add(&mut self, kind: BreakpointKind) {
        if !self.items.iter().any(|b| b.kind == kind) {
            self.items.push(Breakpoint::new(kind));
        }
    }

    /// Remove the breakpoint at the given index. No-op if out of bounds.
    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
        }
    }

    /// Enable the breakpoint at the given index. No-op if out of bounds.
    pub fn enable(&mut self, index: usize) {
        self.set_enabled(index, true);
    }

    /// Disable the breakpoint at the given index. No-op if out of bounds.
    pub fn disable(&mut self, index: usize) {
        self.set_enabled(index, false);
    }

    fn set_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(bp) = self.items.get_mut(index) {
            bp.enabled = enabled;
        }
    }

    /// Returns an iterator over all breakpoints.
    pub fn iter(&self) -> std::slice::Iter<'_, Breakpoint> {
        self.items.iter()
    }

    /// Returns `true` if there is any PC breakpoint at `addr`, regardless of enabled state.
    /// Useful for checking pre-existence before conditionally adding a temporary breakpoint.
    pub fn has_pc_breakpoint_at(&self, addr: u16) -> bool {
        self.items
            .iter()
            .any(|b| b.kind == BreakpointKind::Pc(addr))
    }

    /// Returns `true` if there is an enabled PC breakpoint at `addr`.
    pub fn has_enabled_pc_breakpoint_at(&self, addr: u16) -> bool {
        self.items
            .iter()
            .any(|b| b.kind == BreakpointKind::Pc(addr) && b.enabled)
    }

    /// Serialize all breakpoints to a multi-line string for `.debug` file storage.
    pub fn save_to_string(&self) -> String {
        self.items
            .iter()
            .map(|bp| bp.serialize())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parse breakpoints from a `.debug` file string, skipping blank/comment lines.
    pub fn load_from_str(text: &str) -> Self {
        let items = text.lines().filter_map(Breakpoint::parse).collect();
        Self { items }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- BreakpointKind display ---

    #[test]
    fn test_breakpoint_kind_pc_displays_as_hex() {
        assert_eq!(format!("{}", BreakpointKind::Pc(0xC000)), "PC=C000");
    }

    #[test]
    fn test_breakpoint_kind_cycle_displays_with_count() {
        assert_eq!(format!("{}", BreakpointKind::Cycle(12345)), "CYC=12345");
    }

    #[test]
    fn test_breakpoint_kind_frame_displays_with_count() {
        assert_eq!(format!("{}", BreakpointKind::Frame(42)), "FRM=42");
    }

    #[test]
    fn test_breakpoint_kind_write_address_displays_as_hex() {
        assert_eq!(
            format!("{}", BreakpointKind::WriteAddress(0x2006)),
            "WR=2006"
        );
    }

    // --- PC breakpoint evaluation ---

    #[test]
    fn test_pc_breakpoint_hits_when_pc_matches() {
        let bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        let ctx = EvalContext {
            pc: 0xC000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(bp.is_hit(&ctx));
    }

    #[test]
    fn test_pc_breakpoint_does_not_hit_when_pc_differs() {
        let bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        let ctx = EvalContext {
            pc: 0xC001,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    #[test]
    fn test_pc_breakpoint_does_not_hit_when_disabled() {
        let mut bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        bp.enabled = false;
        let ctx = EvalContext {
            pc: 0xC000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    // --- Cycle breakpoint evaluation ---

    #[test]
    fn test_cycle_breakpoint_hits_when_cycle_crosses_threshold() {
        let bp = Breakpoint::new(BreakpointKind::Cycle(1000));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 998,
            cpu_cycles: 1002,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(bp.is_hit(&ctx));
    }

    #[test]
    fn test_cycle_breakpoint_hits_when_cycle_matches_exactly() {
        let bp = Breakpoint::new(BreakpointKind::Cycle(1000));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 999,
            cpu_cycles: 1000,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(bp.is_hit(&ctx));
    }

    #[test]
    fn test_cycle_breakpoint_does_not_hit_before_target_cycle() {
        let bp = Breakpoint::new(BreakpointKind::Cycle(1000));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 999,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    #[test]
    fn test_cycle_breakpoint_does_not_fire_again_after_threshold_crossed() {
        let bp = Breakpoint::new(BreakpointKind::Cycle(1000));
        // Both prev and current are past the target — threshold already crossed, should not re-fire.
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 1001,
            cpu_cycles: 1003,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    #[test]
    fn test_cycle_breakpoint_does_not_hit_when_disabled() {
        let mut bp = Breakpoint::new(BreakpointKind::Cycle(1000));
        bp.enabled = false;
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 999,
            cpu_cycles: 1002,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    // --- Frame breakpoint evaluation ---

    #[test]
    fn test_frame_breakpoint_hits_when_frame_crosses_threshold() {
        let bp = Breakpoint::new(BreakpointKind::Frame(5));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 4,
            frame: 5,
        };
        assert!(bp.is_hit(&ctx));
    }

    #[test]
    fn test_frame_breakpoint_does_not_hit_before_target_frame() {
        let bp = Breakpoint::new(BreakpointKind::Frame(5));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 4,
            frame: 4,
        };
        assert!(!bp.is_hit(&ctx));
    }

    // --- WriteAddress breakpoint evaluation ---

    #[test]
    fn test_write_address_breakpoint_hits_when_write_matches() {
        let bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: Some(0x2006),
            prev_frame: 0,
            frame: 0,
        };
        assert!(bp.is_hit(&ctx));
    }

    #[test]
    fn test_write_address_breakpoint_does_not_hit_on_read() {
        let bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    #[test]
    fn test_write_address_breakpoint_does_not_hit_on_different_address_write() {
        let bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: Some(0x2007),
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    #[test]
    fn test_write_address_breakpoint_does_not_hit_when_disabled() {
        let mut bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        bp.enabled = false;
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: Some(0x2006),
            prev_frame: 0,
            frame: 0,
        };
        assert!(!bp.is_hit(&ctx));
    }

    // --- BreakpointList management ---

    #[test]
    fn test_breakpoint_list_starts_empty() {
        let list = BreakpointList::new();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_breakpoint_list_add_pc_breakpoint() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_add_does_not_duplicate_pc() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.add(BreakpointKind::Pc(0xC000));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_add_does_not_duplicate_cycle() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Cycle(1000));
        list.add(BreakpointKind::Cycle(1000));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_add_does_not_duplicate_frame() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Frame(60));
        list.add(BreakpointKind::Frame(60));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_add_does_not_duplicate_write_address() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::WriteAddress(0x2006));
        list.add(BreakpointKind::WriteAddress(0x2006));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_remove_by_index() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.add(BreakpointKind::Cycle(500));
        list.remove(0);
        assert_eq!(list.len(), 1);
        assert!(matches!(
            list.iter().next().map(|b| &b.kind),
            Some(BreakpointKind::Cycle(500))
        ));
    }

    #[test]
    fn test_breakpoint_list_remove_out_of_bounds_is_noop() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.remove(5); // no-op
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_breakpoint_list_enable_by_index() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.disable(0);
        list.enable(0);
        assert!(list.iter().next().map(|b| b.enabled).unwrap_or(false));
    }

    #[test]
    fn test_breakpoint_list_disable_by_index() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.disable(0);
        assert!(!list.iter().next().map(|b| b.enabled).unwrap_or(true));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_true_on_matching_pc() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        let ctx = EvalContext {
            pc: 0xC000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_true_on_matching_cycle() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Cycle(500));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 498,
            cpu_cycles: 501,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_true_on_matching_write_address() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::WriteAddress(0x2006));
        let ctx = EvalContext {
            pc: 0x0000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: Some(0x2006),
            prev_frame: 0,
            frame: 0,
        };
        assert!(list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_false_when_no_match() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        let ctx = EvalContext {
            pc: 0xD000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_false_when_disabled() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.disable(0);
        let ctx = EvalContext {
            pc: 0xC000,
            prev_cpu_cycles: 0,
            cpu_cycles: 0,
            write_addr: None,
            prev_frame: 0,
            frame: 0,
        };
        assert!(!list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    #[test]
    fn test_breakpoint_list_check_hit_returns_false_when_empty() {
        let list = BreakpointList::new();
        let ctx = EvalContext {
            pc: 0xC000,
            prev_cpu_cycles: 498,
            cpu_cycles: 501,
            write_addr: Some(0x2006),
            prev_frame: 0,
            frame: 0,
        };
        assert!(!list.iter().any(|bp| bp.is_hit(&ctx)));
    }

    // --- Serialization (plain text .debug format) ---

    #[test]
    fn test_breakpoint_kind_serialize_pc() {
        let bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        assert_eq!(bp.serialize(), "pc 0xC000 enabled");
    }

    #[test]
    fn test_breakpoint_kind_serialize_cycle() {
        let bp = Breakpoint::new(BreakpointKind::Cycle(12345));
        assert_eq!(bp.serialize(), "cycle 12345 enabled");
    }

    #[test]
    fn test_breakpoint_kind_serialize_frame() {
        let bp = Breakpoint::new(BreakpointKind::Frame(42));
        assert_eq!(bp.serialize(), "frame 42 enabled");
    }

    #[test]
    fn test_breakpoint_kind_serialize_write_address() {
        let bp = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        assert_eq!(bp.serialize(), "write 0x2006 enabled");
    }

    #[test]
    fn test_breakpoint_kind_serialize_disabled() {
        let mut bp = Breakpoint::new(BreakpointKind::Pc(0xC000));
        bp.enabled = false;
        assert_eq!(bp.serialize(), "pc 0xC000 disabled");
    }

    #[test]
    fn test_breakpoint_list_parse_pc_line() {
        let bp = Breakpoint::parse("pc 0xC000 enabled").unwrap();
        assert!(matches!(bp.kind, BreakpointKind::Pc(0xC000)));
        assert!(bp.enabled);
    }

    #[test]
    fn test_breakpoint_list_parse_cycle_line() {
        let bp = Breakpoint::parse("cycle 12345 disabled").unwrap();
        assert!(matches!(bp.kind, BreakpointKind::Cycle(12345)));
        assert!(!bp.enabled);
    }

    #[test]
    fn test_breakpoint_list_parse_frame_line() {
        let bp = Breakpoint::parse("frame 42 enabled").unwrap();
        assert!(matches!(bp.kind, BreakpointKind::Frame(42)));
        assert!(bp.enabled);
    }

    #[test]
    fn test_breakpoint_list_parse_write_line() {
        let bp = Breakpoint::parse("write 0x2006 enabled").unwrap();
        assert!(matches!(bp.kind, BreakpointKind::WriteAddress(0x2006)));
        assert!(bp.enabled);
    }

    #[test]
    fn test_breakpoint_list_parse_ignores_comment_lines() {
        assert!(Breakpoint::parse("# this is a comment").is_none());
    }

    #[test]
    fn test_breakpoint_list_parse_ignores_empty_lines() {
        assert!(Breakpoint::parse("").is_none());
    }

    #[test]
    fn test_breakpoint_list_roundtrip_serialize_parse() {
        let original = Breakpoint::new(BreakpointKind::WriteAddress(0x2006));
        let serialized = original.serialize();
        let parsed = Breakpoint::parse(&serialized).unwrap();
        assert_eq!(parsed.kind, original.kind);
        assert_eq!(parsed.enabled, original.enabled);
    }

    #[test]
    fn test_breakpoint_list_save_and_load_roundtrip() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0xC000));
        list.add(BreakpointKind::Cycle(500));
        list.add(BreakpointKind::WriteAddress(0x2006));
        list.disable(1);

        let text = list.save_to_string();
        let loaded = BreakpointList::load_from_str(&text);

        assert_eq!(loaded.len(), 3);
        assert!(matches!(
            loaded.iter().next().map(|b| &b.kind),
            Some(BreakpointKind::Pc(0xC000))
        ));
        assert!(loaded.iter().next().map(|b| b.enabled).unwrap_or(false));
        assert!(matches!(
            loaded.iter().nth(1).map(|b| &b.kind),
            Some(BreakpointKind::Cycle(500))
        ));
        assert!(!loaded.iter().nth(1).map(|b| b.enabled).unwrap_or(true));
        assert!(matches!(
            loaded.iter().nth(2).map(|b| &b.kind),
            Some(BreakpointKind::WriteAddress(0x2006))
        ));
    }
}
