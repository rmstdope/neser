//! Mapper 19 - Namco 163
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use std::cell::Cell;

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 19 - Namco 163 (Namco 129/163 with expansion audio)
///
/// Hardware: Namco's advanced mapper with 8 wavetable synthesis channels
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/Namco_163>
/// - Audio: <https://www.nesdev.org/wiki/Namco_163_audio>
/// - Expansion audio: <https://www.nesdev.org/wiki/Namco_163_audio#Synthesis>
/// - PRG-ROM: Up to 512KB (three 8KB switchable banks + one fixed)
/// - PRG-RAM: 8KB internal RAM at $6000-$7FFF
/// - CHR: Up to 256KB (eight 1KB switchable banks) or CHR-RAM
/// - Mirroring: Programmable via nametable control
///
/// Common boards: NAMCOT-163, NAMCOT-175, NAMCOT-340
///
/// Notes:
/// - CPU-cycle driven IRQ counter (15-bit down-counter)
/// - 128-byte internal RAM for wavetable storage and audio registers
/// - Up to 8 channels of wavetable synthesis expansion audio
/// - Nametable control allows flexible mirroring configurations
/// - Used in Megami Tensei II, King of Kings, Famista series
///
/// Implementation:
/// - Basic banking and IRQ fully implemented
/// - Expansion audio implemented with wavetable synthesis
pub struct Namco163Mapper {
    base: BaseMapper,
    prg_ram: PrgRam,
    ciram: [u8; 0x800],
    chr_nt_regs: [u8; 12],
    prg_select: [u8; 3],
    regs: [u8; 16],
    namco_ram: [u8; 128],
    audio_addr: Cell<u8>,
    audio_autoinc: Cell<bool>,
    audio_disabled: bool,
    audio_channel_output: [i16; 8],
    audio_update_counter: u8,
    audio_current_channel: i8,
    audio_last_output: i16,
    e800_control: u8,
    wram_protect: u8,
    irq_counter: u16, // 15-bit counter
    irq_enabled: bool,
    irq_pending: bool,
}

