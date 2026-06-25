//! Breakpoint engine for the NES and GB debuggers.
//!
//! Supports multiple condition types: PC match, CPU cycle count, CPU frame count,
//! CPU write-address, and GB-specific interrupt breakpoints.

use std::fmt;

/// Game Boy interrupt types for interrupt breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbInterruptKind {
    /// VBlank interrupt (bit 0 of IF/IE)
    VBlank,
    /// STAT (LCD) interrupt (bit 1 of IF/IE)
    Stat,
    /// Timer interrupt (bit 2 of IF/IE)
    Timer,
    /// Serial interrupt (bit 3 of IF/IE)
    Serial,
    /// Joypad interrupt (bit 4 of IF/IE)
    Joypad,
}

impl fmt::Display for GbInterruptKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GbInterruptKind::VBlank => write!(f, "VBlank"),
            GbInterruptKind::Stat => write!(f, "STAT"),
            GbInterruptKind::Timer => write!(f, "Timer"),
            GbInterruptKind::Serial => write!(f, "Serial"),
            GbInterruptKind::Joypad => write!(f, "Joypad"),
        }
    }
}

impl GbInterruptKind {
    /// Get the interrupt bit mask for this interrupt type.
    pub fn bit_mask(self) -> u8 {
        match self {
            GbInterruptKind::VBlank => 0x01,
            GbInterruptKind::Stat => 0x02,
            GbInterruptKind::Timer => 0x04,
            GbInterruptKind::Serial => 0x08,
            GbInterruptKind::Joypad => 0x10,
        }
    }

    /// Parse interrupt kind from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vblank" => Some(GbInterruptKind::VBlank),
            "stat" => Some(GbInterruptKind::Stat),
            "timer" => Some(GbInterruptKind::Timer),
            "serial" => Some(GbInterruptKind::Serial),
            "joypad" => Some(GbInterruptKind::Joypad),
            _ => None,
        }
    }
}

/// Address types usable as breakpoint targets.
///
/// Implemented for `u16` (NES / Game Boy 16-bit address space) and `u32`
/// (Game Boy Advance ARM7TDMI 32-bit address space). Provides the hex width
/// used for display/serialization and a hex-or-decimal parser, so the
/// breakpoint types can be shared across consoles of different address widths.
pub trait BreakpointAddr: Copy + Eq + fmt::UpperHex {
    /// Number of hex digits used when formatting an address of this width.
    const HEX_DIGITS: usize;
    /// Parse from a hex (`0x`/`0X` prefix) or decimal string.
    fn parse(s: &str) -> Option<Self>;
}

impl BreakpointAddr for u16 {
    const HEX_DIGITS: usize = 4;
    fn parse(s: &str) -> Option<Self> {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u16::from_str_radix(hex, 16).ok()
        } else {
            s.parse().ok()
        }
    }
}

impl BreakpointAddr for u32 {
    const HEX_DIGITS: usize = 8;
    fn parse(s: &str) -> Option<Self> {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            s.parse().ok()
        }
    }
}

/// The condition that triggers a breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointKind<A = u16> {
    /// Break when the program counter reaches the given address.
    Pc(A),
    /// Break when the CPU cycle count equals the given value.
    Cycle(u64),
    /// Break when execution reaches the first instruction boundary on the given frame.
    Frame(u64),
    /// Break when the CPU writes to the given address (non-dummy write).
    WriteAddress(A),
    /// Break when a specific GB interrupt is about to fire (GB only).
    GbInterrupt(GbInterruptKind),
}

impl<A: BreakpointAddr> fmt::Display for BreakpointKind<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BreakpointKind::Pc(addr) => write!(f, "PC={:0width$X}", addr, width = A::HEX_DIGITS),
            BreakpointKind::Cycle(n) => write!(f, "CYC={}", n),
            BreakpointKind::Frame(n) => write!(f, "FRM={}", n),
            BreakpointKind::WriteAddress(addr) => {
                write!(f, "WR={:0width$X}", addr, width = A::HEX_DIGITS)
            }
            BreakpointKind::GbInterrupt(kind) => write!(f, "INT={}", kind),
        }
    }
}

/// A single breakpoint with a condition and an enabled state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint<A = u16> {
    pub kind: BreakpointKind<A>,
    pub enabled: bool,
}

