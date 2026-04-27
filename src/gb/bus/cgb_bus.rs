use crate::gb::apu::Apu;
use crate::gb::bus::GbBus;
use crate::gb::bus::hdma::{HdmaAction, HdmaState};
use crate::gb::cartridge::GbCartridge;
use crate::gb::input::joypad::Joypad;
use crate::gb::ppu::Ppu;
use crate::gb::timer::Timer;

/// Full CGB (Game Boy Color) memory bus.
///
/// Implements the CGB memory map for use with the generic `Gb<CgbBus>` console.
/// Supports all CGB-specific PPU registers: VRAM bank (`$FF4F`), color palettes
/// (`$FF68`–`$FF6B`), and object priority mode (`$FF6C`).
///
/// Differences from DMG:
/// - No boot ROM: starts execution at `$0100` with the CGB post-boot CPU state (A=$11).
/// - CGB-mode PPU (`Ppu::new_cgb()`): VRAM bank 1, color palette RAM.
/// - `$FF4F`, `$FF68`–`$FF6B`, `$FF6C` route to CGB PPU helpers.
/// - `$FF6C` OPRI: object priority mode register.
/// - Double-speed, WRAM banking are **not** implemented (not required for
///   cgb-acid2).
///
/// HDMA ($FF51–$FF55) supports both GDMA (immediate bulk transfer) and
/// HDMA (HBlank DMA: 16 bytes per HBlank, synchronized with PPU Mode 3→0).
///
/// Memory map (same as DMG unless noted):
/// - `$0000–$7FFF`: Cartridge ROM
/// - `$8000–$9FFF`: VRAM (bank-switched in CGB mode via `$FF4F`)
/// - `$A000–$BFFF`: Cartridge RAM
/// - `$C000–$DFFF`: WRAM (single bank)
/// - `$E000–$FDFF`: Echo RAM
/// - `$FE00–$FE9F`: OAM
/// - `$FF40–$FF4B`: PPU registers (same as DMG)
/// - `$FF4F`:        VBK — VRAM bank select
/// - `$FF68–$FF6B`:  BCPS/BCPD/OCPS/OCPD — color palette registers
/// - `$FF6C`:        OPRI — object priority mode
/// - `$FF80–$FFFE`:  HRAM
/// - `$FFFF`:        IE register
pub struct CgbBus {
    cart: Box<dyn GbCartridge>,
    pub ppu: Ppu,
    wram: [u8; 0x2000],
    hram: [u8; 0x7F],
    timer: Timer,
    pub joypad: Joypad,
    apu: Apu,
    if_reg: u8,
    ie_reg: u8,
    /// Whether an OAM DMA transfer is currently in progress.
    dma_active: bool,
    /// High byte of the OAM DMA source address.
    dma_source: u8,
    /// DMA position: 0=warm-up, 1–160=copy, 161=teardown.
    dma_position: u8,
    /// Whether OAM access is blocked by an active DMA transfer.
    dma_oam_blocked: bool,
    /// CGB VRAM DMA (HDMA/GDMA) state for registers $FF51–$FF55.
    hdma: HdmaState,
}

impl CgbBus {
    /// Create a new CGB bus, starting at `$0100` (post-boot-ROM entry).
    ///
    /// The PPU is initialised in CGB mode with the LCD disabled.  Callers are
    /// expected to call `Sm83::reset_registers_cgb()` on the CPU to set A=$11
    /// and all other registers to the CGB post-boot-ROM state.
    pub fn new(cart: Box<dyn GbCartridge>) -> Self {
        let is_cgb = cart.is_cgb();
        let mut bus = Self {
            cart,
            ppu: Ppu::new_cgb(),
            wram: [0u8; 0x2000],
            hram: [0u8; 0x7F],
            timer: Timer::new(),
            joypad: Joypad::new(),
            apu: Apu::new(is_cgb),
            if_reg: 0,
            ie_reg: 0,
            dma_active: false,
            dma_source: 0,
            dma_position: 0,
            dma_oam_blocked: false,
            hdma: HdmaState::new(),
        };
        // Start with LCD disabled; the cartridge code will enable it.
        bus.ppu.write_register(0xFF40, 0x00);
        bus
    }

