//! Generic snapshot and view state types for debuggers.
//!
//! This module provides the shared types used by both GB and NES debuggers
//! for capturing emulator state and managing debugger UI state.

use super::disasm::{DisasmLine, DisasmWindowState};

/// Memory watch entry with current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryWatchEntry {
    /// Address being watched.
    pub address: u16,
    /// Current value at the address.
    pub value: u8,
}

impl MemoryWatchEntry {
    /// Create a new memory watch entry.
    pub fn new(address: u16, value: u8) -> Self {
        Self { address, value }
    }
}

/// Single CPU trace entry (executed instruction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTraceLine {
    /// Address of the instruction.
    pub addr: u16,
    /// Raw instruction bytes.
    pub bytes: Vec<u8>,
    /// Formatted instruction text.
    pub text: String,
}

impl CpuTraceLine {
    /// Create a new trace line.
    pub fn new(addr: u16, bytes: Vec<u8>, text: String) -> Self {
        Self { addr, bytes, text }
    }
}

/// Hexdump region state for a single memory region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexdumpRegionState {
    /// Base address for hexdump display.
    pub base: u16,
    /// Hexdump bytes (typically 256 bytes = 16 rows × 16 bytes).
    pub bytes: Vec<u8>,
}

impl Default for HexdumpRegionState {
    fn default() -> Self {
        Self {
            base: 0,
            bytes: vec![0; 256],
        }
    }
}

/// Generic debugger view state.
///
/// Tracks viewport positions and user preferences that persist across frames.
/// Used by both GB and NES debuggers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebuggerViewState {
    /// Disassembly window state.
    pub disasm_state: DisasmWindowState,
    /// Whether PPU viewer is visible.
    pub show_ppu_viewer: bool,
    /// Memory watch addresses.
    pub watch_addresses: Vec<u16>,
    /// Primary hexdump region base address override.
    pub primary_hexdump_base: Option<u16>,
    /// Secondary hexdump region base address override (for platforms with 2 regions).
    pub secondary_hexdump_base: Option<u16>,
}

impl DebuggerViewState {
    /// Create a new view state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle PPU viewer window visibility.
    pub fn toggle_ppu_viewer(&mut self) {
        self.show_ppu_viewer = !self.show_ppu_viewer;
    }

    /// Check if PPU viewer is visible.
    pub fn is_ppu_viewer_visible(&self) -> bool {
        self.show_ppu_viewer
    }

    // ── Watch address management ───────────────────────────────────────

    /// Get current memory watch addresses.
    pub fn watch_addresses(&self) -> Vec<u16> {
        self.watch_addresses.clone()
    }

    /// Clear all memory watch addresses.
    pub fn clear_watch_addresses(&mut self) {
        self.watch_addresses.clear();
    }

    /// Add a new memory watch address.
    pub fn add_watch_address(&mut self, address: u16) {
        if !self.watch_addresses.contains(&address) {
            self.watch_addresses.push(address);
        }
    }

    /// Remove a memory watch address by index.
    pub fn remove_watch_address(&mut self, index: usize) {
        if index < self.watch_addresses.len() {
            self.watch_addresses.remove(index);
        }
    }

    /// Update a memory watch address at index.
    pub fn update_watch_address(&mut self, index: usize, address: u16) {
        if index < self.watch_addresses.len() {
            self.watch_addresses[index] = address;
            // Enforce uniqueness by removing any other occurrences of `address`.
            let mut current_index = 0usize;
            self.watch_addresses.retain(|&entry| {
                let keep = current_index == index || entry != address;
                current_index += 1;
                keep
            });
        }
    }

    // ── Hexdump base management ────────────────────────────────────────

    /// Set the primary hexdump base address.
    pub fn set_primary_hexdump_base(&mut self, base: u16) {
        self.primary_hexdump_base = Some(base);
    }