impl<A: BreakpointAddr> Breakpoint<A> {
    pub fn new(kind: BreakpointKind<A>) -> Self {
        Self {
            kind,
            enabled: true,
        }
    }

    /// Returns true if this breakpoint is enabled and its condition matches `ctx`.
    pub fn is_hit(&self, ctx: &EvalContext<A>) -> bool {
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
            BreakpointKind::GbInterrupt(kind) => {
                // Check if GB interrupt is about to fire
                if let (Some(ie), Some(if_reg), Some(ime)) = (ctx.gb_ie, ctx.gb_if, ctx.gb_ime) {
                    let mask = kind.bit_mask();
                    // Interrupt is pending and enabled
                    ime && (ie & if_reg & mask) != 0
                } else {
                    false
                }
            }
        }
    }

    /// Serialize to a plain-text line suitable for a `.debug` file.
    pub fn serialize(&self) -> String {
        let state = if self.enabled { "enabled" } else { "disabled" };
        match self.kind {
            BreakpointKind::Pc(addr) => {
                format!("pc 0x{:0width$X} {}", addr, state, width = A::HEX_DIGITS)
            }
            BreakpointKind::Cycle(n) => format!("cycle {} {}", n, state),
            BreakpointKind::Frame(n) => format!("frame {} {}", n, state),
            BreakpointKind::WriteAddress(addr) => {
                format!("write 0x{:0width$X} {}", addr, state, width = A::HEX_DIGITS)
            }
            BreakpointKind::GbInterrupt(kind) => {
                let name = match kind {
                    GbInterruptKind::VBlank => "vblank",
                    GbInterruptKind::Stat => "stat",
                    GbInterruptKind::Timer => "timer",
                    GbInterruptKind::Serial => "serial",
                    GbInterruptKind::Joypad => "joypad",
                };
                format!("gbinterrupt {} {}", name, state)
            }
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
                let addr = A::parse(parts[1])?;
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
                let addr = A::parse(parts[1])?;
                BreakpointKind::WriteAddress(addr)
            }
            "gbinterrupt" => {
                let int_kind = GbInterruptKind::parse(parts[1])?;
                BreakpointKind::GbInterrupt(int_kind)
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
pub struct EvalContext<A = u16> {
    /// Current program counter value.
    pub pc: A,
    /// Total CPU cycles before this instruction executed.
    pub prev_cpu_cycles: u64,
    /// Total CPU cycles after this instruction executed.
    pub cpu_cycles: u64,
    /// PPU frame counter value before this instruction executed.
    pub prev_frame: u64,
    /// PPU frame counter value after this instruction executed.
    pub frame: u64,
    /// Address written during this instruction (non-dummy write), if any.
    pub write_addr: Option<A>,
    /// GB interrupt enable register (IE at $FFFF) - for GB interrupt breakpoints.
    pub gb_ie: Option<u8>,
    /// GB interrupt flag register (IF at $FF0F) - for GB interrupt breakpoints.
    pub gb_if: Option<u8>,
    /// GB IME (Interrupt Master Enable) flag - for GB interrupt breakpoints.
    pub gb_ime: Option<bool>,
}

/// An ordered list of persistent breakpoints.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BreakpointList<A = u16> {
    items: Vec<Breakpoint<A>>,
}

impl<A: BreakpointAddr> BreakpointList<A> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
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
    pub fn add(&mut self, kind: BreakpointKind<A>) {
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

    /// Remove the first breakpoint that matches the given kind. No-op if not found.
    pub fn remove_first_matching(&mut self, kind: &BreakpointKind<A>) {
        if let Some(index) = self.items.iter().position(|b| &b.kind == kind) {
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
    pub fn iter(&self) -> std::slice::Iter<'_, Breakpoint<A>> {
        self.items.iter()
    }

    /// Returns `true` if there is any PC breakpoint at `addr`, regardless of enabled state.
    /// Useful for checking pre-existence before conditionally adding a temporary breakpoint.
    pub fn has_pc_breakpoint_at(&self, addr: A) -> bool {
        self.items
            .iter()
            .any(|b| b.kind == BreakpointKind::Pc(addr))
    }

    /// Returns `true` if there is an enabled PC breakpoint at `addr`.
    pub fn has_enabled_pc_breakpoint_at(&self, addr: A) -> bool {
        self.items
            .iter()
            .any(|b| b.kind == BreakpointKind::Pc(addr) && b.enabled)
    }

    /// Force-enables the PC breakpoint at `addr` and returns its previous enabled state.
    /// Returns `None` if no PC breakpoint exists at `addr`.
    pub fn force_enable_pc_breakpoint_at(&mut self, addr: A) -> Option<bool> {
        self.items
            .iter_mut()
            .find(|b| b.kind == BreakpointKind::Pc(addr))
            .map(|b| {
                let was_enabled = b.enabled;
                b.enabled = true;
                was_enabled
            })
    }

    /// Sets the enabled state of the PC breakpoint at `addr`.
    pub fn set_pc_breakpoint_enabled(&mut self, addr: A, enabled: bool) {
        if let Some(b) = self
            .items
            .iter_mut()
            .find(|b| b.kind == BreakpointKind::Pc(addr))
        {
            b.enabled = enabled;
        }
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

/// Serialize a slice of watch addresses to lines suitable for a `.debug` file.
///
/// Returns an empty string when `addresses` is empty.
pub fn serialize_watch_addresses(addresses: &[u16]) -> String {
    addresses
        .iter()
        .map(|a| format!("watch {:#06X}", a))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse watch addresses from the text of a `.debug` file.
///
/// Lines that look like `watch 0x????` are collected; all other lines are
/// silently ignored so this function can be called on the full file content.
pub fn parse_watch_addresses(text: &str) -> Vec<u16> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("watch ").and_then(parse_u16)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests target the 16-bit (NES / Game Boy) instantiation. The local
    // aliases shadow the generic types from `super::*` so the existing tests
    // read unchanged; dedicated 32-bit coverage lives in `tests_u32` below.
    type BreakpointKind = super::BreakpointKind<u16>;
    type Breakpoint = super::Breakpoint<u16>;
    type BreakpointList = super::BreakpointList<u16>;
    type EvalContext = super::EvalContext<u16>;

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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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
            gb_ie: None,
            gb_if: None,
            gb_ime: None,
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

    #[test]
    fn test_force_enable_pc_breakpoint_enables_disabled() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x8000));
        list.disable(0);
        assert!(!list.has_enabled_pc_breakpoint_at(0x8000));

        let was_enabled = list.force_enable_pc_breakpoint_at(0x8000);
        assert_eq!(was_enabled, Some(false));
        assert!(list.has_enabled_pc_breakpoint_at(0x8000));
    }

    #[test]
    fn test_force_enable_pc_breakpoint_returns_true_when_already_enabled() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x8000));

        let was_enabled = list.force_enable_pc_breakpoint_at(0x8000);
        assert_eq!(was_enabled, Some(true));
        assert!(list.has_enabled_pc_breakpoint_at(0x8000));
    }

    #[test]
    fn test_force_enable_pc_breakpoint_returns_none_when_missing() {
        let mut list = BreakpointList::new();
        let was_enabled = list.force_enable_pc_breakpoint_at(0x8000);
        assert_eq!(was_enabled, None);
    }

    #[test]
    fn test_set_pc_breakpoint_enabled_disables() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x8000));
        assert!(list.has_enabled_pc_breakpoint_at(0x8000));

        list.set_pc_breakpoint_enabled(0x8000, false);
        assert!(!list.has_enabled_pc_breakpoint_at(0x8000));
        assert!(list.has_pc_breakpoint_at(0x8000));
    }

    #[test]
    fn test_set_pc_breakpoint_enabled_re_enables() {
        let mut list = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x8000));
        list.disable(0);

        list.set_pc_breakpoint_enabled(0x8000, true);
        assert!(list.has_enabled_pc_breakpoint_at(0x8000));
    }

    // ── Watch address serialization (#1860) ───────────────────────────────────

    #[test]
    fn test_serialize_watch_addresses_empty() {
        assert_eq!(serialize_watch_addresses(&[]), "");
    }

    #[test]
    fn test_serialize_watch_addresses_single() {
        assert_eq!(serialize_watch_addresses(&[0x0300]), "watch 0x0300");
    }

    #[test]
    fn test_serialize_watch_addresses_multiple() {
        let s = serialize_watch_addresses(&[0x0300, 0x00FF]);
        assert_eq!(s, "watch 0x0300\nwatch 0x00FF");
    }

    #[test]
    fn test_parse_watch_addresses_empty_string() {
        assert!(parse_watch_addresses("").is_empty());
    }

    #[test]
    fn test_parse_watch_addresses_single_hex() {
        assert_eq!(parse_watch_addresses("watch 0x0300"), vec![0x0300u16]);
    }

    #[test]
    fn test_parse_watch_addresses_multiple_lines() {
        let text = "watch 0x0300\nwatch 0x00FF";
        assert_eq!(parse_watch_addresses(text), vec![0x0300u16, 0x00FF]);
    }

    #[test]
    fn test_parse_watch_addresses_ignores_breakpoint_lines() {
        let text = "pc 0x8000 enabled\nwatch 0x0300\ncycle 100 disabled";
        assert_eq!(parse_watch_addresses(text), vec![0x0300u16]);
    }

    #[test]
    fn test_watch_address_roundtrip() {
        let original = vec![0x0300u16, 0x00FF, 0x2006];
        let serialized = serialize_watch_addresses(&original);
        let parsed = parse_watch_addresses(&serialized);
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_breakpoint_list_load_from_str_ignores_watch_lines() {
        // Watch lines must be silently ignored when loading breakpoints so
        // files with both kinds parse correctly.
        let text = "pc 0x8000 enabled\nwatch 0x0300\nframe 5 disabled";
        let list = BreakpointList::load_from_str(text);
        assert_eq!(list.len(), 2, "only the two breakpoints should be loaded");
    }
}

