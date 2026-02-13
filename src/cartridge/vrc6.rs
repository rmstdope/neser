use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};
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

    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: PrgRam,

    prg_bank_16k: u8,
    prg_bank_8k: u8,
    chr_banks_1k: [u8; 8],

    b003: u8,
    mirroring: MirroringMode,

    // --- VRC IRQ (used by VRC6) ---
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    irq_mode_cycle: bool,
    irq_enable_after_ack: bool,
    irq_asserted: bool,
    irq_prescaler: i32,

    // --- VRC6 expansion audio ---
    audio: Vrc6Audio,
}

impl VRC6Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const DEFAULT_CHR_RAM_SIZE: usize = 0x2000;

    pub fn new(
        mapper_number: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> Self {
        let variant = match mapper_number {
            24 => Vrc6Variant::Mapper24,
            26 => Vrc6Variant::Mapper26,
            _ => Vrc6Variant::Mapper24,
        };

        let chr_ram = if chr_rom.is_empty() {
            vec![0; Self::DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            variant,
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_bank_16k: 0,
            prg_bank_8k: 0,
            chr_banks_1k: [0; 8],
            b003: 0,
            mirroring,

            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_mode_cycle: false,
            irq_enable_after_ack: false,
            irq_asserted: false,
            irq_prescaler: 0,

            audio: Vrc6Audio::default(),
        }
    }

    fn has_chr_ram(&self) -> bool {
        self.chr_rom.is_empty()
    }

    fn prg_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE_8K
    }

    fn chr_bank_count_1k(&self) -> usize {
        let chr_len = if self.has_chr_ram() {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        chr_len / Self::CHR_BANK_SIZE_1K
    }

    fn prg_bank_index_8k(&self, bank: usize) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        bank % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn fixed_last_prg_bank_8k(&self) -> usize {
        let count = self.prg_bank_count_8k();
        count.saturating_sub(1)
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

    fn update_mirroring_from_b003(&mut self) {
        // Commercial VRC6 games use banking mode 0 and write values where (b003 & 0x0F)
        // is one of: 0, 4, 8, C.
        self.mirroring = match self.b003 & 0x0F {
            0x0 => MirroringMode::Vertical,
            0x4 => MirroringMode::Horizontal,
            0x8 | 0xC => MirroringMode::SingleScreen,
            _ => self.mirroring,
        };
    }

    fn read_prg_rom_8k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE_8K + bank_offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn read_chr_1k(&self, bank_index: usize, bank_offset: usize) -> u8 {
        let addr = bank_index * Self::CHR_BANK_SIZE_1K + bank_offset;
        if self.has_chr_ram() {
            self.chr_ram.get(addr).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(addr).copied().unwrap_or(0)
        }
    }

    fn reset_irq_prescaler(&mut self) {
        // VRC IRQ scanline-mode prescaler (nesdev): 341 master ticks / 3 per CPU cycle.
        // Using the simple model: start at 341 and subtract 3 each CPU cycle; when <= 0,
        // add 341 and clock the IRQ counter. This makes the first clock after 114 cycles.
        self.irq_prescaler = 341;
    }

    fn acknowledge_irq(&mut self) {
        self.irq_asserted = false;
    }

    fn clock_vrc_irq_counter(&mut self) {
        // VRC IRQ (nesdev):
        // If counter is $FF, reload from latch and trip IRQ; otherwise increment.
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_asserted = true;
        } else {
            self.irq_counter = self.irq_counter.wrapping_add(1);
        }
    }

    fn tick_vrc_irq(&mut self) {
        if !self.irq_enabled {
            return;
        }

        if self.irq_mode_cycle {
            self.clock_vrc_irq_counter();
            return;
        }

        self.irq_prescaler -= 3;
        if self.irq_prescaler <= 0 {
            self.irq_prescaler += 341;
            self.clock_vrc_irq_counter();
        }
    }
}

impl Mapper for VRC6Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0xBFFF => {
                let offset = (addr - 0x8000) as usize;

                // 16KB bank at $8000-$BFFF, selected by 4-bit value.
                // Express in 8KB banks: bank16k * 2, then +0/+1 based on address.
                let bank16k = (self.prg_bank_16k & 0x0F) as usize;
                let bank8k = bank16k * 2 + (offset / Self::PRG_BANK_SIZE_8K);
                let bank_offset = offset % Self::PRG_BANK_SIZE_8K;

                self.read_prg_rom_8k(self.prg_bank_index_8k(bank8k), bank_offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr - 0xC000) as usize;
                let bank8k = (self.prg_bank_8k & 0x1F) as usize;
                self.read_prg_rom_8k(self.prg_bank_index_8k(bank8k), offset)
            }
            0xE000..=0xFFFF => {
                let offset = (addr - 0xE000) as usize;
                self.read_prg_rom_8k(self.fixed_last_prg_bank_8k(), offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = self.normalize_reg_addr(addr);
            match reg {
                0x8000..=0x8003 => self.prg_bank_16k = value & 0x0F,
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
                0xC000..=0xC003 => self.prg_bank_8k = value & 0x1F,
                0xB003 => {
                    self.b003 = value;
                    self.update_mirroring_from_b003();
                }
                0xF000 => {
                    // IRQ Latch
                    self.irq_latch = value;
                }
                0xF001 => {
                    // IRQ Control (.... .MEA)
                    // M: mode (1=cycle, 0=scanline)
                    // E: enable (1=enabled)
                    // A: enable after acknowledgement (copied to E on $F002 writes)
                    self.acknowledge_irq();
                    self.reset_irq_prescaler();

                    self.irq_mode_cycle = (value & 0b0000_0100) != 0;
                    let enable = (value & 0b0000_0010) != 0;
                    self.irq_enable_after_ack = (value & 0b0000_0001) != 0;

                    if enable {
                        self.irq_enabled = true;
                        self.irq_counter = self.irq_latch;
                    } else {
                        self.irq_enabled = false;
                    }
                }
                0xF002 => {
                    // IRQ Acknowledge
                    // Any write acknowledges pending IRQ and copies A->E.
                    self.acknowledge_irq();
                    self.irq_enabled = self.irq_enable_after_ack;
                }
                0xD000..=0xD003 => {
                    let idx = (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                }
                0xE000..=0xE003 => {
                    let idx = 4 + (reg & 0x0003) as usize;
                    self.chr_banks_1k[idx] = value;
                }
                // Other VRC6 registers not currently modeled.
                _ => {}
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let addr = addr & 0x1FFF;
        let bank_slot = (addr as usize) / Self::CHR_BANK_SIZE_1K;
        let bank_offset = (addr as usize) % Self::CHR_BANK_SIZE_1K;

        let bank = self.chr_banks_1k.get(bank_slot).copied().unwrap_or(0);
        self.read_chr_1k(self.chr_bank_index_1k(bank), bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram() {
            return;
        }
        let addr = (addr & 0x1FFF) as usize;
        if addr < self.chr_ram.len() {
            self.chr_ram[addr] = value;
        }
    }

    fn cpu_cycle(&mut self) {
        trace_mapper!(5; "[vrc6] cpu_cycle (irq)");
        self.audio.cpu_cycle();
        self.tick_vrc_irq();
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio.sample()
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        match self.variant {
            Vrc6Variant::Mapper24 => 24,
            Vrc6Variant::Mapper26 => 26,
        }
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

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.clone()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.chr_ram.len());
        if to_copy > 0 {
            self.chr_ram[..to_copy].copy_from_slice(&data[..to_copy]);
        }
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
        snapshot.push(self.irq_latch);
        snapshot.push(self.irq_counter);
        let flags = (self.irq_enabled as u8)
            | ((self.irq_mode_cycle as u8) << 1)
            | ((self.irq_enable_after_ack as u8) << 2)
            | ((self.irq_asserted as u8) << 3);
        snapshot.push(flags);
        let prescaler_bytes = self.irq_prescaler.to_le_bytes();
        snapshot.extend_from_slice(&prescaler_bytes);
        snapshot.push(self.mirroring as u8);
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
            self.irq_latch = data[11];
            self.irq_counter = data[12];
            let flags = data[13];
            self.irq_enabled = (flags & 1) != 0;
            self.irq_mode_cycle = (flags & 2) != 0;
            self.irq_enable_after_ack = (flags & 4) != 0;
            self.irq_asserted = (flags & 8) != 0;
            self.irq_prescaler = i32::from_le_bytes([data[14], data[15], data[16], data[17]]);
            self.mirroring = match data[18] {
                0 => MirroringMode::Horizontal,
                1 => MirroringMode::Vertical,
                2 => MirroringMode::SingleScreen,
                3 => MirroringMode::FourScreen,
                _ => MirroringMode::Horizontal,
            };
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
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge::MirroringMode;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    fn create_vrc6_mapper(
        mapper_number: u16,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: MirroringMode,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(
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

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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
            MirroringMode::Horizontal,
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

        let mut restored = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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

        let mut mapper = create_vrc6_mapper(24, prg_rom, chr_rom, MirroringMode::Horizontal)
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
            MirroringMode::Horizontal,
        )
        .expect("VRC6 (mapper 24) should be implemented");
        m24.write_prg(0xD001, 7);
        assert_eq!(m24.read_chr(0x0400), 7);

        // Mapper 26: the same CPU address $D001 should target internal R2 (not R1).
        // So $0400 should remain at default bank 0, while $0800 uses bank 7.
        let mut m26 = create_vrc6_mapper(26, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("VRC6 (mapper 26) should be implemented");
        m26.write_prg(0xD001, 7);
        assert_eq!(m26.read_chr(0x0400), 0);
        assert_eq!(m26.read_chr(0x0800), 7);

        // And writing $D002 should then target internal R1.
        m26.write_prg(0xD002, 9);
        assert_eq!(m26.read_chr(0x0400), 9);
    }
}