    /// Advance system timers, PPU, and APU by `m_cycles` M-cycles.
    pub fn tick(&mut self, m_cycles: u8) {
        self.if_reg |= self.ppu.take_pending_interrupts();

        for _ in 0..m_cycles {
            self.timer.tick(1);
            if self.timer.interrupt_pending {
                self.if_reg |= 0x04;
                self.timer.interrupt_pending = false;
            }

            if self.dma_active {
                match self.dma_position {
                    0 => {
                        self.dma_position = 1;
                    }
                    1..=160 => {
                        self.dma_oam_blocked = true;
                        let byte_idx = (self.dma_position - 1) as u16;
                        let src = (self.dma_source as u16) << 8 | byte_idx;
                        self.ppu.oam[byte_idx as usize] = self.read_raw(src);
                        self.dma_position += 1;
                    }
                    161 => {
                        self.dma_active = false;
                        self.dma_oam_blocked = false;
                    }
                    _ => unreachable!(),
                }
            }
        }
        self.ppu.tick_dots(u32::from(m_cycles) * 4);
        self.apu.tick(m_cycles);

        // HDMA: transfer one 16-byte block per HBlank (Mode 3→0).
        if self.hdma.is_active() && self.hdma.is_hblank_mode() && self.ppu.take_hblank_entered() {
            self.do_hdma_block_transfer();
            // Tick subsystems forward by 8 M-cycles (the transfer duration).
            self.tick_subsystems_for_hdma(8);
        }
    }

    /// Execute one HDMA block transfer (16 bytes from source to VRAM).
    fn do_hdma_block_transfer(&mut self) {
        let vbk = self.ppu.vbk;
        let source = self.hdma.source();
        let dest = self.hdma.destination();

        // Transfer 16 bytes: read from source via read_raw, write directly to VRAM.
        for i in 0u16..16 {
            let byte = self.read_raw(source.wrapping_add(i));
            let vram_offset = dest.wrapping_add(i) as usize;
            if vram_offset < 0x2000 {
                if vbk & 0x01 != 0 {
                    self.ppu.vram_bank1[vram_offset] = byte;
                } else {
                    self.ppu.vram[vram_offset] = byte;
                }
            }
        }

        // Advance addresses and decrement remaining blocks.
        self.hdma.advance_after_block();
    }

    /// Execute a GDMA (General-Purpose DMA) — transfer all blocks at once.
    /// Called when $FF55 is written with bit 7 = 0 and no HDMA is active.
    fn do_gdma_transfer(&mut self) {
        let total_blocks = self.hdma.remaining_blocks() as u32 + 1;

        for _ in 0..total_blocks {
            self.do_hdma_block_transfer();
        }

        // Tick subsystems forward by 8 M-cycles per block.
        self.tick_subsystems_for_hdma(8 * total_blocks);
    }

    /// Tick PPU, timer, and APU forward (used during HDMA/GDMA transfers).
    fn tick_subsystems_for_hdma(&mut self, m_cycles: u32) {
        for _ in 0..m_cycles {
            self.timer.tick(1);
            if self.timer.interrupt_pending {
                self.if_reg |= 0x04;
                self.timer.interrupt_pending = false;
            }
        }
        self.ppu.tick_dots(m_cycles * 4);
        self.if_reg |= self.ppu.take_pending_interrupts();
        // APU tick takes u8, so tick in chunks for large GDMA transfers.
        let mut remaining = m_cycles;
        while remaining > 0 {
            let chunk = remaining.min(255) as u8;
            self.apu.tick(chunk);
            remaining -= u32::from(chunk);
        }
    }