#[cfg(test)]
mod tests_u32 {
    //! 32-bit (Game Boy Advance) coverage for the generic breakpoint types.
    use super::{Breakpoint, BreakpointKind, BreakpointList};

    #[test]
    fn pc_breakpoint_displays_as_8_hex_digits() {
        let kind: BreakpointKind<u32> = BreakpointKind::Pc(0x0800_0000);
        assert_eq!(format!("{kind}"), "PC=08000000");
    }

    #[test]
    fn write_breakpoint_displays_as_8_hex_digits() {
        let kind: BreakpointKind<u32> = BreakpointKind::WriteAddress(0x0300_1234);
        assert_eq!(format!("{kind}"), "WR=03001234");
    }

    #[test]
    fn pc_breakpoint_serialize_roundtrip_preserves_u32_address() {
        let bp = Breakpoint::<u32>::new(BreakpointKind::Pc(0xFFFF_FFFC));
        let line = bp.serialize();
        assert_eq!(line, "pc 0xFFFFFFFC enabled");
        let parsed = Breakpoint::<u32>::parse(&line).expect("parse");
        assert_eq!(parsed, bp);
    }

    #[test]
    fn parses_hex_and_decimal_u32_addresses() {
        let hex = Breakpoint::<u32>::parse("pc 0x08000000 enabled").expect("hex");
        assert_eq!(hex.kind, BreakpointKind::Pc(0x0800_0000));
        let dec = Breakpoint::<u32>::parse("pc 134217728 enabled").expect("dec");
        assert_eq!(dec.kind, BreakpointKind::Pc(0x0800_0000));
    }

    #[test]
    fn has_pc_breakpoint_at_matches_u32_addresses() {
        let mut list: BreakpointList<u32> = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x0800_0000));
        list.add(BreakpointKind::Pc(0x0800_0004));
        assert!(list.has_pc_breakpoint_at(0x0800_0000));
        assert!(list.has_pc_breakpoint_at(0x0800_0004));
        assert!(!list.has_pc_breakpoint_at(0x0800_0008));
    }

    #[test]
    fn add_deduplicates_by_kind() {
        let mut list: BreakpointList<u32> = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x0800_0000));
        list.add(BreakpointKind::Pc(0x0800_0000));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn remove_first_matching_removes_the_pc_breakpoint() {
        let mut list: BreakpointList<u32> = BreakpointList::new();
        list.add(BreakpointKind::Pc(0x0300_1234));
        assert!(list.has_pc_breakpoint_at(0x0300_1234));
        list.remove_first_matching(&BreakpointKind::Pc(0x0300_1234));
        assert!(!list.has_pc_breakpoint_at(0x0300_1234));
    }
}
