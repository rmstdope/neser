use crate::gb::boot_rom::DMG_BOOT_ROM;
use crate::gb::bus::GbBus;
use crate::gb::cartridge::GbCartridge;
use crate::gb::input::joypad::Joypad;
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
/// - $FF00:       Joypad (P1 register)
/// - Everything else in $FF01–$FF7F: I/O stubs (reads return 0xFF)
pub struct DmgBus {
    cart: Box<dyn GbCartridge>,
    pub ppu: Ppu,
    wram: [u8; 0x2000],
    hram: [u8; 0x7F],
    timer: Timer,
    pub joypad: Joypad,
    /// IF register ($FF0F): interrupt flag.
    if_reg: u8,
    /// IE register ($FFFF): interrupt enable.
    ie_reg: u8,
    /// Boot ROM contents (256 bytes).
    boot_rom: [u8; 256],
    /// When `true`, reads from $0000–$00FF are satisfied by `boot_rom`
    /// instead of the cartridge.  Writing any value to $FF50 sets this
    /// to `false` (mirrors real DMG hardware behaviour).
    boot_rom_active: bool,
    /// $FF01 Serial Data Register (SB).
    sb: u8,
    /// $FF02 Serial Control Register (SC).
    sc: u8,
    /// Bytes captured via serial transfer (written by ROM via SB/SC).
    serial_buf: Vec<u8>,
    /// OAM scan row captured at the start of the currently-executing instruction
    /// (before M1's bus tick).  Used by `notify_idu_glitch` to check whether the
    /// IDU glitch triggers OAM corruption with the correct pre-instruction PPU state.
    saved_oam_row: Option<usize>,
}

impl DmgBus {
    pub fn new(cart: Box<dyn GbCartridge>) -> Self {
        let mut bus = Self {
            cart,
            ppu: Ppu::new(),
            wram: [0u8; 0x2000],
            hram: [0u8; 0x7F],
            timer: Timer::new(),
            joypad: Joypad::new(),
            if_reg: 0,
            ie_reg: 0,
            boot_rom: DMG_BOOT_ROM,
            boot_rom_active: true,
            sb: 0xFF,
            sc: 0x7E,
            serial_buf: Vec::new(),
            saved_oam_row: None,
        };
        // Real DMG hardware powers on with LCDC=$00 (LCD disabled).
        // The boot ROM tile-loading runs while the LCD is off so VRAM writes
        // are never blocked by Mode 3; our boot ROM explicitly re-enables the
        // LCD (LCDC=$91) just before starting the scroll animation.
        bus.ppu.write_register(0xFF40, 0x00);
        bus
    }

    /// Reset all bus state to power-on defaults.
    ///
    /// Reinitialises the PPU, timer, and joypad; zeroes WRAM and HRAM;
    /// clears IF and IE. The cartridge is not touched by this reset, so
    /// ROM, cartridge RAM, and any mapper state are preserved.
    pub fn reset(&mut self) {
        self.ppu = Ppu::new();
        self.ppu.write_register(0xFF40, 0x00); // power-on: LCD disabled
        self.timer = Timer::new();
        self.joypad = Joypad::new();
        self.wram = [0u8; 0x2000];
        self.hram = [0u8; 0x7F];
        self.if_reg = 0;
        self.ie_reg = 0;
        self.boot_rom_active = true;
        self.sb = 0xFF;
        self.sc = 0x7E;
        self.serial_buf.clear();
    }

    /// Returns `true` while the boot ROM is still mapped at $0000–$00FF.
    pub fn is_boot_rom_active(&self) -> bool {
        self.boot_rom_active
    }

    /// Returns bytes captured via serial transfer ($FF01/$FF02).
    ///
    /// Each byte pushed by the ROM via `SB`/`SC` appears here in order.
    pub fn serial_output(&self) -> &[u8] {
        &self.serial_buf
    }

