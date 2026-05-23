use crate::gb::apu::Apu;
use crate::gb::boot_rom::{CGB_BOOT_ROM, CGB0_BOOT_ROM};
use crate::gb::bus::GbBus;
use crate::gb::bus::hdma::{HdmaAction, HdmaState};
use crate::gb::cartridge::GbCartridge;
use crate::gb::compat_palettes;
use crate::gb::input::joypad::Joypad;
use crate::gb::model::CgbModel;
use crate::gb::ppu::Ppu;
use crate::gb::ppu::timing::PpuMode;
use crate::gb::timer::{DIV_APU_BIT_DOUBLE, DIV_APU_BIT_NORMAL, Timer};

/// Full CGB (Game Boy Color) memory bus.
///
/// Implements the CGB memory map for use with the generic `Gb<CgbBus>` console.
/// Supports all CGB-specific PPU registers: VRAM bank (`$FF4F`), color palettes
/// (`$FF68`–`$FF6B`), and object priority mode (`$FF6C`).
///
/// Boot ROM support:
/// - When `boot_rom_active` is true, reads from `$0000–$00FF` and `$0200–$08FF`
///   return data from the boot ROM instead of the cartridge.
/// - Reads from `$0100–$01FF` always return cartridge data (header gap).
/// - Writing any value to `$FF50` unmaps the boot ROM.
///
/// CGB-specific features:
/// - CGB-mode PPU (`Ppu::new_cgb()`): VRAM bank 1, color palette RAM.
/// - `$FF4F`, `$FF68`–`$FF6B`, `$FF6C` route to CGB PPU helpers.
/// - `$FF6C` OPRI: object priority mode register.
/// - WRAM banking (`$FF70` / SVBK): 8 × 4 KB banks; bank 0 at `$C000–$CFFF`,
///   switchable banks 1–7 at `$D000–$DFFF`.
/// - Double-speed mode supported via KEY1 ($FF4D).
///
/// HDMA ($FF51–$FF55) supports both GDMA (immediate bulk transfer) and
/// HDMA (HBlank DMA: 16 bytes per HBlank, synchronized with PPU Mode 3→0).
///
/// Memory map (same as DMG unless noted):
/// - `$0000–$00FF`: Boot ROM (when active) or Cartridge ROM
/// - `$0100–$01FF`: Cartridge ROM (header, always from cartridge)
/// - `$0200–$08FF`: Boot ROM (when active) or Cartridge ROM
/// - `$0900–$7FFF`: Cartridge ROM
/// - `$8000–$9FFF`: VRAM (bank-switched in CGB mode via `$FF4F`)
/// - `$A000–$BFFF`: Cartridge RAM
/// - `$C000–$CFFF`: WRAM bank 0 (fixed)
/// - `$D000–$DFFF`: WRAM banks 1–7 (switchable via `$FF70`)
/// - `$E000–$FDFF`: Echo RAM
/// - `$FE00–$FE9F`: OAM
/// - `$FF40–$FF4B`: PPU registers (same as DMG)
/// - `$FF4F`:        VBK — VRAM bank select
/// - `$FF50`:        Boot ROM disable (write-only)
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
    /// Serial control register (SC / $FF02). Bit 7 = transfer start, cleared by hardware.
    sc: u8,
    /// Serial data register (SB / $FF01).
    sb: u8,
    /// CGB VRAM DMA (HDMA/GDMA) state for registers $FF51–$FF55.
    hdma: HdmaState,
    /// Number of M-cycles the CPU should be halted due to active HDMA transfer.
    /// When HDMA performs a transfer, this is set to 8 per block transferred.
    /// For GDMA, this can be up to 128 blocks × 8 cycles = 1024 M-cycles.
    /// The CPU should stall for this many M-cycles, calling tick(1) each cycle
    /// without executing instructions.
    hdma_halt_cycles: u16,
    /// SVBK register ($FF70): selects the active WRAM bank for $D000–$DFFF.
    svbk: u8,
    /// KEY1 register ($FF4D): CGB speed switch.
    /// Bit 7 = current speed (0=normal, 1=double), bit 0 = switch armed.
    key1: u8,
    /// Accumulator for half-rate APU ticking in double-speed mode.
    apu_tick_accumulator: u8,
    /// Double-speed APU tick accumulator phase when the APU was powered on.
    apu_power_on_accumulator: u8,
    /// Hardware model variant (CGB-0 through CGB-E).
    /// Stored for model-specific hardware initialization (DIV counter initial
    /// state, post-boot register values).
    model: CgbModel,
    /// Undocumented CGB register $FF72 (fully R/W, initial value $00).
    /// Pan Docs: "CGB Registers" — undocumented registers section.
    ff72: u8,
    /// Undocumented CGB register $FF73 (fully R/W, initial value $00).
    ff73: u8,
    /// Undocumented CGB register $FF74 (fully R/W in CGB mode, initial value $FF).
    /// Per Mooneye boot_hwio-C test (verified against real hardware).
    ff74: u8,
    /// Undocumented CGB register $FF75 (bits 4-6 R/W, initial value $00).
    ff75: u8,
    /// Boot ROM contents (2048 bytes).
    /// The CGB boot ROM is split: $0000-$00FF (256 bytes) and $0200-$08FF (1792 bytes).
    /// The array stores both regions contiguously; addresses $0100-$01FF are a gap
    /// that always reads from the cartridge.
    boot_rom: [u8; 2048],
    /// When `true`, reads from `$0000–$00FF` and `$0200–$08FF` are satisfied by
    /// `boot_rom` instead of the cartridge. Writing any value to `$FF50` sets
    /// this to `false` (mirrors real CGB hardware behaviour).
    boot_rom_active: bool,
    /// When `true`, the boot ROM is skipped on reset (starts at $0100 with
    /// post-boot register state). Set at construction time.
    skip_boot_rom: bool,
    /// KEY0 register ($FF4C): CGB CPU mode select.
    /// Bit 2 = DMG compatibility mode (0 = CGB mode, 1 = DMG mode).
    /// Written by the boot ROM based on cartridge header ($0143).
    /// Locked (writes ignored) after boot ROM unmaps.
    key0: u8,
    /// Whether KEY0 is locked (writes ignored). Set when $FF50 is written.
    key0_locked: bool,
}

impl CgbBus {
    fn needs_mode3_lcdc_write_phase(&self, addr: u16, val: u8) -> bool {
        const LCDC_TILE_DATA: u8 = 0x10;

        addr == 0xFF40
            && self.ppu.is_lcd_enabled()
            && self.ppu.mode() == PpuMode::PixelTransfer
            && self.ppu.read_register(0xFF40) & LCDC_TILE_DATA != val & LCDC_TILE_DATA
    }

