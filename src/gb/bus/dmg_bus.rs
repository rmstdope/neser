use crate::gb::bus::GbBus;
use crate::gb::cartridge::GbCartridge;
use crate::gb::ppu::Ppu;
use crate::gb::timer::Timer;

/// Full DMG memory bus.
///
/// Implements the Game Boy (DMG) memory map, routing reads and writes to the
/// correct hardware region. Owns the cartridge, static RAM buffers, the Timer
/// subsystem, the PPU, and the IF/IE interrupt registers.
///
/// Memory map:
/// - $0000–$7FFF: Cartridge ROM  (bank 0 fixed + switchable bank)
/// - $8000–$9FFF: VRAM            (routed through PPU; blocked during Mode 3)
/// - $A000–$BFFF: Cartridge RAM  (external/MBC-controlled)
/// - $C000–$DFFF: WRAM
/// - $E000–$FDFF: Echo RAM       (mirrors WRAM)
/// - $FE00–$FE9F: OAM             (routed through PPU; blocked during Mode 2–3)
/// - $FEA0–$FEFF: Forbidden      (reads return 0xFF; writes ignored)
/// - $FF04–$FF07: Timer          (DIV/TIMA/TMA/TAC)
/// - $FF0F:       IF register
/// - $FF40–$FF4B: PPU I/O registers
/// - $FF46:       OAM DMA (write-only trigger)
/// - $FF80–$FFFE: HRAM
/// - $FFFF:       IE register
/// - Everything else in $FF00–$FF7F: I/O stubs (reads return 0xFF)
pub struct DmgBus {
    cart: Box<dyn GbCartridge>,
    pub ppu: Ppu,
    wram: [u8; 0x2000],
    hram: [u8; 0x7F],
    timer: Timer,
    /// IF register ($FF0F): interrupt flag.
    if_reg: u8,
    /// IE register ($FFFF): interrupt enable.
    ie_reg: u8,
}

impl DmgBus {
    pub fn new(cart: Box<dyn GbCartridge>) -> Self {
        Self {
            cart,
            ppu: Ppu::new(),
            wram: [0u8; 0x2000],
            hram: [0u8; 0x7F],
            timer: Timer::new(),
            if_reg: 0,
            ie_reg: 0,
        }
    }

    /// Advance system timers and PPU by `m_cycles` M-cycles.
    ///
    /// Propagates any timer interrupt to the IF register ($FF0F bit 2).
    /// Propagates PPU VBlank (bit 0) and STAT (bit 1) interrupts.
    pub fn tick(&mut self, m_cycles: u8) {
        self.timer.tick(m_cycles);
        if self.timer.interrupt_pending {
            self.if_reg |= 0x04;
            self.timer.interrupt_pending = false;
        }
        self.ppu.tick_dots(u32::from(m_cycles) * 4);
        self.if_reg |= self.ppu.take_pending_interrupts();
    }

    /// Bypass PPU access-blocking for OAM DMA transfers.
    fn read_raw(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.vram[(addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            _ => 0xFF,
        }
    }

    /// Execute an OAM DMA transfer: copy 160 bytes from `(val << 8)` into OAM.
    fn do_oam_dma(&mut self, val: u8) {
        let src = u16::from(val) << 8;
        for i in 0..0xA0u16 {
            self.ppu.oam[i as usize] = self.read_raw(src + i);
        }
    }
}

impl GbBus for DmgBus {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0xFF,
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg | 0xE0,
            0xFF40..=0xFF4B => self.ppu.read_register(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF => self.cart.write(addr, val),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, val),
            0xA000..=0xBFFF => self.cart.write(addr, val),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = val,
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = val,
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, val),
            0xFEA0..=0xFEFF => {}
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.if_reg = val & 0x1F,
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.write_register(addr, val),
            0xFF46 => self.do_oam_dma(val),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        DmgBus::tick(self, m_cycles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::cartridge::load_cartridge;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Build a minimal valid ROM-only .gb cartridge for bus tests.
    fn rom_only_cart() -> Box<dyn GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        // compute header checksum
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn make_bus() -> DmgBus {
        DmgBus::new(rom_only_cart())
    }

    // ── VRAM ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_vram_read_write_round_trip() {
        // Given: DmgBus; When: write 0xAB to $8100; Then: read back 0xAB
        let mut bus = make_bus();
        bus.write(0x8100, 0xAB);
        assert_eq!(bus.read(0x8100), 0xAB);
    }

