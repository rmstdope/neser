use std::cell::Cell;

use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, DEFAULT_CHR_RAM_SIZE, PrgRam};
use crate::cartridge::{Mapper, MirroringMode};

/// Namco 163 (iNES mapper 19) – basic banking + IRQ (audio omitted).
pub struct Namco163Mapper {
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    prg_ram: PrgRam,
    mirroring: MirroringMode,
    regs: [u8; 16],
    namco_ram: [u8; 128],
    audio_addr: Cell<u8>,
    audio_autoinc: Cell<bool>,
    audio_disabled: bool,
    audio_channel_output: [i16; 8],
    audio_update_counter: u8,
    audio_current_channel: i8,
    audio_last_output: i16,
    irq_counter: u16, // 15-bit counter
    irq_enabled: bool,
    irq_pending: bool,
}

impl Namco163Mapper {
    const PRG_BANK_SIZE_8K: usize = 0x2000;
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const IRQ_COUNTER_MAX: u16 = 0x7FFF;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let chr_ram = if chr_rom.is_empty() {
            vec![0; DEFAULT_CHR_RAM_SIZE]
        } else {
            Vec::new()
        };

        Self {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            mirroring,
            regs: [0; 16],
            namco_ram: [0; 128],
            audio_addr: Cell::new(0),
            audio_autoinc: Cell::new(false),
            audio_disabled: false,
            audio_channel_output: [0; 8],
            audio_update_counter: 0,
            audio_current_channel: 7,
            audio_last_output: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
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

    fn prg_bank_index_8k(&self, bank: u8) -> usize {
        let count = self.prg_bank_count_8k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn map_mirroring(&mut self, value: u8) {
        self.mirroring = match value & 0x3 {
            0 => MirroringMode::Vertical,
            1 => MirroringMode::Horizontal,
            2 => MirroringMode::SingleScreenLower,
            3 => MirroringMode::SingleScreenUpper,
            _ => unreachable!("value is masked to 0..3"),
        };
    }

    fn load_irq_counter_from_regs(&mut self) {
        let high = (self.regs[13] as u16) & 0x7F;
        let low = self.regs[12] as u16;
        self.irq_counter = (high << 8) | low;
        self.irq_pending = false;
    }

    fn handle_register_write(&mut self, reg: usize, value: u8) {
        self.regs[reg] = value;
        match reg {
            11 => self.map_mirroring(value),
            12 => {
                // IRQ counter low bits
                self.load_irq_counter_from_regs();
            }
            13 => {
                // IRQ counter high bits + enable flag (bit 7)
                self.irq_enabled = (value & 0x80) != 0;
                self.load_irq_counter_from_regs();
            }
            _ => {}
        }
    }

    fn read_namco_ram(&self, addr: u16) -> u8 {
        let offset = ((addr as usize).saturating_sub(0x4800)) & 0x7F;
        self.namco_ram[offset]
    }

    fn write_namco_ram(&mut self, addr: u16, value: u8) {
        let offset = ((addr as usize).saturating_sub(0x4800)) & 0x7F;
        self.namco_ram[offset] = value;
    }

    fn audio_write_data(&mut self, value: u8) {
        let idx = self.audio_addr.get() as usize;
        if idx < self.namco_ram.len() {
            self.namco_ram[idx] = value;
        }
        if self.audio_autoinc.get() {
            self.audio_addr.set(self.audio_addr.get().wrapping_add(1) & 0x7F);
        }
    }

    fn audio_read_data(&self) -> u8 {
        let val = self.namco_ram[self.audio_addr.get() as usize];
        if self.audio_autoinc.get() {
            self.audio_addr
                .set(self.audio_addr.get().wrapping_add(1) & 0x7F);
        }
        val
    }

    fn audio_set_address(&mut self, value: u8) {
        self.audio_addr.set(value & 0x7F);
        self.audio_autoinc.set((value & 0x80) != 0);
    }

    fn audio_channel_count_minus_one(&self) -> u8 {
        (self.namco_ram[0x7F] >> 4) & 0x07
    }

    fn audio_effective_channel_range(&self) -> (i8, i8) {
        let count_minus_one = self.audio_channel_count_minus_one() as i8;
        let min = 7 - count_minus_one;
        (min, 7)
    }

    fn audio_get_frequency(&self, channel: usize) -> u32 {
        let base = 0x40 + channel * 8;
        let f_low = self.namco_ram[base] as u32;
        let f_mid = self.namco_ram[base + 2] as u32;
        let f_high = (self.namco_ram[base + 4] & 0x03) as u32;
        (f_high << 16) | (f_mid << 8) | f_low
    }

    fn audio_get_phase(&self, channel: usize) -> u32 {
        let base = 0x40 + channel * 8;
        let p_low = self.namco_ram[base + 1] as u32;
        let p_mid = self.namco_ram[base + 3] as u32;
        let p_high = self.namco_ram[base + 5] as u32;
        (p_high << 16) | (p_mid << 8) | p_low
    }

    fn audio_set_phase(&mut self, channel: usize, phase: u32) {
        let base = 0x40 + channel * 8;
        self.namco_ram[base + 1] = (phase & 0xFF) as u8;
        self.namco_ram[base + 3] = ((phase >> 8) & 0xFF) as u8;
        self.namco_ram[base + 5] = ((phase >> 16) & 0xFF) as u8;
    }

    fn audio_wave_length(&self, channel: usize) -> u8 {
        let base = 0x40 + channel * 8;
        let raw = self.namco_ram[base + 4] & 0xFC;
        256u16
            .saturating_sub(raw as u16)
            .max(4) as u8 // clamp to minimum length of 4
    }

    fn audio_wave_address(&self, channel: usize) -> u8 {
        let base = 0x40 + channel * 8;
        self.namco_ram[base + 6]
    }

    fn audio_volume(&self, channel: usize) -> u8 {
        let base = 0x40 + channel * 8;
        self.namco_ram[base + 7] & 0x0F
    }

    fn audio_sample_nibble(&self, sample_pos: u8) -> i8 {
        let byte = self.namco_ram[(sample_pos as usize) / 2];
        if (sample_pos & 0x01) != 0 {
            ((byte >> 4) & 0x0F) as i8
        } else {
            (byte & 0x0F) as i8
        }
    }

    fn audio_update_channel(&mut self, channel: usize) {
        let length = self.audio_wave_length(channel);
        let freq = self.audio_get_frequency(channel);
        let mut phase = self.audio_get_phase(channel);

        if length == 0 {
            phase = 0;
        } else {
            phase = (phase + freq) % ((length as u32) << 16);
        }

        let sample_pos = ((phase >> 16) as u8).wrapping_add(self.audio_wave_address(channel));
        let sample = self.audio_sample_nibble(sample_pos);
        let volume = self.audio_volume(channel) as i16;
        self.audio_channel_output[channel] = (sample as i16 - 8) * volume;

        self.audio_set_phase(channel, phase);
        self.audio_update_mix();
    }

    fn audio_update_mix(&mut self) {
        let count_minus_one = self.audio_channel_count_minus_one() as i16;
        let min_channel = 7 - count_minus_one;
        let mut sum = 0i16;
        for ch in (min_channel..=7).rev() {
            sum = sum.saturating_add(self.audio_channel_output[ch as usize]);
        }
        let channels = count_minus_one + 1;
        self.audio_last_output = if channels > 0 {
            sum / channels
        } else {
            0
        };
    }

    fn audio_clock(&mut self) {
        if self.audio_disabled {
            return;
        }

        self.audio_update_counter = self.audio_update_counter.wrapping_add(1);
        if self.audio_update_counter == 15 {
            let (min_channel, _) = self.audio_effective_channel_range();
            let ch = self.audio_current_channel as usize;
            self.audio_update_channel(ch);

            self.audio_update_counter = 0;
            self.audio_current_channel -= 1;
            if self.audio_current_channel < min_channel {
                self.audio_current_channel = 7;
            }
        }
    }

    #[cfg(test)]
    fn debug_audio_last_output(&self) -> i16 {
        self.audio_last_output
    }
}

impl Mapper for Namco163Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr & 0xF800 {
            0x4800 => self.audio_read_data(),
            _ => {
                if let Some(value) = self.prg_ram.try_read(addr) {
                    return value;
                }

                if self.prg_rom.is_empty() {
                    return 0;
                }

                match addr {
                    0x8000..=0x9FFF => {
                        let bank = self.prg_bank_index_8k(self.regs[8]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xA000..=0xBFFF => {
                        let bank = self.prg_bank_index_8k(self.regs[9]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xC000..=0xDFFF => {
                        let bank = self.prg_bank_index_8k(self.regs[10]);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    0xE000..=0xFFFF => {
                        let bank = self.prg_bank_count_8k().saturating_sub(1);
                        let offset = (addr as usize) & (Self::PRG_BANK_SIZE_8K - 1);
                        let index = bank * Self::PRG_BANK_SIZE_8K + offset;
                        self.prg_rom.get(index).copied().unwrap_or(0)
                    }
                    _ => 0,
                }
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x4800 => {
                self.audio_write_data(value);
            }
            0xF800 => {
                self.audio_set_address(value);
            }
            0xE000 => {
                self.audio_disabled = (value & 0x40) != 0;
            }
            _ => {
                if self.prg_ram.try_write(addr, value) {
                    return;
                }

                if (0x8000..=0xFFFF).contains(&addr) {
                    let reg = (addr & 0x000F) as usize;
                    self.handle_register_write(reg, value);
                }
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let chr_addr = (addr & 0x1FFF) as usize;
        let slot = (chr_addr / Self::CHR_BANK_SIZE_1K).min(7);
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE_1K - 1);
        let bank_reg = self.regs.get(slot).copied().unwrap_or(0);
        let bank = self.chr_bank_index_1k(bank_reg);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;

        if self.has_chr_ram() {
            self.chr_ram.get(index).copied().unwrap_or(0)
        } else {
            self.chr_rom.get(index).copied().unwrap_or(0)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram() {
            return;
        }

        let chr_addr = (addr & 0x1FFF) as usize;
        let slot = (chr_addr / Self::CHR_BANK_SIZE_1K).min(7);
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE_1K - 1);
        let bank_reg = self.regs.get(slot).copied().unwrap_or(0);
        let bank = self.chr_bank_index_1k(bank_reg);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;

        if index < self.chr_ram.len() {
            self.chr_ram[index] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {}

    fn cpu_cycle(&mut self) {
        self.audio_clock();

        if !self.irq_enabled {
            return;
        }

        if self.irq_counter == Self::IRQ_COUNTER_MAX {
            self.irq_counter = 0;
            self.irq_pending = true;
        } else {
            self.irq_counter = (self.irq_counter + 1) & Self::IRQ_COUNTER_MAX;
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio_last_output as f32 / 128.0
    }

    fn get_mirroring(&self) -> MirroringMode {
        self.mirroring
    }

    fn reset(&mut self) {
        self.regs = [0; 16];
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.audio_addr.set(0);
        self.audio_autoinc.set(false);
        self.audio_disabled = false;
        self.audio_channel_output = [0; 8];
        self.audio_update_counter = 0;
        self.audio_current_channel = 7;
        self.audio_last_output = 0;
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
}

#[cfg(test)]
mod tests {
    use crate::cartridge::MirroringMode;
    use crate::cartridge::mapper::{create_mapper, Mapper};
    use crate::cartridge::namco163::Namco163Mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    #[test]
    fn namco163_prg_chr_banking_and_mirroring() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper: Namco163Mapper = Namco163Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Select PRG banks for $8000/$A000/$C000.
        mapper.write_prg(0x8008, 1);
        mapper.write_prg(0x8009, 2);
        mapper.write_prg(0x800A, 3);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR banking: 1KB banks across the 8 slots.
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);
        mapper.write_prg(0x8003, 7);
        mapper.write_prg(0x8004, 8);
        mapper.write_prg(0x8005, 9);
        mapper.write_prg(0x8006, 10);
        mapper.write_prg(0x8007, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);

        // Mirroring register (reg 11).
        mapper.write_prg(0x800B, 1);
        assert_eq!(mapper.get_mirroring(), MirroringMode::Horizontal);
    }

    #[test]
    fn namco163_irq_counter_overflow_triggers_and_write_clears() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_mapper(19, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("Mapper 19 should be implemented");

        // Load counter to 0x7FFF and enable (bit 7 of reg 13).
        mapper.write_prg(0x800C, 0xFF);
        mapper.write_prg(0x800D, 0xFF);

        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // Writing to reg 13 should clear the pending IRQ and disable when bit7 is 0.
        mapper.write_prg(0x800D, 0x00);
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn namco163_internal_ram_and_wram_snapshot() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = Vec::new(); // CHR-RAM path is fine for this test.

        let mut mapper = Namco163Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Internal 128-byte RAM via data port + address port.
        mapper.write_prg(0xF800, 0x80 | 0x00); // ptr=0, auto-inc
        mapper.write_prg(0x4800, 0xAA);
        mapper.write_prg(0x4800, 0xBB);

        mapper.write_prg(0xF800, 0x00); // ptr=0, no auto-inc
        assert_eq!(mapper.read_prg(0x4800), 0xAA);
        assert_eq!(mapper.read_prg(0x4800), 0xAA); // still ptr=0

        // PRG-RAM snapshot/restore.
        mapper.write_prg(0x6000, 0x11);
        assert_eq!(mapper.read_prg(0x6000), 0x11);

        let snap = mapper.wram_snapshot();
        mapper.write_prg(0x6000, 0x00);
        mapper.load_wram_snapshot(&snap);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
    }

    #[test]
    fn namco163_audio_ram_port_and_autoincrement() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);

        let mut mapper = Namco163Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Set RAM pointer to 0 with auto-increment.
        mapper.write_prg(0xF800, 0x80 | 0x00);
        mapper.write_prg(0x4800, 0x12);
        mapper.write_prg(0x4800, 0x34);

        // Reset pointer without auto-increment and read back.
        mapper.write_prg(0xF800, 0x00);
        assert_eq!(mapper.read_prg(0x4800), 0x12);
        assert_eq!(mapper.read_prg(0x4800), 0x12); // no auto-inc, still 0x12

        // Enable auto-increment on read.
        mapper.write_prg(0xF800, 0x80 | 0x00);
        assert_eq!(mapper.read_prg(0x4800), 0x12);
        assert_eq!(mapper.read_prg(0x4800), 0x34);
    }

    #[test]
    fn namco163_audio_outputs_sample() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);

        let mut mapper: Namco163Mapper =
            Namco163Mapper::new(prg_rom, chr_rom, MirroringMode::Vertical);

        // Write a simple waveform: nibble 0xF at position 0.
        mapper.write_prg(0xF800, 0x80 | 0x00); // pointer=0, auto-inc
        mapper.write_prg(0x4800, 0x0F);

        // Configure channel 7 (single-channel mode) registers.
        let base = 0x78u8; // 0x40 + 7*8
        // freq low
        mapper.write_prg(0xF800, 0x80 | base as u8);
        mapper.write_prg(0x4800, 0x01);
        // phase low
        mapper.write_prg(0x4800, 0x00);
        // freq mid
        mapper.write_prg(0x4800, 0x00);
        // phase mid
        mapper.write_prg(0x4800, 0x00);
        // freq high + length (length=4 -> reg=0xFC)
        mapper.write_prg(0x4800, 0xFC);
        // phase high
        mapper.write_prg(0x4800, 0x00);
        // wave address
        mapper.write_prg(0x4800, 0x00);
        // volume
        mapper.write_prg(0x4800, 0x0F);

        // Verify volume register was written.
        mapper.write_prg(0xF800, 0x00 | (base + 7));
        assert_eq!(mapper.read_prg(0x4800), 0x0F);

        // Set channel count to 1 (high nibble = 0) while keeping volume nibble.
        mapper.write_prg(0xF800, 0x00 | 0x7F);
        mapper.write_prg(0x4800, 0x0F);

        // Run enough CPU cycles for an update tick.
        for _ in 0..32 {
            mapper.cpu_cycle();
        }

        mapper.write_prg(0xF800, base + 1); // phase low
        let phase_low = mapper.read_prg(0x4800);
        assert!(phase_low != 0);

        assert!(mapper.debug_audio_last_output() != 0);
        assert!(mapper.expansion_audio_sample() != 0.0);
    }
}
