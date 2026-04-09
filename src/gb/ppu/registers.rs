/// DMG PPU I/O registers (writable fields only).
///
/// The read-only mode and coincidence bits of STAT are composed dynamically
/// by `Ppu::read_register`; this struct stores only the CPU-writable portions.
pub struct Registers {
    /// $FF40 — LCD Control
    pub lcdc: u8,
    /// $FF41 — STAT writable bits \[6:3\] (interrupt enables)
    pub stat_irq_enables: u8,
    /// $FF42 — Scroll Y
    pub scy: u8,
    /// $FF43 — Scroll X
    pub scx: u8,
    /// $FF45 — LY Compare
    pub lyc: u8,
    /// $FF47 — BG Palette
    pub bgp: u8,
    /// $FF48 — OBJ Palette 0
    pub obp0: u8,
    /// $FF49 — OBJ Palette 1
    pub obp1: u8,
    /// $FF4A — Window Y
    pub wy: u8,
    /// $FF4B — Window X (minus 7)
    pub wx: u8,
}

impl Registers {
    /// LCDC reset value after boot ROM: LCD on, BG tile data $8000, BG map $9800.
    pub const LCDC_RESET: u8 = 0x91;

    pub fn new() -> Self {
        Self {
            lcdc: Self::LCDC_RESET,
            stat_irq_enables: 0,
            scy: 0,
            scx: 0,
            lyc: 0,
            bgp: 0xFC, // default "white" palette after boot
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
        }
    }

    /// Write to a register address ($FF40–$FF4B).
    ///
    /// Returns `true` if the address was handled.
    pub fn write(&mut self, addr: u16, val: u8) -> bool {
        match addr {
            0xFF40 => self.lcdc = val,
            // Only bits 3–6 are CPU-writable; bits 0–2 are read-only mode/coincidence bits.
            0xFF41 => self.stat_irq_enables = val & 0x78,
            0xFF42 => self.scy = val,
            0xFF43 => self.scx = val,
            0xFF44 => {} // LY is read-only; writes are silently ignored
            0xFF45 => self.lyc = val,
            0xFF47 => self.bgp = val,
            0xFF48 => self.obp0 = val,
            0xFF49 => self.obp1 = val,
            0xFF4A => self.wy = val,
            0xFF4B => self.wx = val,
            _ => return false,
        }
        true
    }

    /// Read from a register address ($FF40–$FF4B).
    ///
    /// `ly`    — current scanline from timing (fills $FF44 read).
    /// `stat`  — full pre-composed STAT byte (mode + coincidence + irq bits).
    ///
    /// Returns `Some(value)` if handled, `None` otherwise.
    pub fn read(&self, addr: u16, ly: u8, stat: u8) -> Option<u8> {
        match addr {
            0xFF40 => Some(self.lcdc),
            // Compose STAT: CPU-writable irq-enable bits OR'd with live mode/coincidence bits.
            0xFF41 => Some((self.stat_irq_enables & 0x78) | (stat & 0x07)),
            0xFF42 => Some(self.scy),
            0xFF43 => Some(self.scx),
            0xFF44 => Some(ly),
            0xFF45 => Some(self.lyc),
            0xFF47 => Some(self.bgp),
            0xFF48 => Some(self.obp0),
            0xFF49 => Some(self.obp1),
            0xFF4A => Some(self.wy),
            0xFF4B => Some(self.wx),
            _ => None,
        }
    }