    /// Raw read bypassing PPU access blocking (used by OAM DMA).
    fn read_raw(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => {
                let vram_addr = (addr - 0x8000) as usize;
                if self.ppu.vbk & 0x01 != 0 {
                    self.ppu.vram_bank1[vram_addr]
                } else {
                    self.ppu.vram[vram_addr]
                }
            }
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFFFF => self.wram[(addr - 0xE000) as usize],
        }
    }

    fn do_oam_dma(&mut self, val: u8) {
        let preserve_blocking = self.dma_active && self.dma_oam_blocked;
        self.dma_active = true;
        self.dma_source = val;
        self.dma_position = 0;
        self.dma_oam_blocked = preserve_blocking;
    }

    /// Returns bytes captured via serial transfer ($FF01/$FF02).
    /// CGB bus accepts serial writes but discards the data (no test harness needed).
    pub fn serial_output(&self) -> &[u8] {
        &[]
    }

    /// Set a button state on the joypad and propagate any resulting interrupt.
    pub fn set_joypad_button(&mut self, id: u8, pressed: bool) {
        if self.joypad.set_button(id, pressed) {
            self.if_reg |= 0x10;
        }
    }

    /// Returns `true` when the PPU has completed a full frame.
    pub fn is_frame_ready(&self) -> bool {
        self.ppu.is_frame_ready()
    }

    /// Clear the frame-ready flag.
    pub fn clear_frame_ready(&mut self) {
        self.ppu.clear_frame_ready();
    }

    /// Returns `true` when the APU has a sample ready to retrieve.
    pub fn sample_ready(&self) -> bool {
        self.apu.sample_ready()
    }

    /// Consume and return the next audio sample, or `None` if not ready.
    pub fn take_sample(&mut self) -> Option<f32> {
        self.apu.take_sample()
    }

    /// Set the APU output sample rate in Hz.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        self.apu.set_sample_rate(rate);
    }

    /// Reset bus state (PPU, timer, joypad, APU, RAM, DMA).
    pub fn reset(&mut self) {
        let apu_rate = self.apu.sample_rate();
        self.ppu = Ppu::new_cgb();
        self.ppu.write_register(0xFF40, 0x00);
        self.timer = Timer::new();
        self.joypad = Joypad::new();
        self.apu = Apu::new(self.cart.is_cgb());
        self.apu.set_sample_rate(apu_rate);
        self.wram = [0u8; 0x2000];
        self.hram = [0u8; 0x7F];
        self.if_reg = 0;
        self.ie_reg = 0;
        self.dma_active = false;
        self.dma_source = 0;
        self.dma_position = 0;
        self.dma_oam_blocked = false;
        self.hdma = HdmaState::new();
    }

    // ── Save-state capture / restore ───────────────────────────────────────

    /// Capture the full bus state for serialization.
    pub fn capture_bus_state(&self) -> crate::gb::console::save_state::BusState {
        use crate::gb::console::save_state::{BusState, GbBusType};
        BusState {
            bus_type: GbBusType::Cgb,
            ppu: self.ppu.clone(),
            wram: self.wram,
            hram: self.hram,
            timer: self.timer.clone(),
            joypad: self.joypad.clone(),
            apu: self.apu.clone(),
            if_reg: self.if_reg,
            ie_reg: self.ie_reg,
            dma_active: self.dma_active,
            dma_source: self.dma_source,
            dma_position: self.dma_position,
            dma_oam_blocked: self.dma_oam_blocked,
            hdma: Some(self.hdma.clone()),
            boot_rom_active: None,
            sb: None,
            sc: None,
            serial_buf: None,
            serial_bits_remaining: None,
            serial_master_clock: None,
            model: None,
        }
    }

    /// Restore bus state from a deserialized snapshot.
    ///
    /// Returns an error if the save state was captured from a DMG bus.
    pub fn restore_bus_state(
        &mut self,
        state: &crate::gb::console::save_state::BusState,
    ) -> Result<(), String> {
        use crate::gb::console::save_state::GbBusType;
        if state.bus_type != GbBusType::Cgb {
            return Err(format!(
                "bus type mismatch: expected CGB, found {:?}",
                state.bus_type
            ));
        }
        self.ppu = state.ppu.clone();
        self.wram = state.wram;
        self.hram = state.hram;
        self.timer = state.timer.clone();
        self.joypad = state.joypad.clone();
        self.apu = state.apu.clone();
        self.if_reg = state.if_reg;
        self.ie_reg = state.ie_reg;
        self.dma_active = state.dma_active;
        self.dma_source = state.dma_source;
        self.dma_position = state.dma_position;
        self.dma_oam_blocked = state.dma_oam_blocked;
        self.hdma = state.hdma.clone().unwrap_or_default();
        Ok(())
    }

    /// Returns `true` when the cartridge has battery-backed RAM.
    pub fn has_battery(&self) -> bool {
        self.cart.has_battery()
    }

    /// Snapshot cartridge RAM.
    pub fn cart_ram_snapshot(&self) -> Vec<u8> {
        self.cart.ram_snapshot()
    }

    /// Restore cartridge RAM from snapshot.
    pub fn restore_cart_ram(&mut self, data: &[u8]) {
        self.cart.restore_ram(data);
    }

    /// Snapshot MBC register state.
    pub fn mbc_state_snapshot(&self) -> Vec<u8> {
        self.cart.mbc_state_snapshot()
    }

    /// Restore MBC register state from snapshot.
    pub fn restore_mbc_state(&mut self, data: &[u8]) {
        self.cart.restore_mbc_state(data);
    }
}

