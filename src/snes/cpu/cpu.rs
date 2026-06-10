//! WDC 65C816 CPU core.

use crate::snes::bus::SnesBus;

// Status register P flags (8 bits)
// Bit 7: N (Negative)
// Bit 6: V (Overflow)
// Bit 5: M (Accumulator/Memory width: 1=8-bit, 0=16-bit)
// Bit 4: X (Index register width: 1=8-bit, 0=16-bit)
// Bit 3: D (Decimal mode)
// Bit 2: I (Interrupt disable)
// Bit 1: Z (Zero)
// Bit 0: C (Carry)
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
const FLAG_DECIMAL: u8 = 0b0000_1000;
const FLAG_INDEX_WIDTH: u8 = 0b0001_0000; // X flag
const FLAG_ACCUM_WIDTH: u8 = 0b0010_0000; // M flag
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

/// WDC 65C816 CPU
pub struct Cpu<B: SnesBus> {
    /// Accumulator (16-bit: B:A)
    /// When M=1 (8-bit mode), only low byte (A) is used; B is preserved
    a: u16,

    /// X index register (16-bit)
    /// When X=1 (8-bit mode), high byte is forced to 0
    x: u16,

    /// Y index register (16-bit)
    /// When X=1 (8-bit mode), high byte is forced to 0
    y: u16,

    /// Direct page register (16-bit)
    /// Relocates "zero page" to D:$00–D:$FF
    d: u16,

    /// Data bank register (8-bit)
    /// Default bank for data accesses
    dbr: u8,

    /// Program bank register (8-bit)
    /// Bank for current PC (24-bit address = PBR:PC)
    pbr: u8,

    /// Stack pointer (16-bit)
    /// In emulation mode, high byte forced to $01
    s: u16,

    /// Program counter (16-bit offset within PBR)
    pc: u16,

    /// Processor status register (8 bits: N V M X D I Z C)
    p: u8,

    /// Emulation flag (hidden, not in P)
    /// E=1: emulation mode (6502-compatible)
    /// E=0: native mode (full 65816)
    e: bool,

    /// Bus for memory access
    bus: B,
}

