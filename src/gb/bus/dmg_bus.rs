use crate::gb::apu::Apu;
use crate::gb::boot_rom::{DMG_BOOT_ROM, DMG0_BOOT_ROM};
use crate::gb::bus::GbBus;
use crate::gb::cartridge::{GbCartridge, has_canonical_nintendo_logo};
use crate::gb::input::joypad::Joypad;
use crate::gb::model::{CgbModel, DmgBootVariant, DmgModel};
use crate::gb::ppu::{Ppu, StopDisplayMode, timing::PpuMode};
use crate::gb::sgb::SgbState;
use crate::gb::timer::Timer;

/// Full DMG memory bus.
///
/// Implements the Game Boy (DMG) memory map, routing reads and writes to the
/// correct hardware region. Owns the cartridge, static RAM buffers, the Timer
/// subsystem, the PPU, the APU, and the IF/IE interrupt registers.
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
/// - $FF10–$FF3F: APU             (CH1–CH4 + wave RAM)
/// - $FF40–$FF4B: PPU I/O registers
/// - $FF46:       OAM DMA source address (read: last-written value; write: starts transfer)
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
    /// APU ($FF10–$FF3F).
    apu: Apu,
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
    /// Whether an OAM DMA transfer is currently in progress.
    dma_active: bool,
    /// High byte of the OAM DMA source address (written to $FF46).
    dma_source: u8,
    /// Current DMA position: 0 = warm-up, 1–160 = copying bytes 0–159,
    /// 161 = teardown (sets `dma_active = false`).  Total: 162 M-cycles.
    dma_position: u8,
    /// Whether OAM access is currently blocked by an active DMA transfer.
    ///
    /// Separate from `dma_active` because OAM is still accessible during the
    /// warm-up M-cycle (position 0).  Blocking begins at the first copy tick
    /// (position 1 → 2) and is preserved when a DMA is restarted while one is
    /// already running.
    dma_oam_blocked: bool,
    /// $FF01 Serial Data Register (SB).
    sb: u8,
    /// $FF02 Serial Control Register (SC).
    sc: u8,
    /// Bytes captured via serial transfer (written by ROM via SB/SC).
    serial_buf: Vec<u8>,
    /// Bits remaining in the current internal-clock serial transfer.
    /// 0 means no transfer is in progress; 1–8 means a transfer is active.
    /// Each time the serial master clock (see below) transitions to false,
    /// one bit is shifted; when this reaches 0 the transfer completes.
    serial_bits_remaining: u8,
    /// Persistent serial master clock toggle.
    ///
    /// Toggles on every falling edge of bit 7 of the internal DIV counter
    /// (i.e., every 256 T-cycles = 64 M-cycles).  A serial bit is shifted
    /// only when this flag transitions to `false`, giving an effective clock
    /// period of 512 T-cycles = 128 M-cycles = 8192 Hz.
    ///
    /// The flag is NOT reset when a transfer starts; it persists continuously
    /// from power-on.  On SC write (start/restart transfer), if the flag is
    /// currently `true` it is immediately forced to `false` — ensuring the
    /// first serial bit is
    /// timed at the correct phase relative to the div counter.
    serial_master_clock: bool,
    /// Hardware model variant (DMG-ABC or DMG-0).
    /// Determines which boot ROM is loaded and which CPU post-boot register
    /// values are used on reset.
    model: DmgModel,
    /// Optional minimal SGB command/input overlay.
    sgb: Option<SgbState>,
}

impl DmgBus {
    fn needs_mode3_lcdc_write_phase(&self, addr: u16, val: u8) -> bool {
        const LCDC_TILE_DATA: u8 = 0x10;

        addr == 0xFF40
            && self.ppu.is_lcd_enabled()
            && self.ppu.mode() == PpuMode::PixelTransfer
            && self.ppu.read_register(0xFF40) & LCDC_TILE_DATA != val & LCDC_TILE_DATA
    }

    pub fn new(cart: Box<dyn GbCartridge>, model: DmgModel) -> Self {
        // The boot ROM and initial div_counter depend on the hardware variant.
        // Production (DMG-A/B/C) scroll-animation ROM: div_counter = 5036.
        // DMG-0 simpler ROM: div_counter = 204 (same as real hardware).
        let (boot_rom, div_counter) = match model.boot_variant() {
            DmgBootVariant::Production => (DMG_BOOT_ROM, 5036),
            DmgBootVariant::Dmg0 => (DMG0_BOOT_ROM, 204),
        };
        let is_cgb = cart.is_cgb();
        let mut apu = Apu::new(is_cgb);
        if is_cgb {
            apu.set_cgb_model(CgbModel::CgbD);
        }
        let mut bus = Self {
            cart,
            ppu: Ppu::new(),
            wram: [0u8; 0x2000],
            hram: [0u8; 0x7F],
            // The initial div_counter compensates for the difference between
            // our custom boot ROM's cycle count and real DMG hardware timing.
            timer: Timer::with_div_counter(div_counter),
            joypad: Joypad::new(),
            apu,
            if_reg: 1, // VBlank flag set at power-on (real DMG hardware)
            ie_reg: 0,
            boot_rom,
            boot_rom_active: true,
            sb: 0x00,
            sc: 0x7E,
            serial_buf: Vec::new(),
            serial_bits_remaining: 0,
            serial_master_clock: false,
            dma_active: false,
            dma_source: 0xFF,
            dma_position: 0,
            dma_oam_blocked: false,
            model,
            sgb: None,
        };
        // Real DMG hardware powers on with LCDC=$00 (LCD disabled).
        // The boot ROM tile-loading runs while the LCD is off so VRAM writes
        // are never blocked by Mode 3; our boot ROM explicitly re-enables the
        // LCD (LCDC=$91) just before starting the scroll animation.
        bus.ppu.write_register(0xFF40, 0x00);
        bus
    }

    /// Construct a DMG bus with the minimal SGB command/input overlay enabled.
    ///
    /// This is intentionally explicit so normal DMG emulation does not imply
    /// full Super Game Boy support.
    pub fn new_sgb(cart: Box<dyn GbCartridge>, model: DmgModel) -> Self {
        let mut bus = Self::new(cart, model);
        bus.sgb = Some(SgbState::new());
        bus
    }

    /// Reset all bus state to power-on defaults.
    ///
    /// Reinitialises the PPU, timer, and joypad; zeroes WRAM and HRAM;
    /// clears IF and IE. The cartridge is not touched by this reset, so
    /// ROM, cartridge RAM, and any mapper state are preserved.
    pub fn reset(&mut self) {
        let apu_rate = self.apu.sample_rate();
        let (boot_rom, div_counter) = match self.model.boot_variant() {
            DmgBootVariant::Production => (DMG_BOOT_ROM, 5036),
            DmgBootVariant::Dmg0 => (DMG0_BOOT_ROM, 204),
        };
        self.ppu = Ppu::new();
        self.ppu.write_register(0xFF40, 0x00); // power-on: LCD disabled
        self.timer = Timer::with_div_counter(div_counter);
        self.joypad = Joypad::new();
        let is_cgb = self.cart.is_cgb();
        self.apu = Apu::new(is_cgb);
        if is_cgb {
            self.apu.set_cgb_model(CgbModel::CgbD);
        }
        self.apu.set_sample_rate(apu_rate);
        self.wram = [0u8; 0x2000];
        self.hram = [0u8; 0x7F];
        self.if_reg = 1; // VBlank flag set at power-on (real DMG hardware)
        self.ie_reg = 0;
        self.boot_rom = boot_rom;
        self.boot_rom_active = true;
        self.sb = 0x00;
        self.sc = 0x7E;
        self.serial_buf.clear();
        self.serial_bits_remaining = 0;
        self.serial_master_clock = false;
        self.dma_active = false;
        self.dma_source = 0xFF;
        self.dma_position = 0;
        self.dma_oam_blocked = false;
        if self.sgb.is_some() {
            self.sgb = Some(SgbState::new());
        }
    }

