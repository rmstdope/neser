//! Mapper 24/26 - Konami VRC6
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/VRC6>
//! - Audio: <https://www.nesdev.org/wiki/VRC6_audio>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//! - Cycle-accuracy edge cases and less-common board wiring variants are not yet
//!   comprehensively validated by dedicated ROM test suites.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::nes::cartridge::vrc_irq::VrcIrq;
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vrc6Variant {
    Mapper24,
    Mapper26,
}

#[derive(Debug, Default, Clone, Copy)]
struct Vrc6Pulse {
    enabled: bool,
    mode_ignore_duty: bool,
    duty: u8,
    volume: u8,
    period: u16,
    divider: u16,
    duty_step: u8,
}

impl Vrc6Pulse {
    fn write_control(&mut self, value: u8) {
        self.mode_ignore_duty = (value & 0x80) != 0;
        self.duty = (value >> 4) & 0x07;
        self.volume = value & 0x0F;
    }

    fn write_period_low(&mut self, value: u8) {
        self.period = (self.period & 0x0F00) | (value as u16);
    }

    fn write_period_high_and_enable(&mut self, value: u8) {
        let enabled = (value & 0x80) != 0;
        self.period = (self.period & 0x00FF) | (((value & 0x0F) as u16) << 8);

        if self.enabled && !enabled {
            // Disabling forces output to 0 and resets/halts duty.
            self.duty_step = 15;
        }
        if !self.enabled && enabled {
            // Enabling resumes from the beginning.
            self.duty_step = 15;
        }

        self.enabled = enabled;
    }