impl<B: SnesBus> Cpu<B> {
    /// Create a new 65816 CPU in reset state (emulation mode).
    pub fn new(bus: B) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            d: 0,
            dbr: 0,
            pbr: 0,
            s: 0x01FF, // Emulation mode starts with S at top of page 1
            pc: 0,
            p: FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH | FLAG_INTERRUPT, // M=1, X=1, I=1
            e: true,                                                 // Start in emulation mode
            bus,
        }
    }

    /// Read accumulator value (respects M flag width).
    /// Returns 16-bit value; in 8-bit mode (M=1), high byte is B (preserved).
    pub fn read_a(&self) -> u16 {
        self.a
    }

    /// Write accumulator value (respects M flag width).
    /// In 8-bit mode (M=1), only low byte is updated; B (high byte) preserved.
    pub fn write_a(&mut self, value: u16) {
        if self.m_flag() {
            // 8-bit mode: update low byte only, preserve B
            self.a = (self.a & 0xFF00) | (value & 0x00FF);
        } else {
            // 16-bit mode: update full 16 bits
            self.a = value;
        }
    }

    /// Read X register value (respects X flag width).
    pub fn read_x(&self) -> u16 {
        self.x
    }

    /// Write X register value (respects X flag width).
    /// In 8-bit mode (X=1), high byte forced to 0.
    pub fn write_x(&mut self, value: u16) {
        if self.x_flag() {
            // 8-bit mode: force high byte to 0
            self.x = value & 0x00FF;
        } else {
            // 16-bit mode: full 16 bits
            self.x = value;
        }
    }

    /// Read Y register value (respects X flag width).
    pub fn read_y(&self) -> u16 {
        self.y
    }

    /// Write Y register value (respects X flag width).
    /// In 8-bit mode (X=1), high byte forced to 0.
    pub fn write_y(&mut self, value: u16) {
        if self.x_flag() {
            // 8-bit mode: force high byte to 0
            self.y = value & 0x00FF;
        } else {
            // 16-bit mode: full 16 bits
            self.y = value;
        }
    }

    /// Read direct page register.
    pub fn read_d(&self) -> u16 {
        self.d
    }

    /// Write direct page register.
    pub fn write_d(&mut self, value: u16) {
        self.d = value;
    }

    /// Read data bank register.
    pub fn read_dbr(&self) -> u8 {
        self.dbr
    }

    /// Write data bank register.
    pub fn write_dbr(&mut self, value: u8) {
        self.dbr = value;
    }

    /// Read program bank register.
    pub fn read_pbr(&self) -> u8 {
        self.pbr
    }

    /// Write program bank register.
    pub fn write_pbr(&mut self, value: u8) {
        self.pbr = value;
    }

    /// Read stack pointer.
    pub fn read_s(&self) -> u16 {
        self.s
    }

    /// Write stack pointer.
    /// In emulation mode, high byte forced to $01.
    pub fn write_s(&mut self, value: u16) {
        if self.e {
            // Emulation mode: force high byte to $01
            self.s = 0x0100 | (value & 0x00FF);
        } else {
            // Native mode: full 16 bits
            self.s = value;
        }
    }

    /// Read program counter.
    pub fn read_pc(&self) -> u16 {
        self.pc
    }

    /// Write program counter.
    pub fn write_pc(&mut self, value: u16) {
        self.pc = value;
    }

    /// Read processor status register.
    pub fn read_p(&self) -> u8 {
        self.p
    }

    /// Check if in emulation mode.
    pub fn emulation_mode(&self) -> bool {
        self.e
    }

    /// Get M flag (accumulator/memory width: 1=8-bit, 0=16-bit).
    pub fn m_flag(&self) -> bool {
        self.p & FLAG_ACCUM_WIDTH != 0
    }

    /// Get X flag (index width: 1=8-bit, 0=16-bit).
    pub fn x_flag(&self) -> bool {
        self.p & FLAG_INDEX_WIDTH != 0
    }

    /// Get carry flag.
    pub fn flag_c(&self) -> bool {
        self.p & FLAG_CARRY != 0
    }

    /// Set carry flag.
    pub fn set_flag_c(&mut self, value: bool) {
        if value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
    }

    /// Get zero flag.
    pub fn flag_z(&self) -> bool {
        self.p & FLAG_ZERO != 0
    }

    /// Set zero flag.
    pub fn set_flag_z(&mut self, value: bool) {
        if value {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }
    }

    /// Get interrupt disable flag.
    pub fn flag_i(&self) -> bool {
        self.p & FLAG_INTERRUPT != 0
    }

    /// Set interrupt disable flag.
    pub fn set_flag_i(&mut self, value: bool) {
        if value {
            self.p |= FLAG_INTERRUPT;
        } else {
            self.p &= !FLAG_INTERRUPT;
        }
    }

    /// Get decimal mode flag.
    pub fn flag_d(&self) -> bool {
        self.p & FLAG_DECIMAL != 0
    }

    /// Set decimal mode flag.
    pub fn set_flag_d(&mut self, value: bool) {
        if value {
            self.p |= FLAG_DECIMAL;
        } else {
            self.p &= !FLAG_DECIMAL;
        }
    }

    /// Get overflow flag.
    pub fn flag_v(&self) -> bool {
        self.p & FLAG_OVERFLOW != 0
    }

    /// Set overflow flag.
    pub fn set_flag_v(&mut self, value: bool) {
        if value {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    /// Get negative flag.
    pub fn flag_n(&self) -> bool {
        self.p & FLAG_NEGATIVE != 0
    }

    /// Set negative flag.
    pub fn set_flag_n(&mut self, value: bool) {
        if value {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    /// XCE - Exchange Carry with Emulation flag.
    /// Swaps the C flag (bit 0 of P) with the hidden E flag.
    /// When E transitions:
    /// - E 0→1 (native→emulation): force M=1, X=1, S high byte→$01
    /// - E 1→0 (emulation→native): M/X remain 1 until cleared by REP
    pub fn xce(&mut self) {
        let old_c = self.flag_c();
        let old_e = self.e;

        // Swap C and E
        self.set_flag_c(old_e);
        self.e = old_c;

        // Enforce mode constraints when entering emulation mode
        if !old_e && self.e {
            // Entering emulation mode (E 0→1)
            self.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // Force M=1, X=1
            self.s = 0x0100 | (self.s & 0x00FF); // Force S high byte to $01
        }
        // Note: When leaving emulation mode (E 1→0), M/X remain 1 until REP clears them
    }

    /// REP - Reset Processor Status Bits.
    /// Clears bits in P specified by the immediate byte.
    /// In emulation mode, M and X flags cannot be cleared (remain 1).
    pub fn rep(&mut self, mask: u8) {
        if self.e {
            // Emulation mode: cannot clear M or X
            let protected_mask = mask & !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
            self.p &= !protected_mask;
        } else {
            // Native mode: can clear any bits including M and X
            self.p &= !mask;

            // When X flag transitions from 1→0 (8→16 bit), high bytes of X/Y start at 0
            // (they were already 0 due to write_x/write_y forcing)
            // No action needed here as write_x/y already enforce this
        }
    }

    /// SEP - Set Processor Status Bits.
    /// Sets bits in P specified by the immediate byte.
    pub fn sep(&mut self, mask: u8) {
        let old_x = self.x_flag();

        self.p |= mask;

        // Handle width transitions
        // When X flag transitions from 0→1 (16→8 bit), force high bytes of X/Y to 0
        if !old_x && self.x_flag() {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
        // When M flag transitions from 0→1 (16→8 bit), B (high byte of A) is preserved
        // No action needed - read_a/write_a already handle this

        // Note: M 1→0 transition handled naturally by read_a/write_a
    }
}

// Private helpers — suppressed until opcode dispatch is wired up.
#[allow(dead_code)]
impl<B: SnesBus> Cpu<B> {
    // -------------------------------------------------------------------------
    // Addressing mode helpers
    // Each returns a 24-bit effective address (u32, upper byte always 0 for
    // bank-0 modes).  Indirect helpers read pointer bytes via self.bus.
    // -------------------------------------------------------------------------

    /// Direct Page: EA = (D + offset) & 0xFFFF  [bank 0]
    fn addr_dp(&self, offset: u8) -> u32 {
        (self.d as u32 + offset as u32) & 0xFFFF
    }

    /// Direct Page Indexed X: EA = (D + offset + X) & 0xFFFF  [bank 0]
    fn addr_dp_x(&self, offset: u8) -> u32 {
        (self.d as u32 + offset as u32 + self.x as u32) & 0xFFFF
    }

    /// Direct Page Indexed Y: EA = (D + offset + Y) & 0xFFFF  [bank 0]
    fn addr_dp_y(&self, offset: u8) -> u32 {
        (self.d as u32 + offset as u32 + self.y as u32) & 0xFFFF
    }

    /// Absolute: EA = DBR:abs
    fn addr_abs(&self, abs: u16) -> u32 {
        (self.dbr as u32) << 16 | abs as u32
    }

    /// Absolute Indexed X: EA = (DBR:abs + X) & 0xFF_FFFF
    fn addr_abs_x(&self, abs: u16) -> u32 {
        ((self.dbr as u32) << 16 | abs as u32).wrapping_add(self.x as u32) & 0xFF_FFFF
    }

    /// Absolute Indexed Y: EA = (DBR:abs + Y) & 0xFF_FFFF
    fn addr_abs_y(&self, abs: u16) -> u32 {
        ((self.dbr as u32) << 16 | abs as u32).wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    /// Absolute Long: EA = 24-bit operand (pass-through, masked to 24 bits)
    fn addr_abs_long(&self, addr: u32) -> u32 {
        addr & 0xFF_FFFF
    }

    /// Absolute Long Indexed X: EA = (24-bit operand + X) & 0xFF_FFFF
    fn addr_abs_long_x(&self, addr: u32) -> u32 {
        addr.wrapping_add(self.x as u32) & 0xFF_FFFF
    }

    /// Stack Relative: EA = (S + offset) & 0xFFFF  [bank 0]
    fn addr_sr(&self, offset: u8) -> u32 {
        (self.s as u32 + offset as u32) & 0xFFFF
    }

    /// Direct Page Indirect: pointer at (D+offset), EA = DBR:ptr16
    fn addr_dp_ind(&self, offset: u8) -> u32 {
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let hi = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let ptr = lo as u32 | (hi as u32) << 8;
        (self.dbr as u32) << 16 | ptr
    }

    /// Direct Page Indirect Long: 24-bit pointer at (D+offset)
    fn addr_dp_ind_long(&self, offset: u8) -> u32 {
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let mid = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let hi = self.bus.read((ptr_addr + 2) & 0xFFFF);
        lo as u32 | (mid as u32) << 8 | (hi as u32) << 16
    }

    /// Direct Page Indexed Indirect X: pointer at (D+offset+X), EA = DBR:ptr16
    fn addr_dp_x_ind(&self, offset: u8) -> u32 {
        let ptr_addr = (self.d as u32 + offset as u32 + self.x as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let hi = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let ptr = lo as u32 | (hi as u32) << 8;
        (self.dbr as u32) << 16 | ptr
    }

    /// Direct Page Indirect Indexed Y: ptr16 at (D+offset), EA = (DBR:ptr16+Y) & 0xFF_FFFF
    fn addr_dp_ind_y(&self, offset: u8) -> u32 {
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let hi = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let ptr = lo as u32 | (hi as u32) << 8;
        ((self.dbr as u32) << 16 | ptr).wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    /// Direct Page Indirect Long Indexed Y: 24-bit ptr at (D+offset), EA = (ptr+Y) & 0xFF_FFFF
    fn addr_dp_ind_long_y(&self, offset: u8) -> u32 {
        let ptr_addr = (self.d as u32 + offset as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let mid = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let hi = self.bus.read((ptr_addr + 2) & 0xFFFF);
        let base = lo as u32 | (mid as u32) << 8 | (hi as u32) << 16;
        base.wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    /// Stack Relative Indirect Indexed Y: ptr16 at (S+offset), EA = (DBR:ptr16+Y) & 0xFF_FFFF
    fn addr_sr_ind_y(&self, offset: u8) -> u32 {
        let ptr_addr = (self.s as u32 + offset as u32) & 0xFFFF;
        let lo = self.bus.read(ptr_addr);
        let hi = self.bus.read((ptr_addr + 1) & 0xFFFF);
        let ptr = lo as u32 | (hi as u32) << 8;
        ((self.dbr as u32) << 16 | ptr).wrapping_add(self.y as u32) & 0xFF_FFFF
    }

    // -------------------------------------------------------------------------
    // Width-aware memory access helpers
    // -------------------------------------------------------------------------

    /// Read one byte from the bus.
    fn read8(&self, addr: u32) -> u8 {
        self.bus.read(addr & 0xFF_FFFF)
    }

    /// Write one byte to the bus.
    fn write8(&mut self, addr: u32, value: u8) {
        self.bus.write(addr & 0xFF_FFFF, value);
    }

    /// Read two bytes little-endian; high byte wraps within the same bank.
    fn read16(&self, addr: u32) -> u16 {
        let bank = addr & 0xFF_0000;
        let offset = addr & 0x0000_FFFF;
        let lo = self.bus.read(bank | offset);
        let hi = self.bus.read(bank | ((offset + 1) & 0xFFFF));
        lo as u16 | (hi as u16) << 8
    }

    /// Write two bytes little-endian; high byte wraps within the same bank.
    fn write16(&mut self, addr: u32, value: u16) {
        let bank = addr & 0xFF_0000;
        let offset = addr & 0x0000_FFFF;
        self.bus.write(bank | offset, value as u8);
        self.bus
            .write(bank | ((offset + 1) & 0xFFFF), (value >> 8) as u8);
    }

    /// Read M-flag width: 8-bit when M=1, 16-bit when M=0.
    fn read_m(&self, addr: u32) -> u16 {
        if self.m_flag() {
            self.read8(addr) as u16
        } else {
            self.read16(addr)
        }
    }

    /// Write M-flag width: 8-bit when M=1, 16-bit when M=0.
    fn write_m(&mut self, addr: u32, value: u16) {
        if self.m_flag() {
            self.write8(addr, value as u8);
        } else {
            self.write16(addr, value);
        }
    }

    /// Read X-flag width: 8-bit when X=1, 16-bit when X=0.
    fn read_idx(&self, addr: u32) -> u16 {
        if self.x_flag() {
            self.read8(addr) as u16
        } else {
            self.read16(addr)
        }
    }

    /// Write X-flag width: 8-bit when X=1, 16-bit when X=0.
    fn write_idx(&mut self, addr: u32, value: u16) {
        if self.x_flag() {
            self.write8(addr, value as u8);
        } else {
            self.write16(addr, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::bus::StubBus;

    #[test]
    fn reset_state_is_emulation_mode() {
        let cpu = Cpu::new(StubBus);
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());
        assert_eq!(cpu.read_s(), 0x01FF);
        assert!(cpu.flag_i());
    }

    #[test]
    fn write_a_8bit_preserves_b() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (M=1, 8-bit A)
        cpu.a = 0x1234; // Set B:A
        cpu.write_a(0x56); // Write only A
        assert_eq!(cpu.read_a(), 0x1256); // B preserved
    }

    #[test]
    fn write_a_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with M=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.a = 0x1234;
        cpu.write_a(0x5678);
        assert_eq!(cpu.read_a(), 0x5678);
    }

    #[test]
    fn write_x_8bit_clears_high_byte() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (X=1, 8-bit X)
        cpu.x = 0x1234;
        cpu.write_x(0xFF56);
        assert_eq!(cpu.read_x(), 0x0056); // High byte forced to 0
    }

    #[test]
    fn write_x_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with X=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.write_x(0x5678);
        assert_eq!(cpu.read_x(), 0x5678);
    }

    #[test]
    fn write_y_8bit_clears_high_byte() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (X=1, 8-bit Y)
        cpu.y = 0x1234;
        cpu.write_y(0xFF56);
        assert_eq!(cpu.read_y(), 0x0056); // High byte forced to 0
    }

    #[test]
    fn write_y_16bit_updates_full() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode with X=0 (16-bit)
        cpu.e = false;
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.write_y(0x5678);
        assert_eq!(cpu.read_y(), 0x5678);
    }

    #[test]
    fn emulation_mode_forces_stack_high_byte_01() {
        let mut cpu = Cpu::new(StubBus);
        // Emulation mode (E=1)
        cpu.write_s(0x5678);
        assert_eq!(cpu.read_s(), 0x0178); // High byte forced to $01
    }

    #[test]
    fn native_mode_allows_full_16bit_stack() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false; // Native mode
        cpu.write_s(0x5678);
        assert_eq!(cpu.read_s(), 0x5678); // Full 16-bit
    }

    #[test]
    fn flag_accessors_work() {
        let mut cpu = Cpu::new(StubBus);

        cpu.set_flag_c(true);
        assert!(cpu.flag_c());
        cpu.set_flag_c(false);
        assert!(!cpu.flag_c());

        cpu.set_flag_z(true);
        assert!(cpu.flag_z());
        cpu.set_flag_z(false);
        assert!(!cpu.flag_z());

        cpu.set_flag_i(true);
        assert!(cpu.flag_i());
        cpu.set_flag_i(false);
        assert!(!cpu.flag_i());

        cpu.set_flag_d(true);
        assert!(cpu.flag_d());
        cpu.set_flag_d(false);
        assert!(!cpu.flag_d());

        cpu.set_flag_v(true);
        assert!(cpu.flag_v());
        cpu.set_flag_v(false);
        assert!(!cpu.flag_v());

        cpu.set_flag_n(true);
        assert!(cpu.flag_n());
        cpu.set_flag_n(false);
        assert!(!cpu.flag_n());
    }

    #[test]
    fn xce_emulation_to_native() {
        let mut cpu = Cpu::new(StubBus);
        // Start in emulation mode (E=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Set C=0 before XCE (to switch to native mode)
        cpu.set_flag_c(false);

        // Execute XCE: swap E and C
        // Before: E=1, C=0
        // After:  E=0 (takes C's value), C=1 (takes E's value)
        cpu.xce();

        assert!(!cpu.emulation_mode()); // Now in native mode (E=0)
        assert!(cpu.flag_c()); // C now has old E value (1)
        assert!(cpu.m_flag()); // M still 1 (not auto-cleared)
        assert!(cpu.x_flag()); // X still 1 (not auto-cleared)
    }

    #[test]
    fn xce_native_to_emulation() {
        let mut cpu = Cpu::new(StubBus);
        // Start in native mode with M=0, X=0
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0
        cpu.s = 0x2345; // Full 16-bit stack
        cpu.set_flag_c(true); // C=1 (to switch to emulation mode)

        // Execute XCE: swap E and C
        // Before: E=0, C=1
        // After:  E=1 (takes C's value), C=0 (takes E's value)
        cpu.xce();

        assert!(cpu.emulation_mode()); // Now in emulation mode (E=1)
        assert!(!cpu.flag_c()); // C now has old E value (0)
        assert!(cpu.m_flag()); // M forced to 1
        assert!(cpu.x_flag()); // X forced to 1
        assert_eq!(cpu.read_s(), 0x0145); // S high byte forced to $01
    }

    #[test]
    fn xce_preserves_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.set_flag_n(true);
        cpu.set_flag_v(true);
        cpu.set_flag_d(true);
        cpu.set_flag_i(false);
        cpu.set_flag_z(true);
        cpu.set_flag_c(true);

        cpu.xce();

        // All flags except C should be preserved
        assert!(cpu.flag_n());
        assert!(cpu.flag_v());
        assert!(cpu.flag_d());
        assert!(!cpu.flag_i());
        assert!(cpu.flag_z());
    }

    #[test]
    fn rep_in_native_mode_clears_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        // Switch to native mode
        cpu.e = false;
        cpu.p |= FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH; // M=1, X=1

        // REP to clear M and X
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(!cpu.m_flag());
        assert!(!cpu.x_flag());
    }

    #[test]
    fn rep_in_emulation_mode_cannot_clear_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        // Emulation mode (E=1, M=1, X=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Try to REP M and X - should have no effect
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(cpu.m_flag()); // Still 1
        assert!(cpu.x_flag()); // Still 1
    }

    #[test]
    fn rep_clears_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.set_flag_c(true);
        cpu.set_flag_z(true);
        cpu.set_flag_i(true);
        cpu.set_flag_d(true);
        cpu.set_flag_v(true);
        cpu.set_flag_n(true);

        // Clear C, Z, I flags
        cpu.rep(FLAG_CARRY | FLAG_ZERO | FLAG_INTERRUPT);

        assert!(!cpu.flag_c());
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_i());
        // Others preserved
        assert!(cpu.flag_d());
        assert!(cpu.flag_v());
        assert!(cpu.flag_n());
    }

    #[test]
    fn sep_sets_m_and_x() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0
        cpu.p &= !FLAG_INDEX_WIDTH; // X=0

        // Set 16-bit values
        cpu.write_x(0x1234);
        cpu.write_y(0x5678);

        // SEP to set M and X (switch to 8-bit)
        cpu.sep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);

        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // X and Y high bytes should be forced to 0
        assert_eq!(cpu.read_x(), 0x0034);
        assert_eq!(cpu.read_y(), 0x0078);
    }

    #[test]
    fn sep_m_transition_preserves_b() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.p &= !FLAG_ACCUM_WIDTH; // M=0 (16-bit)

        cpu.write_a(0x1234); // Set full 16-bit value

        // SEP to set M (switch to 8-bit)
        cpu.sep(FLAG_ACCUM_WIDTH);

        assert!(cpu.m_flag());
        assert_eq!(cpu.read_a(), 0x1234); // B preserved (full value readable)
    }

    #[test]
    fn sep_sets_other_flags() {
        let mut cpu = Cpu::new(StubBus);
        cpu.e = false;
        cpu.set_flag_c(false);
        cpu.set_flag_z(false);
        cpu.set_flag_i(false);

        // Set C, Z, I flags
        cpu.sep(FLAG_CARRY | FLAG_ZERO | FLAG_INTERRUPT);

        assert!(cpu.flag_c());
        assert!(cpu.flag_z());
        assert!(cpu.flag_i());
    }

    #[test]
    fn integration_full_mode_switching_cycle() {
        let mut cpu = Cpu::new(StubBus);

        // Start in emulation mode (E=1, M=1, X=1)
        assert!(cpu.emulation_mode());
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());
        assert_eq!(cpu.read_s(), 0x01FF);

        // Set A/X/Y with 8-bit values (B:A = 0x00:42, X = 0x00:34, Y = 0x00:56)
        cpu.write_a(0x42);
        cpu.write_x(0x34);
        cpu.write_y(0x56);
        assert_eq!(cpu.read_a(), 0x0042);
        assert_eq!(cpu.read_x(), 0x0034);
        assert_eq!(cpu.read_y(), 0x0056);

        // Switch to native mode via XCE (C=0, E=1 → C=1, E=0)
        cpu.set_flag_c(false);
        cpu.xce();
        assert!(!cpu.emulation_mode());
        assert!(cpu.flag_c()); // Got old E=1
        assert!(cpu.m_flag()); // Still 1 (not auto-cleared)
        assert!(cpu.x_flag()); // Still 1 (not auto-cleared)

        // Use REP to switch to 16-bit mode (M=0, X=0)
        cpu.rep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        assert!(!cpu.m_flag());
        assert!(!cpu.x_flag());

        // Write full 16-bit values
        cpu.write_a(0x1234);
        cpu.write_x(0x5678);
        cpu.write_y(0x9ABC);
        assert_eq!(cpu.read_a(), 0x1234);
        assert_eq!(cpu.read_x(), 0x5678);
        assert_eq!(cpu.read_y(), 0x9ABC);

        // Switch back to 8-bit via SEP
        cpu.sep(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
        assert!(cpu.m_flag());
        assert!(cpu.x_flag());

        // Verify width behavior:
        // - A: B preserved (0x1234 → 0x1234, but only low byte accessible in 8-bit mode)
        // - X/Y: high bytes forced to 0 (0x5678 → 0x0078, 0x9ABC → 0x00BC)
        assert_eq!(cpu.read_a(), 0x1234); // B preserved
        assert_eq!(cpu.read_x(), 0x0078); // High byte cleared
        assert_eq!(cpu.read_y(), 0x00BC); // High byte cleared

        // Write 8-bit values
        cpu.write_a(0xFF56);
        cpu.write_x(0xFF34);
        cpu.write_y(0xFF12);
        assert_eq!(cpu.read_a(), 0x1256); // B (0x12) preserved, A updated to 0x56
        assert_eq!(cpu.read_x(), 0x0034); // High byte forced to 0
        assert_eq!(cpu.read_y(), 0x0012); // High byte forced to 0

        // Set stack to arbitrary value in native mode
        cpu.s = 0x2345;
        assert_eq!(cpu.read_s(), 0x2345);

        // Switch back to emulation mode via XCE (C=1, E=0 → C=0, E=1)
        cpu.set_flag_c(true);
        cpu.xce();
        assert!(cpu.emulation_mode());
        assert!(!cpu.flag_c()); // Got old E=0
        assert!(cpu.m_flag()); // Forced to 1
        assert!(cpu.x_flag()); // Forced to 1
        assert_eq!(cpu.read_s(), 0x0145); // S high byte forced to $01
    }

    // -------------------------------------------------------------------------
    // Addressing mode tests
    // -------------------------------------------------------------------------

    mod addr_modes {
        use super::*;
        use crate::snes::bus::TestBus;

        fn cpu_with_bus() -> Cpu<TestBus> {
            let mut cpu = Cpu::new(TestBus::default());
            // Switch to native mode, 16-bit A and X/Y by default
            cpu.e = false;
            cpu.p &= !(FLAG_ACCUM_WIDTH | FLAG_INDEX_WIDTH);
            cpu
        }

        // -- Direct Page -------------------------------------------------------

        #[test]
        fn addr_dp_basic() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            assert_eq!(cpu.addr_dp(0x10), 0x0000_0210);
        }

        #[test]
        fn addr_dp_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            assert_eq!(cpu.addr_dp(0xFF), 0x0000_FFFF);
        }

        #[test]
        fn addr_dp_x_adds_x_register() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_x(0x0010);
            assert_eq!(cpu.addr_dp_x(0x10), 0x0000_0220);
        }

        #[test]
        fn addr_dp_x_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            cpu.write_x(0x0001);
            // 0xFF00 + 0xFF + 0x01 = 0x10000 → wraps to 0x0000
            assert_eq!(cpu.addr_dp_x(0xFF), 0x0000_0000);
        }

        #[test]
        fn addr_dp_y_adds_y_register() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_y(0x0005);
            assert_eq!(cpu.addr_dp_y(0x10), 0x0000_0215);
        }

        // -- Absolute ----------------------------------------------------------

        #[test]
        fn addr_abs_uses_dbr_as_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x03);
            assert_eq!(cpu.addr_abs(0x1234), 0x03_1234);
        }

        #[test]
        fn addr_abs_x_adds_x_and_can_cross_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x01);
            cpu.write_x(0x0100);
            // 0x01_FF00 + 0x100 = 0x02_0000
            assert_eq!(cpu.addr_abs_x(0xFF00), 0x02_0000);
        }

        #[test]
        fn addr_abs_y_adds_y_and_can_cross_bank() {
            let mut cpu = cpu_with_bus();
            cpu.write_dbr(0x02);
            cpu.write_y(0x0050);
            assert_eq!(cpu.addr_abs_y(0x1200), 0x02_1250);
        }

        // -- Absolute Long -----------------------------------------------------

        #[test]
        fn addr_abs_long_passes_through_24bit_addr() {
            let cpu = cpu_with_bus();
            assert_eq!(cpu.addr_abs_long(0x12_3456), 0x12_3456);
        }

        #[test]
        fn addr_abs_long_x_adds_x() {
            let mut cpu = cpu_with_bus();
            cpu.write_x(0x0010);
            assert_eq!(cpu.addr_abs_long_x(0x12_3456), 0x12_3466);
        }

        #[test]
        fn addr_abs_long_x_wraps_at_24bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_x(0x0001);
            assert_eq!(cpu.addr_abs_long_x(0xFF_FFFF), 0x00_0000);
        }

        // -- Stack Relative ----------------------------------------------------

        #[test]
        fn addr_sr_adds_offset_to_s() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0x01F0);
            assert_eq!(cpu.addr_sr(0x10), 0x0000_0200);
        }

        #[test]
        fn addr_sr_wraps_at_16bit() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0xFF01);
            // 0xFF01 + 0xFF = 0x1_0000 → wraps to 0x0000
            assert_eq!(cpu.addr_sr(0xFF), 0x0000_0000);
        }

        // -- Direct Page Indirect (dp) -----------------------------------------

        #[test]
        fn addr_dp_ind_reads_ptr16_and_adds_dbr() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x05);
            // Place 16-bit pointer $1234 at DP address $0210
            cpu.bus.load(0x0000_0210, &[0x34, 0x12]);
            assert_eq!(cpu.addr_dp_ind(0x10), 0x05_1234);
        }

        // -- Direct Page Indirect Long [dp] ------------------------------------

        #[test]
        fn addr_dp_ind_long_reads_ptr24() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            // Place 24-bit pointer $78_1234 at DP address $0210
            cpu.bus.load(0x0000_0210, &[0x34, 0x12, 0x78]);
            assert_eq!(cpu.addr_dp_ind_long(0x10), 0x78_1234);
        }

        // -- Direct Page Indexed Indirect (dp,X) -------------------------------

        #[test]
        fn addr_dp_x_ind_adds_x_then_reads_ptr16() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_x(0x0010);
            cpu.write_dbr(0x03);
            // Place pointer $ABCD at D + offset + X = $0200 + $10 + $10 = $0220
            cpu.bus.load(0x0000_0220, &[0xCD, 0xAB]);
            assert_eq!(cpu.addr_dp_x_ind(0x10), 0x03_ABCD);
        }

        // -- Direct Page Indirect Indexed Y (dp),Y -----------------------------

        #[test]
        fn addr_dp_ind_y_reads_ptr16_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x02);
            cpu.write_y(0x0004);
            // Place ptr $1000 at D + offset = $0210
            cpu.bus.load(0x0000_0210, &[0x00, 0x10]);
            // EA = DBR:$1000 + Y = $02_1004
            assert_eq!(cpu.addr_dp_ind_y(0x10), 0x02_1004);
        }

        #[test]
        fn addr_dp_ind_y_bank_crosses_allowed() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_dbr(0x01);
            cpu.write_y(0x0100);
            // ptr = $FF00, EA = $01_FF00 + $100 = $02_0000
            cpu.bus.load(0x0000_0210, &[0x00, 0xFF]);
            assert_eq!(cpu.addr_dp_ind_y(0x10), 0x02_0000);
        }

        // -- Direct Page Indirect Long Indexed Y [dp],Y ------------------------

        #[test]
        fn addr_dp_ind_long_y_reads_ptr24_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0x0200);
            cpu.write_y(0x0010);
            // Place 24-bit ptr $05_1200 at $0210
            cpu.bus.load(0x0000_0210, &[0x00, 0x12, 0x05]);
            assert_eq!(cpu.addr_dp_ind_long_y(0x10), 0x05_1210);
        }

        // -- Stack Relative Indirect Indexed Y (sr,S),Y ------------------------

        #[test]
        fn addr_sr_ind_y_reads_ptr16_at_s_plus_offset_then_adds_y() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0x01F0);
            cpu.write_dbr(0x04);
            cpu.write_y(0x0008);
            // ptr_addr = S + offset = $01F0 + $10 = $0200
            cpu.bus.load(0x0000_0200, &[0x00, 0x30]);
            // EA = DBR:$3000 + Y = $04_3008
            assert_eq!(cpu.addr_sr_ind_y(0x10), 0x04_3008);
        }

        // -- Pointer byte read wrapping at bank-0 $FFFF boundary ---------------

        #[test]
        fn addr_dp_ind_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFF);
            cpu.write_dbr(0x02);
            // Place low byte at $FFFF, high byte wraps to $0000
            cpu.bus.load(0x0000_FFFF, &[0xCD]);
            cpu.bus.load(0x0000_0000, &[0xAB]);
            assert_eq!(cpu.addr_dp_ind(0x00), 0x02_ABCD);
        }

        #[test]
        fn addr_dp_ind_long_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFE);
            // 3-byte pointer: $FE→lo, $FF→mid, $00→hi (wraps)
            cpu.bus.load(0x0000_FFFE, &[0x11, 0x22]);
            cpu.bus.load(0x0000_0000, &[0x33]);
            assert_eq!(cpu.addr_dp_ind_long(0x00), 0x33_2211);
        }

        #[test]
        fn addr_dp_x_ind_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFF00);
            cpu.write_x(0x00FF); // D + offset + X = $FF00 + $00 + $FF = $FFFF
            cpu.write_dbr(0x05);
            cpu.bus.load(0x0000_FFFF, &[0x78]);
            cpu.bus.load(0x0000_0000, &[0x56]);
            assert_eq!(cpu.addr_dp_x_ind(0x00), 0x05_5678);
        }

        #[test]
        fn addr_dp_ind_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFF);
            cpu.write_dbr(0x01);
            cpu.write_y(0x0001);
            cpu.bus.load(0x0000_FFFF, &[0xFF]);
            cpu.bus.load(0x0000_0000, &[0x00]);
            // ptr = $00FF, EA = $01_00FF + 1 = $01_0100
            assert_eq!(cpu.addr_dp_ind_y(0x00), 0x01_0100);
        }

        #[test]
        fn addr_dp_ind_long_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_d(0xFFFE);
            cpu.write_y(0x0002);
            cpu.bus.load(0x0000_FFFE, &[0x00, 0x10]);
            cpu.bus.load(0x0000_0000, &[0x07]);
            // base = $07_1000, EA = $07_1002
            assert_eq!(cpu.addr_dp_ind_long_y(0x00), 0x07_1002);
        }

        #[test]
        fn addr_sr_ind_y_ptr_wraps_at_ffff() {
            let mut cpu = cpu_with_bus();
            cpu.write_s(0xFFFF);
            cpu.write_dbr(0x03);
            cpu.write_y(0x0000);
            cpu.bus.load(0x0000_FFFF, &[0x34]);
            cpu.bus.load(0x0000_0000, &[0x12]);
            assert_eq!(cpu.addr_sr_ind_y(0x00), 0x03_1234);
        }
    }
}

