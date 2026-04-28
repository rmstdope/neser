use crate::gb::apu::Apu;
use crate::gb::bus::GbBus;
use crate::gb::bus::hdma::{HdmaAction, HdmaState};
use crate::gb::cartridge::GbCartridge;
use crate::gb::input::joypad::Joypad;
use crate::gb::model::CgbModel;
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
/// - WRAM banking (`$FF70` / SVBK): 8 × 4 KB banks; bank 0 at `$C000–$CFFF`,
///   switchable banks 1–7 at `$D000–$DFFF`.
/// - Double-speed mode is **not** implemented.
///
/// HDMA ($FF51–$FF55) supports both GDMA (immediate bulk transfer) and
/// HDMA (HBlank DMA: 16 bytes per HBlank, synchronized with PPU Mode 3→0).
///
/// Memory map (same as DMG unless noted):
/// - `$0000–$7FFF`: Cartridge ROM
/// - `$8000–$9FFF`: VRAM (bank-switched in CGB mode via `$FF4F`)
/// - `$A000–$BFFF`: Cartridge RAM
/// - `$C000–$CFFF`: WRAM bank 0 (fixed)
/// - `$D000–$DFFF`: WRAM banks 1–7 (switchable via `$FF70`)
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
    wram: [[u8; 0x1000]; 8],
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
    /// SVBK register ($FF70): selects the active WRAM bank for $D000–$DFFF.
    svbk: u8,
    /// KEY1 register ($FF4D): CGB speed switch.
    /// Bit 7 = current speed (0=normal, 1=double), bit 0 = switch armed.
    key1: u8,
    /// Accumulator for half-rate APU ticking in double-speed mode.
    apu_tick_accumulator: u8,
    /// Hardware model variant (CGB-0 through CGB-E).
    /// Stored for variant-specific future use (e.g., DIV counter initial state,
    /// post-boot register values). Currently not used to initialize hardware state.
    model: CgbModel,
}

impl CgbBus {
    /// Create a new CGB bus, starting at `$0100` (post-boot-ROM entry).
    ///
    /// The PPU is initialised in CGB mode with the LCD disabled. The `model`
    /// is stored for potential future use in variant-specific hardware
    /// initialization (e.g., post-boot CPU register state, DIV initial value).
    /// Callers should call `Sm83::reset_registers_cgb()` on the CPU to set
    /// the CGB post-boot register state.
    pub fn new(cart: Box<dyn GbCartridge>, model: CgbModel) -> Self {
        let is_cgb = cart.is_cgb();
        let mut bus = Self {
            cart,
            ppu: Ppu::new_cgb(),
            wram: [[0u8; 0x1000]; 8],
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
            svbk: 0,
            key1: 0,
            apu_tick_accumulator: 0,
            model,
        };
        // Start with LCD disabled; the cartridge code will enable it.
        bus.ppu.write_register(0xFF40, 0x00);
        bus
    }

    /// Returns the CGB hardware model variant for this bus.
    pub fn model(&self) -> CgbModel {
        self.model
    }

    /// Returns `true` when the CGB is operating in double-speed mode.
    pub fn is_double_speed(&self) -> bool {
        self.key1 & 0x80 != 0
    }

    /// Attempt a CGB double-speed switch.
    ///
    /// If KEY1 bit 0 is armed, this method:
    /// 1. Ticks the PPU for 2050 M-cycles (at the pre-switch dot rate),
    ///    without ticking the timer or APU (DIV is frozen during the switch).
    /// 2. Resets the DIV counter to 0.
    /// 3. Toggles the speed (KEY1 bit 7) and clears the arm bit (bit 0).
    ///
    /// Returns `true` if the switch was performed, `false` if not armed.
    pub fn try_speed_switch(&mut self) -> bool {
        if self.key1 & 0x01 == 0 {
            return false;
        }

        // Determine dots-per-M-cycle using the PRE-switch speed.
        let dots_per_mcycle: u32 = if self.is_double_speed() { 2 } else { 4 };

        // Tick PPU for 2050 M-cycles. Timer and APU are frozen.
        let total_dots = 2050 * dots_per_mcycle;
        self.ppu.tick_dots(total_dots);
        self.if_reg |= self.ppu.take_pending_interrupts();

        // Reset DIV counter (writing any value to $FF04 resets it).
        self.timer.write(0xFF04, 0);

        // Toggle speed and clear arm bit.
        self.key1 ^= 0x80;
        self.key1 &= !0x01;

        // Reset APU accumulator when switching speeds.
        self.apu_tick_accumulator = 0;

        true
    }

