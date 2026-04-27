use serde::{Deserialize, Serialize};

/// Action requested by a write to $FF55 (HDMA5 control register).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdmaAction {
    /// Start a general-purpose DMA (bit 7 = 0, no active HDMA).
    StartGdma,
    /// Start an HBlank DMA (bit 7 = 1).
    StartHdma,
    /// Cancel an active HBlank DMA (bit 7 = 0 while HDMA is active).
    CancelHdma,
}

/// CGB VRAM DMA (HDMA) state for registers $FF51–$FF55.
///
/// Supports two transfer modes:
/// - **GDMA** (General-Purpose DMA): transfers all blocks at once, CPU halted.
/// - **HDMA** (HBlank DMA): transfers one 16-byte block per HBlank (LY 0–143).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HdmaState {
    /// Source address (full 16-bit, lower 4 bits forced to 0).
    source: u16,
    /// Destination offset within VRAM ($0000–$1FF0, lower 4 bits forced to 0).
    destination: u16,
    /// Remaining block count (0-based: 0 = 1 block of 16 bytes, 0x7F = 128 blocks).
    remaining_blocks: u8,
    /// Whether a transfer is currently active.
    active: bool,
    /// `true` for HBlank DMA, `false` for GDMA.
    hblank_mode: bool,
}

impl Default for HdmaState {
    fn default() -> Self {
        Self::new()
    }
}

impl HdmaState {
    /// Create a new HDMA state with all registers cleared and no active transfer.
    pub fn new() -> Self {
        Self {
            source: 0,
            destination: 0,
            remaining_blocks: 0,
            active: false,
            hblank_mode: false,
        }
    }

    /// Write the high byte of the source address ($FF51 — HDMA1).
    pub fn write_source_high(&mut self, val: u8) {
        self.source = (self.source & 0x00F0) | (u16::from(val) << 8);
    }

    /// Write the low byte of the source address ($FF52 — HDMA2).
    /// Lower 4 bits are ignored (forced to 0).
    pub fn write_source_low(&mut self, val: u8) {
        self.source = (self.source & 0xFF00) | u16::from(val & 0xF0);
    }

    /// Write the high byte of the destination address ($FF53 — HDMA3).
    /// Only bits 4–0 of the high byte matter (destination is $8000–$9FF0).
    pub fn write_dest_high(&mut self, val: u8) {
        self.destination = (self.destination & 0x00F0) | (u16::from(val & 0x1F) << 8);
    }

    /// Write the low byte of the destination address ($FF54 — HDMA4).
    /// Lower 4 bits are ignored (forced to 0).
    pub fn write_dest_low(&mut self, val: u8) {
        self.destination = (self.destination & 0xFF00) | u16::from(val & 0xF0);
    }

    /// Write to the control register ($FF55 — HDMA5) and return the requested action.
    ///
    /// - Bit 7 = 0, no active HDMA → `StartGdma` (length = lower 7 bits)
    /// - Bit 7 = 1 → `StartHdma` (length = lower 7 bits)
    /// - Bit 7 = 0, active HDMA → `CancelHdma`
    pub fn write_control(&mut self, val: u8) -> HdmaAction {
        let length = val & 0x7F;
        let start_hblank = val & 0x80 != 0;

        if self.active && self.hblank_mode && !start_hblank {
            // Writing bit 7 = 0 while HDMA is active cancels the transfer.
            self.active = false;
            return HdmaAction::CancelHdma;
        }

        self.remaining_blocks = length;
        self.active = true;

        if start_hblank {
            self.hblank_mode = true;
            HdmaAction::StartHdma
        } else {
            self.hblank_mode = false;
            HdmaAction::StartGdma
        }
    }

    /// Read the control register ($FF55 — HDMA5).
    ///
    /// Returns remaining blocks in bits 0–6, and bit 7 = 1 when **inactive** (0 when active).
    /// Returns $FF when no transfer has ever been started or after GDMA completion.
    pub fn read_control(&self) -> u8 {
        if self.active {
            // Bit 7 = 0 (active), lower 7 bits = remaining blocks.
            self.remaining_blocks & 0x7F
        } else {
            // Bit 7 = 1 (not active). After GDMA completion or cancellation,
            // returns $FF (bit 7 set + lower 7 bits = $7F).
            0xFF
        }
    }

    /// Transfer one 16-byte block using the provided read/write closures.
    ///
    /// Reads 16 bytes from `source`, writes to `destination` (VRAM offset),
    /// advances both addresses by 16, and decrements `remaining_blocks`.
    /// Returns `true` if the transfer is now complete (no more blocks remaining).
    pub fn transfer_block(
        &mut self,
        read_fn: &mut dyn FnMut(u16) -> u8,
        write_fn: &mut dyn FnMut(u16, u8),
    ) -> bool {
        for i in 0u16..16 {
            let byte = read_fn(self.source.wrapping_add(i));
            write_fn(self.destination.wrapping_add(i), byte);
        }
        self.source = self.source.wrapping_add(16);
        self.destination = self.destination.wrapping_add(16);

        if self.remaining_blocks == 0 {
            self.active = false;
            true
        } else {
            self.remaining_blocks -= 1;
            false
        }
    }