    /// Set a button state on the joypad and propagate any resulting interrupt.
    ///
    /// Sets IF bit 4 (joypad interrupt) when pressing a button in the
    /// currently selected group causes the effective nibble to transition
    /// from all-ones to any-zero.
    pub fn set_joypad_button(&mut self, id: u8, pressed: bool) {
        if self.joypad.set_button(id, pressed) {
            self.if_reg |= 0x10;
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
        if self.boot_rom_active && addr <= 0x00FF {
            return self.boot_rom[addr as usize];
        }
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
        if self.boot_rom_active && addr <= 0x00FF {
            return self.boot_rom[addr as usize];
        }
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00 => self.joypad.read(),
            0xFF01 => self.sb,
            0xFF02 => self.sc,
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
            0xFF00 => self.joypad.write(val),
            0xFF01 => self.sb = val,
            0xFF02 => {
                self.sc = val;
                if val & 0x80 != 0 {
                    // Internal clock transfer: capture SB, fire serial interrupt, clear transfer flag
                    self.serial_buf.push(self.sb);
                    self.if_reg |= 0x08;
                    self.sc &= 0x7F;
                }
            }
            0xFF04..=0xFF07 => self.timer.write(addr, val),
            0xFF0F => self.if_reg = val & 0x1F,
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.write_register(addr, val),
            0xFF46 => self.do_oam_dma(val),
            0xFF50 => self.boot_rom_active = false,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        DmgBus::tick(self, m_cycles);
    }

    fn begin_instruction(&mut self) {
        // Snapshot the OAM-scan row that is active at the very start of the
        // instruction (before the M1 bus tick advances the PPU).  Used by
        // notify_idu_glitch to determine the correct corruption row.
        self.saved_oam_row = self.ppu.current_oam_row();
    }

    fn notify_idu_glitch(&mut self, addr: u16) {
        if matches!(addr, 0xFE00..=0xFEFF)
            && let Some(row) = self.saved_oam_row
            && (1..=16).contains(&row)
        {
            self.ppu.apply_oam_write_corruption(row);
        }
    }

    fn notify_idu_with_prior_read(&mut self, addr: u16) {
        if matches!(addr, 0xFE00..=0xFEFF)
            && let Some(row) = self.ppu.current_oam_row()
        {
            self.ppu.apply_oam_read_idu_corruption(row);
        }
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
        // LCD is off at power-on; enable it so the PPU can advance to VBlank.
        if bus.read(0xFF40) & 0x80 == 0 {
            bus.write(0xFF40, 0x91);
        }
        // VBlank starts at scanline 144 = 456*144 dots = 16416 M-cycles.
        // Tick in chunks to avoid overflow in the bus tick path.
        let mut remaining = 16_416u32;
        while remaining > 0 {
            let chunk = remaining.min(255) as u8;
            bus.tick(chunk);
            remaining -= u32::from(chunk);
        }
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
        // At hardware power-on the LCD is disabled; STAT mode bits report 0.
        let mut bus = make_bus();
        assert_eq!(
            bus.ppu.read_register(0xFF41) & 0x03,
            0x00,
            "STAT mode should be 0 while LCD is off"
        );
        // After enabling the LCD the PPU resets to the first scanline after enable,
        // which reports Mode 0 (HBlank) instead of Mode 2 (OAM Scan).
        bus.write(0xFF40, 0x91);
        let stat = bus.ppu.read_register(0xFF41);
        assert_eq!(
            stat & 0x03,
            0x00,
            "STAT mode should be HBlank (0) on first scanline after LCD enable"
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
        // $FF01 (serial), $FF08 (unmapped) — $FF00 is now the joypad register
        assert_eq!(bus.read(0xFF01), 0xFF);
        assert_eq!(bus.read(0xFF08), 0xFF);
    }

    // ── Joypad ($FF00) ────────────────────────────────────────────────────────

    #[test]
    fn test_ff00_read_reflects_joypad_state_no_buttons_pressed() {
        // Given: fresh bus (neither group selected = default after new)
        let mut bus = make_bus();
        // When: write 0x10 to $FF00 (select P15 / action group)
        bus.write(0xFF00, 0x10);
        // Then: read reflects select bits and all-released nibble = 0xDF
        // 0xC0 | 0x10 | 0x0F = 0xDF
        assert_eq!(bus.read(0xFF00), 0xDF);
    }

    #[test]
    fn test_ff00_write_updates_select_and_read_reflects_it() {
        // Given: select P14 (direction group)
        let mut bus = make_bus();
        bus.write(0xFF00, 0x20);
        // Then: bits 5-4 of read = 0x20
        assert_eq!(bus.read(0xFF00) & 0x30, 0x20);
    }

    #[test]
    fn test_set_joypad_button_affects_ff00_read() {
        // Given: P15 selected
        let mut bus = make_bus();
        bus.write(0xFF00, 0x10); // select P15 (action buttons)
        // When: press A (id=0)
        bus.set_joypad_button(0, true);
        // Then: bit0 of lower nibble is 0 (active-low) → read = 0xDE
        assert_eq!(bus.read(0xFF00), 0xDE);
    }

    #[test]
    fn test_set_joypad_button_sets_if_bit4_on_first_press() {
        // Given: P15 selected
        let mut bus = make_bus();
        bus.write(0xFF00, 0x10); // select P15
        // When: press A (first button — nibble transitions 0xF → non-0xF)
        bus.set_joypad_button(0, true);
        // Then: IF bit 4 (joypad interrupt) is set
        assert_eq!(bus.read(0xFF0F) & 0x10, 0x10, "IF bit 4 should be set");
    }

    #[test]
    fn test_set_joypad_button_no_if_when_group_not_selected() {
        // Given: neither group selected (default)
        let mut bus = make_bus();
        // When: press A — action group not selected, no IRQ
        bus.set_joypad_button(0, true);
        // Then: IF bit 4 not set
        assert_eq!(bus.read(0xFF0F) & 0x10, 0x00, "IF bit 4 should NOT be set");
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

    // ── Serial port ($FF01 SB / $FF02 SC) ────────────────────────────────────

    #[test]
    fn test_serial_sb_write_read_roundtrip() {
        // Given: fresh bus; When: write 0xAB to $FF01 (SB); Then: read back 0xAB
        let mut bus = make_bus();
        bus.write(0xFF01, 0xAB);
        assert_eq!(bus.read(0xFF01), 0xAB);
    }

    #[test]
    fn test_serial_sc_write_no_transfer_roundtrip() {
        // Given: fresh bus; When: write 0x40 (bit 7 clear, no transfer) to $FF02 (SC);
        // Then: read back 0x40
        let mut bus = make_bus();
        bus.write(0xFF02, 0x40);
        assert_eq!(bus.read(0xFF02), 0x40);
    }

    #[test]
    fn test_serial_transfer_captures_byte() {
        // Given: SB = 0x41; When: write SC = 0x81 (bit 7 set → start transfer);
        // Then: serial_output contains the SB byte 0x41
        let mut bus = make_bus();
        bus.write(0xFF01, 0x41);
        bus.write(0xFF02, 0x81);
        assert_eq!(bus.serial_output(), &[0x41]);
    }

    #[test]
    fn test_serial_transfer_sets_if_bit3() {
        // Given: trigger a serial transfer; Then: IF bit 3 (serial interrupt) is set
        let mut bus = make_bus();
        bus.write(0xFF01, 0x42);
        bus.write(0xFF02, 0x81);
        assert_eq!(bus.read(0xFF0F) & 0x08, 0x08);
    }

    #[test]
    fn test_serial_transfer_clears_sc_bit7() {
        // Given: write 0x81 to SC; Then: reading SC immediately after returns bit 7 = 0
        let mut bus = make_bus();
        bus.write(0xFF02, 0x81);
        assert_eq!(bus.read(0xFF02) & 0x80, 0x00);
    }

    // ── IDU glitch notifications (Phase C) ───────────────────────────────────

    fn set_row_words(oam: &mut [u8; 0xA0], row: usize, words: [u16; 4]) {
        let base = row * 8;
        for (i, &w) in words.iter().enumerate() {
            oam[base + i * 2] = w as u8;
            oam[base + i * 2 + 1] = (w >> 8) as u8;
        }
    }

    fn get_row_words(oam: &[u8; 0xA0], row: usize) -> [u16; 4] {
        let base = row * 8;
        [0, 1, 2, 3].map(|i| u16::from_le_bytes([oam[base + i * 2], oam[base + i * 2 + 1]]))
    }

    fn enable_lcd_and_tick_to_row(bus: &mut DmgBus, row: usize) {
        bus.write(0xFF40, 0x91); // enable LCD → timing resets
        // Skip the first scanline after LCD enable (no Mode 2; 452 dots = 113 M-cycles)
        for _ in 0..113 {
            bus.tick(1);
        }
        // Now on scanline 1 with normal Mode 2; tick to the desired OAM row
        for _ in 0..row {
            bus.tick(1); // 1 M-cycle = 4 dots = 1 OAM row
        }
    }

    #[test]
    fn test_notify_idu_glitch_applies_write_corruption_in_mode_2() {
        // Given: PPU at row 2 (dot 8); OAM rows 1 and 2 have known values.
        // row 1: b=0x0002, w1=0x0003, c=0x0004, w3=0x0005
        // row 2: a=0x0001
        // write formula: ((0x0001^0x0004)&(0x0002^0x0004))^0x0004 = 0x0000
        // Expected row 2 after: [0x0000, 0x0003, 0x0004, 0x0005]
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut bus.ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        enable_lcd_and_tick_to_row(&mut bus, 2);
        // Snapshot pre-instruction PPU state (as Sm83::execute() does).
        bus.begin_instruction();
        bus.notify_idu_glitch(0xFE10); // any addr in $FE00-$FEFF
        let row2 = get_row_words(&bus.ppu.oam, 2);
        assert_eq!(
            row2,
            [0x0000, 0x0003, 0x0004, 0x0005],
            "notify_idu_glitch in Mode 2 must apply write corruption to current OAM row"
        );
    }

    #[test]
    fn test_notify_idu_glitch_ignored_outside_oam_range() {
        // Given: PPU in Mode 2; addr outside $FE00-$FEFF → no corruption.
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        enable_lcd_and_tick_to_row(&mut bus, 2);
        let snapshot = bus.ppu.oam;
        bus.notify_idu_glitch(0xC000); // non-OAM address
        assert_eq!(
            bus.ppu.oam, snapshot,
            "IDU glitch outside OAM range must not corrupt OAM"
        );
    }

    #[test]
    fn test_notify_idu_glitch_ignored_outside_mode_2() {
        // Given: PPU in H-Blank (Mode 0); IDU glitch in OAM range → no corruption.
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        // Tick to H-Blank: 252 dots = 63 M-cycles into scanline 0
        bus.write(0xFF40, 0x91);
        bus.tick(63); // dot=252 → Mode 0 (H-Blank)
        let snapshot = bus.ppu.oam;
        bus.notify_idu_glitch(0xFE10);
        assert_eq!(
            bus.ppu.oam, snapshot,
            "IDU glitch outside Mode 2 must not corrupt OAM"
        );
    }

    #[test]
    fn test_notify_idu_with_prior_read_applies_complex_corruption_in_mode_2() {
        // Given: PPU at row 5 (dot 20); rows 3, 4, 5 have known values.
        // a=0x00A0 (row3), b=0x0055 (row4[0]), c=0x000F (row5[0]), d=0x00C0 (row4[2])
        // new_b = 0x0045; all three rows become [0x0045, 0x0011, 0x00C0, 0x0022]
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 3, [0x00A0, 0x0001, 0x0002, 0x0003]);
        set_row_words(&mut bus.ppu.oam, 4, [0x0055, 0x0011, 0x00C0, 0x0022]);
        set_row_words(&mut bus.ppu.oam, 5, [0x000F, 0x0099, 0x0088, 0x0077]);
        enable_lcd_and_tick_to_row(&mut bus, 5);
        bus.notify_idu_with_prior_read(0xFE28);
        let expected = [0x0045u16, 0x0011, 0x00C0, 0x0022];
        assert_eq!(
            get_row_words(&bus.ppu.oam, 3),
            expected,
            "notify_idu_with_prior_read: row n-2 should equal corrupted row n-1"
        );
        assert_eq!(
            get_row_words(&bus.ppu.oam, 4),
            expected,
            "notify_idu_with_prior_read: row n-1 word0 should use complex formula"
        );
        assert_eq!(
            get_row_words(&bus.ppu.oam, 5),
            expected,
            "notify_idu_with_prior_read: row n should be copied then read-corrupted"
        );
    }
}