    /// Returns whether the LCD is enabled (LCDC bit 7).
    pub fn lcd_enabled(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    /// Returns whether window rendering is enabled (LCDC bit 5).
    pub fn window_enabled(&self) -> bool {
        self.lcdc & 0x20 != 0
    }

    /// Returns whether OBJ (sprite) rendering is enabled (LCDC bit 1).
    pub fn obj_enabled(&self) -> bool {
        self.lcdc & 0x02 != 0
    }

    /// Returns whether BG/Window display is enabled (LCDC bit 0).
    pub fn bg_window_enabled(&self) -> bool {
        self.lcdc & 0x01 != 0
    }

    /// Returns OBJ size: false = 8×8, true = 8×16 (LCDC bit 2).
    pub fn obj_size_16(&self) -> bool {
        self.lcdc & 0x04 != 0
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcdc_reset_value_is_0x91() {
        // Given/When: fresh registers
        let regs = Registers::new();
        // Then: LCDC reads as 0x91
        assert_eq!(regs.read(0xFF40, 0, 0), Some(0x91));
    }

    #[test]
    fn test_lcdc_write_and_read() {
        // Given: fresh registers
        let mut regs = Registers::new();
        // When: write 0x00 to LCDC (disable LCD)
        regs.write(0xFF40, 0x00);
        // Then: LCDC reads back 0x00
        assert_eq!(regs.read(0xFF40, 0, 0), Some(0x00));
    }

    #[test]
    fn test_stat_read_includes_live_mode_bits() {
        // Given: fresh registers; the PPU composes full STAT
        let regs = Registers::new();
        // When: read STAT with mode=2 (OAM scan), coincidence=0
        // stat byte = (irq_enables & 0x78) | (0 << 2) | 2
        let stat_composed = 0b0000_0010u8; // mode 2, no coincidence, no irq enables
        let result = regs.read(0xFF41, 0, stat_composed);
        // Then: lower 2 bits are the mode, bit 2 is coincidence
        assert_eq!(result, Some(stat_composed));
    }

    #[test]
    fn test_stat_write_only_stores_irq_enable_bits() {
        // Given: fresh registers
        let mut regs = Registers::new();
        // When: write full 0xFF to STAT
        regs.write(0xFF41, 0xFF);
        // Then: only bits 3–6 are stored (bits 0–2 are read-only; bit 7 always 1 on read)
        // Re-read with composed stat = 0 to verify only irq enables affect the value
        let result = regs.read(0xFF41, 0, 0).unwrap();
        // Bits 3–6 should be set (from write), bits 0–2 from composed (0)
        assert_eq!(result & 0x78, 0x78);
        assert_eq!(result & 0x07, 0x00);
    }

    #[test]
    fn test_ly_register_is_read_only_and_returns_live_ly() {
        // Given: fresh registers
        let regs = Registers::new();
        // When: read LY ($FF44) with ly=77
        let result = regs.read(0xFF44, 77, 0);
        // Then: returns 77 regardless of any write
        assert_eq!(result, Some(77));
    }

    #[test]
    fn test_ly_write_is_ignored() {
        // Given: fresh registers
        let mut regs = Registers::new();
        // When: write to LY ($FF44)
        regs.write(0xFF44, 0x42);
        // Then: next read still returns the live ly (0 in this test)
        assert_eq!(regs.read(0xFF44, 0, 0), Some(0));
    }

    #[test]
    fn test_lyc_write_and_read() {
        let mut regs = Registers::new();
        regs.write(0xFF45, 0x60);
        assert_eq!(regs.read(0xFF45, 0, 0), Some(0x60));
    }

    #[test]
    fn test_scy_scx_write_and_read() {
        let mut regs = Registers::new();
        regs.write(0xFF42, 0x10); // SCY
        regs.write(0xFF43, 0x20); // SCX
        assert_eq!(regs.read(0xFF42, 0, 0), Some(0x10));
        assert_eq!(regs.read(0xFF43, 0, 0), Some(0x20));
    }

    #[test]
    fn test_bgp_obp0_obp1_write_and_read() {
        let mut regs = Registers::new();
        regs.write(0xFF47, 0xE4); // BGP
        regs.write(0xFF48, 0xD0); // OBP0
        regs.write(0xFF49, 0xC0); // OBP1
        assert_eq!(regs.read(0xFF47, 0, 0), Some(0xE4));
        assert_eq!(regs.read(0xFF48, 0, 0), Some(0xD0));
        assert_eq!(regs.read(0xFF49, 0, 0), Some(0xC0));
    }

    #[test]
    fn test_lcd_enabled_reflects_lcdc_bit7() {
        let mut regs = Registers::new();
        assert!(regs.lcd_enabled()); // reset value 0x91 has bit 7 set
        regs.write(0xFF40, 0x11); // bit 7 clear
        assert!(!regs.lcd_enabled());
    }
}