    /// Returns `true` if a transfer (GDMA or HDMA) is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Advance source/destination addresses and decrement remaining blocks
    /// after one 16-byte block has been transferred externally.
    ///
    /// Used by `CgbBus` which handles the actual memory reads/writes directly
    /// (to avoid borrow-checker issues with closures over `&mut self`).
    /// Returns `true` if the transfer is now complete.
    pub fn advance_after_block(&mut self) -> bool {
        self.source = self.source.wrapping_add(16);
        self.destination = self.destination.wrapping_add(16);

        if self.remaining_blocks == 0 {
            self.active = false;
            true
        } else {
            self.remaining_blocks -= 1;
            false
        }
    }

    /// Returns `true` if the active transfer is HBlank DMA.
    pub fn is_hblank_mode(&self) -> bool {
        self.hblank_mode
    }

    /// Returns the current source address.
    pub fn source(&self) -> u16 {
        self.source
    }

    /// Returns the current VRAM destination offset.
    pub fn destination(&self) -> u16 {
        self.destination
    }

    /// Returns the number of remaining blocks (0-based).
    pub fn remaining_blocks(&self) -> u8 {
        self.remaining_blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Source register tests ────────────────────────────────────────────────

    #[test]
    fn test_write_source_high_sets_upper_byte() {
        // Given: fresh HdmaState
        let mut hdma = HdmaState::new();
        // When: write $C0 to HDMA1 (source high)
        hdma.write_source_high(0xC0);
        // Then: source address upper byte is $C0
        assert_eq!(hdma.source() & 0xFF00, 0xC000);
    }

    #[test]
    fn test_write_source_low_masks_lower_4_bits() {
        // Given: fresh HdmaState
        let mut hdma = HdmaState::new();
        // When: write $3F to HDMA2 (source low)
        hdma.write_source_low(0x3F);
        // Then: lower 4 bits are forced to 0, so source low byte is $30
        assert_eq!(hdma.source() & 0x00FF, 0x0030);
    }

    #[test]
    fn test_write_source_preserves_other_byte() {
        // Given: HdmaState with source high set to $C0
        let mut hdma = HdmaState::new();
        hdma.write_source_high(0xC0);
        // When: write $50 to source low
        hdma.write_source_low(0x50);
        // Then: full source is $C050
        assert_eq!(hdma.source(), 0xC050);
    }

    // ── Destination register tests ──────────────────────────────────────────

    #[test]
    fn test_write_dest_high_masks_upper_3_bits() {
        // Given: fresh HdmaState
        let mut hdma = HdmaState::new();
        // When: write $9F to HDMA3 (dest high)
        hdma.write_dest_high(0x9F);
        // Then: upper 3 bits ignored, only bits 4–0 preserved → $1F
        assert_eq!(hdma.destination() >> 8, 0x1F);
    }

    #[test]
    fn test_write_dest_low_masks_lower_4_bits() {
        // Given: fresh HdmaState
        let mut hdma = HdmaState::new();
        // When: write $AB to HDMA4 (dest low)
        hdma.write_dest_low(0xAB);
        // Then: lower 4 bits forced to 0 → $A0
        assert_eq!(hdma.destination() & 0x00FF, 0x00A0);
    }

    #[test]
    fn test_write_dest_preserves_other_byte() {
        // Given: HdmaState with dest high set to $80
        let mut hdma = HdmaState::new();
        hdma.write_dest_high(0x80);
        // When: write $F0 to dest low
        hdma.write_dest_low(0xF0);
        // Then: full destination is $00F0 (high bits masked: $80 & $1F = $00)
        assert_eq!(hdma.destination(), 0x00F0);
    }

    // ── Control register write tests ────────────────────────────────────────

    #[test]
    fn test_write_control_bit7_clear_starts_gdma() {
        // Given: no active transfer
        let mut hdma = HdmaState::new();
        // When: write $0F to HDMA5 (bit 7=0, length=0x0F → 16 blocks)
        let action = hdma.write_control(0x0F);
        // Then: GDMA started
        assert_eq!(action, HdmaAction::StartGdma);
        assert!(hdma.is_active());
        assert!(!hdma.is_hblank_mode());
        assert_eq!(hdma.remaining_blocks(), 0x0F);
    }

    #[test]
    fn test_write_control_bit7_set_starts_hdma() {
        // Given: no active transfer
        let mut hdma = HdmaState::new();
        // When: write $83 to HDMA5 (bit 7=1, length=$03 → 4 blocks)
        let action = hdma.write_control(0x83);
        // Then: HDMA started
        assert_eq!(action, HdmaAction::StartHdma);
        assert!(hdma.is_active());
        assert!(hdma.is_hblank_mode());
        assert_eq!(hdma.remaining_blocks(), 0x03);
    }

    #[test]
    fn test_write_control_bit7_clear_during_active_hdma_cancels() {
        // Given: active HDMA transfer
        let mut hdma = HdmaState::new();
        hdma.write_control(0x83); // Start HDMA
        // When: write $00 to HDMA5 (bit 7=0 while HDMA active)
        let action = hdma.write_control(0x00);
        // Then: HDMA cancelled
        assert_eq!(action, HdmaAction::CancelHdma);
        assert!(!hdma.is_active());
    }

    // ── Control register read tests ─────────────────────────────────────────

    #[test]
    fn test_read_control_when_inactive_returns_ff() {
        // Given: no transfer ever started
        let hdma = HdmaState::new();
        // Then: $FF (bit 7 set = inactive, lower bits all 1)
        assert_eq!(hdma.read_control(), 0xFF);
    }

    #[test]
    fn test_read_control_during_active_hdma_returns_remaining() {
        // Given: active HDMA with 4 blocks remaining (remaining_blocks = 3)
        let mut hdma = HdmaState::new();
        hdma.write_control(0x83); // Start HDMA, length=$03
        // Then: bit 7 = 0 (active), lower 7 bits = 3
        assert_eq!(hdma.read_control(), 0x03);
    }

    #[test]
    fn test_read_control_after_gdma_completion_returns_ff() {
        // Given: GDMA with 1 block (length=0)
        let mut hdma = HdmaState::new();
        hdma.write_source_high(0xC0);
        hdma.write_source_low(0x00);
        hdma.write_dest_high(0x80);
        hdma.write_dest_low(0x00);
        hdma.write_control(0x00); // GDMA, 1 block
        // When: transfer the single block
        let dummy_mem = [0u8; 0x10000];
        let complete =
            hdma.transfer_block(&mut |addr| dummy_mem[addr as usize], &mut |_addr, _val| {});
        // Then: transfer complete, read returns $FF
        assert!(complete);
        assert_eq!(hdma.read_control(), 0xFF);
    }

    // ── Transfer block tests ────────────────────────────────────────────────

    #[test]
    fn test_transfer_block_copies_16_bytes() {
        // Given: source at $C000, destination at $0000 (VRAM offset)
        let mut hdma = HdmaState::new();
        hdma.write_source_high(0xC0);
        hdma.write_source_low(0x00);
        hdma.write_dest_high(0x80);
        hdma.write_dest_low(0x00);
        hdma.write_control(0x00); // GDMA, 1 block

        let mut src_mem = [0u8; 0x10000];
        for i in 0..16u8 {
            src_mem[0xC000 + i as usize] = i + 1;
        }
        let mut dest_writes: Vec<(u16, u8)> = Vec::new();

        // When: transfer one block
        hdma.transfer_block(&mut |addr| src_mem[addr as usize], &mut |addr, val| {
            dest_writes.push((addr, val))
        });

        // Then: 16 bytes written to VRAM offsets $0000–$000F
        assert_eq!(dest_writes.len(), 16);
        for i in 0..16u16 {
            assert_eq!(dest_writes[i as usize], (i, (i + 1) as u8));
        }
    }

    #[test]
    fn test_transfer_block_advances_addresses() {
        // Given: source at $C000, dest at $0000, 2 blocks
        let mut hdma = HdmaState::new();
        hdma.write_source_high(0xC0);
        hdma.write_source_low(0x00);
        hdma.write_dest_high(0x80);
        hdma.write_dest_low(0x00);
        hdma.write_control(0x01); // GDMA, 2 blocks (remaining=1)

        let dummy_mem = [0u8; 0x10000];
        // When: transfer first block
        let complete =
            hdma.transfer_block(&mut |addr| dummy_mem[addr as usize], &mut |_addr, _val| {});
        // Then: not complete, addresses advanced by 16
        assert!(!complete);
        assert_eq!(hdma.source(), 0xC010);
        assert_eq!(hdma.destination(), 0x0010);
        assert_eq!(hdma.remaining_blocks(), 0x00);

        // When: transfer second block
        let complete =
            hdma.transfer_block(&mut |addr| dummy_mem[addr as usize], &mut |_addr, _val| {});
        // Then: complete
        assert!(complete);
    }

    #[test]
    fn test_transfer_block_returns_true_on_last_block() {
        // Given: GDMA with 1 block
        let mut hdma = HdmaState::new();
        hdma.write_source_high(0xC0);
        hdma.write_source_low(0x00);
        hdma.write_dest_high(0x80);
        hdma.write_dest_low(0x00);
        hdma.write_control(0x00); // 1 block (remaining=0)

        let dummy_mem = [0u8; 0x10000];
        let complete =
            hdma.transfer_block(&mut |addr| dummy_mem[addr as usize], &mut |_addr, _val| {});
        assert!(complete);
        assert!(!hdma.is_active());
    }
}