impl Namco163Mapper {
    const CHR_BANK_SIZE_1K: usize = 0x0400;
    const CIRAM_BANK_THRESHOLD: u8 = 0xE0;
    const CIRAM_INDEX_MASK: usize = 0x07FF;
    const IRQ_COUNTER_MAX: u16 = 0x7FFF;
    const PRG_SELECT_MASK: u8 = 0x3F;
    const MIRRORING_REG: usize = 11;
    const IRQ_LOW_REG: usize = 12;
    const IRQ_HIGH_ENABLE_REG: usize = 13;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 0, // Namco163 manages PRG-RAM separately
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);

        let mut mapper = Self {
            base,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            ciram: [0; 0x800],
            chr_nt_regs: [0; 12],
            prg_select: [0; 3],
            regs: [0; 16],
            namco_ram: [0; 128],
            audio_addr: Cell::new(0),
            audio_autoinc: Cell::new(false),
            audio_disabled: false,
            audio_channel_output: [0; 8],
            audio_update_counter: 0,
            audio_current_channel: 7,
            audio_last_output: 0,
            e800_control: 0,
            wram_protect: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.base.set_mirroring(mirroring);
        mapper.update_banks();
        mapper
    }

    fn chr_bank_count_1k(&self) -> usize {
        self.base.chr_bank_count()
    }

    fn ciram_page_index(bank: u8, bank_offset: usize) -> usize {
        (((bank & 1) as usize) * Self::CHR_BANK_SIZE_1K) + bank_offset
    }

    fn ciram_index(bank: u8, bank_offset: usize) -> usize {
        Self::ciram_page_index(bank, bank_offset) & Self::CIRAM_INDEX_MASK
    }

    fn set_prg_select_bank(&mut self, slot: usize, value: u8) {
        self.prg_select[slot] = value & Self::PRG_SELECT_MASK;
        self.update_banks();
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_select[0] as i16);
        self.base.select_prg_page(1, self.prg_select[1] as i16);
        self.base.select_prg_page(2, self.prg_select[2] as i16);
        self.base.select_prg_page(3, -1); // fixed last bank
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        let count = self.chr_bank_count_1k();
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn map_mirroring(&mut self, value: u8) {
        self.base.set_mirroring(match value & 0x3 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            3 => NametableLayout::SingleScreenUpper,
            _ => unreachable!("value is masked to 0..3"),
        });
    }

    fn load_irq_counter_from_regs(&mut self) {
        let high = (self.regs[Self::IRQ_HIGH_ENABLE_REG] as u16) & 0x7F;
        let low = self.regs[Self::IRQ_LOW_REG] as u16;
        self.irq_counter = (high << 8) | low;
        self.irq_pending = false;
    }

    fn irq_status_register_value(&self) -> u8 {
        ((self.irq_counter >> 8) as u8 & 0x7F) | (((!self.irq_enabled) as u8) << 7)
    }

    fn handle_register_write(&mut self, reg: usize, value: u8) {
        self.regs[reg] = value;
        match reg {
            Self::MIRRORING_REG => self.map_mirroring(value),
            Self::IRQ_LOW_REG => {
                // IRQ counter low bits
                self.load_irq_counter_from_regs();
            }
            Self::IRQ_HIGH_ENABLE_REG => {
                // IRQ counter high bits + disable flag (bit 7: 1=disabled, 0=enabled)
                self.irq_enabled = (value & 0x80) == 0;
                self.load_irq_counter_from_regs();
            }
            _ => {}
        }
    }

    fn chr_nt_register_index(addr: u16) -> Option<usize> {
        if (0x8000..=0xDFFF).contains(&addr) {
            Some(((addr - 0x8000) / 0x0800) as usize)
        } else {
            None
        }
    }

    fn chr_uses_ciram(&self, slot: usize, bank: u8) -> bool {
        if bank < Self::CIRAM_BANK_THRESHOLD {
            return false;
        }
        let disable = if slot < 4 {
            (self.e800_control & 0x40) != 0
        } else {
            (self.e800_control & 0x80) != 0
        };
        !disable
    }

    fn read_chr_slot(&self, slot: usize, bank_offset: usize) -> u8 {
        let bank = self.chr_nt_regs.get(slot).copied().unwrap_or(0);
        if self.chr_uses_ciram(slot, bank) {
            return self.ciram[Self::ciram_index(bank, bank_offset)];
        }

        let bank = self.chr_bank_index_1k(bank);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;
        self.base.read_chr_at_index(index)
    }

    fn write_chr_slot(&mut self, slot: usize, bank_offset: usize, value: u8) {
        let bank = self.chr_nt_regs.get(slot).copied().unwrap_or(0);
        if self.chr_uses_ciram(slot, bank) {
            self.ciram[Self::ciram_index(bank, bank_offset)] = value;
            return;
        }

        let bank = self.chr_bank_index_1k(bank);
        let index = bank * Self::CHR_BANK_SIZE_1K + bank_offset;
        self.base.write_chr_at_index(index, value);
    }

    fn nametable_bank_for_addr(&self, addr: u16) -> u8 {
        let quadrant = ((addr & 0x0FFF) >> 10) as usize;
        self.chr_nt_regs[8 + quadrant]
    }

    fn normalize_nametable_addr(addr: u16) -> Option<u16> {
        let addr = addr & 0x2FFF;
        (0x2000..=0x2FFF).contains(&addr).then_some(addr)
    }

    fn nametable_bank_and_offset(&self, addr: u16) -> Option<(u8, usize)> {
        let addr = Self::normalize_nametable_addr(addr)?;
        let bank = self.nametable_bank_for_addr(addr);
        let offset = (addr & 0x03FF) as usize;
        Some((bank, offset))
    }

    fn wram_write_enabled(&self, addr: u16) -> bool {
        let has_valid_wram_protect_prefix = (0x40..=0x4E).contains(&self.wram_protect);
        if !has_valid_wram_protect_prefix {
            return false;
        }

        let window = ((addr - 0x6000) / 0x0800) as u8;
        let window_write_protect_bit = 1u8 << window;
        (self.wram_protect & window_write_protect_bit) == 0
    }

    fn nametable_ciram_index(&self, addr: u16) -> Option<usize> {
        let (bank, offset) = self.nametable_bank_and_offset(addr)?;
        if bank < Self::CIRAM_BANK_THRESHOLD {
            return None;
        }

        Some(Self::ciram_index(bank, offset))
    }

    fn nametable_chr_index(&self, addr: u16) -> Option<usize> {
        let (bank, offset) = self.nametable_bank_and_offset(addr)?;
        if bank >= Self::CIRAM_BANK_THRESHOLD {
            return None;
        }

        let bank = self.chr_bank_index_1k(bank);
        Some(bank * Self::CHR_BANK_SIZE_1K + offset)
    }

    #[cfg(test)]
    fn read_namco_ram(&self, addr: u16) -> u8 {
        let offset = ((addr as usize).saturating_sub(0x4800)) & 0x7F;
        self.namco_ram[offset]
    }

    #[cfg(test)]
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
            self.audio_addr
                .set(self.audio_addr.get().wrapping_add(1) & 0x7F);
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
        256u16.saturating_sub(raw as u16).max(4) as u8 // clamp to minimum length of 4
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
        self.audio_last_output = if channels > 0 { sum / channels } else { 0 };
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

    fn clock_irq_counter(&mut self) {
        if self.irq_counter < Self::IRQ_COUNTER_MAX {
            self.irq_counter += 1;
            if self.irq_counter == Self::IRQ_COUNTER_MAX {
                self.irq_pending = true;
            }
            return;
        }

        self.irq_pending = true;
    }

    #[cfg(test)]
    fn debug_audio_last_output(&self) -> i16 {
        self.audio_last_output
    }
}