    /// Returns `true` while the boot ROM is still mapped at $0000–$00FF.
    pub fn is_boot_rom_active(&self) -> bool {
        self.boot_rom_active
    }

    /// Returns the hardware model variant this bus was constructed for.
    pub fn model(&self) -> DmgModel {
        self.model
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

    /// Advance system timers, PPU, and APU by `m_cycles` M-cycles.
    ///
    /// Propagates any timer interrupt to the IF register ($FF0F bit 2).
    /// Propagates PPU VBlank (bit 0) and STAT (bit 1) interrupts.
    /// Drives the serial transfer: on each falling edge of bit 7 of the
    /// internal DIV counter (64 M-cycles = 256 T-cycles), `serial_master_clock`
    /// is toggled.  When it transitions to `false` and an internal-clock
    /// transfer is active (SC = $81), one bit is shifted.  After 8 such
    /// transitions the transfer completes, SB is overwritten with 0xFF,
    /// SC bit 7 is cleared, and IF bit 3 (serial interrupt) is raised.
    ///
    /// PPU interrupt propagation is **deferred by one tick**: interrupts
    /// accumulated during the *previous* `tick()` call are propagated to IF
    /// at the start of the current call, before the PPU advances.  This
    /// models the real hardware behavior where the STAT interrupt line
    /// update from one M-cycle is sampled by the interrupt controller on
    /// the following M-cycle boundary.  Timer and serial IF updates remain
    /// immediate (set during the same tick they occur in).
    ///
    /// The APU frame sequencer is clocked by DIV-APU falling edges from the
    /// timer (bit 12 of the 16-bit internal counter = DIV bit 4).
    pub fn tick(&mut self, m_cycles: u8) {
        self.tick_before_ppu(m_cycles);
        self.ppu.tick_dots(u32::from(m_cycles) * 4);
        self.tick_after_ppu(m_cycles);
    }

    fn tick_before_ppu(&mut self, m_cycles: u8) {
        // Propagate PPU interrupts accumulated during the previous tick.
        self.if_reg |= self.ppu.take_pending_interrupts();

        for _ in 0..m_cycles {
            let pre_counter = self.timer.raw_counter();
            let pre_bit7 = pre_counter & 0x080;
            let (div_apu_falling, div_apu_rising) = self.timer.tick(1);
            if self.timer.interrupt_pending {
                self.if_reg |= 0x04;
                self.timer.interrupt_pending = false;
            }

            // Rising edge fires the APU secondary event (for envelope phantom-tick detection).
            for _ in 0..div_apu_rising {
                self.apu.clock_div_apu_secondary();
            }
            // Falling edge steps the APU frame sequencer.
            for _ in 0..div_apu_falling {
                self.apu.clock_div_apu();
            }

            // Falling edge of bit 7 of the DIV counter (runs continuously).
            let post_bit7 = self.timer.raw_counter() & 0x080;
            if pre_bit7 != 0 && post_bit7 == 0 {
                self.serial_master_clock ^= true;
                if !self.serial_master_clock
                    && self.serial_bits_remaining > 0
                    && self.sc & 0x81 == 0x81
                {
                    self.serial_bits_remaining -= 1;
                    if self.serial_bits_remaining == 0 {
                        self.serial_buf.push(self.sb);
                        self.sb = 0xFF;
                        self.if_reg |= 0x08;
                        self.sc &= 0x7F;
                    }
                }
            }

            // OAM DMA: advance one M-cycle.
            //
            // Sequence: 1 warm-up tick (no copy) + 160 copy ticks (bytes 0–159)
            // + 1 teardown tick = 162 DMA M-cycles total.
            //
            // OAM blocking (dma_oam_blocked) starts at the first COPY tick, not
            // the warm-up.  tick() runs before each CPU memory access, so:
            //   M=0: CPU writes $FF46 → dma_oam_blocked=false (fresh) or true (restart)
            //   M=1: tick(warm-up) runs → dma_oam_blocked unchanged; CPU read/write
            //        to OAM is accessible for a fresh DMA (blocked for a restart)
            //   M=2: tick(copy) runs → dma_oam_blocked=true; OAM blocked
            if self.dma_active {
                match self.dma_position {
                    0 => {
                        // warm-up: bus is captured but no byte copied yet;
                        // dma_oam_blocked is unchanged (false for fresh, true for restart)
                        self.dma_position = 1;
                    }
                    1..=160 => {
                        // Block OAM for the entire copy phase.
                        self.dma_oam_blocked = true;
                        let byte_idx = (self.dma_position - 1) as u16;
                        let src = (self.dma_source as u16) << 8 | byte_idx;
                        self.ppu.oam[byte_idx as usize] = self.read_raw(src);
                        self.dma_position += 1;
                    }
                    161 => {
                        // teardown: transfer complete, unblock OAM
                        self.dma_active = false;
                        self.dma_oam_blocked = false;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    fn tick_after_ppu(&mut self, m_cycles: u8) {
        // PPU interrupts are now buffered in ppu.pending_interrupts and
        // will be propagated to IF at the start of the NEXT tick() call.
        self.apu.tick(m_cycles);
        // Tick the cartridge (for MBC3 RTC)
        self.cart.tick(u32::from(m_cycles));
    }

    /// Returns `true` when an audio sample is ready to be retrieved.
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

    /// Bypass PPU access-blocking for OAM DMA transfers.
    ///
    /// On DMG hardware, the DMA controller uses the external bus. For addresses
    /// in `$E000–$FFFF`, the external bus mirrors WRAM (`$C000–$DFFF`) by
    /// clearing address bit 13 (equivalent to `addr & !0x2000`). This means
    /// DMA from `$FE00` reads WRAM at `$DE00`, and DMA from `$FF00` reads
    /// WRAM at `$DF00`, rather than reading OAM or I/O registers.
    fn read_raw(&self, addr: u16) -> u8 {
        if self.boot_rom_active && addr <= 0x00FF {
            return self.boot_rom[addr as usize];
        }
        match addr {
            0x0000..=0x7FFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.ppu.vram[(addr - 0x8000) as usize],
            0xA000..=0xBFFF => self.cart.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            // $E000–$FFFF: the DMA controller uses the external bus, which
            // mirrors WRAM here (same as echo RAM, clearing bit 13).
            0xE000..=0xFFFF => self.wram[(addr - 0xE000) as usize],
        }
    }

    fn dma_conflict_active(&self) -> bool {
        self.dma_active && self.dma_oam_blocked && !matches!(self.dma_source, 0x80..=0x9F)
    }

    fn dma_conflict_byte(&self) -> u8 {
        let byte_idx = self.dma_position.saturating_sub(2) as u16;
        self.read_raw((u16::from(self.dma_source) << 8) + byte_idx)
    }

    /// Begin a cycle-accurate OAM DMA transfer from `(val << 8)`.
    ///
    /// Sets the DMA state so that `tick()` copies one byte per M-cycle.
    /// Total duration: 162 M-cycles (1 warm-up + 160 copy + 1 teardown).
    ///
    /// OAM blocking (`dma_oam_blocked`) is preserved when restarting an
    /// in-progress DMA — the running transfer was already blocking OAM, so
    /// the warm-up of the new transfer does not restore access.  For a fresh
    /// start, blocking begins after the warm-up M-cycle.
    fn do_oam_dma(&mut self, val: u8) {
        let preserve_blocking = self.dma_active && self.dma_oam_blocked;
        self.dma_active = true;
        self.dma_source = val;
        self.dma_position = 0;
        self.dma_oam_blocked = preserve_blocking;
    }

    // ── Save-state capture / restore ───────────────────────────────────────

    /// Capture the full bus state for serialization.
    pub fn capture_bus_state(&self) -> crate::gb::console::save_state::BusState {
        use crate::gb::console::save_state::{BusState, GbBusType};
        let mut wram_padded = [0u8; 0x8000];
        wram_padded[..0x2000].copy_from_slice(&self.wram);
        BusState {
            bus_type: GbBusType::Dmg,
            ppu: self.ppu.clone(),
            wram: wram_padded,
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
            hdma: None,
            svbk: None,
            key1: None,
            apu_tick_accumulator: None,
            rtc_tick_accumulator: None,
            ff72: None,
            ff73: None,
            ff74: None,
            ff75: None,
            key0: None,
            key0_locked: None,
            cgb_extra_oam: None,
            boot_rom_active: Some(self.boot_rom_active),
            sb: Some(self.sb),
            sc: Some(self.sc),
            serial_buf: Some(self.serial_buf.clone()),
            serial_bits_remaining: Some(self.serial_bits_remaining),
            serial_master_clock: Some(self.serial_master_clock),
            model: Some(self.model),
            sgb: self.sgb.clone(),
        }
    }

    /// Restore bus state from a deserialized snapshot.
    ///
    /// Returns an error if the save state was captured from a CGB bus.
    pub fn restore_bus_state(
        &mut self,
        state: &crate::gb::console::save_state::BusState,
    ) -> Result<(), String> {
        use crate::gb::console::save_state::GbBusType;
        if state.bus_type != GbBusType::Dmg {
            return Err(format!(
                "bus type mismatch: expected DMG, found {:?}",
                state.bus_type
            ));
        }
        self.ppu = state.ppu.clone();
        self.ppu.fixup_after_state_load();
        self.wram.copy_from_slice(&state.wram[..0x2000]);
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
        if let Some(active) = state.boot_rom_active {
            self.boot_rom_active = active;
        }
        if let Some(sb) = state.sb {
            self.sb = sb;
        }
        if let Some(sc) = state.sc {
            self.sc = sc;
        }
        if let Some(ref buf) = state.serial_buf {
            self.serial_buf = buf.clone();
        }
        if let Some(bits) = state.serial_bits_remaining {
            self.serial_bits_remaining = bits;
        }
        if let Some(clock) = state.serial_master_clock {
            self.serial_master_clock = clock;
        }
        if let Some(model) = state.model {
            self.model = model;
            self.boot_rom = match model.boot_variant() {
                DmgBootVariant::Production => DMG_BOOT_ROM,
                DmgBootVariant::Dmg0 => DMG0_BOOT_ROM,
            };
        }
        self.sgb = state.sgb.clone();
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
            0xFE00..=0xFE9F => {
                if self.dma_oam_blocked {
                    return 0xFF;
                }
                self.ppu.read_oam(addr)
            }
            0xFEA0..=0xFEFF => {
                if self.dma_oam_blocked {
                    return 0xFF;
                }
                self.ppu.read_forbidden_zone()
            }
            0xFF00 => {
                let value = self.joypad.read();
                self.sgb.as_ref().map_or(value, |sgb| sgb.read_p1(value))
            }
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E, // bits 6-1 unused on DMG, always read as 1
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
            _ => {
                println!("[DMG] Unmapped read: ${:04X}", addr);
                0xFF
            }
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
            0xFF00 => {
                let previous_select = self.joypad.read() & 0x30;
                if let Some(sgb) = &mut self.sgb {
                    sgb.write_p1(previous_select, val);
                }
                self.joypad.write(val);
            }
            0xFF01 => self.sb = val,
            0xFF02 => {
                // Clock-alignment step when *starting* an
                // internal-clock transfer and serial_master_clock is currently
                // true, immediately force it to false before latching SC.
                // This ensures the first serial bit is always timed at the
                // correct phase — exactly what real hardware does when a
                // transfer begins mid-period.  Restricting to internal-clock
                // starts avoids unintentionally shifting the clock phase on
                // writes that merely inspect or clear SC.
                if val & 0x81 == 0x81 && self.serial_master_clock {
                    self.serial_master_clock = false;
                }
                self.sc = val;
                if val & 0x80 != 0 && val & 0x01 != 0 {
                    // Internal clock (bit 0 set): start / restart 8-bit transfer.
                    self.serial_bits_remaining = 8;
                }
                // External clock (bit 0 clear): store SC but never start a transfer.
            }
            0xFF04..=0xFF07 => {
                let div_apu_edge = self.timer.write(addr, val);
                // If a DIV-APU falling edge occurred (DIV write with bit 4 HIGH),
                // clock the APU frame sequencer.
                if div_apu_edge {
                    self.apu.clock_div_apu();
                }
                // Immediately fire any write-triggered TIMA overflow, so the interrupt is
                // visible to service_interrupts() at the start of the next instruction.
                if self.timer.fire_write_overflow_if_pending() {
                    self.if_reg |= 0x04;
                    self.timer.take_interrupt();
                }
            }
            0xFF0F => self.if_reg = val & 0x1F,
            0xFF26 => {
                // NR52 special handling: pass DIV-APU bit state for power-on skip logic.
                let div_apu_high = self.timer.is_div_apu_bit_high();
                self.apu.write_nr52_with_div_state(val, div_apu_high);
            }
            0xFF10..=0xFF25 | 0xFF27..=0xFF3F => self.apu.write_register(addr, val),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => {
                self.ppu.write_register(addr, val);
                // Immediately flush any interrupt generated by the register write
                // (e.g. STAT LYC=LY match on LCD re-enable), so the interrupt is visible to
                // service_interrupts() at the start of the next instruction.
                self.if_reg |= self.ppu.take_pending_interrupts();
            }
            0xFF46 => self.do_oam_dma(val),
            0xFF50 => {
                if self.boot_rom_active
                    && matches!(self.model.boot_variant(), DmgBootVariant::Production)
                    && has_canonical_nintendo_logo(self.cart.as_ref())
                {
                    self.ppu.seed_boot_logo();
                    self.ppu.seed_boot_registered_mark_tile();
                }
                self.boot_rom_active = false;
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
            _ => {
                println!("[DMG] Unmapped write: ${:04X} = ${:02X}", addr, val);
            }
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        DmgBus::tick(self, m_cycles);
    }

    fn write_cpu_m_cycle(&mut self, addr: u16, val: u8) {
        if self.needs_mode3_lcdc_write_phase(addr, val) {
            self.tick_before_ppu(1);
            self.ppu.tick_dots(3);
            self.write(addr, val);
            self.ppu.tick_dots(1);
            self.tick_after_ppu(1);
        } else {
            self.tick(1);
            if !self.dma_conflict_active() || matches!(addr, 0xFF46 | 0xFF80..=0xFFFE) {
                self.write(addr, val);
            }
        }
    }

    fn read_cpu_m_cycle(&mut self, addr: u16) -> u8 {
        if self.dma_conflict_active() && !matches!(addr, 0xFF46 | 0xFF80..=0xFFFE) {
            self.dma_conflict_byte()
        } else {
            self.read(addr)
        }
    }

    fn enter_stop_mode(&mut self) {
        self.ppu
            .enter_stop_display_mode(StopDisplayMode::SolidWhite);
    }

    fn exit_stop_mode(&mut self) {
        self.ppu.exit_stop_display_mode();
    }

    fn notify_idu_glitch(&mut self, addr: u16) {
        if matches!(addr, 0xFE00..=0xFEFF)
            && let Some(row) = self.ppu.current_oam_row()
            && (1..=19).contains(&row)
        {
            self.ppu.apply_oam_write_corruption(row);
        }
    }

    fn notify_idu_with_prior_read(&mut self, addr: u16) {
        // Handles only the forbidden zone ($FEA0–$FEFF): reads from real OAM
        // ($FE00–$FE9F) already apply plain read corruption via Ppu::read_oam,
        // so restricting to $FEA0–$FEFF avoids a double-corruption there.
        if matches!(addr, 0xFEA0..=0xFEFF)
            && let Some(row) = self.ppu.current_oam_row()
        {
            self.ppu.apply_oam_read_idu_corruption(row);
        }
    }

    fn notify_oam_read(&mut self, addr: u16) {
        // Handles only the forbidden zone ($FEA0–$FEFF): reads from real OAM
        // ($FE00–$FE9F) already apply plain read corruption via Ppu::read_oam.
        if matches!(addr, 0xFEA0..=0xFEFF)
            && let Some(row) = self.ppu.current_oam_row()
        {
            self.ppu.apply_oam_read_corruption(row);
        }
    }

    fn notify_oam_write(&mut self, addr: u16) {
        // Handles only the forbidden zone ($FEA0–$FEFF): writes to real OAM
        // ($FE00–$FE9F) already apply write corruption via Ppu::write_oam.
        if matches!(addr, 0xFEA0..=0xFEFF)
            && let Some(row) = self.ppu.current_oam_row()
        {
            self.ppu.apply_oam_write_corruption(row);
        }
    }

    fn ppu(&self) -> &Ppu {
        &self.ppu
    }

    fn ppu_mut(&mut self) -> &mut Ppu {
        &mut self.ppu
    }

    fn read_for_debugger(&self, addr: u16) -> u8 {
        // Debugger reads mirror normal read() address decoding (including register
        // readback behavior like `if_reg | 0xE0` and `sc | 0x7E`) but avoid side
        // effects such as OAM corruption or serial transfer state changes.
        // Boot ROM is checked — debugger should see what CPU will actually execute.
        if self.boot_rom_active && addr <= 0x00FF {
            return self.boot_rom[addr as usize];
        }
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
            0xFF00 => {
                let value = self.joypad.read();
                self.sgb.as_ref().map_or(value, |sgb| sgb.read_p1(value))
            }
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E,
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
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
        DmgBus::new(rom_only_cart(), DmgModel::DmgB)
    }

    fn make_sgb_bus() -> DmgBus {
        DmgBus::new_sgb(rom_only_cart(), DmgModel::DmgB)
    }

    fn send_sgb_packet(bus: &mut DmgBus, packet: &[u8; 16]) {
        bus.write(0xFF00, 0x00);
        bus.write(0xFF00, 0x30);
        for byte in packet {
            let mut bits = *byte;
            for _ in 0..8 {
                bus.write(0xFF00, if bits & 1 == 0 { 0x20 } else { 0x10 });
                bus.write(0xFF00, 0x30);
                bits >>= 1;
            }
        }
        bus.write(0xFF00, 0x20);
        bus.write(0xFF00, 0x30);
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

    #[test]
    fn test_lcdc_write_cpu_m_cycle_applies_ppu_write_at_t3_during_mode3() {
        let mut bus = make_bus();
        bus.write(0xFF40, 0x91);
        while bus.ppu.dot() < 84 {
            bus.tick(1);
        }
        assert_eq!(bus.ppu.mode(), PpuMode::PixelTransfer);

        let write_cycle_start_dot = bus.ppu.dot();
        bus.write_cpu_m_cycle(0xFF40, 0x00);

        assert_eq!(
            bus.ppu.dot(),
            write_cycle_start_dot + 3,
            "LCDC writes should disable the PPU at T3, before the last dot of the CPU write M-cycle"
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

    // ── Forbidden ($FEA0–$FEFF) ──────────────────────────────────────────────
    //
    // Per Pan Docs: "This area returns $FF when OAM is blocked, and otherwise
    // the behavior depends on the hardware revision."
    // On DMG: reads during OAM block trigger OAM corruption. Reads otherwise
    // return $00.

    #[test]
    fn test_forbidden_region_reads_return_0x00_when_oam_not_blocked() {
        // Given: PPU in VBlank (OAM not blocked, DMA not active)
        // When: read from $FEA0 and $FEFF
        // Then: both return $00
        let mut bus = make_bus();
        tick_to_vblank(&mut bus);
        assert_eq!(
            bus.read(0xFEA0),
            0x00,
            "FEA0 should return 0x00 when OAM accessible"
        );
        assert_eq!(
            bus.read(0xFEFF),
            0x00,
            "FEFF should return 0x00 when OAM accessible"
        );
    }

    #[test]
    fn test_forbidden_region_reads_return_0xff_when_ppu_oam_blocked_mode2() {
        // Given: PPU in Mode 2 (OAM Scan) — OAM is blocked
        // When: read from $FEA0
        // Then: returns $FF
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        enable_lcd_and_tick_to_row(&mut bus, 2);
        assert_eq!(
            bus.read(0xFEA0),
            0xFF,
            "FEA0 should return 0xFF when OAM blocked by Mode 2"
        );
    }

    #[test]
    fn test_forbidden_region_reads_return_0xff_when_ppu_oam_blocked_mode3() {
        // Given: PPU in Mode 3 (Pixel Transfer) — OAM is blocked
        // When: read from $FEA0
        // Then: returns $FF
        let mut bus = make_bus();
        // Enable LCD; scan 0 has Mode 3 starting at dot 84.
        // Tick 22 M-cycles (22*4=88 dots) to reach dot 88, which is in Mode 3.
        bus.write(0xFF40, 0x91);
        for _ in 0..22 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFEA0),
            0xFF,
            "FEA0 should return 0xFF when OAM blocked by Mode 3"
        );
    }

    #[test]
    fn test_forbidden_region_reads_return_0xff_when_dma_oam_blocked() {
        // Given: DMA OAM blocking is active
        // When: read from $FEA0
        // Then: returns $FF
        let mut bus = make_bus();
        tick_to_vblank(&mut bus);
        bus.dma_oam_blocked = true;
        assert_eq!(
            bus.read(0xFEA0),
            0xFF,
            "FEA0 should return 0xFF when OAM blocked by DMA"
        );
    }

    #[test]
    fn test_forbidden_region_read_triggers_corruption_during_mode2() {
        // Given: PPU in Mode 2 (OAM Scan) at row 2; OAM rows 1 and 2 have known values
        // When: read from $FEA0 (forbidden region)
        // Then: OAM row 2 is read-corrupted
        //
        // Read corruption formula: row[0] = b | (a & c)
        // where a=row2[0], b=row1[0], c=row1[2]
        // row 1: b=0x0002, c=0x0004
        // row 2: a=0x0001
        // result: 0x0002 | (0x0001 & 0x0004) = 0x0002 | 0x0000 = 0x0002
        // Words 1–3 copied from previous row: [0x0003, 0x0004, 0x0005]
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut bus.ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        enable_lcd_and_tick_to_row(&mut bus, 2);
        bus.read(0xFEA0); // read from forbidden region
        let row2 = get_row_words(&bus.ppu.oam, 2);
        assert_eq!(
            row2,
            [0x0002, 0x0003, 0x0004, 0x0005],
            "Forbidden region read in Mode 2 must apply read corruption to current OAM row"
        );
    }

    #[test]
    fn test_forbidden_region_read_no_corruption_during_mode3() {
        // Given: PPU in Mode 3 (Pixel Transfer); OAM has known values
        // When: read from $FEA0 (forbidden region)
        // Then: OAM data is NOT corrupted (corruption only happens in Mode 2)
        let mut bus = make_bus();
        // Enable LCD; tick to Mode 3 on scan 0 (dot 88).
        bus.write(0xFF40, 0x91);
        for _ in 0..22 {
            bus.tick(1);
        }
        let snapshot = bus.ppu.oam;
        bus.read(0xFEA0);
        assert_eq!(
            bus.ppu.oam, snapshot,
            "Forbidden region read in Mode 3 must not corrupt OAM"
        );
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
        // $FF08 (unmapped) — $FF01 (SB) now starts at $00, so use only unmapped ranges
        assert_eq!(bus.read(0xFF08), 0xFF);
        assert_eq!(bus.read(0xFF4E), 0xFF);
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
        // Given: neither group selected (must be set explicitly; power-on default
        // is both groups selected, matching real DMG hardware $CF)
        let mut bus = make_bus();
        bus.write(0xFF00, 0x30); // deselect both groups
        // When: press A — action group not selected, no IRQ
        bus.set_joypad_button(0, true);
        // Then: IF bit 4 not set
        assert_eq!(bus.read(0xFF0F) & 0x10, 0x00, "IF bit 4 should NOT be set");
    }

    #[test]
    fn test_sgb_mlt_req_deselect_both_reads_current_player_id() {
        let mut bus = make_sgb_bus();
        let mlt_req_1 = [0x89, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        send_sgb_packet(&mut bus, &mlt_req_1);
        assert_eq!(bus.read(0xFF00), 0xFF);
        bus.write(0xFF00, 0x10);
        bus.write(0xFF00, 0x30);

        assert_eq!(bus.read(0xFF00), 0xFE);
    }

    #[test]
    fn test_normal_dmg_deselect_both_still_reads_all_released_nibble() {
        let mut bus = make_bus();

        bus.write(0xFF00, 0x30);

        assert_eq!(bus.read(0xFF00), 0xFF);
    }

    #[test]
    fn test_sgb_state_round_trips_through_dmg_bus_save_state() {
        let mut bus = make_sgb_bus();
        let mlt_req_1 = [0x89, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        send_sgb_packet(&mut bus, &mlt_req_1);
        bus.write(0xFF00, 0x10);
        bus.write(0xFF00, 0x30);
        let state = bus.capture_bus_state();

        let mut restored = make_sgb_bus();
        restored
            .restore_bus_state(&state)
            .expect("restore DMG SGB bus state");

        assert_eq!(restored.read(0xFF00), 0xFE);
    }

    #[test]
    fn test_missing_sgb_state_in_save_state_disables_sgb_overlay() {
        let mut bus = make_sgb_bus();
        let mut state = bus.capture_bus_state();
        state.sgb = None;

        bus.restore_bus_state(&state)
            .expect("restore old DMG bus state without SGB field");
        bus.write(0xFF00, 0x30);

        assert_eq!(bus.read(0xFF00), 0xFF);
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
        // Given: fresh DmgB bus (div_counter=5036 → DIV=19); When: read $FF04; Then: returns 19
        let mut bus = make_bus();
        assert_eq!(bus.read(0xFF04), 19);
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
        // Then: read back 0x7E — bits 6-1 are unused on DMG and always return 1
        let mut bus = make_bus();
        bus.write(0xFF02, 0x40);
        assert_eq!(bus.read(0xFF02), 0x7E);
    }

    #[test]
    fn test_sc_unused_bits_always_one() {
        // DMG hardware: SC bits 6-1 are unused and must always read as 1.
        // Given: write 0x00 to SC; When: read SC; Then: bits 6-1 are still 1.
        let mut bus = make_bus();
        bus.write(0xFF02, 0x00);
        assert_eq!(bus.read(0xFF02) & 0x7E, 0x7E);
    }

    #[test]
    fn test_serial_transfer_captures_byte() {
        // Given: SB = 0x41; When: write SC = 0x81 (start internal-clock transfer) and tick
        // 1024 M-cycles; Then: serial_output contains 0x41
        let mut bus = make_bus();
        bus.write(0xFF01, 0x41);
        bus.write(0xFF02, 0x81);
        for _ in 0..1024 {
            bus.tick(1);
        }
        assert_eq!(bus.serial_output(), &[0x41]);
    }

    #[test]
    fn test_serial_transfer_sets_if_bit3() {
        // Given: trigger a serial transfer; When: 1024 M-cycles pass; Then: IF bit 3 is set
        let mut bus = make_bus();
        bus.write(0xFF01, 0x42);
        bus.write(0xFF02, 0x81);
        for _ in 0..1024 {
            bus.tick(1);
        }
        assert_eq!(bus.read(0xFF0F) & 0x08, 0x08);
    }

    #[test]
    fn test_serial_transfer_clears_sc_bit7() {
        // Given: write 0x81 to SC; When: 1024 M-cycles pass; Then: SC bit 7 = 0
        let mut bus = make_bus();
        bus.write(0xFF02, 0x81);
        for _ in 0..1024 {
            bus.tick(1);
        }
        assert_eq!(bus.read(0xFF02) & 0x80, 0x00);
    }

    #[test]
    fn test_serial_transfer_not_immediate() {
        // Given: SB = 0x42, SC = 0x81 (transfer started);
        // When: fewer than 77 M-cycles have elapsed (before the first serial
        //   count, which happens at M-cycle 77 with initial div_counter = 204
        //   and serial_master_clock = false);
        // Then: SC bit 7 still set, IF bit 3 still clear, serial_output empty
        let mut bus = make_bus();
        bus.write(0xFF01, 0x42);
        bus.write(0xFF02, 0x81);
        for _ in 0..76 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFF02) & 0x80,
            0x80,
            "SC bit 7 should still be set"
        );
        assert_eq!(
            bus.read(0xFF0F) & 0x08,
            0x00,
            "IF bit 3 should still be clear"
        );
        assert!(
            bus.serial_output().is_empty(),
            "serial_output should be empty"
        );
    }

    #[test]
    fn test_serial_transfer_completes_within_1024_m_cycles() {
        // Given: transfer started at M-cycle 0;
        // When: 1024 M-cycles have elapsed (the maximum transfer duration);
        // Then: transfer has completed (IF bit 3 set, SC bit 7 clear).
        // Note: with initial div_counter = 204 and SMC = false, the 8th bit
        // is counted at M-cycle 973; this test confirms completion within the
        // maximum window without asserting the exact cycle.
        let mut bus = make_bus();
        bus.write(0xFF01, 0x99);
        bus.write(0xFF02, 0x81);
        for _ in 0..1024 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFF0F) & 0x08,
            0x08,
            "IF bit 3 should be set within 1024 M-cycles"
        );
        assert_eq!(
            bus.read(0xFF02) & 0x80,
            0x00,
            "SC bit 7 should be cleared within 1024 M-cycles"
        );
    }

    #[test]
    fn test_serial_transfer_sets_sb_to_ff() {
        // Given: SB = 0x42; When: transfer completes (1024 M-cycles);
        // Then: SB is 0xFF (simulates receiving 0xFF from absent remote)
        let mut bus = make_bus();
        bus.write(0xFF01, 0x42);
        bus.write(0xFF02, 0x81);
        for _ in 0..1024 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFF01),
            0xFF,
            "SB should be 0xFF after transfer completes"
        );
    }

    #[test]
    fn test_serial_external_clock_never_fires() {
        // Given: SC = 0x80 (bit 7 set, bit 0 clear = external clock);
        // When: 2048 M-cycles pass;
        // Then: IF bit 3 never set and SC bit 7 remains set
        let mut bus = make_bus();
        bus.write(0xFF01, 0x55);
        bus.write(0xFF02, 0x80);
        for _ in 0..2048 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFF0F) & 0x08,
            0x00,
            "IF bit 3 should never fire for external clock"
        );
        assert_eq!(
            bus.read(0xFF02) & 0x80,
            0x80,
            "SC bit 7 should remain set for external clock"
        );
    }

    #[test]
    fn test_serial_transfer_restart() {
        // Initial div_counter = 5036 (DmgB); bit-7 falls at M-cycles 21, 85, 149, 213, …
        // With serial_master_clock (SMC) starting false, counts (SMC→false) at 85, 213, …, 981.
        // Write SC=0x81 at M-cycle 0 (first transfer, byte 0x11).
        // Restart with byte 0x22 at M-cycle 64 — before the first count at M-cycle 85.
        //   At restart: SMC=true (one bit-7 fall at M-cycle 21) → force-aligned to false.
        //   After restart: next bit-7 falls at 85 (SMC→true, no count), 149 (SMC→false,
        //   COUNT), …, 1045 (8th count).  Interrupt fires at M-cycle 1045:
        //   - no interrupt at M-cycle 1044 (980 M-cycles after restart)
        //   - interrupt fires at M-cycle 1045 (981 M-cycles after restart)
        let mut bus = make_bus();
        bus.write(0xFF01, 0x11);
        bus.write(0xFF02, 0x81); // first transfer
        for _ in 0..64 {
            bus.tick(1);
        }
        // Confirm no interrupt fired yet (first count not until M-cycle 85)
        assert_eq!(bus.read(0xFF0F) & 0x08, 0x00, "no interrupt before restart");
        // Restart
        bus.write(0xFF01, 0x22);
        bus.write(0xFF02, 0x81);
        // Tick 980 more M-cycles (total 1044 from start) — one tick short of completion
        for _ in 0..980 {
            bus.tick(1);
        }
        assert_eq!(
            bus.read(0xFF0F) & 0x08,
            0x00,
            "no interrupt 980 M-cycles after restart"
        );
        // One more tick reaches M-cycle 1045 — 8th counted bit fires interrupt
        bus.tick(1);
        assert_eq!(
            bus.read(0xFF0F) & 0x08,
            0x08,
            "interrupt fires at 8th bit after restart"
        );
        assert_eq!(
            bus.serial_output(),
            &[0x22],
            "only the restarted byte captured"
        );
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
        // Skip scan 0 (452 dots = 113 M-cycles) plus the 4T Mode 0 gap at the start
        // of scan 1 (1 M-cycle) → lands at dot=4 of scan 1, which is OamScan row 0.
        for _ in 0..114 {
            bus.tick(1);
        }
        // Now at dot=4 of scan 1 (OamScan row 0); tick to the desired row.
        for _ in 0..row {
            bus.tick(1); // 1 M-cycle = 4 dots = 1 OAM row
        }
    }

    #[test]
    fn test_notify_idu_glitch_applies_write_corruption_in_mode_2() {
        // Given: PPU at row 2 (dot 12 of scan 1, where OamScan starts at dot=4);
        // OAM rows 1 and 2 have known values.
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

    // ── IDU glitch: row 17–19 and frame-boundary coverage (Phase C+) ─────────

    /// tick_to_vblank puts PPU at dot=4, scanline=144 (VBlank start + 1 M-cycle).
    /// From there, ticking 1138 M-cycles (4552 T) reaches dot=452, scanline=153
    /// — the last safe position before the frame wraps.
    fn tick_to_late_vblank(bus: &mut DmgBus) {
        tick_to_vblank(bus);
        for _ in 0..1138u16 {
            bus.tick(1);
        }
        // PPU is now at dot=452, scanline=153 (VBlank).
    }

    #[test]
    fn test_notify_idu_glitch_applies_corruption_at_row_18() {
        // Given: PPU at OamScan row 18 (dot=76 of scan 1; last valid row — OamScan [4,80)).
        // Row 18 = dot 76: (76-4)/4 = 18.
        // row 17: b=0x0002, w1=0x0003, c=0x0004, w3=0x0005
        // row 18: a=0x0001
        // write formula: ((0x0001^0x0004)&(0x0002^0x0004))^0x0004 = 0x0000
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 17, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut bus.ppu.oam, 18, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        enable_lcd_and_tick_to_row(&mut bus, 18); // dot=76, OamScan, row 18 (last on scan 1)
        bus.begin_instruction(); // pre-instruction snapshot (row 18)
        bus.notify_idu_glitch(0xFE00);
        let row18 = get_row_words(&bus.ppu.oam, 18);
        assert_eq!(
            row18,
            [0x0000, 0x0003, 0x0004, 0x0005],
            "notify_idu_glitch at OamScan row 18 must apply write corruption"
        );
    }

    #[test]
    fn test_notify_idu_glitch_at_frame_boundary_applies_corruption() {
        // Given: begin_instruction fires in VBlank (saved_oam_row would be None),
        // then the simulated fetch+execute ticks cross into OamScan of the new
        // frame, landing at dot=4 (row 1).
        // When: notify_idu_glitch is called after those ticks.
        // Then: OAM row 1 must be write-corrupted.
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        tick_to_late_vblank(&mut bus); // dot=452, scanline=153
        // Set OAM rows after ticking to avoid overwriting PPU-internal resets.
        // row 0: prev row data
        // row 1: a=0x0001  → should become 0x0000 after corruption
        set_row_words(&mut bus.ppu.oam, 0, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut bus.ppu.oam, 1, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        bus.begin_instruction(); // in VBlank → saved_oam_row would be None
        bus.tick(1); // simulate fetch M-cycle: dot=456→0, new frame, OamScan, row 0
        bus.tick(1); // simulate execute M-cycle: dot=4, OamScan, row 1
        bus.notify_idu_glitch(0xFE00);
        let row1 = get_row_words(&bus.ppu.oam, 1);
        assert_eq!(
            row1,
            [0x0000, 0x0003, 0x0004, 0x0005],
            "notify_idu_glitch must corrupt OAM when execute tick crosses into OamScan at frame boundary"
        );
    }

    #[test]
    fn test_notify_idu_glitch_no_corruption_at_row_0() {
        // Row 0 (dot=0) is immune — INC/DEC rp just before the frame wrap
        // must not corrupt OAM even though the execute tick lands in OamScan.
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        // Tick to a late-VBlank position such that the simulated execute tick
        // lands exactly at dot=0 (one M-cycle earlier than the frame-boundary test).
        tick_to_late_vblank(&mut bus); // dot=452, scanline=153
        set_row_words(&mut bus.ppu.oam, 0, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut bus.ppu.oam, 1, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        let snapshot = bus.ppu.oam;
        // Only ONE tick: lands at dot=0 of new frame (row 0 — immune).
        bus.tick(1); // dot=456→0, new frame, OamScan, row 0
        bus.notify_idu_glitch(0xFE00);
        assert_eq!(
            bus.ppu.oam, snapshot,
            "notify_idu_glitch at OamScan row 0 (dot=0) must not corrupt OAM"
        );
    }

    #[test]
    fn test_notify_idu_with_prior_read_applies_complex_corruption_in_mode_2() {
        // Given: PPU at row 5 (dot 20); rows 3, 4, 5 have known values.
        // a=0x00A0 (row3), b=0x0055 (row4[0]), c=0x000F (row5[0]), d=0x00C0 (row4[2])
        // new_b = 0x0045; all three rows become [0x0045, 0x0011, 0x00C0, 0x0022]
        // Address $FEA8 is in the forbidden zone ($FEA0–$FEFF): only the notify
        // hook applies corruption (Ppu::read_oam is not called for this range).
        use crate::gb::bus::GbBus;
        let mut bus = make_bus();
        set_row_words(&mut bus.ppu.oam, 3, [0x00A0, 0x0001, 0x0002, 0x0003]);
        set_row_words(&mut bus.ppu.oam, 4, [0x0055, 0x0011, 0x00C0, 0x0022]);
        set_row_words(&mut bus.ppu.oam, 5, [0x000F, 0x0099, 0x0088, 0x0077]);
        enable_lcd_and_tick_to_row(&mut bus, 5);
        bus.notify_idu_with_prior_read(0xFEA8); // forbidden zone address
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

    // ── APU register routing ──────────────────────────────────────────────────

    #[test]
    fn test_apu_nr52_power_on_via_bus() {
        // Given: write NR52=$80 via bus; When: read back via bus; Then: bit 7 is set.
        let mut bus = make_bus();
        bus.write(0xFF26, 0x80);
        assert_eq!(bus.read(0xFF26) & 0x80, 0x80);
    }

    #[test]
    fn test_apu_nr50_write_read_via_bus() {
        // Given: APU powered on; When: write NR50=$77 via bus; Then: read back $77.
        let mut bus = make_bus();
        bus.write(0xFF26, 0x80); // power on
        bus.write(0xFF24, 0x77);
        assert_eq!(bus.read(0xFF24), 0x77);
    }

    #[test]
    fn test_apu_nr51_write_read_via_bus() {
        let mut bus = make_bus();
        bus.write(0xFF26, 0x80);
        bus.write(0xFF25, 0xF3);
        assert_eq!(bus.read(0xFF25), 0xF3);
    }

    #[test]
    fn test_apu_registers_return_cleared_values_when_powered_off() {
        // When the APU is powered off, NR10–NR51 return their cleared values
        // (writes are ignored; registers were cleared on power-off).
        let mut bus = make_bus();
        // APU starts powered off. Write a non-zero value to NR51 — should be ignored.
        bus.write(0xFF25, 0xAB);
        // Read NR51: must return 0x00 (cleared value) because APU is off.
        assert_eq!(
            bus.read(0xFF25),
            0x00,
            "NR51 must read 0x00 (cleared) when APU is powered off"
        );
        // Same for NR50.
        bus.write(0xFF24, 0x77);
        assert_eq!(
            bus.read(0xFF24),
            0x00,
            "NR50 must read 0x00 (cleared) when APU is powered off"
        );
    }

    #[test]
    fn test_apu_wave_ram_accessible_via_bus() {
        let mut bus = make_bus();
        bus.write(0xFF30, 0xAB);
        assert_eq!(bus.read(0xFF30), 0xAB);
        bus.write(0xFF3F, 0xCD);
        assert_eq!(bus.read(0xFF3F), 0xCD);
    }

    #[test]
    fn test_apu_sample_ready_after_ticks() {
        // After ticking enough M-cycles the APU should have a sample ready.
        let mut bus = make_bus();
        for _ in 0..30u8 {
            bus.tick(1);
        }
        assert!(
            bus.sample_ready(),
            "APU sample must be ready after 30 M-cycles"
        );
    }

    #[test]
    fn test_apu_take_sample_returns_value_and_clears_ready() {
        let mut bus = make_bus();
        for _ in 0..30u8 {
            bus.tick(1);
        }
        assert!(bus.take_sample().is_some());
        assert!(!bus.sample_ready());
    }

    #[test]
    fn test_apu_sample_rate_preserved_across_reset() {
        // Given: sample rate set to 48 000 Hz (not the default 44 100 Hz).
        // When: bus.reset() is called.
        // Then: the APU still generates samples at 48 000 Hz.
        //
        // At 48 000 Hz: cycles_per_sample ≈ 21.8 M-cycles → sample ready after 22 ticks.
        // At 44 100 Hz (default): cycles_per_sample ≈ 23.8 M-cycles → NOT ready after 22 ticks.
        let mut bus = make_bus();
        bus.set_audio_sample_rate(48_000.0);
        bus.reset();
        for _ in 0..22u8 {
            bus.tick(1);
        }
        assert!(
            bus.sample_ready(),
            "sample rate must be preserved as 48 000 Hz after reset; \
             if it reverted to 44 100 Hz, the first sample arrives after ~24 M-cycles, not 22"
        );
    }

    // ── OAM DMA ──────────────────────────────────────────────────────────────

    /// Trigger DMA from WRAM ($C0xx) to avoid PPU VRAM-access restrictions.
    fn start_dma_from_wram(bus: &mut DmgBus) {
        // Fill WRAM $C000–$C09F with a known pattern.
        for i in 0..0xA0u16 {
            bus.wram[i as usize] = (i as u8).wrapping_add(1);
        }
        // Write $C0 to $FF46 to start DMA from $C000.
        bus.write(0xFF46, 0xC0);
    }

    #[test]
    fn test_oam_dma_read_returns_ff_while_active() {
        // Hardware timing: after writing $FF46, there is 1 warm-up M-cycle where
        // OAM is still accessible (position 0 → 1 on tick).  OAM blocking begins
        // on the first copy tick (position 1 → 2).
        //
        // Use a sentinel value (0xAB, not 0xFF) written directly to OAM before
        // starting DMA so we can unambiguously detect accessibility vs blocking.
        let mut bus = make_bus();
        bus.ppu.oam[0] = 0xAB; // sentinel at $FE00
        bus.ppu.oam[0x9F] = 0xCD; // sentinel at $FE9F
        start_dma_from_wram(&mut bus);

        // Position 0 (no ticks): warm-up, OAM still accessible — reads sentinel.
        assert_eq!(
            bus.read(0xFE00),
            0xAB,
            "OAM must return sentinel during DMA warm-up (position 0)"
        );

        // After 1 tick (warm-up completes, position = 1): still accessible.
        bus.tick(1);
        assert_eq!(
            bus.read(0xFE00),
            0xAB,
            "OAM must return sentinel after warm-up tick (position 1)"
        );

        // After 2nd tick (first copy tick, position = 2): OAM now blocked.
        bus.tick(1);
        assert_eq!(
            bus.read(0xFE00),
            0xFF,
            "OAM read must return $FF once copy phase begins"
        );
        assert_eq!(
            bus.read(0xFE9F),
            0xFF,
            "OAM read at last byte must return $FF during copy phase"
        );
    }

    #[test]
    fn test_oam_dma_write_is_ignored_while_active() {
        // Writes to OAM are blocked only during the copy phase (after warm-up).
        let mut bus = make_bus();
        start_dma_from_wram(&mut bus);
        bus.tick(162); // complete DMA
        // Write a sentinel value directly to OAM via bus — no DMA active.
        bus.write(0xFE00, 0xAB);
        assert_eq!(
            bus.ppu.oam[0], 0xAB,
            "sanity: write succeeds when DMA is inactive"
        );

        // Restart DMA and wait for blocking to begin (2 ticks), then attempt a write.
        start_dma_from_wram(&mut bus);
        bus.tick(2); // position = 2: copy phase, OAM now blocked
        bus.write(0xFE00, 0xFF); // must be ignored
        bus.tick(160); // complete remaining 160 ticks
        assert_eq!(
            bus.ppu.oam[0], bus.wram[0],
            "CPU write during DMA copy phase must be ignored; OAM must contain DMA data"
        );
    }

    #[test]
    fn test_oam_dma_completes_after_162_ticks() {
        // Given: DMA triggered; When: ticked 161 times; Then: DMA still active.
        // When: ticked 1 more time (total 162); Then: DMA inactive, OAM accessible.
        let mut bus = make_bus();
        start_dma_from_wram(&mut bus);
        bus.tick(161);
        assert!(bus.dma_active, "DMA must still be active after 161 ticks");
        bus.tick(1); // 162nd tick — teardown
        assert!(!bus.dma_active, "DMA must be inactive after 162 ticks");
        // OAM is now accessible and contains the transferred data.
        assert_ne!(
            bus.read(0xFE00),
            0xFF,
            "OAM must be accessible (not blocked) after DMA completes"
        );
    }

    #[test]
    fn test_oam_dma_copies_bytes_to_oam() {
        // Given: WRAM $C000–$C09F filled with a known pattern and DMA triggered;
        // When: DMA completes (162 ticks); Then: OAM contains those bytes exactly.
        let mut bus = make_bus();
        start_dma_from_wram(&mut bus);
        bus.tick(162);
        for i in 0..0xA0usize {
            assert_eq!(
                bus.ppu.oam[i],
                (i as u8).wrapping_add(1),
                "OAM[{i}] must equal source byte after DMA"
            );
        }
    }

    #[test]
    fn test_oam_dma_ff46_read_returns_source() {
        // $FF46 read always returns the last-written DMA source byte,
        // regardless of whether a transfer is in progress.
        let mut bus = make_bus();
        bus.write(0xFF46, 0x9F);
        bus.tick(162); // complete transfer
        assert_eq!(
            bus.read(0xFF46),
            0x9F,
            "$FF46 must return last-written value after DMA"
        );

        bus.write(0xFF46, 0xC0);
        assert_eq!(
            bus.read(0xFF46),
            0xC0,
            "$FF46 must return latest value mid-transfer"
        );
    }

    #[test]
    fn test_oam_dma_restart_preserves_blocking() {
        // When DMA is restarted while already running, OAM must remain blocked
        // during the warm-up of the new transfer (no 1-cycle grace window).
        let mut bus = make_bus();
        start_dma_from_wram(&mut bus);
        bus.tick(2); // enter copy phase — OAM is now blocked

        // Restart DMA
        bus.write(0xFF46, 0xC0);
        // After warm-up tick of the restarted DMA, OAM must still be blocked
        bus.tick(1);
        assert_eq!(
            bus.read(0xFE00),
            0xFF,
            "OAM must remain blocked during warm-up of a restarted DMA"
        );
    }

    #[test]
    fn oam_dma_conflict_feeds_source_byte_to_cpu_reads_outside_hram() {
        let mut bus = make_bus();
        bus.wram[0] = 0x06;
        bus.wram[1] = 0x13;
        bus.write(0xFF46, 0xC0);

        bus.tick(2);
        assert_eq!(bus.read_cpu_m_cycle(0x2000), 0x06);
        bus.tick(1);
        assert_eq!(bus.read_cpu_m_cycle(0x2001), 0x13);
    }

    #[test]
    fn oam_dma_conflict_blocks_cpu_writes_outside_hram_but_allows_hram() {
        let mut bus = make_bus();
        bus.wram[0x0100] = 0x42;
        bus.write(0xFF46, 0xC0);
        bus.tick(2);

        bus.write_cpu_m_cycle(0xC100, 0x99);
        bus.write_cpu_m_cycle(0xFF80, 0x77);

        assert_eq!(bus.wram[0x0100], 0x42);
        assert_eq!(bus.hram[0], 0x77);
    }

    #[test]
    fn oam_dma_from_vram_allows_cpu_reads_outside_oam() {
        let mut bus = make_bus();
        bus.wram[0x1DFF] = 0xE8;
        bus.ppu.vram[0] = 0x42;
        bus.write(0xFF46, 0x80);
        bus.tick(2);

        assert_eq!(bus.read_cpu_m_cycle(0xFDFF), 0xE8);
        assert_eq!(bus.read_cpu_m_cycle(0xFE00), 0xFF);
    }

    #[test]
    fn oam_dma_register_remains_cpu_accessible_during_dma() {
        let mut bus = make_bus();
        bus.write(0xFF46, 0xC0);
        bus.tick(2);

        assert_eq!(bus.read_cpu_m_cycle(0xFF46), 0xC0);
        bus.write_cpu_m_cycle(0xFF46, 0x80);
        assert_eq!(bus.read(0xFF46), 0x80);
    }
}