    fn clock(&mut self, effective_period: u16) {
        if !self.enabled {
            return;
        }

        if self.divider == 0 {
            self.divider = effective_period;
            self.duty_step = (self.duty_step.wrapping_sub(1)) & 0x0F;
        } else {
            self.divider = self.divider.wrapping_sub(1);
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled {
            return 0;
        }

        if self.mode_ignore_duty {
            return self.volume;
        }

        // Duty generator counts down 15..0. Output is V when step <= duty, else 0.
        if self.duty_step <= self.duty {
            self.volume
        } else {
            0
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Vrc6Saw {
    enabled: bool,
    rate: u8,
    period: u16,
    divider: u16,
    accumulator: u8,
    step: u8,
}

impl Vrc6Saw {
    fn write_rate(&mut self, value: u8) {
        self.rate = value & 0x3F;
    }

    fn write_period_low(&mut self, value: u8) {
        self.period = (self.period & 0x0F00) | (value as u16);
    }

    fn write_period_high_and_enable(&mut self, value: u8) {
        let enabled = (value & 0x80) != 0;
        self.period = (self.period & 0x00FF) | (((value & 0x0F) as u16) << 8);

        if self.enabled && !enabled {
            // Accumulator forced to zero while disabled.
            self.accumulator = 0;
            self.step = 0;
        }
        if !self.enabled && enabled {
            // Re-enabling resumes from a mostly reset phase.
            self.accumulator = 0;
            self.step = 0;
        }

        self.enabled = enabled;
    }

    fn clock(&mut self, effective_period: u16) {
        // Divider still runs even if the channel is disabled (per nesdev), but output is forced 0.
        if self.divider == 0 {
            self.divider = effective_period;

            if self.enabled {
                self.step = (self.step + 1) % 14;
                if self.step == 0 {
                    self.accumulator = 0;
                } else if self.step.is_multiple_of(2) {
                    self.accumulator = self.accumulator.wrapping_add(self.rate);
                }
            }
        } else {
            self.divider = self.divider.wrapping_sub(1);
        }
    }

    fn output(&self) -> u8 {
        if !self.enabled {
            return 0;
        }
        // High 5 bits of accumulator.
        self.accumulator >> 3
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Vrc6Audio {
    global_halt: bool,
    global_shift: u8,
    pulse1: Vrc6Pulse,
    pulse2: Vrc6Pulse,
    saw: Vrc6Saw,
}

impl Vrc6Audio {
    // Conservative gain: keep expansion audio from dominating the base APU mix.
    const MIX_GAIN: f32 = 0.35;

    fn write_freq_control(&mut self, value: u8) {
        let halt = (value & 0b0000_0001) != 0;
        self.global_halt = halt;

        // 256x overrides 16x.
        self.global_shift = if (value & 0b0000_0100) != 0 {
            8
        } else if (value & 0b0000_0010) != 0 {
            4
        } else {
            0
        };
    }

    fn effective_period(&self, period: u16) -> u16 {
        let shift = self.global_shift.min(8);
        period >> shift
    }

    fn cpu_cycle(&mut self) {
        trace_mapper!(5; "[vrc6] cpu_cycle (audio)");
        if self.global_halt {
            return;
        }

        let p1 = self.effective_period(self.pulse1.period);
        let p2 = self.effective_period(self.pulse2.period);
        let saw = self.effective_period(self.saw.period);

        self.pulse1.clock(p1);
        self.pulse2.clock(p2);
        self.saw.clock(saw);
    }

    fn raw_output(&self) -> u8 {
        let p1 = self.pulse1.output();
        let p2 = self.pulse2.output();
        let saw = self.saw.output();
        p1.saturating_add(p2).saturating_add(saw)
    }

    fn sample(&self) -> f32 {
        // VRC6 audio DAC is linear; the chip sums 2x4-bit pulse + 5-bit saw (max 61).
        (self.raw_output() as f32 / 61.0) * Self::MIX_GAIN
    }
}

/// Mappers 24, 26 - Konami VRC6 (with expansion audio)
///
/// Hardware: Konami's VRC6 mapper with three expansion audio channels
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/VRC6>
/// - Audio: <https://www.nesdev.org/wiki/VRC6_audio>
/// - IRQ: <https://www.nesdev.org/wiki/VRC_IRQ>
/// - PRG-ROM: Up to 512KB (two banks: 16KB + 8KB switchable, one fixed)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR: Up to 256KB (eight 1KB switchable banks) or CHR-RAM
/// - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
/// - Expansion audio: 2 pulse channels + 1 sawtooth channel
///
/// Mapper variants (different address line connections):
/// - Mapper 24: VRC6a (Akumajou Densetsu / Castlevania III Japan)
/// - Mapper 26: VRC6b (Madara, Esper Dream 2)
///
/// Notes:
/// - CPU-cycle or scanline-driven IRQ counter (same as VRC4)
/// - Three expansion audio channels add extra sound capability
/// - Different mappers due to different A0/A1 pin connections
/// - Used in Castlevania III (Japan), Madara, Akumajou Densetsu
///
/// Implementation:
/// - Supports PRG/CHR banking + mirroring control
/// - VRC IRQ fully implemented
/// - VRC6 expansion audio (2 pulse + 1 sawtooth) fully implemented
pub struct VRC6Mapper {
    variant: Vrc6Variant,

    base: BaseMapper,
    prg_ram: PrgRam,

    prg_bank_16k: u8,
    prg_bank_8k: u8,
    chr_banks_1k: [u8; 8],

    b003: u8,
    prg_ram_enabled: bool,

    /// Internal 2KB CIRAM (two 1KB nametable pages: A10=0 and A10=1).
    ciram: Box<[u8; 2048]>,

    // --- VRC IRQ (used by VRC6) ---
    irq: VrcIrq,

    // --- VRC6 expansion audio ---
    audio: Vrc6Audio,
}

impl VRC6Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mapper_number = ctx.mapper as u8;
        let variant = match mapper_number {
            24 => Vrc6Variant::Mapper24,
            26 => Vrc6Variant::Mapper26,
            _ => Vrc6Variant::Mapper24,
        };

        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 0, // VRC6 manages PRG-RAM separately with gating
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);

        let mut mapper = Self {
            variant,
            base,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_bank_16k: 0,
            prg_bank_8k: 0,
            chr_banks_1k: [0; 8],
            b003: 0,
            prg_ram_enabled: false,
            ciram: Box::new([0u8; 2048]),

            irq: VrcIrq::new(341, 3),

            audio: Vrc6Audio::default(),
        };
        mapper.base.set_mirroring(mirroring);
        mapper.update_banks();
        mapper
    }

    fn normalize_reg_addr(&self, addr: u16) -> u16 {
        // Only A0, A1, and A12-A15 are used for register selection.
        // Mirrors can be found by ANDing with $F003.
        let mut a = addr & 0xF003;

        // Mapper 26 swaps A0 and A1.
        if self.variant == Vrc6Variant::Mapper26 {
            let bit0 = a & 0x0001;
            let bit1 = a & 0x0002;
            a = (a & !0x0003) | (bit0 << 1) | (bit1 >> 1);
        }

        a
    }

    /// Compute the CIRAM A10 (page select: 0=NTA, 1=NTB) for a given PPU nametable address.
    ///
    /// Implements the same logic as vrc6test's `maketesttable.c` `getbankval` for nametables.
    ///
    /// Step 1: select the CHR register that drives this nametable quadrant (via `b003 & 7`).
    /// Step 2: if bit 5 of b003 is set (Mode 0), optionally replace A10 based on `b003 & 0x0F`.
    /// Step 3: return the LSB of the resulting register value as A10.
    fn ciram_a10(&self, addr: u16) -> usize {
        let quadrant = ((addr >> 10) & 3) as usize;

        // Step 1: determine which CHR register controls this quadrant.
        let bankreg: usize = match self.b003 & 0x07 {
            0 | 6 | 7 => {
                if quadrant < 2 {
                    6
                } else {
                    7
                }
            } // h-pattern: R6-R6-R7-R7
            2..=4 => {
                if quadrant & 1 == 0 {
                    6
                } else {
                    7
                }
            } // v-pattern: R6-R7-R6-R7
            1 | 5 => 4 + quadrant, // 4-screen: R4-R5-R6-R7
            _ => unreachable!(),
        };
        let mut bankval = self.chr_banks_1k[bankreg];

        // Step 2: if bit 5 set (Mode 0), possibly override A10 based on b003 & 0x0F.
        if (self.b003 & 0x20) != 0 {
            match self.b003 & 0x0F {
                0 | 7 => {
                    bankval = (bankval & !1) | ((quadrant & 1) as u8);
                } // vertical
                3 | 4 => {
                    bankval = (bankval & !1) | ((quadrant >> 1) as u8);
                } // horizontal
                8 | 15 => {
                    bankval &= !1;
                } // 1-screen A (A10=0)
                11..=12 => {
                    bankval |= 1;
                } // 1-screen B (A10=1)
                _ => {} // all other values: use register LSB as-is
            }
        }

        // Step 3: A10 = LSB of bankval.
        (bankval & 1) as usize
    }

    fn update_prg_ram_enable(&mut self) {
        self.prg_ram_enabled = (self.b003 & 0x80) != 0;
    }

    fn update_banks(&mut self) {
        // PRG: 16KB at $8000 (= 2×8KB), 8KB at $C000, fixed 8KB at $E000
        let bank16k_lo = (self.prg_bank_16k as i16 & 0x0F) * 2;
        self.base.select_prg_page(0, bank16k_lo);
        self.base.select_prg_page(1, bank16k_lo + 1);
        self.base
            .select_prg_page(2, (self.prg_bank_8k & 0x1F) as i16);
        self.base.select_prg_page(3, -1);

        // CHR banking: $B003 bits 1-0 (DD) select the banking mode.
        // Bit 5 (N) controls whether 2KB pairs have independent A10 halves.
        // When N=1: even block = base & ~1, odd block = (base & ~1) | 1 (proper 2KB).
        // When N=0 ("Other"): both blocks of a pair get the raw register value.
        let mode = self.b003 & 0x03;
        let split_2k = (self.b003 & 0x20) != 0;

        match mode {
            0 => {
                // Mode 0: 8 × 1KB, independent R0-R7
                for i in 0..8 {
                    self.base.select_chr_page(i, self.chr_banks_1k[i] as i16);
                }
            }
            1 => {
                // Mode 1: 4 × 2KB — pairs (0,1), (2,3), (4,5), (6,7) from R0-R3
                for pair in 0..4usize {
                    let base = self.chr_banks_1k[pair] as i16;
                    let (even, odd) = if split_2k {
                        (base & !1, (base & !1) | 1)
                    } else {
                        (base, base)
                    };
                    self.base.select_chr_page(pair * 2, even);
                    self.base.select_chr_page(pair * 2 + 1, odd);
                }
            }
            2 | 3 => {
                // Mode 2/3: R0-R3 independent 1KB; R4 controls 2KB pair (4,5); R5 controls (6,7)
                for i in 0..4 {
                    self.base.select_chr_page(i, self.chr_banks_1k[i] as i16);
                }
                for (reg_idx, block_base) in [(4usize, 4usize), (5, 6)] {
                    let base = self.chr_banks_1k[reg_idx] as i16;
                    let (even, odd) = if split_2k {
                        (base & !1, (base & !1) | 1)
                    } else {
                        (base, base)
                    };
                    self.base.select_chr_page(block_base, even);
                    self.base.select_chr_page(block_base + 1, odd);
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Mapper for VRC6Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // PRG-RAM is gated by $B003 bit 7 (W); when disabled reads return open bus.
                if !self.prg_ram_enabled {
                    return open_bus;
                }
                self.read_prg(addr)
            }
            _ if addr < 0x6000 => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if self.prg_ram_enabled
            && let Some(value) = self.prg_ram.try_read(addr)
        {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF, enabled by $B003 bit 7 (W)
        if self.prg_ram_enabled && self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = self.normalize_reg_addr(addr);
            match reg {
                0x8000..=0x8003 => {
                    self.prg_bank_16k = value & 0x0F;
                    self.update_banks();
                }
                0x9000 => self.audio.pulse1.write_control(value),
                0x9001 => self.audio.pulse1.write_period_low(value),
                0x9002 => self.audio.pulse1.write_period_high_and_enable(value),
                0x9003 => self.audio.write_freq_control(value),
                0xA000 => self.audio.pulse2.write_control(value),
                0xA001 => self.audio.pulse2.write_period_low(value),
                0xA002 => self.audio.pulse2.write_period_high_and_enable(value),
                0xB000 => self.audio.saw.write_rate(value),
                0xB001 => self.audio.saw.write_period_low(value),
                0xB002 => self.audio.saw.write_period_high_and_enable(value),
                0xC000..=0xC003 => {
                    self.prg_bank_8k = value & 0x1F;
                    self.update_banks();
                }
                0xB003 => {
                    self.b003 = value;
                    self.update_banks(); // bits 1-0 (DD) affect CHR banking mode
                    self.update_prg_ram_enable(); // bit 7 (W) gates PRG-RAM
                }
                0xF000 => {
                    // IRQ Latch
                    self.irq.write_latch(value);
                }
                0xF001 => {
                    // IRQ Control (.... .MEA)
                    // M: mode (1=cycle, 0=scanline)
                    // E: enable (1=enabled)
                    // A: enable after acknowledgement (copied to E on $F002 writes)
                    self.irq.write_control(value);
                }
                0xF002 => {
                    // IRQ Acknowledge
                    // Any write acknowledges pending IRQ and copies A->E.
                    self.irq.write_acknowledge();
                }
                0xD000..=0xD003 => {
                    let idx = (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                    self.update_banks();
                }
                0xE000..=0xE003 => {
                    let idx = 4 + (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                    self.update_banks();
                }
                // Other VRC6 registers not currently modeled.
                _ => {}
            }
        }
    }

    fn cpu_cycle(&mut self) {
        self.audio.cpu_cycle();
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let quadrant = ((addr >> 10) & 3) as usize;
        let offset = (addr & 0x3FF) as usize;

        if (self.b003 & 0x10) != 0 {
            // Bit 4 set: nametables sourced from CHR ROM.
            // Select CHR register using b003 & 7 pattern, then compute bank with A10 override.
            let bankreg = match self.b003 & 0x07 {
                0 | 6 | 7 => {
                    if quadrant < 2 {
                        6
                    } else {
                        7
                    }
                }
                2..=4 => {
                    if quadrant & 1 == 0 {
                        6
                    } else {
                        7
                    }
                }
                1 | 5 => 4 + quadrant,
                _ => unreachable!(),
            };
            let mut bank = self.chr_banks_1k[bankreg] as usize;

            // When bit 5 set (Mode 0), apply A10 override same as CIRAM path.
            if (self.b003 & 0x20) != 0 {
                let a10: usize = match self.b003 & 0x0F {
                    0 | 7 => quadrant & 1,
                    3 | 4 => quadrant >> 1,
                    8 | 15 => 0,
                    11..=12 => 1,
                    _ => bank & 1,
                };
                bank = (bank & !1) | a10;
            }

            let chr_offset = bank * 0x400 + offset;
            return Some(self.base.read_chr_at_index(chr_offset));
        }

        // Bit 4 clear: nametables sourced from CIRAM.
        // Use ciram_a10 to determine which 1KB CIRAM page this address maps to.
        let a10 = self.ciram_a10(addr);
        Some(self.ciram[a10 * 0x400 + offset])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        if (self.b003 & 0x10) != 0 {
            // CHR ROM nametable mode: ignore writes to read-only ROM.
            return true;
        }
        let a10 = self.ciram_a10(addr);
        let offset = (addr & 0x3FF) as usize;
        self.ciram[a10 * 0x400 + offset] = value;
        true
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio.sample()
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize VRC6 internal registers:
        // [0]: prg_bank_16k
        // [1]: prg_bank_8k
        // [2-9]: chr_banks_1k[0-7]
        // [10]: b003
        // [11]: irq_latch
        // [12]: irq_counter
        // [13]: flags (irq_enabled, irq_mode_cycle, irq_enable_after_ack, irq_asserted)
        // [14-17]: irq_prescaler (little endian i32)
        // [18]: mirroring
        // [19]: audio.global_halt
        // [20]: audio.global_shift
        // [21-27]: pulse1 (enabled, mode_ignore_duty, duty, volume, period LE, divider LE, duty_step)
        // [28-34]: pulse2 (enabled, mode_ignore_duty, duty, volume, period LE, divider LE, duty_step)
        // [35-40]: saw (enabled, rate, period LE, divider LE, accumulator, step)
        let mut snapshot = Vec::with_capacity(41);
        snapshot.push(self.prg_bank_16k);
        snapshot.push(self.prg_bank_8k);
        snapshot.extend_from_slice(&self.chr_banks_1k);
        snapshot.push(self.b003);
        snapshot.push(self.irq.latch());
        snapshot.push(self.irq.counter());
        let flags = (self.irq.enabled() as u8)
            | ((self.irq.mode_cycle() as u8) << 1)
            | ((self.irq.enable_after_ack() as u8) << 2)
            | ((self.irq.pending() as u8) << 3);
        snapshot.push(flags);
        let prescaler_bytes = self.irq.prescaler().to_le_bytes();
        snapshot.extend_from_slice(&prescaler_bytes);
        snapshot.push(self.base.mirroring().to_snapshot_byte());
        snapshot.push(self.audio.global_halt as u8);
        snapshot.push(self.audio.global_shift);

        snapshot.push(self.audio.pulse1.enabled as u8);
        snapshot.push(self.audio.pulse1.mode_ignore_duty as u8);
        snapshot.push(self.audio.pulse1.duty);
        snapshot.push(self.audio.pulse1.volume);
        snapshot.extend_from_slice(&self.audio.pulse1.period.to_le_bytes());
        snapshot.extend_from_slice(&self.audio.pulse1.divider.to_le_bytes());
        snapshot.push(self.audio.pulse1.duty_step);

        snapshot.push(self.audio.pulse2.enabled as u8);
        snapshot.push(self.audio.pulse2.mode_ignore_duty as u8);
        snapshot.push(self.audio.pulse2.duty);
        snapshot.push(self.audio.pulse2.volume);
        snapshot.extend_from_slice(&self.audio.pulse2.period.to_le_bytes());
        snapshot.extend_from_slice(&self.audio.pulse2.divider.to_le_bytes());
        snapshot.push(self.audio.pulse2.duty_step);

        snapshot.push(self.audio.saw.enabled as u8);
        snapshot.push(self.audio.saw.rate);
        snapshot.extend_from_slice(&self.audio.saw.period.to_le_bytes());
        snapshot.extend_from_slice(&self.audio.saw.divider.to_le_bytes());
        snapshot.push(self.audio.saw.accumulator);
        snapshot.push(self.audio.saw.step);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 19 {
            self.prg_bank_16k = data[0];
            self.prg_bank_8k = data[1];
            self.chr_banks_1k.copy_from_slice(&data[2..10]);
            self.b003 = data[10];
            self.prg_ram_enabled = (self.b003 & 0x80) != 0;
            self.irq.write_latch(data[11]);
            self.irq.set_counter(data[12]);
            let flags = data[13];
            self.irq.set_enabled((flags & 1) != 0);
            self.irq.set_mode_cycle((flags & 2) != 0);
            self.irq.set_enable_after_ack((flags & 4) != 0);
            self.irq.set_asserted((flags & 8) != 0);
            self.irq
                .set_prescaler(i32::from_le_bytes([data[14], data[15], data[16], data[17]]));
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[18]));
        }

        if data.len() >= 41 {
            self.audio.global_halt = data[19] != 0;
            self.audio.global_shift = data[20];

            self.audio.pulse1.enabled = data[21] != 0;
            self.audio.pulse1.mode_ignore_duty = data[22] != 0;
            self.audio.pulse1.duty = data[23];
            self.audio.pulse1.volume = data[24];
            self.audio.pulse1.period = u16::from_le_bytes([data[25], data[26]]);
            self.audio.pulse1.divider = u16::from_le_bytes([data[27], data[28]]);
            self.audio.pulse1.duty_step = data[29];

            self.audio.pulse2.enabled = data[30] != 0;
            self.audio.pulse2.mode_ignore_duty = data[31] != 0;
            self.audio.pulse2.duty = data[32];
            self.audio.pulse2.volume = data[33];
            self.audio.pulse2.period = u16::from_le_bytes([data[34], data[35]]);
            self.audio.pulse2.divider = u16::from_le_bytes([data[36], data[37]]);
            self.audio.pulse2.duty_step = data[38];

            self.audio.saw.enabled = data[39] != 0;
            self.audio.saw.rate = data[40];
            if data.len() >= 46 {
                self.audio.saw.period = u16::from_le_bytes([data[41], data[42]]);
                self.audio.saw.divider = u16::from_le_bytes([data[43], data[44]]);
                self.audio.saw.accumulator = data[45];
                if data.len() >= 47 {
                    self.audio.saw.step = data[46];
                }
            }
        }

        self.update_banks();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_vrc6_mapper(
        mapper_number: u16,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            mapper_number,
            prg_rom,
            chr_rom,
            mirroring,
        ))
    }

    #[test]
    fn test_vrc6_audio_pulse_mode_ignores_duty_outputs_volume_when_enabled() {
        // Red-phase test: VRC6 expansion audio should produce non-zero output when configured.
        // Pulse control: MDDD VVVV (M=1 => ignore duty)
        // Freq high: E... FFFF (E=1 enables channel)
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        // Pulse 1: mode=1, duty doesn't matter, volume=15
        mapper.write_prg(0x9000, 0b1000_1111);
        mapper.write_prg(0x9001, 0x00);
        mapper.write_prg(0x9002, 0b1000_0000); // enable, period high = 0

        // With mode set, we expect an audible non-zero contribution.
        assert!(mapper.expansion_audio_sample() > 0.0);
    }

    #[test]
    fn test_vrc6_audio_saw_outputs_non_zero_when_enabled_with_rate() {
        // Red-phase test: Saw should output non-zero when enabled and clocked.
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        // Saw: rate=8, period=0, enable
        mapper.write_prg(0xB000, 0b0000_1000);
        mapper.write_prg(0xB001, 0x00);
        mapper.write_prg(0xB002, 0b1000_0000);

        // Clock a few CPU cycles so the accumulator advances.
        for _ in 0..8 {
            mapper.cpu_cycle();
        }

        assert!(mapper.expansion_audio_sample() > 0.0);
    }

    #[test]
    fn test_vrc6_registers_snapshot_restores_audio_state() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(
            24,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("VRC6 (mapper 24) should be implemented");

        mapper.write_prg(0x9000, 0b1000_0111); // pulse1 volume 7, ignore duty
        mapper.write_prg(0x9001, 0x10);
        mapper.write_prg(0x9002, 0b1000_0000);

        mapper.write_prg(0xA000, 0b0000_1010); // pulse2 volume 10
        mapper.write_prg(0xA001, 0x08);
        mapper.write_prg(0xA002, 0b1000_0000);

        mapper.write_prg(0xB000, 0x20); // saw rate
        mapper.write_prg(0xB001, 0x02);
        mapper.write_prg(0xB002, 0b1000_0000);

        for _ in 0..8 {
            mapper.cpu_cycle();
        }

        let saved = mapper.registers_snapshot();

        for _ in 0..2 {
            mapper.cpu_cycle();
        }
        let sample = mapper.expansion_audio_sample();

        let mut restored = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");
        restored.restore_registers(&saved);

        for _ in 0..2 {
            restored.cpu_cycle();
        }

        let restored_sample = restored.expansion_audio_sample();
        assert!((restored_sample - sample).abs() < 1e-6);
    }

    #[test]
    fn test_vrc6_irq_cycle_mode_trips_and_ack_clears_and_disables_when_a_is_0() {
        // VRC IRQ (nesdev):
        // - $F000: latch
        // - $F001: control (M=mode, E=enable, A=enable-after-ack)
        // - Any write to $F001 acknowledges pending IRQ and resets prescaler.
        // - If writing $F001 with E set, counter reloads from latch.
        // - In cycle mode (M=1), counter clocks every CPU cycle.
        // - When clocked: if counter == $FF => reload from latch and trip IRQ; else counter += 1.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        mapper.write_prg(0xF000, 0xFE);
        // M=1, E=1, A=0
        mapper.write_prg(0xF001, 0b0000_0110);

        // After enable, counter reloaded to 0xFE.
        // Cycle 1: 0xFE -> 0xFF (no IRQ)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        // Cycle 2: counter == 0xFF -> trip IRQ
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // Ack should clear IRQ and, since A=0, leave IRQ disabled.
        mapper.write_prg(0xF002, 0);
        assert!(!mapper.irq_pending());

        // Many more cycles should not re-assert since IRQ is disabled after ack.
        for _ in 0..1000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc6_irq_scanline_mode_prescaler_trips_after_114_cycles() {
        // In scanline mode (M=0), the prescaler divides CPU cycles by ~113 2/3.
        // A common emulation approach uses a prescaler starting at 341 and subtracting 3
        // each CPU cycle; when it drops <= 0, add 341 and clock the IRQ counter.
        // This means the first clock happens after 114 CPU cycles.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        // Force immediate trip on first counter clock by starting at 0xFF.
        mapper.write_prg(0xF000, 0xFF);
        // M=0, E=1, A=0
        mapper.write_prg(0xF001, 0b0000_0010);

        for _ in 0..113 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());

        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
    }

    #[test]
    fn test_vrc6_mapper_24_prg_banking() {
        // VRC6 banking (nesdev):
        // - $8000-$BFFF: 16KB switchable bank (selected via $8000-$8003)
        // - $C000-$DFFF: 8KB switchable bank (selected via $C000-$C003)
        // - $E000-$FFFF: 8KB fixed to last bank
        // This test uses PRG ROM filled with one byte value per 8KB bank.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 24) should be implemented");

        // Select 16KB bank #1 at $8000-$BFFF.
        // With 8KB banks, this is banks 2 and 3.
        mapper.write_prg(0x8000, 0x01);

        // Select 8KB bank #5 at $C000-$DFFF.
        mapper.write_prg(0xC000, 0x05);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_vrc6_chr_register_address_swap_mapper_24_vs_26() {
        // VRC6 registers use only A0, A1, and A12-A15.
        // For mapper 26, A0/A1 are swapped, i.e. swap bits 0 and 1 of the address.
        // This should swap the meaning of writes to $D001 and $D002 (R1 and R2).

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 32);

        // Mapper 24: write R1 at $D001 to bank 7 -> $0400-$07FF reads bank 7.
        let mut m24 = create_vrc6_mapper(
            24,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("VRC6 (mapper 24) should be implemented");
        m24.write_prg(0xD001, 7);
        assert_eq!(m24.read_chr(0x0400), 7);

        // Mapper 26: the same CPU address $D001 should target internal R2 (not R1).
        // So $0400 should remain at default bank 0, while $0800 uses bank 7.
        let mut m26 = create_vrc6_mapper(26, prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("VRC6 (mapper 26) should be implemented");
        m26.write_prg(0xD001, 7);
        assert_eq!(m26.read_chr(0x0400), 0);
        assert_eq!(m26.read_chr(0x0800), 7);

        // And writing $D002 should then target internal R1.
        m26.write_prg(0xD002, 9);
        assert_eq!(m26.read_chr(0x0400), 9);
    }
}