impl Mapper for Namco163Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            // N163 PRG-RAM at $6000-$7FFF is always accessible (no enable gate).
            0x6000..=0x7FFF => self.read_prg(addr),
            _ if addr < 0x6000 => open_bus,
            _ => self.read_prg(addr),
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr & 0xF800 {
            0x4800 => self.audio_read_data(),
            0x5000 => (self.irq_counter & 0x00FF) as u8,
            0x5800 => self.irq_status_register_value(),
            _ => {
                if let Some(value) = self.prg_ram.try_read(addr) {
                    return value;
                }

                self.base.read_prg_banked(addr)
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x4800 => {
                self.audio_write_data(value);
            }
            0x5000 => {
                self.handle_register_write(Self::IRQ_LOW_REG, value);
            }
            0x5800 => {
                self.handle_register_write(Self::IRQ_HIGH_ENABLE_REG, value);
            }
            0xF800 => {
                self.audio_set_address(value);
                self.wram_protect = value;
            }
            0xE000 => {
                self.audio_disabled = (value & 0x40) != 0;
                self.set_prg_select_bank(0, value);
            }
            0xE800 => {
                self.e800_control = value;
                self.set_prg_select_bank(1, value);
            }
            0xF000 => {
                self.set_prg_select_bank(2, value);
            }
            _ => {
                if (0x6000..=0x7FFF).contains(&addr) {
                    if self.wram_write_enabled(addr) {
                        let _ = self.prg_ram.try_write(addr, value);
                    }
                    return;
                }

                if self.prg_ram.try_write(addr, value) {
                    return;
                }

                if let Some(reg) = Self::chr_nt_register_index(addr) {
                    self.chr_nt_regs[reg] = value;
                    self.regs[reg] = value;
                    if reg == Self::MIRRORING_REG {
                        self.map_mirroring(value);
                    }
                }
            }
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let chr_addr = (addr & 0x1FFF) as usize;
        let slot = (chr_addr / Self::CHR_BANK_SIZE_1K).min(7);
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE_1K - 1);
        self.read_chr_slot(slot, bank_offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let chr_addr = (addr & 0x1FFF) as usize;
        let slot = (chr_addr / Self::CHR_BANK_SIZE_1K).min(7);
        let bank_offset = chr_addr & (Self::CHR_BANK_SIZE_1K - 1);
        self.write_chr_slot(slot, bank_offset, value);
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        if let Some(index) = self.nametable_ciram_index(addr) {
            return Some(self.ciram[index]);
        }
        self.nametable_chr_index(addr)
            .map(|index| self.base.read_chr_at_index(index))
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        if let Some(index) = self.nametable_ciram_index(addr) {
            self.ciram[index] = value;
            return true;
        }
        if let Some(index) = self.nametable_chr_index(addr) {
            self.base.write_chr_at_index(index, value);
            return true;
        }
        false
    }

    fn cpu_cycle(&mut self) {
        self.audio_clock();

        if !self.irq_enabled {
            return;
        }

        self.clock_irq_counter();
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn expansion_audio_sample(&self) -> f32 {
        self.audio_last_output as f32 / 128.0
    }

    fn reset(&mut self) {
        self.regs = [0; 16];
        self.chr_nt_regs = [0; 12];
        self.prg_select = [0; 3];
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
        self.e800_control = 0;
        self.wram_protect = 0;
        self.update_banks();
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
        // Serialize Namco163 internal registers:
        // [0-15]: regs[0-15]
        // [16-143]: namco_ram[0-127]
        // [144]: irq_counter low byte
        // [145]: irq_counter high byte (7 bits) + irq_enabled (1 bit)
        // [146]: flags (irq_pending, audio_disabled)
        // [147]: audio_addr
        // [148]: audio_autoinc
        // [149-164]: audio_channel_output[0-7] (i16 LE)
        // [165]: audio_update_counter
        // [166]: audio_current_channel
        // [167-168]: audio_last_output (i16 LE)
        let mut snapshot = Vec::with_capacity(169);
        snapshot.extend_from_slice(&self.regs);
        snapshot.extend_from_slice(&self.namco_ram);
        snapshot.push((self.irq_counter & 0xFF) as u8);
        snapshot.push(((self.irq_counter >> 8) as u8) | (((!self.irq_enabled) as u8) << 7));
        let flags = (self.irq_pending as u8) | ((self.audio_disabled as u8) << 1);
        snapshot.push(flags);
        snapshot.push(self.audio_addr.get());
        snapshot.push(self.audio_autoinc.get() as u8);
        for value in self.audio_channel_output {
            snapshot.extend_from_slice(&value.to_le_bytes());
        }
        snapshot.push(self.audio_update_counter);
        snapshot.push(self.audio_current_channel as u8);
        snapshot.extend_from_slice(&self.audio_last_output.to_le_bytes());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 147 {
            self.regs.copy_from_slice(&data[0..16]);
            self.chr_nt_regs.copy_from_slice(&self.regs[0..12]);
            self.map_mirroring(self.regs[Self::MIRRORING_REG]);
            self.namco_ram.copy_from_slice(&data[16..144]);
            self.irq_counter = (data[144] as u16) | (((data[145] & 0x7F) as u16) << 8);
            self.irq_enabled = (data[145] & 0x80) == 0;
            let flags = data[146];
            self.irq_pending = (flags & 1) != 0;
            self.audio_disabled = (flags & 2) != 0;
        }

        if data.len() >= 169 {
            self.audio_addr.set(data[147] & 0x7F);
            self.audio_autoinc.set((data[148] & 1) != 0);
            let mut offset = 149;
            for slot in &mut self.audio_channel_output {
                let lo = data[offset];
                let hi = data[offset + 1];
                *slot = i16::from_le_bytes([lo, hi]);
                offset += 2;
            }
            self.audio_update_counter = data[165];
            self.audio_current_channel = data[166] as i8;
            self.audio_last_output = i16::from_le_bytes([data[167], data[168]]);
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
    use crate::nes::cartridge::namco163::Namco163Mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_namco163_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(19, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn namco163_prg_chr_banking_and_mirroring() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper: Namco163Mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Select PRG banks for $8000/$A000/$C000.
        mapper.write_prg(0xE000, 1);
        mapper.write_prg(0xE800, 2);
        mapper.write_prg(0xF000, 3);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR banking: 1KB banks across the 8 slots.
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8800, 5);
        mapper.write_prg(0x9000, 6);
        mapper.write_prg(0x9800, 7);
        mapper.write_prg(0xA000, 8);
        mapper.write_prg(0xA800, 9);
        mapper.write_prg(0xB000, 10);
        mapper.write_prg(0xB800, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);

        // Mirroring register (reg 11).
        mapper.write_prg(0xD800, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn namco163_irq_counter_overflow_triggers_and_write_clears() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_namco163_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 19 should be implemented");

        // Load counter to 0x7FFF and enable (bit 7 clear in reg 13).
        mapper.write_prg(0x5000, 0xFF);
        mapper.write_prg(0x5800, 0x7F);

        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // Writing to reg 13 should clear the pending IRQ and disable when bit7 is 1.
        mapper.write_prg(0x5800, 0x80);
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn namco163_register_windows_use_0x800_ranges() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 16);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Given CHR bank writes to distinct 0x800-sized register windows
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0x8800, 5);

        // When reading from slot 0 and slot 1
        let slot0 = mapper.read_chr(0x0000);
        let slot1 = mapper.read_chr(0x0400);

        // Then each slot should reflect its own window write
        assert_eq!(slot0, 3);
        assert_eq!(slot1, 5);
    }

    #[test]
    fn namco163_irq_registers_are_directly_readable_at_5000_and_5800() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Given direct writes to IRQ low/high registers
        mapper.write_prg(0x5000, 0x34);
        mapper.write_prg(0x5800, 0x80 | 0x12);

        // Then reads expose current counter low and high+enable
        assert_eq!(mapper.read_prg(0x5000), 0x34);
        assert_eq!(mapper.read_prg(0x5800), 0x80 | 0x12);

        // Given a pending IRQ from counter overflow
        mapper.write_prg(0x5000, 0xFF);
        mapper.write_prg(0x5800, 0x7F);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // When writing either IRQ register again
        mapper.write_prg(0x5000, 0x00);

        // Then pending IRQ should be acknowledged (cleared)
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn namco163_chr_e0_bank_uses_ciram_when_enabled_by_e800() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 256);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Given bank $E0 in $0000-$03FF and E800.6 = 0 (CIRAM enabled for $0000-$0FFF)
        mapper.write_prg(0x8000, 0xE0);
        mapper.write_prg(0xE800, 0x00);

        // When writing through CHR port, it should target CIRAM (writable) instead of CHR-ROM
        mapper.write_chr(0x0000, 0x5A);
        assert_eq!(mapper.read_chr(0x0000), 0x5A);

        // And when E800.6 is set, $E0-$FF should resolve to CHR-ROM again
        mapper.write_prg(0xE800, 0x40);
        assert_eq!(mapper.read_chr(0x0000), 0xE0);
    }

    #[test]
    fn namco163_c000_dfff_registers_bank_nametable_pages() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 16);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Given $C000/$C800 select nametable pages (E0 -> CIRAM A, E1 -> CIRAM B)
        mapper.write_prg(0xC000, 0xE0);
        mapper.write_prg(0xC800, 0xE1);

        // When writing two different nametable quadrants
        assert!(mapper.write_nametable(0x2000, 0x11));
        assert!(mapper.write_nametable(0x2400, 0x22));

        // Then reads should come from independently banked CIRAM pages
        assert_eq!(mapper.read_nametable(0x2000), Some(0x11));
        assert_eq!(mapper.read_nametable(0x2400), Some(0x22));
    }

    #[test]
    fn namco163_f800_controls_wram_write_protect_windows() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Given invalid high nibble, all WRAM writes should be blocked
        mapper.write_prg(0xF800, 0x00);
        mapper.write_prg(0x6000, 0xAA);
        assert_ne!(mapper.read_prg(0x6000), 0xAA);

        // Given 0x40 (valid high nibble + all windows writable), writes are allowed
        mapper.write_prg(0xF800, 0x40);
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x6800, 0x22);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0x6800), 0x22);

        // Given bit A set, only $6000-$67FF is write-protected
        mapper.write_prg(0xF800, 0x41);
        mapper.write_prg(0x6000, 0x33);
        mapper.write_prg(0x6800, 0x44);

        // Then protected window remains unchanged while unprotected window updates
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0x6800), 0x44);
    }

    #[test]
    fn namco163_irq_counter_saturates_at_7fff() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Given IRQ counter at $7FFF and enabled
        mapper.write_prg(0x5000, 0xFF);
        mapper.write_prg(0x5800, 0x7F);

        // When clocked, IRQ should assert and counter should remain saturated at $7FFF
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
        assert_eq!(mapper.read_prg(0x5000), 0xFF);
        assert_eq!(mapper.read_prg(0x5800), 0x7F);

        // Additional clocks should keep it saturated
        mapper.cpu_cycle();
        assert_eq!(mapper.read_prg(0x5000), 0xFF);
        assert_eq!(mapper.read_prg(0x5800), 0x7F);
    }

    #[test]
    fn namco163_e800_prg_bank_uses_low_6_bits_only() {
        let prg_rom = banked_data(8 * 1024, 128);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Given E800 with upper control bits set and low bank bits = 3
        mapper.write_prg(0xE800, 0xC3);

        // Then $A000 PRG bank should use only low 6 bits for bank selection
        assert_eq!(mapper.read_prg(0xA000), 3);
    }

    #[test]
    fn namco163_c000_nt_bank_below_e0_reads_from_chr_memory() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 16);
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Given nametable page register points to CHR bank 2
        mapper.write_prg(0xC000, 0x02);

        // Then nametable read should be provided by mapper from CHR memory
        assert_eq!(mapper.read_nametable(0x2000), Some(2));
    }

    #[test]
    fn namco163_c000_nt_bank_below_e0_writes_chr_ram_when_present() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_ram = Vec::new(); // CHR-RAM configuration
        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_ram,
            NametableLayout::Vertical,
        ));

        // Given nametable page register points to CHR bank 1
        mapper.write_prg(0xC000, 0x01);

        // When writing through nametable path, mapper should handle it in CHR-RAM
        assert!(mapper.write_nametable(0x2000, 0x7B));
        assert_eq!(mapper.read_nametable(0x2000), Some(0x7B));
    }

    #[test]
    fn namco163_internal_ram_and_wram_snapshot() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = Vec::new(); // CHR-RAM path is fine for this test.

        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Internal 128-byte RAM via data port + address port.
        mapper.write_prg(0xF800, 0x80); // ptr=0, auto-inc
        mapper.write_prg(0x4800, 0xAA);
        mapper.write_prg(0x4800, 0xBB);

        mapper.write_prg(0xF800, 0x00); // ptr=0, no auto-inc
        assert_eq!(mapper.read_prg(0x4800), 0xAA);
        assert_eq!(mapper.read_prg(0x4800), 0xAA); // still ptr=0

        assert_eq!(mapper.read_namco_ram(0x4800), 0xAA);
        mapper.write_namco_ram(0x4800, 0xCC);
        assert_eq!(mapper.read_namco_ram(0x4800), 0xCC);

        // PRG-RAM snapshot/restore.
        mapper.write_prg(0xF800, 0x40);
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

        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Set RAM pointer to 0 with auto-increment.
        mapper.write_prg(0xF800, 0x80);
        mapper.write_prg(0x4800, 0x12);
        mapper.write_prg(0x4800, 0x34);

        // Reset pointer without auto-increment and read back.
        mapper.write_prg(0xF800, 0x00);
        assert_eq!(mapper.read_prg(0x4800), 0x12);
        assert_eq!(mapper.read_prg(0x4800), 0x12); // no auto-inc, still 0x12

        // Enable auto-increment on read.
        mapper.write_prg(0xF800, 0x80);
        assert_eq!(mapper.read_prg(0x4800), 0x12);
        assert_eq!(mapper.read_prg(0x4800), 0x34);
    }

    #[test]
    fn namco163_audio_outputs_sample() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);

        let mut mapper: Namco163Mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        // Write a simple waveform: nibble 0xF at position 0.
        mapper.write_prg(0xF800, 0x80); // pointer=0, auto-inc
        mapper.write_prg(0x4800, 0x0F);

        // Configure channel 7 (single-channel mode) registers.
        let base = 0x78u8; // 0x40 + 7*8
        // freq low
        mapper.write_prg(0xF800, 0x80 | base);
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
        mapper.write_prg(0xF800, base + 7);
        assert_eq!(mapper.read_prg(0x4800), 0x0F);

        // Set channel count to 1 (high nibble = 0) while keeping volume nibble.
        mapper.write_prg(0xF800, 0x7F);
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

    #[test]
    fn namco163_registers_snapshot_restores_audio_state_and_mirroring() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);

        let mut mapper = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        // Configure audio so we have a non-zero last output.
        mapper.write_prg(0xF800, 0x80); // pointer=0, auto-inc
        mapper.write_prg(0x4800, 0x0F);

        let base = 0x78u8; // channel 7 base
        mapper.write_prg(0xF800, 0x80 | base);
        mapper.write_prg(0x4800, 0x01); // freq low
        mapper.write_prg(0x4800, 0x00); // phase low
        mapper.write_prg(0x4800, 0x00); // freq mid
        mapper.write_prg(0x4800, 0x00); // phase mid
        mapper.write_prg(0x4800, 0xFC); // freq high + length
        mapper.write_prg(0x4800, 0x00); // phase high
        mapper.write_prg(0x4800, 0x00); // wave address
        mapper.write_prg(0x4800, 0x0F); // volume

        mapper.write_prg(0xF800, 0x7F);
        mapper.write_prg(0x4800, 0x0F);

        for _ in 0..32 {
            mapper.cpu_cycle();
        }

        assert!(mapper.debug_audio_last_output() != 0);

        // Set a known audio pointer/autoinc state.
        mapper.write_prg(0xF800, 0x80 | 0x05);
        mapper.write_prg(0x4800, 0xAA);
        mapper.write_prg(0x4800, 0xBB);
        mapper.write_prg(0xF800, 0x80 | 0x05);

        // Set mirroring to SingleScreenLower.
        mapper.write_prg(0xD800, 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        let snapshot = mapper.registers_snapshot();

        let mut restored = Namco163Mapper::new(MapperContext::new_for_test(
            19,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&snapshot);

        assert_eq!(restored.get_mirroring(), NametableLayout::SingleScreenLower);
        assert_eq!(restored.read_prg(0x4800), 0xAA);
        assert_eq!(restored.read_prg(0x4800), 0xBB);
        assert_eq!(
            restored.debug_audio_last_output(),
            mapper.debug_audio_last_output()
        );
    }
}