    /// Returns the effective WRAM bank index for `$D000–$DFFF`.
    ///
    /// Writing 0 to SVBK selects bank 1, not bank 0.
    fn effective_wram_bank(&self) -> usize {
        let bank = (self.svbk & 0x07) as usize;
        if bank == 0 { 1 } else { bank }
    }

    /// Advance system timers, PPU, and APU by `m_cycles` M-cycles.
    ///
    /// In double-speed mode, PPU receives half the dots per M-cycle (2 instead
    /// of 4) and APU ticks at half rate, since each CPU M-cycle takes half the
    /// real time.  Timer and OAM DMA are M-cycle driven and naturally run at 2x.
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

        let double = self.is_double_speed();
        let dots_per_mcycle: u32 = if double { 2 } else { 4 };
        self.ppu.tick_dots(u32::from(m_cycles) * dots_per_mcycle);

        if double {
            // APU runs at normal speed; in double-speed mode each CPU M-cycle
            // is half the real time, so tick APU at half rate via accumulator.
            self.apu_tick_accumulator += m_cycles;
            let apu_ticks = self.apu_tick_accumulator / 2;
            self.apu_tick_accumulator %= 2;
            if apu_ticks > 0 {
                self.apu.tick(apu_ticks);
            }
        } else {
            self.apu.tick(m_cycles);
        }
        // Tick the cartridge (for MBC3 RTC)
        self.cart.tick(u32::from(m_cycles));

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
    ///
    /// In double-speed mode, PPU receives fewer dots and APU ticks at half rate,
    /// matching the scaling applied in [`tick()`].
    fn tick_subsystems_for_hdma(&mut self, m_cycles: u32) {
        for _ in 0..m_cycles {
            self.timer.tick(1);
            if self.timer.interrupt_pending {
                self.if_reg |= 0x04;
                self.timer.interrupt_pending = false;
            }
        }
        let dots_per_mcycle: u32 = if self.is_double_speed() { 2 } else { 4 };
        self.ppu.tick_dots(m_cycles * dots_per_mcycle);
        self.if_reg |= self.ppu.take_pending_interrupts();
        // APU: in double-speed mode, tick at half rate via accumulator.
        if self.is_double_speed() {
            // m_cycles is u32 but accumulator is u8; process in chunks.
            let mut remaining = m_cycles;
            while remaining > 0 {
                let chunk = remaining.min(255) as u8;
                self.apu_tick_accumulator += chunk;
                let apu_ticks = self.apu_tick_accumulator / 2;
                self.apu_tick_accumulator %= 2;
                if apu_ticks > 0 {
                    self.apu.tick(apu_ticks);
                }
                remaining -= u32::from(chunk);
            }
        } else {
            let mut remaining = m_cycles;
            while remaining > 0 {
                let chunk = remaining.min(255) as u8;
                self.apu.tick(chunk);
                remaining -= u32::from(chunk);
            }
        }
        // Tick the cartridge (for MBC3 RTC)
        self.cart.tick(m_cycles);
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
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.effective_wram_bank()][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize],
            // OAM DMA uses the external bus where the full $F000–$FFFF range
            // mirrors the current WRAM bank (unlike normal reads which stop at $FDFF).
            0xF000..=0xFFFF => self.wram[self.effective_wram_bank()][(addr - 0xF000) as usize],
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
        self.wram = [[0u8; 0x1000]; 8];
        self.hram = [0u8; 0x7F];
        self.if_reg = 0;
        self.ie_reg = 0;
        self.dma_active = false;
        self.dma_source = 0;
        self.dma_position = 0;
        self.dma_oam_blocked = false;
        self.hdma = HdmaState::new();
        self.svbk = 0;
        self.key1 = 0;
        self.apu_tick_accumulator = 0;
    }

    // ── Save-state capture / restore ───────────────────────────────────────

    /// Capture the full bus state for serialization.
    pub fn capture_bus_state(&self) -> crate::gb::console::save_state::BusState {
        use crate::gb::console::save_state::{BusState, GbBusType};
        let mut wram_flat = [0u8; 0x8000];
        for (bank, bank_data) in self.wram.iter().enumerate() {
            let offset = bank * 0x1000;
            wram_flat[offset..offset + 0x1000].copy_from_slice(bank_data);
        }
        BusState {
            bus_type: GbBusType::Cgb,
            ppu: self.ppu.clone(),
            wram: wram_flat,
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
            svbk: Some(self.svbk),
            key1: Some(self.key1),
            apu_tick_accumulator: Some(self.apu_tick_accumulator),
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
        for (bank, bank_data) in self.wram.iter_mut().enumerate() {
            let offset = bank * 0x1000;
            bank_data.copy_from_slice(&state.wram[offset..offset + 0x1000]);
        }
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
        self.svbk = state.svbk.unwrap_or(0);
        self.key1 = state.key1.unwrap_or(0);
        self.apu_tick_accumulator = state.apu_tick_accumulator.unwrap_or(0);
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
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.effective_wram_bank()][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.effective_wram_bank()][(addr - 0xF000) as usize],
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
            // CGB KEY1 — speed switch register
            0xFF4D => (self.key1 & 0x80) | 0x7E | (self.key1 & 0x01),
            // CGB HDMA registers
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
            // CGB PCM registers
            0xFF76 => self.apu.read_pcm12(),
            0xFF77 => self.apu.read_pcm34(),
            0xFF70 => self.svbk | 0xF8,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => {
                println!("CGB bus: unhandled read at ${:04X}", addr);
                0xFF
            }
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x7FFF => self.cart.write(addr, val),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, val),
            0xA000..=0xBFFF => self.cart.write(addr, val),
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize] = val,
            0xD000..=0xDFFF => {
                self.wram[self.effective_wram_bank()][(addr - 0xD000) as usize] = val
            }
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize] = val,
            0xF000..=0xFDFF => {
                self.wram[self.effective_wram_bank()][(addr - 0xF000) as usize] = val
            }
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
            // CGB KEY1 — only bit 0 (arm) is writable; bit 7 (current speed) is read-only
            0xFF4D => self.key1 = (self.key1 & 0x80) | (val & 0x01),
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
            0xFF70 => self.svbk = val & 0x07,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {
                println!("CGB bus: unhandled write at ${:04X} = ${:02X}", addr, val);
            }
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        CgbBus::tick(self, m_cycles);
    }

    fn try_speed_switch(&mut self) -> bool {
        CgbBus::try_speed_switch(self)
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
            0xC000..=0xCFFF => self.wram[0][(addr - 0xC000) as usize],
            0xD000..=0xDFFF => self.wram[self.effective_wram_bank()][(addr - 0xD000) as usize],
            0xE000..=0xEFFF => self.wram[0][(addr - 0xE000) as usize],
            0xF000..=0xFDFF => self.wram[self.effective_wram_bank()][(addr - 0xF000) as usize],
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
            // CGB KEY1 — speed switch register (debugger)
            0xFF4D => (self.key1 & 0x80) | 0x7E | (self.key1 & 0x01),
            // CGB HDMA registers
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
            // CGB PCM registers
            0xFF76 => self.apu.read_pcm12(),
            0xFF77 => self.apu.read_pcm34(),
            0xFF70 => self.svbk | 0xF8,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => {
                println!("CGB bus: unhandled debugger read at ${:04X}", addr);
                0xFF
            }
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
        CgbBus::new(cgb_rom_only_cart(), CgbModel::default())
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
        let mut bus = CgbBus::new(cart, CgbModel::default());
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

    // ── SVBK / WRAM banking ─────────────────────────────────────────────────

    #[test]
    fn test_svbk_default_value_after_init() {
        // Given: freshly created CGB bus
        let mut bus = make_bus();
        // Then: $FF70 reads $F8 (upper 5 bits set, lower 3 = 0)
        assert_eq!(bus.read(0xFF70), 0xF8);
    }

    #[test]
    fn test_svbk_register_read_write() {
        // Given: CGB bus
        let mut bus = make_bus();
        // When: write $03 to SVBK
        bus.write(0xFF70, 0x03);
        // Then: read returns $FB ($03 | $F8)
        assert_eq!(bus.read(0xFF70), 0xFB);
    }

    #[test]
    fn test_svbk_write_zero_selects_bank_1() {
        // Given: CGB bus with SVBK set to 3
        let mut bus = make_bus();
        bus.write(0xFF70, 0x03);
        // When: write 0 to SVBK
        bus.write(0xFF70, 0x00);
        // Then: read returns $F8 (raw value 0, upper bits set)
        assert_eq!(bus.read(0xFF70), 0xF8);
        // And: writing distinct data to $D000 with SVBK=0 and SVBK=1 accesses
        //      the same bank (both map to bank 1).
        bus.write(0xFF70, 0x00);
        bus.write(0xD000, 0xAA);
        bus.write(0xFF70, 0x01);
        assert_eq!(bus.read(0xD000), 0xAA);
    }

    #[test]
    fn test_svbk_only_lower_3_bits_used() {
        // Given: CGB bus
        let mut bus = make_bus();
        // When: write $F8 (upper bits set, lower 3 clear → effective raw = 0)
        bus.write(0xFF70, 0xF8);
        // Then: read returns $F8 (raw 0x00 | 0xF8)
        assert_eq!(bus.read(0xFF70), 0xF8);
        // And: write $FF (all bits set → effective raw = 7)
        bus.write(0xFF70, 0xFF);
        // Then: read returns $FF ($07 | $F8)
        assert_eq!(bus.read(0xFF70), 0xFF);
    }

    #[test]
    fn test_wram_c000_cfff_always_bank_0() {
        // Given: CGB bus with data written to $C000 in default bank config
        let mut bus = make_bus();
        bus.write(0xC000, 0x42);
        bus.write(0xCFFF, 0x99);
        // When: switch to various banks
        for bank in 1..=7u8 {
            bus.write(0xFF70, bank);
            // Then: $C000-$CFFF always reads the same data (bank 0)
            assert_eq!(
                bus.read(0xC000),
                0x42,
                "C000 should be bank 0 regardless of SVBK={}",
                bank
            );
            assert_eq!(
                bus.read(0xCFFF),
                0x99,
                "CFFF should be bank 0 regardless of SVBK={}",
                bank
            );
        }
    }

    #[test]
    fn test_wram_d000_dfff_uses_selected_bank() {
        // Given: CGB bus
        let mut bus = make_bus();
        // When: select bank 2, write $42 to $D000
        bus.write(0xFF70, 0x02);
        bus.write(0xD000, 0x42);
        // And: select bank 3, write $99 to $D000
        bus.write(0xFF70, 0x03);
        bus.write(0xD000, 0x99);
        // Then: switching back to bank 2, $D000 reads $42
        bus.write(0xFF70, 0x02);
        assert_eq!(bus.read(0xD000), 0x42);
        // And: switching to bank 3, $D000 reads $99
        bus.write(0xFF70, 0x03);
        assert_eq!(bus.read(0xD000), 0x99);
    }

    #[test]
    fn test_wram_bank_data_isolation() {
        // Given: CGB bus with distinct data in each switchable bank
        let mut bus = make_bus();
        for bank in 1..=7u8 {
            bus.write(0xFF70, bank);
            bus.write(0xD000, bank * 10);
            bus.write(0xDFFF, bank * 10 + 1);
        }
        // Then: reading each bank returns the correct values
        for bank in 1..=7u8 {
            bus.write(0xFF70, bank);
            assert_eq!(bus.read(0xD000), bank * 10, "bank {} D000 mismatch", bank);
            assert_eq!(
                bus.read(0xDFFF),
                bank * 10 + 1,
                "bank {} DFFF mismatch",
                bank
            );
        }
    }

    #[test]
    fn test_echo_ram_mirrors_wram_banking() {
        // Given: CGB bus with different data in banks 2 and 3 at $D000
        let mut bus = make_bus();
        bus.write(0xFF70, 0x02);
        bus.write(0xD000, 0xBE);
        bus.write(0xFF70, 0x03);
        bus.write(0xD000, 0xEF);
        // When: switch back to bank 2
        bus.write(0xFF70, 0x02);
        // Then: echo RAM at $F000 mirrors the selected bank (bank 2)
        assert_eq!(bus.read(0xF000), 0xBE);
        // And: switching to bank 3, echo RAM reflects bank 3
        bus.write(0xFF70, 0x03);
        assert_eq!(bus.read(0xF000), 0xEF);
    }

    #[test]
    fn test_echo_ram_c000_mirror_always_bank_0() {
        // Given: CGB bus with data at $C000
        let mut bus = make_bus();
        bus.write(0xC000, 0x55);
        // When: switch to bank 5
        bus.write(0xFF70, 0x05);
        // Then: echo RAM at $E000 reads bank 0 data
        assert_eq!(bus.read(0xE000), 0x55);
    }

    #[test]
    fn test_svbk_reset_restores_default() {
        // Given: CGB bus with SVBK set to 5
        let mut bus = make_bus();
        bus.write(0xFF70, 0x05);
        assert_eq!(bus.read(0xFF70), 0xFD); // verify write took effect
        // When: reset
        bus.reset();
        // Then: SVBK is back to 0
        assert_eq!(bus.read(0xFF70), 0xF8);
    }

    // ── KEY1 register ($FF4D) — CGB double-speed mode ───────────────────────

    #[test]
    fn test_key1_initial_value_is_normal_speed_not_armed() {
        // Given: freshly created CGB bus
        let mut bus = make_bus();
        // Then: KEY1 reads $7E (normal speed, not armed, bits 6-1 set)
        assert_eq!(bus.read(0xFF4D), 0x7E);
    }

    #[test]
    fn test_key1_write_arms_speed_switch() {
        // Given: CGB bus
        let mut bus = make_bus();
        // When: write $01 to KEY1 (arm speed switch)
        bus.write(0xFF4D, 0x01);
        // Then: KEY1 reads $7F (normal speed, armed)
        assert_eq!(bus.read(0xFF4D), 0x7F);
    }

    #[test]
    fn test_key1_write_disarms_speed_switch() {
        // Given: CGB bus with KEY1 armed
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        assert_eq!(bus.read(0xFF4D), 0x7F);
        // When: write $00 to KEY1 (disarm)
        bus.write(0xFF4D, 0x00);
        // Then: KEY1 reads $7E (not armed)
        assert_eq!(bus.read(0xFF4D), 0x7E);
    }

    #[test]
    fn test_key1_bit7_is_read_only() {
        // Given: CGB bus in normal speed
        let mut bus = make_bus();
        // When: write $FF to KEY1 (attempt to set bit 7)
        bus.write(0xFF4D, 0xFF);
        // Then: bit 7 remains 0 (normal speed), only bit 0 set
        assert_eq!(bus.read(0xFF4D), 0x7F);
    }

    #[test]
    fn test_key1_speed_switch_toggles_to_double_speed() {
        // Given: CGB bus with KEY1 armed
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        // When: speed switch is triggered
        let switched = bus.try_speed_switch();
        // Then: switch happened
        assert!(switched);
        // And: KEY1 reads $FE (double speed, not armed)
        assert_eq!(bus.read(0xFF4D), 0xFE);
        // And: is_double_speed() returns true
        assert!(bus.is_double_speed());
    }

    #[test]
    fn test_key1_speed_switch_toggles_back_to_normal() {
        // Given: CGB bus in double speed mode (after one switch)
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(bus.is_double_speed());
        // When: arm and switch again
        bus.write(0xFF4D, 0x01);
        let switched = bus.try_speed_switch();
        // Then: switched back to normal
        assert!(switched);
        assert_eq!(bus.read(0xFF4D), 0x7E);
        assert!(!bus.is_double_speed());
    }

    #[test]
    fn test_key1_speed_switch_not_armed_returns_false() {
        // Given: CGB bus without KEY1 armed
        let mut bus = make_bus();
        // When: attempt speed switch
        let switched = bus.try_speed_switch();
        // Then: no switch
        assert!(!switched);
        assert!(!bus.is_double_speed());
        assert_eq!(bus.read(0xFF4D), 0x7E);
    }

    #[test]
    fn test_key1_speed_switch_clears_arm_bit() {
        // Given: CGB bus with KEY1 armed
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        // When: speed switch
        bus.try_speed_switch();
        // Then: bit 0 is cleared
        assert_eq!(bus.read(0xFF4D) & 0x01, 0x00);
    }

    #[test]
    fn test_key1_speed_switch_resets_div() {
        // Given: CGB bus with timer ticked some amount
        let mut bus = make_bus();
        for _ in 0..100 {
            bus.tick(1);
        }
        // Verify timer has advanced
        assert_ne!(bus.read(0xFF04), 0x00, "DIV should have advanced");
        // When: arm KEY1 and switch speed
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        // Then: DIV is reset to 0
        assert_eq!(bus.read(0xFF04), 0x00);
    }

    /// Helper: compute total dot position from LY and dot.
    fn total_ppu_dots(bus: &CgbBus) -> u32 {
        u32::from(bus.ppu.ly()) * 456 + u32::from(bus.ppu.dot())
    }

    #[test]
    fn test_double_speed_ppu_gets_half_dots_per_mcycle() {
        // Given: CGB bus in normal speed, LCD enabled.
        // Warm up past the LCD-enable transient so subsequent ticks
        // advance by exactly m_cycles × dots_per_mcycle.
        let mut bus = make_bus();
        enable_lcd(&mut bus);
        bus.tick(10); // warm-up

        // Measure normal-speed PPU advance for 5 M-cycles.
        let pre = total_ppu_dots(&bus);
        bus.tick(5);
        let normal_advance = total_ppu_dots(&bus) - pre;
        assert!(normal_advance > 0, "PPU should advance in normal speed");

        // Switch to double speed.
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(bus.is_double_speed());

        // Measure double-speed PPU advance for 5 M-cycles.
        let pre2 = total_ppu_dots(&bus);
        bus.tick(5);
        let post2 = total_ppu_dots(&bus);
        // Handle potential frame wrapping (154 scanlines × 456 dots = 70224).
        let double_advance = if post2 >= pre2 {
            post2 - pre2
        } else {
            post2 + 70224 - pre2
        };
        assert!(double_advance > 0, "PPU should advance in double speed");

        // In double speed, PPU gets 2 dots/M-cycle instead of 4.
        assert_eq!(
            normal_advance,
            double_advance * 2,
            "normal advance ({}) should be 2× double advance ({})",
            normal_advance,
            double_advance
        );
    }

    #[test]
    fn test_double_speed_apu_ticks_at_half_rate() {
        // Given: CGB bus in double speed mode
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(bus.is_double_speed());
        // When: tick 4 M-cycles in double speed
        bus.tick(4);
        // Then: APU accumulator should reflect half-rate ticking
        // 4 double-speed M-cycles → 2 normal M-cycles worth of APU ticks
        // Accumulator should be 0 (4 mod 2 = 0, all accumulated ticks dispatched)
        assert_eq!(bus.apu_tick_accumulator, 0);
    }

    #[test]
    fn test_double_speed_apu_odd_mcycles_leaves_accumulator() {
        // Given: CGB bus in double speed mode
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        // When: tick 3 M-cycles (odd number)
        bus.tick(3);
        // Then: accumulator has 1 leftover (3 mod 2 = 1)
        assert_eq!(bus.apu_tick_accumulator, 1);
    }

    #[test]
    fn test_normal_speed_apu_accumulator_stays_zero() {
        // Given: CGB bus in normal speed
        let mut bus = make_bus();
        assert!(!bus.is_double_speed());
        // When: tick some M-cycles
        bus.tick(5);
        // Then: accumulator remains 0 (no half-rate logic in normal speed)
        assert_eq!(bus.apu_tick_accumulator, 0);
    }

    #[test]
    fn test_speed_switch_round_trip_restores_normal_tick_rates() {
        // Given: CGB bus switched to double, then back to normal
        let mut bus = make_bus();
        enable_lcd(&mut bus);
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(
            bus.is_double_speed(),
            "should be in double speed after first switch"
        );
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(
            !bus.is_double_speed(),
            "should be back to normal after second switch"
        );
        // When: tick 5 M-cycles
        let pre_dot = bus.ppu.dot();
        let pre_ly = bus.ppu.ly();
        bus.tick(5);
        let total_post = u32::from(bus.ppu.ly()) * 456 + u32::from(bus.ppu.dot());
        let total_pre = u32::from(pre_ly) * 456 + u32::from(pre_dot);
        let dots = if total_post >= total_pre {
            total_post - total_pre
        } else {
            total_post + 70224 - total_pre
        };
        // Then: PPU gets 4 dots/M-cycle (normal rate restored)
        assert_eq!(dots, 20, "normal speed restored: 5 M-cycles × 4 dots = 20");
        // And: APU accumulator is 0
        assert_eq!(bus.apu_tick_accumulator, 0);
    }

    #[test]
    fn test_key1_reset_clears_speed_state() {
        // Given: CGB bus in double speed with KEY1 armed
        let mut bus = make_bus();
        bus.write(0xFF4D, 0x01);
        bus.try_speed_switch();
        assert!(bus.is_double_speed());
        // When: reset
        bus.reset();
        // Then: back to normal speed, not armed
        assert_eq!(bus.read(0xFF4D), 0x7E);
        assert!(!bus.is_double_speed());
    }
}