    #[test]
    fn test_vram_boundary_values_accessible() {
        let mut bus = make_bus();
        bus.write(0x8000, 0x11);
        bus.write(0x9FFF, 0x22);
        assert_eq!(bus.read(0x8000), 0x11);
        assert_eq!(bus.read(0x9FFF), 0x22);
    }

    // ── WRAM ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_wram_read_write_round_trip() {
        let mut bus = make_bus();
        bus.write(0xC100, 0x55);
        assert_eq!(bus.read(0xC100), 0x55);
    }

    #[test]
    fn test_wram_boundary_values_accessible() {
        let mut bus = make_bus();
        bus.write(0xC000, 0x01);
        bus.write(0xDFFF, 0x02);
        assert_eq!(bus.read(0xC000), 0x01);
        assert_eq!(bus.read(0xDFFF), 0x02);
    }

    // ── Echo RAM ──────────────────────────────────────────────────────────────

    #[test]
    fn test_echo_ram_mirrors_wram() {
        // Given: write to WRAM; Then: reading via echo range returns same value
        let mut bus = make_bus();
        bus.write(0xC000, 0x77);
        assert_eq!(bus.read(0xE000), 0x77);
    }

    #[test]
    fn test_echo_ram_write_mirrors_to_wram() {
        // Given: write to echo range; Then: reading via WRAM returns same value
        let mut bus = make_bus();
        bus.write(0xE100, 0x88);
        assert_eq!(bus.read(0xC100), 0x88);
    }

    // ── OAM ──────────────────────────────────────────────────────────────────

    /// Tick the bus enough M-cycles to reach VBlank (scanline 144).
    /// At that point the PPU enters Mode 1 and both OAM and VRAM are accessible.
    fn tick_to_vblank(bus: &mut DmgBus) {
        // VBlank starts at scanline 144 = 456*144 dots = 16416 M-cycles.
        bus.tick(255); // tick in chunks to avoid overflow
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(255);
        bus.tick(201); // 64*255 + 201 = 16320 + 201 = 16521 >= 16416 M-cycles
    }

    #[test]
    fn test_oam_read_write_round_trip() {
        // OAM is blocked during Mode 2 (startup); tick to VBlank (Mode 1) first.
        let mut bus = make_bus();
        tick_to_vblank(&mut bus);
        bus.write(0xFE00, 0x33);
        bus.write(0xFE9F, 0x44);
        assert_eq!(bus.read(0xFE00), 0x33);
        assert_eq!(bus.read(0xFE9F), 0x44);
    }

    // ── PPU registers ─────────────────────────────────────────────────────────

    #[test]
    fn test_ppu_lcdc_register_accessible_via_bus() {
        let mut bus = make_bus();
        bus.write(0xFF40, 0x00);
        assert_eq!(bus.read(0xFF40), 0x00);
    }

    #[test]
    fn test_ppu_stat_register_reflects_mode() {
        // At startup, PPU is in Mode 2 (OAM Scan); STAT bits 1:0 should be 0b10.
        let bus = make_bus();
        let stat = bus.ppu.read_register(0xFF41);
        assert_eq!(
            stat & 0x03,
            0x02,
            "initial STAT mode should be OAM Scan (2)"
        );
    }

    // ── OAM DMA ───────────────────────────────────────────────────────────────

    #[test]
    fn test_oam_dma_copies_160_bytes_from_source() {
        let mut bus = make_bus();
        // Put known data in WRAM starting at $C000.
        for i in 0..160u16 {
            bus.write(0xC000 + i, i as u8);
        }
        // Trigger OAM DMA from $C000 (val = 0xC0).
        bus.write(0xFF46, 0xC0);
        // Tick to VBlank so OAM is readable.
        tick_to_vblank(&mut bus);
        for i in 0..160u16 {
            assert_eq!(
                bus.read(0xFE00 + i),
                i as u8,
                "OAM byte {i} should match DMA source"
            );
        }
    }

    // ── VBlank interrupt ──────────────────────────────────────────────────────

    #[test]
    fn test_vblank_interrupt_propagates_to_if_after_tick() {
        let mut bus = make_bus();
        tick_to_vblank(&mut bus);
        assert_eq!(
            bus.read(0xFF0F) & 0x01,
            0x01,
            "VBlank interrupt should be set in IF"
        );
    }

    // ── Forbidden ─────────────────────────────────────────────────────────────

    #[test]
    fn test_forbidden_region_reads_return_0xff() {
        let mut bus = make_bus();
        assert_eq!(bus.read(0xFEA0), 0xFF);
        assert_eq!(bus.read(0xFEFF), 0xFF);
    }

    // ── HRAM ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_hram_read_write_round_trip() {
        let mut bus = make_bus();
        bus.write(0xFF80, 0x12);
        bus.write(0xFFFE, 0x34);
        assert_eq!(bus.read(0xFF80), 0x12);
        assert_eq!(bus.read(0xFFFE), 0x34);
    }

    // ── IE register ──────────────────────────────────────────────────────────

    #[test]
    fn test_ie_register_read_write() {
        let mut bus = make_bus();
        bus.write(0xFFFF, 0x1F);
        assert_eq!(bus.read(0xFFFF), 0x1F);
    }

    // ── I/O stubs ────────────────────────────────────────────────────────────

    #[test]
    fn test_unmapped_io_reads_return_0xff() {
        let mut bus = make_bus();
        // $FF00 (joypad stub), $FF01 (serial), $FF08 (unmapped)
        assert_eq!(bus.read(0xFF00), 0xFF);
        assert_eq!(bus.read(0xFF01), 0xFF);
        assert_eq!(bus.read(0xFF08), 0xFF);
    }

    // ── IF register ──────────────────────────────────────────────────────────

    #[test]
    fn test_if_register_read_write() {
        // Given: write valid interrupt bits; Then: read back with upper 3 bits as 1
        let mut bus = make_bus();
        bus.write(0xFF0F, 0x05); // bits 0 and 2
        // Upper 3 bits ($E0) always read as 1 per Pan Docs open-bus behavior
        assert_eq!(bus.read(0xFF0F), 0xE5);
    }

    #[test]
    fn test_if_upper_bits_always_read_as_1() {
        // Given: clear IF; Then: upper 3 bits still read as 1
        let mut bus = make_bus();
        bus.write(0xFF0F, 0x00);
        assert_eq!(bus.read(0xFF0F), 0xE0);
    }

    #[test]
    fn test_if_write_ignores_upper_3_bits() {
        // Given: write 0xFF to IF; Then: upper bits not stored, lower 5 bits + open-bus = 0xFF
        let mut bus = make_bus();
        bus.write(0xFF0F, 0xFF);
        assert_eq!(bus.read(0xFF0F), 0xFF); // 0xE0 | 0x1F = 0xFF
    }

    // ── Timer routing ─────────────────────────────────────────────────────────

    #[test]
    fn test_timer_div_register_readable_via_bus() {
        // Given: fresh bus with DIV = 0; When: read $FF04; Then: returns 0
        let mut bus = make_bus();
        assert_eq!(bus.read(0xFF04), 0x00);
    }

    #[test]
    fn test_timer_div_reset_on_write() {
        // Given: advance timer so DIV is non-zero; When: write to $FF04; Then: DIV returns 0
        let mut bus = make_bus();
        // Advance enough M-cycles to make DIV non-zero (DIV increments every 64 M-cycles)
        bus.tick(64);
        let div_before = bus.read(0xFF04);
        assert!(div_before > 0, "DIV should be non-zero after 64 M-cycles");
        bus.write(0xFF04, 0x00); // any write resets DIV
        assert_eq!(bus.read(0xFF04), 0x00);
    }

    #[test]
    fn test_timer_tima_write_readable_via_bus() {
        let mut bus = make_bus();
        bus.write(0xFF05, 0x42);
        assert_eq!(bus.read(0xFF05), 0x42);
    }

    #[test]
    fn test_timer_interrupt_propagates_to_if_register() {
        // Given: configure timer to overflow quickly; When: tick enough; Then: IF bit 2 set
        let mut bus = make_bus();
        // Enable timer, fastest clock (TAC = 0b101 = timer on, 16 T-cycles per tick)
        bus.write(0xFF07, 0x05); // TAC: enable + clock select 01 (16 T-cycles)
        // Set TIMA to 0xFF so it overflows on the next tick
        bus.write(0xFF05, 0xFF);
        // Tick 4 M-cycles = 16 T-cycles → TIMA should overflow → interrupt_pending
        bus.tick(4);
        // IF bit 2 (timer interrupt) should be set
        assert_eq!(bus.read(0xFF0F) & 0x04, 0x04);
    }
}