    /// Adjust primary hexdump base by byte offset.
    pub fn nudge_primary_hexdump_base(&mut self, visible_base: u16, delta: i16) {
        let current = self.primary_hexdump_base.unwrap_or(visible_base);
        let nudged = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16)
        };
        self.primary_hexdump_base = Some(nudged);
    }

    /// Set the secondary hexdump base address.
    pub fn set_secondary_hexdump_base(&mut self, base: u16) {
        self.secondary_hexdump_base = Some(base);
    }

    /// Adjust secondary hexdump base by byte offset.
    pub fn nudge_secondary_hexdump_base(&mut self, visible_base: u16, delta: i16) {
        let current = self.secondary_hexdump_base.unwrap_or(visible_base);
        let nudged = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16)
        };
        self.secondary_hexdump_base = Some(nudged);
    }

    /// Get the primary hexdump base address.
    pub fn primary_hexdump_base(&self) -> Option<u16> {
        self.primary_hexdump_base
    }

    /// Get the secondary hexdump base address.
    pub fn secondary_hexdump_base(&self) -> Option<u16> {
        self.secondary_hexdump_base
    }

    /// Get mutable reference to disassembly state.
    pub fn disasm_state_mut(&mut self) -> &mut DisasmWindowState {
        &mut self.disasm_state
    }
}

/// Generic debugger snapshot.
///
/// Contains all information needed to render the debugger UI.
/// Platform-specific types (CPU registers, hexdump regions) are provided
/// by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebuggerSnapshot<CpuRegs> {
    /// CPU register state.
    pub cpu_regs: CpuRegs,

    /// Disassembly window lines.
    pub cpu_disasm: Vec<DisasmLine>,

    /// Formatted CPU state string for display.
    pub cpu: String,

    /// Memory watch entries with live values.
    pub watch_values: Vec<MemoryWatchEntry>,

    /// Recent executed instructions (ring buffer tail, typically last 32).
    pub recent_trace: Vec<CpuTraceLine>,
}

impl<CpuRegs: Default> Default for DebuggerSnapshot<CpuRegs> {
    fn default() -> Self {
        Self {
            cpu_regs: CpuRegs::default(),
            cpu_disasm: Vec::new(),
            cpu: String::new(),
            watch_values: Vec::new(),
            recent_trace: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_state_watch_addresses() {
        let mut state = DebuggerViewState::new();

        // Add addresses
        state.add_watch_address(0x0010);
        state.add_watch_address(0x0020);
        state.add_watch_address(0x0010); // Duplicate - should be ignored
        assert_eq!(state.watch_addresses(), vec![0x0010, 0x0020]);

        // Update address
        state.update_watch_address(1, 0x0030);
        assert_eq!(state.watch_addresses(), vec![0x0010, 0x0030]);

        // Remove address
        state.remove_watch_address(0);
        assert_eq!(state.watch_addresses(), vec![0x0030]);

        // Clear all
        state.clear_watch_addresses();
        assert!(state.watch_addresses().is_empty());
    }

    #[test]
    fn test_view_state_ppu_viewer_toggle() {
        let mut state = DebuggerViewState::new();
        assert!(!state.is_ppu_viewer_visible());

        state.toggle_ppu_viewer();
        assert!(state.is_ppu_viewer_visible());

        state.toggle_ppu_viewer();
        assert!(!state.is_ppu_viewer_visible());
    }

    #[test]
    fn test_view_state_hexdump_base() {
        let mut state = DebuggerViewState::new();

        // Primary region
        state.set_primary_hexdump_base(0xC000);
        assert_eq!(state.primary_hexdump_base(), Some(0xC000));

        state.nudge_primary_hexdump_base(0xC000, 16);
        assert_eq!(state.primary_hexdump_base(), Some(0xC010));

        state.nudge_primary_hexdump_base(0xC000, -16);
        assert_eq!(state.primary_hexdump_base(), Some(0xC000));

        // Secondary region
        state.set_secondary_hexdump_base(0x8000);
        assert_eq!(state.secondary_hexdump_base(), Some(0x8000));
    }

    #[test]
    fn test_update_watch_address_removes_duplicates() {
        let mut state = DebuggerViewState::new();
        state.add_watch_address(0x0010);
        state.add_watch_address(0x0020);
        state.add_watch_address(0x0030);

        // Update index 0 to the same value as index 2
        state.update_watch_address(0, 0x0030);

        // Should remove the duplicate at index 2
        assert_eq!(state.watch_addresses(), vec![0x0030, 0x0020]);
    }
}