impl GbBus for CgbBus {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => {
                if self.dma_oam_blocked {
                    return 0xFF;
                }
                self.ppu.read_oam(addr)
            }
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00 => self.joypad.read(),
            0xFF01 => 0xFF, // SB — stub
            0xFF02 => 0xFF, // SC — stub
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
            // CGB HDMA registers
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
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
            0xFE00..=0xFE9F => {
                if !self.dma_oam_blocked {
                    self.ppu.write_oam(addr, val);
                }
            }
            0xFEA0..=0xFEFF => {}
            0xFF00 => self.joypad.write(val),
            0xFF01 | 0xFF02 => {} // SB/SC — stub
            0xFF04..=0xFF07 => {
                self.timer.write(addr, val);
                if self.timer.fire_write_overflow_if_pending() {
                    self.if_reg |= 0x04;
                    self.timer.take_interrupt();
                }
            }
            0xFF0F => self.if_reg = val & 0x1F,
            0xFF10..=0xFF3F => self.apu.write_register(addr, val),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => {
                self.ppu.write_register(addr, val);
                self.if_reg |= self.ppu.take_pending_interrupts();
            }
            0xFF46 => self.do_oam_dma(val),
            // CGB HDMA registers
            0xFF51 => self.hdma.write_source_high(val),
            0xFF52 => self.hdma.write_source_low(val),
            0xFF53 => self.hdma.write_dest_high(val),
            0xFF54 => self.hdma.write_dest_low(val),
            0xFF55 => match self.hdma.write_control(val) {
                HdmaAction::StartGdma => self.do_gdma_transfer(),
                HdmaAction::StartHdma | HdmaAction::CancelHdma => {}
            },
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => {
                self.ppu.write_cgb_register(addr, val);
            }
            0xFF50 => {} // No boot ROM to unmap on CGB bus
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {}
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        CgbBus::tick(self, m_cycles);
    }

    fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.ppu
    }

    fn read_for_debugger(&self, addr: u16) -> u8 {
        // Debugger reads mirror normal read() address decoding (including register
        // readback behavior like `if_reg | 0xE0`) but avoid side effects such as
        // OAM corruption.
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => {
                if self.dma_oam_blocked {
                    return 0xFF;
                }
                // Direct OAM read to avoid OAM corruption side effects that
                // read_oam() triggers during Mode 2 (debugger reads must be
                // side-effect-free).
                self.ppu.oam[(addr - 0xFE00) as usize]
            }
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00 => self.joypad.read(),
            0xFF01 => 0xFF, // SB — stub
            0xFF02 => 0xFF, // SC — stub
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
            // CGB HDMA registers
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => 0xFF,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::cartridge::load_cartridge;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Build a minimal CGB ROM-only cartridge.
    fn cgb_rom_only_cart() -> Box<dyn GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0x80; // CGB compatible
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    /// Build a CGB ROM-only cartridge with specific data at given addresses.
    fn cgb_rom_with_data(data: &[(u16, u8)]) -> Box<dyn GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0x80; // CGB compatible
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        for &(addr, val) in data {
            rom[addr as usize] = val;
        }
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        load_cartridge(&rom).expect("valid ROM")
    }

    fn make_bus() -> CgbBus {
        CgbBus::new(cgb_rom_only_cart())
    }

    /// Enable LCD (needed for PPU to tick and reach HBlank).
    fn enable_lcd(bus: &mut CgbBus) {
        bus.write(0xFF40, 0x91); // LCD on, BG enabled
    }

    // ── HDMA register read/write through bus ─────────────────────────────────

    #[test]
    fn test_hdma_source_registers_are_write_only() {
        // Given: CgbBus
        let mut bus = make_bus();
        // When: write to $FF51/$FF52, then read
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x50);
        // Then: reads return $FF (write-only)
        assert_eq!(bus.read(0xFF51), 0xFF);
        assert_eq!(bus.read(0xFF52), 0xFF);
    }

    #[test]
    fn test_hdma_dest_registers_are_write_only() {
        // Given: CgbBus
        let mut bus = make_bus();
        // When: write to $FF53/$FF54, then read
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        // Then: reads return $FF (write-only)
        assert_eq!(bus.read(0xFF53), 0xFF);
        assert_eq!(bus.read(0xFF54), 0xFF);
    }

    #[test]
    fn test_hdma5_read_ff_when_inactive() {
        // Given: CgbBus with no transfer started
        let mut bus = make_bus();
        // Then: $FF55 reads $FF (inactive)
        assert_eq!(bus.read(0xFF55), 0xFF);
    }

    // ── GDMA tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_gdma_transfers_data_from_wram_to_vram() {
        // Given: CgbBus with data in WRAM at $C000-$C00F
        let mut bus = make_bus();
        for i in 0u8..16 {
            bus.write(0xC000 + i as u16, i + 1);
        }
        // Configure HDMA: source=$C000, dest=$8000, length=0 (1 block of 16 bytes)
        bus.write(0xFF51, 0xC0); // Source high
        bus.write(0xFF52, 0x00); // Source low
        bus.write(0xFF53, 0x80); // Dest high ($8000)
        bus.write(0xFF54, 0x00); // Dest low
        // When: trigger GDMA (bit 7=0, length=0)
        bus.write(0xFF55, 0x00);
        // Then: VRAM at $8000-$800F contains the transferred data
        // Read directly from PPU VRAM (bypass PPU blocking)
        for i in 0u8..16 {
            assert_eq!(
                bus.ppu.vram[i as usize],
                i + 1,
                "VRAM byte {} should be {}",
                i,
                i + 1
            );
        }
        // And $FF55 reads $FF (transfer complete)
        assert_eq!(bus.read(0xFF55), 0xFF);
    }

    #[test]
    fn test_gdma_transfers_multiple_blocks() {
        // Given: 2 blocks (32 bytes) of data in WRAM
        let mut bus = make_bus();
        for i in 0u8..32 {
            bus.write(0xC000 + i as u16, i + 1);
        }
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        // When: GDMA with length=1 (2 blocks)
        bus.write(0xFF55, 0x01);
        // Then: 32 bytes transferred
        for i in 0u8..32 {
            assert_eq!(bus.ppu.vram[i as usize], i + 1);
        }
        assert_eq!(bus.read(0xFF55), 0xFF);
    }

    #[test]
    fn test_gdma_transfers_from_rom_to_vram() {
        // Given: ROM data at $0100-$010F
        let data: Vec<(u16, u8)> = (0..16).map(|i| (0x0100 + i as u16, 0xA0 + i)).collect();
        let cart = cgb_rom_with_data(&data);
        let mut bus = CgbBus::new(cart);
        // Configure HDMA: source=$0100, dest=$8000, 1 block
        bus.write(0xFF51, 0x01); // Source high
        bus.write(0xFF52, 0x00); // Source low
        bus.write(0xFF53, 0x80); // Dest high
        bus.write(0xFF54, 0x00); // Dest low
        // When: GDMA
        bus.write(0xFF55, 0x00);
        // Then: data transferred from ROM to VRAM
        for i in 0u8..16 {
            assert_eq!(bus.ppu.vram[i as usize], 0xA0 + i);
        }
    }

    #[test]
    fn test_gdma_respects_vram_bank_selection() {
        // Given: VBK=1 (VRAM bank 1), data in WRAM
        let mut bus = make_bus();
        for i in 0u8..16 {
            bus.write(0xC000 + i as u16, 0xBB);
        }
        bus.write(0xFF4F, 0x01); // Select VRAM bank 1
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        // When: GDMA
        bus.write(0xFF55, 0x00);
        // Then: data written to VRAM bank 1
        for i in 0u8..16 {
            assert_eq!(bus.ppu.vram_bank1[i as usize], 0xBB);
        }
        // And VRAM bank 0 untouched
        for i in 0u8..16 {
            assert_eq!(bus.ppu.vram[i as usize], 0x00);
        }
    }

    // ── HDMA (HBlank DMA) tests ─────────────────────────────────────────────

    #[test]
    fn test_hdma_start_marks_active() {
        // Given: CgbBus with HDMA configured
        let mut bus = make_bus();
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        // When: start HDMA (bit 7=1, length=1 → 2 blocks)
        bus.write(0xFF55, 0x81);
        // Then: $FF55 reads 0x01 (active, 2 blocks remaining)
        assert_eq!(bus.read(0xFF55), 0x01);
    }

    #[test]
    fn test_hdma_transfers_on_hblank() {
        // Given: HDMA configured with source=$C000, dest=$8000, 1 block
        let mut bus = make_bus();
        enable_lcd(&mut bus);
        for i in 0u8..16 {
            bus.write(0xC000 + i as u16, 0x50 + i);
        }
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        bus.write(0xFF55, 0x80); // HDMA, 1 block (length=0)

        // When: tick enough to reach HBlank on the first scanline
        // First scanline: Mode 3→0 at dot 256, so 252 dots = 63 M-cycles from LCD enable
        for _ in 0..64 {
            bus.tick(1);
        }

        // Then: after HBlank, 16 bytes should be transferred to VRAM
        for i in 0u8..16 {
            assert_eq!(
                bus.ppu.vram[i as usize],
                0x50 + i,
                "VRAM byte {} should be transferred",
                i
            );
        }
        // And transfer should be complete ($FF55 = $FF)
        assert_eq!(bus.read(0xFF55), 0xFF);
    }

    #[test]
    fn test_hdma_cancel_stops_transfer() {
        // Given: active HDMA with 3 blocks
        let mut bus = make_bus();
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        bus.write(0xFF55, 0x82); // HDMA, 3 blocks (remaining=2)

        // When: cancel by writing bit 7=0 to $FF55
        bus.write(0xFF55, 0x00);

        // Then: $FF55 reads $FF (inactive)
        assert_eq!(bus.read(0xFF55), 0xFF);
    }

    // ── Debugger read ───────────────────────────────────────────────────────

    #[test]
    fn test_debugger_reads_hdma5() {
        // Given: active HDMA
        let mut bus = make_bus();
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        bus.write(0xFF55, 0x83); // HDMA, 4 blocks
        // Then: debugger read matches normal read
        assert_eq!(bus.read_for_debugger(0xFF55), bus.read(0xFF55));
    }
}