    /// Create a new CGB bus.
    ///
    /// When `skip_boot_rom` is `false` (default hardware behavior), the boot ROM
    /// is active and the CPU should start execution at `$0000`. When `skip_boot_rom`
    /// is `true`, the boot ROM is disabled and hardware is initialized to post-boot
    /// state, with the CPU starting at `$0100`.
    ///
    /// The `model` determines variant-specific hardware initialization (DIV counter
    /// initial state for tests like boot_div-cgb0.gb).
    ///
    /// When `skip_boot_rom` is `true`, callers should call
    /// `Sm83::reset_registers_cgb_for_model()` on the CPU to set the CGB post-boot
    /// register state matching the model variant.
    pub fn new(cart: Box<dyn GbCartridge>, model: CgbModel, skip_boot_rom: bool) -> Self {
        let is_cgb = cart.is_cgb();
        // When skipping boot ROM, determine KEY0/OPRI values from cartridge header.
        // DMG-only cartridges ($0143 bit 7 clear) need DMG compatibility mode.
        let (key0_value, opri_value) = if skip_boot_rom {
            if is_cgb {
                // CGB-compatible: KEY0 = CGB flag, OPRI = $00
                (cart.read(0x0143), 0x00)
            } else {
                // DMG-only: KEY0 = $04 (DMG mode), OPRI = $01
                (0x04, 0x01)
            }
        } else {
            // Boot ROM will set these values
            (0x00, 0x00)
        };

        let mut bus = Self {
            cart,
            ppu: Ppu::new_cgb(),
            wram: [[0u8; 0x1000]; 8],
            hram: [0u8; 0x7F],
            timer: Timer::new(),
            joypad: Joypad::new(),
            // This is always a CGB bus, even when a DMG-only cartridge runs in
            // CGB DMG-compatibility mode.
            apu: Apu::new(true),
            // IF = $E1 at CGB boot ROM exit (VBlank flag set).
            // IF stores only lower 5 bits; upper bits read as 1 via read mask.
            // Store internal value $01 (VBlank set), readback produces $E1.
            if_reg: 0x01,
            ie_reg: 0,
            dma_active: false,
            // DMA = $00 at CGB boot (different from DMG which has $FF).
            dma_source: 0x00,
            dma_position: 0,
            dma_oam_blocked: false,
            sc: 0,
            sb: 0,
            hdma: HdmaState::new(),
            hdma_halt_cycles: 0,
            svbk: 0,
            key1: 0,
            apu_tick_accumulator: 0,
            apu_power_on_accumulator: 0,
            model,
            // Undocumented CGB registers.
            // $FF72-$FF73: fully R/W, initial value $00.
            // $FF74: R/W in CGB mode, initial value $FF (per Mooneye boot_hwio-C test).
            // $FF75: bits 4-6 R/W, reads return value | $8F, initial value $00.
            ff72: 0x00,
            ff73: 0x00,
            ff74: 0xFF, // Value on reset per Mooneye test and Pan Docs
            ff75: 0x00,
            // Select boot ROM based on model: CGB-0 uses CGB0_BOOT_ROM (no wave RAM init),
            // all other CGB models use CGB_BOOT_ROM (with wave RAM init).
            boot_rom: match model {
                CgbModel::Cgb0 => CGB0_BOOT_ROM,
                CgbModel::CgbA
                | CgbModel::CgbB
                | CgbModel::CgbC
                | CgbModel::CgbD
                | CgbModel::CgbE => CGB_BOOT_ROM,
            },
            boot_rom_active: !skip_boot_rom,
            skip_boot_rom,
            // KEY0 value depends on boot mode and cartridge type.
            key0: key0_value,
            key0_locked: skip_boot_rom, // If skipping boot ROM, KEY0 is already locked.
        };

        // Set OPRI based on cartridge type when skipping boot ROM.
        bus.ppu.set_cgb_model(model);
        // Propagate the CGB hardware revision before seeding skip-boot APU
        // state (NR52 and channel registers), since channel triggers can observe
        // model-specific timing such as CH1 restart_hold.
        bus.apu.set_cgb_model(model);
        if skip_boot_rom && opri_value != 0 {
            bus.ppu.write_cgb_register(0xFF6C, opri_value);
        }
        if skip_boot_rom {
            bus.seed_skip_boot_post_boot_state();
        }

        // Apply DMG compatibility palette when skipping boot ROM for DMG-only games.
        // DMG-only games (bit 7 of $0143 clear) need colorization palettes.
        if skip_boot_rom && !is_cgb {
            // Read ROM header bytes $0100-$014B for palette lookup.
            let mut header = [0u8; 0x4C];
            for (i, byte) in header.iter_mut().enumerate() {
                *byte = bus.cart.read(0x0100 + i as u16);
            }
            let palette = compat_palettes::get_palette_colors(&header);
            bus.ppu
                .apply_dmg_compat_palettes(&palette.bg0, &palette.obj0, &palette.obj1);
            // Enable DMG-compat mode for correct OBJ palette selection during rendering.
            bus.ppu.set_dmg_compat(true);
        }
        // Initialize DIV to post-boot value for CGB models.
        // The internal 16-bit counter value must be precisely set for Mooneye
        // boot_div tests to pass. These tests verify the exact phase alignment
        // of DIV relative to M-cycle timing after boot ROM hand-off.
        //
        // The boot_div test performs 6 DIV reads with varying NOP counts:
        //   Read 1: 27 NOPs → DIV=$27 (just after increment)
        //   Read 2: +57 NOPs → DIV=$28 (just after increment)
        //   Read 3: +56 NOPs → DIV=$28 (just BEFORE increment - phase shifted)
        //   Read 4: +57 NOPs → DIV=$29 (just before increment)
        //   Read 5: +57 NOPs → DIV=$2A (just before increment)
        //   Read 6: +58 NOPs → DIV=$2C (just after increment - phase restored)
        //
        // For Read 3 to catch DIV=$28 instead of $29, the counter at Read 2
        // must be exactly $2800 (not $2814). Working backwards:
        //   Read 2 counter = $2800
        //   Read 1 counter = $2800 - 256 = $2700
        //   Total M-cycles to Read 1 = 1(NOP) + 4(JP) + 27(NOPs) + 3(LDH) = 35
        //   Initial counter = $2700 - 35*4 = $2700 - 140 = $2674
        //
        // Similar calculation for CGB-0 (24 NOPs instead of 27):
        //   Total M-cycles to Read 1 = 1 + 4 + 24 + 3 = 32
        //   For counter at Read 1 = $2900, initial = $2900 - 32*4 = $2900 - 128 = $2880
        //
        // Reference: Mooneye boot_div-cgb0.s and boot_div-cgbABCDE.s
        let initial_div_counter = match model {
            CgbModel::Cgb0 => 0x2880,
            CgbModel::CgbA | CgbModel::CgbB | CgbModel::CgbC | CgbModel::CgbD | CgbModel::CgbE => {
                0x2674
            }
        };
        bus.timer.set_div_counter(initial_div_counter);

        bus
    }

    fn seed_skip_boot_post_boot_state(&mut self) {
        self.if_reg = 0x01;
        self.joypad.write(0x30);
        self.apu.write_nr52_with_div_state(0x80, false);
        self.apu.write_register(0xFF11, 0x80);
        self.apu.write_register(0xFF12, 0xF3);
        self.apu.write_register(0xFF14, 0x80);
        self.apu.write_register(0xFF24, 0x77);
        self.apu.write_register(0xFF25, 0xF3);

        self.ppu.seed_boot_registered_mark_tile();
        self.ppu.write_register(0xFF40, 0x91);
        self.ppu.write_register(0xFF42, 0x00);
        self.ppu.write_register(0xFF43, 0x00);
        self.ppu.write_register(0xFF45, 0x00);
        self.ppu.write_register(0xFF47, 0xFC);
        self.ppu.write_register(0xFF4A, 0x00);
        self.ppu.write_register(0xFF4B, 0x00);

        self.svbk = 0x07;
        self.ppu.write_cgb_register(0xFF68, 0xC8);
        self.ppu.write_cgb_register(0xFF6A, 0xD0);
    }

    /// Returns the CGB hardware model variant for this bus.
    pub fn model(&self) -> CgbModel {
        self.model
    }

    /// Returns the KEY0 register value ($FF4C).
    ///
    /// KEY0 controls CGB/DMG compatibility mode:
    /// - Bit 2 = 0: CGB mode (full CGB hardware features)
    /// - Bit 2 = 1: DMG compatibility mode (for DMG-only cartridges)
    pub fn key0(&self) -> u8 {
        self.key0
    }

    /// Returns `true` if KEY0 is locked (writes ignored).
    ///
    /// KEY0 becomes locked when the boot ROM unmaps (write to $FF50).
    pub fn is_key0_locked(&self) -> bool {
        self.key0_locked
    }

    /// Returns `true` while the boot ROM is still mapped at `$0000–$00FF` and `$0200–$08FF`.
    pub fn is_boot_rom_active(&self) -> bool {
        self.boot_rom_active
    }

    /// Read from boot ROM if active and address is in a boot ROM region.
    ///
    /// Returns `Some(value)` if boot ROM is active and `addr` is in $0000-$00FF
    /// or $0200-$08FF. Returns `None` otherwise (address should be read from
    /// cartridge or other memory).
    ///
    /// The boot ROM array maps: $0000-$00FF → [0..256], $0200-$08FF → [256..2048]
    fn boot_rom_read(&self, addr: u16) -> Option<u8> {
        if !self.boot_rom_active {
            return None;
        }
        match addr {
            0x0000..=0x00FF => Some(self.boot_rom[addr as usize]),
            0x0200..=0x08FF => Some(self.boot_rom[(addr - 0x0100) as usize]),
            _ => None,
        }
    }

