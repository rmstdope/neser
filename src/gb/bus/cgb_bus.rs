use crate::gb::apu::Apu;
use crate::gb::bus::GbBus;
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
/// - Double-speed, WRAM banking, and HDMA are **not** implemented (not required for
///   cgb-acid2).
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
        self.ppu = Ppu::new_cgb();
        self.ppu.write_register(0xFF40, 0x00);
        self.timer = Timer::new();
        self.joypad = Joypad::new();
        self.apu = Apu::new(self.cart.is_cgb());
        self.wram = [0u8; 0x2000];
        self.hram = [0u8; 0x7F];
        self.if_reg = 0;
        self.ie_reg = 0;
        self.dma_active = false;
        self.dma_source = 0;
        self.dma_position = 0;
        self.dma_oam_blocked = false;
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
    pub fn restore_bus_state(&mut self, state: &crate::gb::console::save_state::BusState) {
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
}