#[cfg(test)]
mod mem_helpers_tests {
    use super::*;
    use crate::snes::bus::TestBus;

    fn make_cpu() -> Cpu<TestBus> {
        Cpu::new(TestBus::default())
    }

    #[test]
    fn read8_returns_byte_at_address() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x01_2000, &[0xAB]);
        assert_eq!(cpu.read8(0x01_2000), 0xAB);
    }

    #[test]
    fn write8_stores_byte_at_address() {
        let mut cpu = make_cpu();
        cpu.write8(0x01_3000, 0x55);
        assert_eq!(cpu.bus.read(0x01_3000), 0x55);
    }

    #[test]
    fn read16_little_endian() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x02_1000, &[0x34, 0x12]);
        assert_eq!(cpu.read16(0x02_1000), 0x1234);
    }

    #[test]
    fn read16_wraps_high_byte_within_bank() {
        let mut cpu = make_cpu();
        cpu.bus.load(0x02_FFFF, &[0x78]);
        cpu.bus.load(0x02_0000, &[0x56]);
        assert_eq!(cpu.read16(0x02_FFFF), 0x5678);
    }

    #[test]
    fn write16_little_endian() {
        let mut cpu = make_cpu();
        cpu.write16(0x03_2000, 0xBEEF);
        assert_eq!(cpu.bus.read(0x03_2000), 0xEF);
        assert_eq!(cpu.bus.read(0x03_2001), 0xBE);
    }

    #[test]
    fn write16_wraps_high_byte_within_bank() {
        let mut cpu = make_cpu();
        cpu.write16(0x04_FFFF, 0xCAFE);
        assert_eq!(cpu.bus.read(0x04_FFFF), 0xFE);
        assert_eq!(cpu.bus.read(0x04_0000), 0xCA);
    }

    #[test]
    fn read_m_reads_8bit_when_m_flag_set() {
        let mut cpu = make_cpu(); // reset default: M=1
        cpu.bus.load(0x00_1000, &[0x42, 0xFF]);
        assert_eq!(cpu.read_m(0x00_1000), 0x0042);
    }

    #[test]
    fn read_m_reads_16bit_when_m_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_ACCUM_WIDTH);
        cpu.bus.load(0x00_1000, &[0x34, 0x12]);
        assert_eq!(cpu.read_m(0x00_1000), 0x1234);
    }

    #[test]
    fn write_m_writes_8bit_when_m_flag_set() {
        let mut cpu = make_cpu(); // default: M=1
        cpu.write_m(0x00_2000, 0x1234);
        assert_eq!(cpu.bus.read(0x00_2000), 0x34);
        assert_eq!(cpu.bus.read(0x00_2001), 0x00); // high byte not written
    }

    #[test]
    fn write_m_writes_16bit_when_m_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_ACCUM_WIDTH);
        cpu.write_m(0x00_3000, 0xABCD);
        assert_eq!(cpu.bus.read(0x00_3000), 0xCD);
        assert_eq!(cpu.bus.read(0x00_3001), 0xAB);
    }

    #[test]
    fn read_idx_reads_8bit_when_x_flag_set() {
        let mut cpu = make_cpu(); // reset default: X=1
        cpu.bus.load(0x00_4000, &[0x77, 0xFF]);
        assert_eq!(cpu.read_idx(0x00_4000), 0x0077);
    }

    #[test]
    fn read_idx_reads_16bit_when_x_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_INDEX_WIDTH);
        cpu.bus.load(0x00_4000, &[0x34, 0x12]);
        assert_eq!(cpu.read_idx(0x00_4000), 0x1234);
    }

    #[test]
    fn write_idx_writes_8bit_when_x_flag_set() {
        let mut cpu = make_cpu(); // default: X=1
        cpu.write_idx(0x00_5000, 0x1234);
        assert_eq!(cpu.bus.read(0x00_5000), 0x34);
        assert_eq!(cpu.bus.read(0x00_5001), 0x00); // high byte not written
    }

    #[test]
    fn write_idx_writes_16bit_when_x_flag_clear() {
        let mut cpu = make_cpu();
        cpu.e = false;
        cpu.rep(FLAG_INDEX_WIDTH);
        cpu.write_idx(0x00_6000, 0xDEAD);
        assert_eq!(cpu.bus.read(0x00_6000), 0xAD);
        assert_eq!(cpu.bus.read(0x00_6001), 0xDE);
    }
}