    /// Check if the CPU should be halted for HDMA and consume one halt cycle.
    ///
    /// Returns `true` if the CPU should perform a halt stall (tick subsystems
    /// without executing an instruction). The caller should call `tick(1)` and
    /// skip instruction execution for this M-cycle.
    pub fn consume_hdma_halt_cycle(&mut self) -> bool {
        if self.hdma_halt_cycles > 0 {
            self.hdma_halt_cycles -= 1;
            // if self.hdma_halt_cycles == 0 {
            //     eprintln!("[DMA] CPU halt complete - resuming instruction execution");
            // }
            true
        } else {
            false
        }
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
    /// 2. Resets the DIV counter to 0 (may trigger DIV-APU event).
    /// 3. Toggles the speed (KEY1 bit 7) and clears the arm bit (bit 0).
    /// 4. Updates the Timer's DIV-APU bit for the new speed mode.
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
        // This may trigger a DIV-APU event if the DIV-APU bit was high.
        let div_apu_edge = self.timer.write(0xFF04, 0);
        if div_apu_edge {
            self.apu.clock_div_apu();
        }

        // Toggle speed and clear arm bit.
        self.key1 ^= 0x80;
        self.key1 &= !0x01;

        // Update the Timer's DIV-APU bit for the new speed mode.
        // In double-speed mode, use bit 13 (DIV bit 5) instead of bit 12 (DIV bit 4)
        // to maintain the same 512 Hz frame sequencer rate.
        let new_div_apu_bit = if self.is_double_speed() {
            DIV_APU_BIT_DOUBLE
        } else {
            DIV_APU_BIT_NORMAL
        };
        self.timer.set_div_apu_bit(new_div_apu_bit);

        // Reset APU accumulator when switching speeds.
        self.apu_tick_accumulator = 0;
        self.apu_power_on_accumulator = 0;

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
    ///
    /// The APU frame sequencer is clocked by DIV-APU falling edges from the
    /// timer (bit 12 of the 16-bit internal counter = DIV bit 4, or bit 13 in
    /// double-speed mode = DIV bit 5).
    pub fn tick(&mut self, m_cycles: u8) {
        let double = self.tick_before_ppu(m_cycles);
        let dots_per_mcycle = Self::dots_per_mcycle(double);
        self.ppu.tick_dots(u32::from(m_cycles) * dots_per_mcycle);
        self.tick_after_ppu(m_cycles, double);
    }

    fn dots_per_mcycle(double_speed: bool) -> u32 {
        if double_speed { 2 } else { 4 }
    }

    fn tick_before_ppu(&mut self, m_cycles: u8) -> bool {
        self.if_reg |= self.ppu.take_pending_interrupts();

        let double = self.is_double_speed();

        for _ in 0..m_cycles {
            let (div_apu_falling, div_apu_rising) = self.timer.tick(1);
            if self.timer.interrupt_pending {
                self.if_reg |= 0x04;
                self.timer.interrupt_pending = false;
            }

            // Rising edge fires the APU secondary event (for envelope phantom-tick detection).
            for _ in 0..div_apu_rising {
                self.apu.clock_div_apu_secondary();
            }
            // Clock APU frame sequencer for each DIV-APU falling edge.
            for _ in 0..div_apu_falling {
                self.apu.clock_div_apu();
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
        double
    }

    fn tick_after_ppu(&mut self, m_cycles: u8, double: bool) {
        if double {
            // In double-speed mode the APU runs at normal speed (half the CPU M-cycle
            // rate). CH3 wave-position timing requires 1-APU-cycle-per-M-cycle
            // granularity, so it is ticked once per CPU M-cycle with 1 cycle.
            // All other channels are still ticked at the half rate via the accumulator.
            for _ in 0..m_cycles {
                self.apu.tick_ch3_one_apu_cycle();
            }
            self.apu_tick_accumulator += m_cycles;
            let apu_ticks = self.apu_tick_accumulator / 2;
            self.apu_tick_accumulator %= 2;
            if apu_ticks > 0 {
                self.apu.tick_except_ch3(apu_ticks);
            }
        } else {
            self.apu.tick(m_cycles);
        }
        // Tick the cartridge (for MBC3 RTC)
        self.cart.tick(u32::from(m_cycles));

        // HDMA state machine: activate pending HDMA when LCD turns on.
        // HDMA requested with LCD off remains pending until LCD is enabled.
        // Once activated, actual transfers occur during HBlank periods (checked below).
        if self.hdma.is_hdma_pending() && self.ppu.is_lcd_enabled() {
            self.hdma.activate_hdma();
            // Clear any pending HBlank flag so we wait for the NEXT HBlank after LCD enable.
            // Per Pan Docs: transfers occur at LY=0-143 during HBlank, starting from the
            // first full scanline after LCD is enabled.
            self.ppu.take_hblank_entered();
        }

        // HDMA: transfer one 16-byte block per HBlank (Mode 3→0).
        // Transfers occur only when LCD is on and HBlank is entered.
        if self.hdma.is_active() && self.hdma.is_hblank_mode() {
            let hblank = self.ppu.take_hblank_entered();
            if hblank {
                self.do_hdma_block_transfer();
                // Signal CPU to halt for 8 M-cycles during the transfer.
                // Per Pan Docs: HDMA takes 8 M-cycles per block regardless of CPU speed mode.
                self.hdma_halt_cycles = 8;
            }
        }
    }

    /// Execute one HDMA block transfer (16 bytes from source to VRAM).
    /// Per Pan Docs: transfer takes 8 M-cycles regardless of double-speed mode.
    /// If destination address overflows beyond VRAM range ($2000), transfer stops prematurely.
    fn do_hdma_block_transfer(&mut self) {
        let vbk = self.ppu.vbk;
        let source = self.hdma.source();
        let dest = self.hdma.destination();

        // Transfer up to 16 bytes: read from source via read_raw, write directly to VRAM.
        // Stop after byte if destination overflows VRAM range.
        for i in 0u16..16 {
            let vram_offset = dest.wrapping_add(i) as usize;

            // Check for destination overflow: VRAM destination range is $0000–$1FFF (offset).
            // Per Pan Docs: if overflow occurs, transfer stops prematurely.
            if vram_offset >= 0x2000 {
                // Destination overflowed — advance addresses by bytes written, mark complete.
                self.hdma.advance_by_partial_block(i);
                return;
            }

            let byte = self.read_raw(source.wrapping_add(i));

            if vbk & 0x01 != 0 {
                self.ppu.vram_bank1[vram_offset] = byte;
            } else {
                self.ppu.vram[vram_offset] = byte;
            }
        }

        // Advance addresses and decrement remaining blocks.
        self.hdma.advance_after_block();
    }

    /// Execute a GDMA (General-Purpose DMA) — transfer all blocks at once.
    /// Called when $FF55 is written with bit 7 = 0 and no HDMA is active.
    fn do_gdma_transfer(&mut self) {
        let total_blocks = self.hdma.remaining_blocks() as u32 + 1;
        let mut blocks_transferred = 0u32;

        for _ in 0..total_blocks {
            self.do_hdma_block_transfer();
            blocks_transferred += 1;
            // Break early if transfer was force-completed due to overflow.
            if !self.hdma.is_active() {
                break;
            }
        }

        // Signal CPU to halt for 8 M-cycles per block transferred.
        // The CPU will stall, calling tick(1) per cycle without executing instructions.
        // For full 128-block GDMA, this can be up to 1024 M-cycles.
        self.hdma_halt_cycles = (8 * blocks_transferred) as u16;
    }

    /// Raw read bypassing PPU access blocking (used by OAM DMA).
    fn read_raw(&self, addr: u16) -> u8 {
        // Boot ROM interception (same as read() for consistency).
        if let Some(value) = self.boot_rom_read(addr) {
            return value;
        }
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
    ///
    /// Respects the `skip_boot_rom` setting from construction: if `skip_boot_rom`
    /// was true, the boot ROM remains disabled after reset.
    pub fn reset(&mut self) {
        let apu_rate = self.apu.sample_rate();
        self.ppu = Ppu::new_cgb();
        self.ppu.set_cgb_model(self.model);
        self.ppu.write_register(0xFF40, 0x00);
        self.timer = Timer::new();
        self.joypad = Joypad::new();
        self.apu = Apu::new(true);
        self.apu.set_sample_rate(apu_rate);
        self.apu.set_cgb_model(self.model);
        self.wram = [[0u8; 0x1000]; 8];
        self.hram = [0u8; 0x7F];
        self.if_reg = 0;
        self.ie_reg = 0;
        self.dma_active = false;
        self.dma_source = 0;
        self.dma_position = 0;
        self.dma_oam_blocked = false;
        self.hdma = HdmaState::new();
        self.sb = 0;
        self.svbk = 0;
        self.key1 = 0;
        self.apu_tick_accumulator = 0;
        self.apu_power_on_accumulator = 0;
        // Respect skip_boot_rom setting from construction
        self.boot_rom_active = !self.skip_boot_rom;
        // Reset undocumented CGB registers
        self.ff72 = 0x00;
        self.ff73 = 0x00;
        self.ff74 = 0xFF; // Initial value per Mooneye test
        self.ff75 = 0x00;
        // Reset KEY0 state: if boot ROM is active, unlock so boot ROM can write;
        // if skipping boot ROM, set appropriate value based on cartridge type.
        if self.skip_boot_rom {
            self.seed_skip_boot_post_boot_state();
            // Same logic as constructor: set KEY0/OPRI based on cartridge header
            let is_cgb = self.cart.is_cgb();
            if is_cgb {
                self.key0 = self.cart.read(0x0143);
            } else {
                self.key0 = 0x04;
                self.ppu.write_cgb_register(0xFF6C, 0x01); // OPRI for DMG mode
                // Apply DMG compatibility palettes for DMG-only games
                let mut header = [0u8; 0x4C];
                for (i, byte) in header.iter_mut().enumerate() {
                    *byte = self.cart.read(0x0100 + i as u16);
                }
                let palette = crate::gb::compat_palettes::get_palette_colors(&header);
                self.ppu
                    .apply_dmg_compat_palettes(&palette.bg0, &palette.obj0, &palette.obj1);
                self.ppu.set_dmg_compat(true);
            }
            self.key0_locked = true;
        } else {
            self.key0 = 0x00;
            self.key0_locked = false;
        }
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
            ff72: Some(self.ff72),
            ff73: Some(self.ff73),
            ff74: Some(self.ff74),
            ff75: Some(self.ff75),
            key0: Some(self.key0),
            key0_locked: Some(self.key0_locked),
            boot_rom_active: Some(self.boot_rom_active),
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
        self.ppu.set_cgb_model(self.model);
        self.ppu.fixup_after_state_load();
        for (bank, bank_data) in self.wram.iter_mut().enumerate() {
            let offset = bank * 0x1000;
            bank_data.copy_from_slice(&state.wram[offset..offset + 0x1000]);
        }
        self.hram = state.hram;
        self.timer = state.timer.clone();
        self.joypad = state.joypad.clone();
        self.apu = state.apu.clone();
        // Re-apply the configured CGB model after restoring the APU snapshot:
        // older save-states won't have `is_cgb`/`cgb_model` serialized on CH1
        // and would otherwise come back on the DMG/default model path.
        self.apu.set_cgb_model(self.model);
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
        self.apu_power_on_accumulator = 0;
        // Restore undocumented CGB registers with defaults for older save states
        self.ff72 = state.ff72.unwrap_or(0x00);
        self.ff73 = state.ff73.unwrap_or(0x00);
        self.ff74 = state.ff74.unwrap_or(0xFF);
        self.ff75 = state.ff75.unwrap_or(0x00);
        // Restore KEY0 state; default to locked with post-boot value for older save states
        self.key0 = state.key0.unwrap_or(0x00);
        self.key0_locked = state.key0_locked.unwrap_or(true);
        // Restore boot ROM state; default to inactive for older save states
        self.boot_rom_active = state.boot_rom_active.unwrap_or(false);
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
        // Boot ROM interception: $0000-$00FF and $0200-$08FF read from boot ROM
        // when active. $0100-$01FF always reads from cartridge (header gap).
        if let Some(value) = self.boot_rom_read(addr) {
            return value;
        }
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
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E, // SC: bits 6-1 unused, read as 1
            0xFF03 => 0xFF,           // unused I/O
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF08..=0xFF0E => 0xFF, // unused I/O range
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
            // CGB KEY1 — speed switch register
            0xFF4D if self.ppu.dmg_compat && self.skip_boot_rom => 0xFF,
            0xFF4D => (self.key1 & 0x80) | 0x7E | (self.key1 & 0x01),
            // CGB KEY0 — CPU mode select register (upper nibble reads as 1)
            0xFF4C if self.ppu.dmg_compat && self.skip_boot_rom => 0xFF,
            0xFF4C => self.key0 | 0xF0,
            0xFF4E => 0xFF,
            // CGB HDMA registers
            0xFF51..=0xFF55 if self.ppu.dmg_compat => 0xFF,
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            0xFF50 => 0xFF,
            0xFF56..=0xFF67 | 0xFF6D..=0xFF6F | 0xFF71 | 0xFF78..=0xFF7F => 0xFF, // Unused/reserved CGB I/O ranges
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
            // CGB undocumented registers ($FF72-$FF75) — Pan Docs "CGB Registers".
            // $FF72-$FF73: fully R/W, initial value $00.
            0xFF72 => self.ff72,
            0xFF73 => self.ff73,
            0xFF74 if self.ppu.dmg_compat => 0xFF,
            0xFF74 => self.ff74,
            // $FF75: bits 4-6 R/W, other bits read as 1.
            0xFF75 => self.ff75 | 0x8F,
            // CGB PCM registers
            0xFF76 => self.apu.read_pcm12(),
            0xFF77 => self.apu.read_pcm34(),
            0xFF70 if self.ppu.dmg_compat => 0xFF,
            0xFF70 => self.svbk | 0xF8,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
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
            0xFF01 => self.sb = val,
            0xFF02 => {
                // SC: Serial Control. Writing bit 7=1 starts transfer.
                // For stub implementation, immediately clear bit 7 to signal completion.
                self.sc = val & 0x7F;
            }
            0xFF03 => {}          // unused I/O
            0xFF08..=0xFF0E => {} // unused I/O range
            0xFF04..=0xFF07 => {
                let div_apu_edge = self.timer.write(addr, val);
                // If a DIV-APU falling edge occurred (DIV write with bit 4/5 HIGH),
                // clock the APU frame sequencer.
                if div_apu_edge {
                    self.apu.clock_div_apu();
                }
                if self.timer.fire_write_overflow_if_pending() {
                    self.if_reg |= 0x04;
                    self.timer.take_interrupt();
                }
            }
            0xFF0F => self.if_reg = val & 0x1F,
            0xFF26 => {
                // NR52 special handling: pass DIV-APU bit state for power-on skip logic.
                let div_apu_high = self.timer.is_div_apu_bit_high();
                let is_powering_on = val & 0x80 != 0 && self.apu.read_register(0xFF26) & 0x80 == 0;
                if is_powering_on {
                    self.apu_power_on_accumulator = self.apu_tick_accumulator & 1;
                }
                self.apu.write_nr52_with_div_state(val, div_apu_high);
            }
            0xFF10..=0xFF25 | 0xFF27..=0xFF3F => {
                // Bit-pack the two double-speed APU phases used by pulse trigger
                // timing: bit 0 is the trigger write phase, bit 1 is the NR52
                // power-on phase captured above.
                let double_speed_phase_bits = self.is_double_speed().then_some(
                    (self.apu_tick_accumulator & 1) | (self.apu_power_on_accumulator << 1),
                );
                self.apu.write_register_with_apu_phase_and_div_counter(
                    addr,
                    val,
                    double_speed_phase_bits,
                    Some(self.timer.raw_counter()),
                );
            }
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => {
                self.ppu.write_register(addr, val);
                self.if_reg |= self.ppu.take_pending_interrupts();
            }
            0xFF46 => self.do_oam_dma(val),
            // CGB KEY1 — only bit 0 (arm) is writable; bit 7 (current speed) is read-only
            0xFF4D if self.ppu.dmg_compat && self.skip_boot_rom => {}
            0xFF4D => self.key1 = (self.key1 & 0x80) | (val & 0x01),
            // CGB KEY0 — CPU mode select, locked after boot ROM unmaps
            0xFF4C if self.ppu.dmg_compat && self.skip_boot_rom => {}
            0xFF4C => {
                if !self.key0_locked {
                    self.key0 = val;
                }
            }
            0xFF4E => {}
            // CGB HDMA registers
            0xFF51..=0xFF55 if self.ppu.dmg_compat => {}
            0xFF51 => self.hdma.write_source_high(val),
            0xFF52 => self.hdma.write_source_low(val),
            0xFF53 => self.hdma.write_dest_high(val),
            0xFF54 => self.hdma.write_dest_low(val),
            0xFF55 => {
                // For HDMA timing decisions, check if we're in an HBlank-like state.
                // When LCD is off, mode is effectively 0 (HBlank) for HDMA purposes.
                // When LCD is on, use the PPU's physical mode (not STAT mode bits which
                // can lag due to mode_for_stat() timing quirks).
                let is_hblank = !self.ppu.is_lcd_enabled() || self.ppu.mode() == PpuMode::HBlank;
                match self.hdma.write_control(val) {
                    HdmaAction::StartGdma => {
                        self.do_gdma_transfer();
                    }
                    HdmaAction::StartHdma => {
                        // If HDMA is started while in HBlank (mode 0), transfer starts
                        // immediately regardless of whether LCD is on or off. When LCD is off,
                        // mode is effectively 0, so one block transfers instantly.
                        if is_hblank {
                            self.hdma.activate_hdma();
                            // Clear pending flag since we're activating now
                            self.hdma.clear_hblank_pending();
                            // Clear HBlank flag so we don't double-transfer in the same HBlank period
                            self.ppu.take_hblank_entered();
                            // Transfer one block immediately
                            self.do_hdma_block_transfer();
                            self.hdma_halt_cycles = 8;
                        }
                        // Otherwise, HDMA stays pending until HBlank occurs
                    }
                    HdmaAction::CancelHdma => {}
                }
            }
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => {
                self.ppu.write_cgb_register(addr, val);
            }
            0xFF50 => {
                // Boot ROM disable: writing any value unmaps the boot ROM.
                // This transition only happens once; subsequent writes are ignored so
                // DMG compatibility palettes are not re-selected mid-game.
                if self.boot_rom_active {
                    self.boot_rom_active = false;
                    self.key0_locked = true;
                    self.ppu.seed_boot_registered_mark_tile();

                    // Apply DMG compatibility palettes for DMG-only games.
                    // KEY0 = $04 indicates DMG compatibility mode (bit 2 set).
                    if self.key0 == 0x04 {
                        // Check for manual palette override via button combo.
                        let button_state = self.joypad.get_states();
                        let manual_palette_id =
                            compat_palettes::button_combo_to_palette_id(button_state);

                        let palette = if let Some(palette_id) = manual_palette_id {
                            // User held valid button combo: use manual palette selection.
                            compat_palettes::get_palette_colors_by_id(palette_id)
                        } else {
                            // No valid combo: use automatic palette based on title hash.
                            let mut header = [0u8; 0x4C];
                            for (i, byte) in header.iter_mut().enumerate() {
                                *byte = self.cart.read(0x0100 + i as u16);
                            }
                            compat_palettes::get_palette_colors(&header)
                        };

                        self.ppu.apply_dmg_compat_palettes(
                            &palette.bg0,
                            &palette.obj0,
                            &palette.obj1,
                        );
                        self.ppu.set_dmg_compat(true);
                    }
                }
            }
            // CGB undocumented registers ($FF72-$FF75) — Pan Docs "CGB Registers".
            0xFF72 => self.ff72 = val,
            0xFF73 => self.ff73 = val,
            0xFF74 if self.ppu.dmg_compat => {}
            0xFF74 => self.ff74 = val,
            // $FF75: only bits 4-6 are writable.
            0xFF75 => self.ff75 = val & 0x70,
            // CGB PCM registers (read-only; ignore writes)
            0xFF76 | 0xFF77 => {}
            0xFF70 if self.ppu.dmg_compat => {}
            0xFF70 => self.svbk = val & 0x07,
            0xFF56..=0xFF67 | 0xFF6D..=0xFF6F | 0xFF71 | 0xFF78..=0xFF7F => {}
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = val,
            0xFFFF => self.ie_reg = val,
        }
    }

    fn tick(&mut self, m_cycles: u8) {
        CgbBus::tick(self, m_cycles);
    }

    fn write_cpu_m_cycle(&mut self, addr: u16, val: u8) {
        if self.needs_mode3_lcdc_write_phase(addr, val) {
            let double = self.tick_before_ppu(1);
            let dots_per_mcycle = Self::dots_per_mcycle(double);
            self.ppu.tick_dots(dots_per_mcycle - 1);
            self.write(addr, val);
            self.ppu.tick_dots(1);
            self.tick_after_ppu(1, double);
        } else {
            self.tick(1);
            self.write(addr, val);
        }
    }

    fn try_speed_switch(&mut self) -> bool {
        CgbBus::try_speed_switch(self)
    }

    fn consume_hdma_halt_cycle(&mut self) -> bool {
        CgbBus::consume_hdma_halt_cycle(self)
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
        // Boot ROM interception (same as read() for consistency).
        if let Some(value) = self.boot_rom_read(addr) {
            return value;
        }
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
            0xFF01 => self.sb,
            0xFF02 => self.sc | 0x7E, // SC: bits 6-1 unused, read as 1
            0xFF03 => 0xFF,           // unused I/O
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF08..=0xFF0E => 0xFF, // unused I/O range
            0xFF0F => self.if_reg | 0xE0,
            0xFF10..=0xFF3F => self.apu.read_register(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma_source,
            // CGB KEY1 — speed switch register (debugger)
            0xFF4D if self.ppu.dmg_compat && self.skip_boot_rom => 0xFF,
            0xFF4D => (self.key1 & 0x80) | 0x7E | (self.key1 & 0x01),
            // CGB KEY0 — CPU mode select register (debugger)
            0xFF4C if self.ppu.dmg_compat && self.skip_boot_rom => 0xFF,
            0xFF4C => self.key0 | 0xF0,
            0xFF4E => 0xFF,
            // CGB HDMA registers
            0xFF51..=0xFF55 if self.ppu.dmg_compat => 0xFF,
            0xFF51..=0xFF54 => 0xFF, // HDMA1-4 are write-only
            0xFF55 => self.hdma.read_control(),
            0xFF50 => 0xFF,
            0xFF56..=0xFF67 | 0xFF6D..=0xFF6F | 0xFF71 | 0xFF78..=0xFF7F => 0xFF,
            // CGB-specific registers
            0xFF4F | 0xFF68..=0xFF6C => self.ppu.read_cgb_register(addr).unwrap_or(0xFF),
            // CGB undocumented registers ($FF72-$FF75).
            0xFF72 => self.ff72,
            0xFF73 => self.ff73,
            0xFF74 if self.ppu.dmg_compat => 0xFF,
            0xFF74 => self.ff74,
            0xFF75 => self.ff75 | 0x8F,
            // CGB PCM registers
            0xFF76 => self.apu.read_pcm12(),
            0xFF77 => self.apu.read_pcm34(),
            0xFF70 if self.ppu.dmg_compat => 0xFF,
            0xFF70 => self.svbk | 0xF8,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie_reg,
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

    /// Build a minimal DMG-only ROM cartridge, as used by Mooneye `*-C` tests
    /// when they run on CGB hardware in DMG compatibility mode.
    fn dmg_only_rom_cart() -> Box<dyn GbCartridge> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0x00; // DMG-only cartridge
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

    /// Create a CgbBus with boot ROM active (skip_boot_rom=false).
    /// This tests hardware DEFAULT values, not post-boot values.
    /// Use this for unit tests that verify register defaults and hardware functionality.
    fn make_bus() -> CgbBus {
        CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false)
    }

    /// Create a CgbBus with post-boot state (skip_boot_rom=true).
    /// Use this for tests that verify post-boot register values.
    #[allow(dead_code)]
    fn make_bus_post_boot() -> CgbBus {
        CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), true)
    }

    fn make_dmg_compat_bus_post_boot() -> CgbBus {
        CgbBus::new(dmg_only_rom_cart(), CgbModel::CgbE, true)
    }

    /// Enable LCD (needed for PPU to tick and reach HBlank).
    fn enable_lcd(bus: &mut CgbBus) {
        bus.write(0xFF40, 0x91); // LCD on, BG enabled
    }

    #[test]
    fn test_dmg_compat_post_boot_hwio_matches_mooneye_cgb_expectations() {
        let mut bus = make_dmg_compat_bus_post_boot();

        let expected: &[(u16, u8)] = &[
            (0xFF00, 0xFF),
            (0xFF01, 0x00),
            (0xFF02, 0x7E),
            (0xFF04, 0x26),
            (0xFF05, 0x00),
            (0xFF06, 0x00),
            (0xFF07, 0xF8),
            (0xFF08, 0xFF),
            (0xFF0F, 0xE1),
            (0xFF10, 0x80),
            (0xFF11, 0xBF),
            (0xFF12, 0xF3),
            (0xFF13, 0xFF),
            (0xFF14, 0xBF),
            (0xFF15, 0xFF),
            (0xFF16, 0x3F),
            (0xFF17, 0x00),
            (0xFF18, 0xFF),
            (0xFF19, 0xBF),
            (0xFF1A, 0x7F),
            (0xFF1B, 0xFF),
            (0xFF1C, 0x9F),
            (0xFF1D, 0xFF),
            (0xFF1E, 0xBF),
            (0xFF1F, 0xFF),
            (0xFF20, 0xFF),
            (0xFF21, 0x00),
            (0xFF22, 0x00),
            (0xFF23, 0xBF),
            (0xFF24, 0x77),
            (0xFF25, 0xF3),
            (0xFF26, 0xF1),
            (0xFF27, 0xFF),
            (0xFF42, 0x00),
            (0xFF43, 0x00),
            (0xFF45, 0x00),
            (0xFF46, 0x00),
            (0xFF47, 0xFC),
            (0xFF4A, 0x00),
            (0xFF4B, 0x00),
            (0xFF4C, 0xFF),
            (0xFF4D, 0xFF),
            (0xFF4E, 0xFF),
            (0xFF4F, 0xFE),
            (0xFF50, 0xFF),
            (0xFF68, 0xC8),
            (0xFF69, 0xFF),
            (0xFF6A, 0xD0),
            (0xFF6B, 0xFF),
            (0xFF70, 0xFF),
            (0xFF72, 0x00),
            (0xFF73, 0x00),
            (0xFF74, 0xFF),
            (0xFF75, 0x8F),
            (0xFF76, 0x00),
            (0xFF77, 0x00),
            (0xFFFF, 0x00),
        ];

        for &(addr, value) in expected {
            assert_eq!(bus.read(addr), value, "read ${addr:04X}");
        }
    }

    #[test]
    fn test_dmg_compat_unused_hwio_reads_as_ff_or_documented_masked_value() {
        let mut bus = make_dmg_compat_bus_post_boot();

        for addr in [
            0xFF03, 0xFF08, 0xFF09, 0xFF0A, 0xFF0B, 0xFF0C, 0xFF0D, 0xFF0E, 0xFF15, 0xFF1F, 0xFF27,
            0xFF28, 0xFF29, 0xFF4C, 0xFF4D, 0xFF4E, 0xFF50, 0xFF51, 0xFF52, 0xFF53, 0xFF54, 0xFF55,
            0xFF56, 0xFF57, 0xFF58, 0xFF59, 0xFF5A, 0xFF5B, 0xFF5C, 0xFF5D, 0xFF5E, 0xFF5F, 0xFF60,
            0xFF61, 0xFF62, 0xFF63, 0xFF64, 0xFF65, 0xFF66, 0xFF67, 0xFF69, 0xFF6B, 0xFF6C, 0xFF6D,
            0xFF6E, 0xFF6F, 0xFF70, 0xFF71, 0xFF74, 0xFF78, 0xFF79, 0xFF7A, 0xFF7B, 0xFF7C, 0xFF7D,
            0xFF7E, 0xFF7F,
        ] {
            bus.write(addr, 0x00);
            assert_eq!(bus.read(addr), 0xFF, "read ${addr:04X} after write 00");
            bus.write(addr, 0xFF);
            assert_eq!(bus.read(addr), 0xFF, "read ${addr:04X} after write FF");
        }

        for (addr, mask, expected) in [
            (0xFF4F, 0xFE, 0xFE),
            (0xFF68, 0x40, 0x40),
            (0xFF6A, 0x40, 0x40),
            (0xFF75, 0xFF, 0x8F),
            (0xFF76, 0xFF, 0x00),
            (0xFF77, 0xFF, 0x00),
        ] {
            bus.write(addr, 0x00);
            assert_eq!(bus.read(addr) & mask, expected & mask, "read ${addr:04X}");
        }
    }

    #[test]
    fn test_dmg_compat_unused_hwio_mooneye_bit_masks() {
        let mut bus = make_dmg_compat_bus_post_boot();

        for (addr, mask, write, expected) in [
            (0xFF00, 0xC0, 0xFF, 0xC0),
            (0xFF00, 0xC0, 0x3F, 0xC0),
            (0xFF02, 0x7E, 0x7E, 0x7E),
            (0xFF02, 0x7E, 0x00, 0x7E),
            (0xFF07, 0xF8, 0xF8, 0xF8),
            (0xFF07, 0xF8, 0x00, 0xF8),
            (0xFF0F, 0xE0, 0xE0, 0xE0),
            (0xFF0F, 0xE0, 0x00, 0xE0),
            (0xFF41, 0x80, 0x80, 0x80),
            (0xFF41, 0x80, 0x00, 0x80),
            (0xFF10, 0x80, 0x00, 0x80),
            (0xFF10, 0x80, 0x80, 0x80),
            (0xFF1A, 0x7F, 0x00, 0x7F),
            (0xFF1A, 0x7F, 0x7F, 0x7F),
            (0xFF1C, 0x9F, 0x00, 0x9F),
            (0xFF1C, 0x9F, 0x9F, 0x9F),
            (0xFF20, 0xC0, 0x00, 0xC0),
            (0xFF20, 0xC0, 0xC0, 0xC0),
            (0xFF23, 0x3F, 0x00, 0x3F),
            (0xFF23, 0x3F, 0x3F, 0x3F),
            (0xFF26, 0x70, 0x80, 0x70),
            (0xFF26, 0x70, 0xF0, 0x70),
            (0xFFFF, 0xE0, 0x00, 0x00),
            (0xFFFF, 0xE0, 0xE0, 0xE0),
            (0xFF4F, 0xFE, 0x00, 0xFE),
            (0xFF4F, 0xFE, 0xFE, 0xFE),
            (0xFF68, 0x40, 0x00, 0x40),
            (0xFF68, 0x40, 0x40, 0x40),
            (0xFF6A, 0x40, 0x00, 0x40),
            (0xFF6A, 0x40, 0x40, 0x40),
            (0xFF72, 0xFF, 0x00, 0x00),
            (0xFF72, 0xFF, 0xFF, 0xFF),
            (0xFF73, 0xFF, 0x00, 0x00),
            (0xFF73, 0xFF, 0xFF, 0xFF),
            (0xFF75, 0xFF, 0x00, 0x8F),
            (0xFF75, 0xFF, 0xFF, 0xFF),
            (0xFF76, 0xFF, 0x00, 0x00),
            (0xFF76, 0xFF, 0xFF, 0x00),
            (0xFF77, 0xFF, 0x00, 0x00),
            (0xFF77, 0xFF, 0xFF, 0x00),
        ] {
            bus.write(addr, write);
            assert_eq!(
                bus.read(addr) & mask,
                expected & mask,
                "addr ${addr:04X} write ${write:02X}"
            );
        }
    }

    #[test]
    fn test_serial_data_register_reads_back_written_value() {
        let mut cgb_bus = make_bus_post_boot();
        cgb_bus.write(0xFF01, 0x5A);
        assert_eq!(cgb_bus.read(0xFF01), 0x5A);
        assert_eq!(cgb_bus.read_for_debugger(0xFF01), 0x5A);

        let mut dmg_compat_bus = make_dmg_compat_bus_post_boot();
        dmg_compat_bus.write(0xFF01, 0xA5);
        assert_eq!(dmg_compat_bus.read(0xFF01), 0xA5);
        assert_eq!(dmg_compat_bus.read_for_debugger(0xFF01), 0xA5);
    }

    #[test]
    fn test_ff74_is_fixed_ff_only_in_dmg_compat_mode() {
        let mut dmg_compat_bus = make_dmg_compat_bus_post_boot();
        dmg_compat_bus.write(0xFF74, 0x00);
        assert_eq!(dmg_compat_bus.read(0xFF74), 0xFF);
        dmg_compat_bus.write(0xFF74, 0x55);
        assert_eq!(dmg_compat_bus.read(0xFF74), 0xFF);

        let mut cgb_bus = make_bus_post_boot();
        cgb_bus.write(0xFF74, 0x00);
        assert_eq!(cgb_bus.read(0xFF74), 0x00);
        cgb_bus.write(0xFF74, 0x55);
        assert_eq!(cgb_bus.read(0xFF74), 0x55);
    }

    #[test]
    fn test_debugger_read_mirrors_dmg_compat_hwio_masks() {
        let mut bus = make_dmg_compat_bus_post_boot();

        for addr in [
            0xFF4C, 0xFF4D, 0xFF51, 0xFF52, 0xFF53, 0xFF54, 0xFF55, 0xFF70, 0xFF74,
        ] {
            bus.write(addr, 0x00);
            assert_eq!(
                bus.read_for_debugger(addr),
                bus.read(addr),
                "debugger read ${addr:04X}"
            );
        }
    }

    #[test]
    fn test_lcdc_write_cpu_m_cycle_applies_ppu_write_at_t3_during_mode3() {
        let mut bus = make_bus_post_boot();
        enable_lcd(&mut bus);
        while bus.ppu.dot() < 84 {
            bus.tick(1);
        }
        assert_eq!(bus.ppu.mode(), PpuMode::PixelTransfer);

        let write_cycle_start_dot = bus.ppu.dot();
        bus.write_cpu_m_cycle(0xFF40, 0x00);

        assert_eq!(
            bus.ppu.dot(),
            write_cycle_start_dot + 3,
            "CGB LCDC writes should disable the PPU at T3, before the last dot of the CPU write M-cycle"
        );
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
    fn test_hdma5_read_ff_when_never_started() {
        // Given: CgbBus with no transfer ever started (make_bus uses boot ROM active)
        let mut bus = make_bus();
        // Then: $FF55 reads $FF (never_started flag set, per Mooneye boot_hwio-C)
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
        // And $FF55 reads $80 (inactive + 0 remaining after completion)
        assert_eq!(bus.read(0xFF55), 0x80);
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
        // $80 = inactive + 0 remaining after completion
        assert_eq!(bus.read(0xFF55), 0x80);
    }

    #[test]
    fn test_gdma_transfers_from_rom_to_vram() {
        // Given: ROM data at $0100-$010F
        let data: Vec<(u16, u8)> = (0..16).map(|i| (0x0100 + i as u16, 0xA0 + i)).collect();
        let cart = cgb_rom_with_data(&data);
        let mut bus = CgbBus::new(cart, CgbModel::default(), true);
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
        // Note: With LCD off (mode 0), HDMA transfers one block immediately.
        let mut bus = make_bus();
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        // When: start HDMA (bit 7=1, length=1 → 2 blocks)
        bus.write(0xFF55, 0x81);
        // Then: $FF55 reads 0x00 (active, 1 block remaining after immediate transfer)
        // With LCD off (mode 0), one block transferred immediately.
        assert_eq!(bus.read(0xFF55), 0x00);
    }

    #[test]
    fn test_hdma_transfers_on_hblank() {
        // Given: HDMA configured with source=$C000, dest=$8000, 2 blocks
        let mut bus = make_bus();
        enable_lcd(&mut bus);
        for i in 0u8..32 {
            bus.write(0xC000 + i as u16, 0x50 + i);
        }
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);

        // Tick to get into mode 3 (OAM+VRAM access) before starting HDMA
        // to avoid immediate transfer.
        for _ in 0..20 {
            bus.tick(1);
        }

        bus.write(0xFF55, 0x81); // HDMA, 2 blocks (length=1)

        // When: tick enough to reach HBlank on the first scanline
        for _ in 0..50 {
            bus.tick(1);
        }

        // Then: after HBlank, at least 16 bytes should be transferred to VRAM
        for i in 0u8..16 {
            assert_eq!(
                bus.ppu.vram[i as usize],
                0x50 + i,
                "VRAM byte {} should be transferred",
                i
            );
        }
    }

    #[test]
    fn test_hdma_cancel_stops_transfer() {
        // Given: active HDMA with 3 blocks
        // Note: With LCD off (mode 0), one block transfers immediately when HDMA starts.
        let mut bus = make_bus();
        bus.write(0xFF51, 0xC0);
        bus.write(0xFF52, 0x00);
        bus.write(0xFF53, 0x80);
        bus.write(0xFF54, 0x00);
        bus.write(0xFF55, 0x83); // HDMA, 4 blocks (remaining=3)
        // After immediate transfer: remaining=2, still active

        // When: cancel by writing bit 7=0 to $FF55
        bus.write(0xFF55, 0x00);

        // Then: $FF55 reads $80 (inactive)
        // Note: cancellation overwrites remaining_blocks with written value (0)
        assert_eq!(bus.read(0xFF55), 0x80);
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

    // ── Boot ROM infrastructure ─────────────────────────────────────────────

    #[test]
    fn test_boot_rom_inactive_by_default_when_skip_flag_true() {
        // Given: CgbBus created with skip_boot_rom = true
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), true);
        // Then: boot ROM should be inactive
        assert!(!bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_active_by_default_when_skip_flag_false() {
        // Given: CgbBus created with skip_boot_rom = false
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // Then: boot ROM should be active
        assert!(bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_reads_from_boot_rom_when_active() {
        // Given: CgbBus with boot ROM active
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // Then: $0000 should read from boot ROM
        // CGB_BOOT_ROM starts with: LD SP, $FFFE ($31 $FE $FF); LD HL, $8000 ($21 $00 $80)
        assert_eq!(bus.read_for_debugger(0x0000), 0x31); // LD SP, nn
        assert_eq!(bus.read_for_debugger(0x0001), 0xFE); // low byte of $FFFE
        assert_eq!(bus.read_for_debugger(0x0002), 0xFF); // high byte of $FFFE
        assert_eq!(bus.read_for_debugger(0x0003), 0x21); // LD HL, nn (VRAM clear)
        assert_eq!(bus.read_for_debugger(0x0004), 0x00); // low byte of $8000
    }

    #[test]
    fn test_boot_rom_cartridge_header_gap_reads_from_cartridge() {
        // Given: CgbBus with boot ROM active and known cartridge header data
        let mut rom = vec![0u8; 0x8000];
        rom[0x0100] = 0xAB; // Entry point in cartridge
        rom[0x0143] = 0x80; // CGB flag
        rom[0x0147] = 0x00;
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        let cart = load_cartridge(&rom).expect("valid ROM");
        let bus = CgbBus::new(cart, CgbModel::default(), false);
        // Then: $0100-$01FF should read from cartridge, not boot ROM
        assert_eq!(bus.read_for_debugger(0x0100), 0xAB);
    }

    #[test]
    fn test_boot_rom_upper_region_reads_from_boot_rom_when_active() {
        // Given: CgbBus with boot ROM active
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // Then: $0200-$08FF should read from boot ROM
        // $0200 contains DoubleBitsAndWriteRowTwice subroutine (CALL $0203 = $CD $03 $02)
        assert_eq!(bus.read_for_debugger(0x0200), 0xCD); // CALL opcode
        // $08FF is padding at end of boot ROM
        assert_eq!(bus.read_for_debugger(0x08FF), 0x00);
    }

    #[test]
    fn test_boot_rom_reads_from_cartridge_when_inactive() {
        // Given: CgbBus with boot ROM inactive and known cartridge data at $0000
        let mut rom = vec![0u8; 0x8000];
        rom[0x0000] = 0xDE;
        rom[0x0143] = 0x80; // CGB flag
        rom[0x0147] = 0x00;
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        let cart = load_cartridge(&rom).expect("valid ROM");
        let bus = CgbBus::new(cart, CgbModel::default(), true);
        // Then: $0000 should read from cartridge
        assert_eq!(bus.read_for_debugger(0x0000), 0xDE);
    }

    #[test]
    fn test_boot_rom_unmaps_on_ff50_write() {
        // Given: CgbBus with boot ROM active
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        assert!(bus.is_boot_rom_active());
        // When: write to $FF50
        bus.write(0xFF50, 0x01);
        // Then: boot ROM should be unmapped
        assert!(!bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_unmaps_on_any_ff50_write_value() {
        // Given: CgbBus with boot ROM active
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // When: write any value to $FF50
        bus.write(0xFF50, 0x00);
        // Then: boot ROM should be unmapped (any write value works)
        assert!(!bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_reset_reactivates_boot_rom() {
        // Given: CgbBus with boot ROM unmapped (skip_boot_rom=false, then disabled via $FF50)
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        bus.write(0xFF50, 0x01);
        assert!(!bus.is_boot_rom_active());
        // When: reset
        bus.reset();
        // Then: boot ROM should be active again (skip_boot_rom=false means boot ROM runs)
        assert!(bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_reset_respects_skip_boot_rom_setting() {
        // Given: CgbBus created with skip_boot_rom=true
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), true);
        assert!(!bus.is_boot_rom_active());
        // When: reset
        bus.reset();
        // Then: boot ROM should remain inactive (skip_boot_rom=true is preserved)
        assert!(!bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_save_state_captures_active_state() {
        // Given: CgbBus with boot ROM active
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // When: capture save state
        let state = bus.capture_bus_state();
        // Then: boot_rom_active should be Some(true)
        assert_eq!(state.boot_rom_active, Some(true));
    }

    #[test]
    fn test_boot_rom_save_state_captures_inactive_state() {
        // Given: CgbBus with boot ROM inactive
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), true);
        // When: capture save state
        let state = bus.capture_bus_state();
        // Then: boot_rom_active should be Some(false)
        assert_eq!(state.boot_rom_active, Some(false));
    }

    #[test]
    fn test_boot_rom_save_state_restores_active_state() {
        // Given: CgbBus with boot ROM inactive
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), true);
        // And: a save state captured when boot ROM was active
        let active_bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        let state = active_bus.capture_bus_state();
        // When: restore the save state
        bus.restore_bus_state(&state)
            .expect("restore should succeed");
        // Then: boot ROM should be active
        assert!(bus.is_boot_rom_active());
    }

    #[test]
    fn test_boot_rom_read_raw_intercepts_boot_rom() {
        // Given: CgbBus with boot ROM active
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::default(), false);
        // Then: read_raw should also return boot ROM data
        // CGB_BOOT_ROM starts with LD SP, $FFFE (0x31)
        assert_eq!(bus.read_raw(0x0000), 0x31); // LD SP, nn opcode
    }

    #[test]
    fn test_ch1_stops_after_div_apu_clocks_length_counter() {
        // Simulates the key behavior tested by SameSuite channel_1_stop_div:
        // CH1 with length_counter=1 should stop after one DIV-APU event clocks the length.
        let mut bus = make_bus();

        // Reset DIV to ensure clean start
        bus.write(0xFF04, 0x00);

        // Power on APU
        bus.write(0xFF26, 0x80);

        // Setup CH1: duty 50%, length=63 → counter=1
        bus.write(0xFF11, 0x80 | 0x3F);
        // Enable DAC
        bus.write(0xFF12, 0x80);
        // Low freq
        bus.write(0xFF13, 0xFC);
        // Trigger with length enabled
        bus.write(0xFF14, 0xC7);

        // Channel should be active (NR52 bit 0)
        assert_ne!(
            bus.read(0xFF26) & 0x01,
            0,
            "CH1 should be active after trigger"
        );

        // Reset DIV to start clean timing
        bus.write(0xFF04, 0x00);

        // Tick to the DIV-APU falling edge (2048 M-cycles)
        for _ in 0..2048 {
            bus.tick(1);
        }

        // The DIV-APU event should have clocked length, stopping the channel
        assert_eq!(
            bus.read(0xFF26) & 0x01,
            0,
            "CH1 should be inactive after length reaches 0"
        );

        // PCM12 should show 0 for CH1
        assert_eq!(bus.read(0xFF76) & 0x0F, 0, "CH1 output should be 0");
    }

    #[test]
    fn test_div_write_triggers_div_apu_event_when_bit_high() {
        // Writing to DIV resets div_counter to 0.
        // If the DIV-APU bit was HIGH before the write, this creates a falling edge.
        // We verify by checking if CH1 with length=1 stops after the DIV write.
        let mut bus = make_bus();

        // Reset DIV to ensure clean start
        bus.write(0xFF04, 0x00);

        // Power on APU
        bus.write(0xFF26, 0x80);

        // Tick to get bit 12 HIGH (at 1024+ M-cycles)
        for _ in 0..1500 {
            bus.tick(1);
        }
        // At this point, div_counter ≈ 6000, bit 12 is HIGH

        // Setup CH1: length=63 → counter=1
        bus.write(0xFF11, 0x80 | 0x3F);
        bus.write(0xFF12, 0x80);
        bus.write(0xFF13, 0xFC);
        bus.write(0xFF14, 0xC7);

        // Channel should be active
        assert_ne!(
            bus.read(0xFF26) & 0x01,
            0,
            "CH1 should be active after trigger"
        );

        // Writing DIV resets counter to 0, creating a falling edge on bit 12
        // This should clock the frame sequencer, which clocks length on step 0
        bus.write(0xFF04, 0x00);

        // CH1 should now be stopped
        assert_eq!(
            bus.read(0xFF26) & 0x01,
            0,
            "CH1 should be inactive after DIV write with bit high"
        );
    }

    // ── KEY0 register tests ──────────────────────────────────────────────────

    #[test]
    fn test_key0_initial_value() {
        // Given: CgbBus with boot ROM active
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, false);

        // Then: KEY0 initial value is $00 (CGB mode)
        assert_eq!(bus.key0(), 0x00);
        assert!(!bus.is_key0_locked());
    }

    #[test]
    fn test_key0_read_returns_unused_bits_as_1() {
        // Given: CgbBus with boot ROM active
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, false);

        // When: KEY0 is $00
        // Then: read returns $F0 (upper nibble set)
        assert_eq!(bus.read(0xFF4C), 0xF0);

        // When: write $04 (DMG mode)
        bus.write(0xFF4C, 0x04);

        // Then: read returns $F4
        assert_eq!(bus.read(0xFF4C), 0xF4);
    }

    #[test]
    fn test_key0_writable_before_boot_rom_unmaps() {
        // Given: CgbBus with boot ROM active
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, false);
        assert!(!bus.is_key0_locked());

        // When: write $04 to KEY0
        bus.write(0xFF4C, 0x04);

        // Then: KEY0 is updated
        assert_eq!(bus.key0(), 0x04);
    }

    #[test]
    fn test_key0_locked_after_boot_rom_unmaps() {
        // Given: CgbBus with boot ROM active
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, false);
        bus.write(0xFF4C, 0x04);
        assert_eq!(bus.key0(), 0x04);

        // When: unmap boot ROM by writing to $FF50
        bus.write(0xFF50, 0x01);

        // Then: KEY0 is locked
        assert!(bus.is_key0_locked());
        assert!(!bus.is_boot_rom_active());

        // When: try to write different value to KEY0
        bus.write(0xFF4C, 0x80);

        // Then: KEY0 is unchanged (write ignored)
        assert_eq!(bus.key0(), 0x04);
    }

    #[test]
    fn test_key0_locked_when_skip_boot_rom() {
        // Given: CgbBus with boot ROM skipped
        let bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, true);

        // Then: KEY0 is locked immediately
        assert!(bus.is_key0_locked());
    }

    #[test]
    fn test_key0_skip_boot_rom_write_ignored() {
        // Given: CgbBus with boot ROM skipped (KEY0 locked)
        let mut bus = CgbBus::new(cgb_rom_only_cart(), CgbModel::CgbE, true);
        assert!(bus.is_key0_locked());
        let initial = bus.key0();

        // When: try to write to KEY0
        bus.write(0xFF4C, 0x04);

        // Then: KEY0 is unchanged
        assert_eq!(bus.key0(), initial);
    }
}
