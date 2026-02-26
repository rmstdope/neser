//! Mapper 056 - Kaiser KS202 (Pirate SMB3)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_056>
//!
//! Known Limitations:
//! - PRG-RAM is exposed at $6000-$7FFF but not backed by battery save.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 056 - Kaiser KS202
///
/// Hardware: KS202 ASIC (an upgrade to Konami's VRC3)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_056>
/// - PRG-ROM: Up to 256 KiB (32 × 8 KiB banks via bank reg + PRG A17 bit)
/// - CHR: Up to 128 KiB (128 × 1 KiB banks, 7-bit registers)
/// - Mirroring: Programmable (H/V)
/// - IRQ: 16-bit CPU-cycle counter, VRC3-like
///
/// Register map (CPU address space):
/// - $8000-$8FFF: IRQ latch nibble 0 [3:0]
/// - $9000-$9FFF: IRQ latch nibble 1 [7:4]
/// - $A000-$AFFF: IRQ latch nibble 2 [11:8]
/// - $B000-$BFFF: IRQ latch nibble 3 [15:12]
/// - $C000-$CFFF: IRQ control (bit0=A: enable-after-ack, bit1=E: enable-now)
/// - $D000-$DFFF: IRQ acknowledge
/// - $E000-$EFFF: Bank register select (bits [2:0], values 1/2/3 for $8000/$A000/$C000)
/// - $F000-$FFFF: Bank data and sub-registers (superimposed):
///   - $F000-$F3FF (mask $FC03): PRG A17 bit for banks 0-3 (bit 3 of written value)
///   - $F800-$FBFF (mask $FC00): Mirroring (bit 0: 0=H, 1=V)
///   - $FC00-$FC07 (mask $FC07): CHR 1KB banks 0-7 (7-bit value)
///   - All $F000-$FFFF: Bank data [3:0] → update last-selected PRG bank register
///
/// PRG effective 5-bit bank = (prg_a17[window] << 4) | prg_reg[window]
/// Power-on: prg_a17[0..3] all = 1
pub struct Mapper56 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    prg_reg: [u8; 3],  // 4-bit PRG bank selects for $8000/$A000/$C000
    prg_a17: [u8; 4],  // A17 extension bit for each 8KB window (0-3)
    chr_regs: [u8; 8], // 7-bit CHR 1KB bank selects
    mirroring: NametableLayout,
    bank_select: u8, // Last value written to $E000
    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_after_ack: bool, // "A" bit from $C000
    irq_pending: bool,
    prg_ram: [u8; 8192],
}

impl Mapper56 {
    const MAPPER_NUMBER: u8 = 56;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        let _ = mirroring; // mirroring is programmable; header ignored
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            prg_reg: [0; 3],
            prg_a17: [1; 4], // power-on: holding 1 per spec
            chr_regs: [0; 8],
            mirroring: NametableLayout::Vertical,
            bank_select: 0,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_after_ack: false,
            irq_pending: false,
            prg_ram: [0; 8192],
        }
    }

    fn num_prg_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn prg_bank_read(&self, bank5: usize, offset: usize) -> u8 {
        let count = self.num_prg_banks();
        if count == 0 {
            return 0;
        }
        let b = bank5 % count;
        self.prg_rom
            .get(b * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn effective_prg_bank(&self, window: usize) -> usize {
        match window {
            0..=2 => {
                let a17 = self.prg_a17[window] as usize & 1;
                (a17 << 4) | (self.prg_reg[window] as usize & 0x0F)
            }
            3 => {
                // Fixed last bank; uses a17[3]
                let count = self.num_prg_banks();
                let a17 = self.prg_a17[3] as usize & 1;
                // Within the "a17 block" we want the last bank
                let block_start = a17 << 4;
                let block_size = 16usize;
                // Pick the highest bank in the block that is within ROM
                if count == 0 {
                    0
                } else {
                    (block_start + block_size - 1).min(count - 1)
                }
            }
            _ => 0,
        }
    }
}

impl Mapper for Mapper56 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr as usize) & 0x1FFF],
            0x8000..=0x9FFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_bank_read(self.effective_prg_bank(0), offset)
            }
            0xA000..=0xBFFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_bank_read(self.effective_prg_bank(1), offset)
            }
            0xC000..=0xDFFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_bank_read(self.effective_prg_bank(2), offset)
            }
            0xE000..=0xFFFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_bank_read(self.effective_prg_bank(3), offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.prg_ram[(addr as usize) & 0x1FFF] = value;
            }
            0x8000..=0x8FFF => {
                self.irq_latch = (self.irq_latch & 0xFFF0) | ((value as u16) & 0x0F);
            }
            0x9000..=0x9FFF => {
                self.irq_latch = (self.irq_latch & 0xFF0F) | (((value as u16) & 0x0F) << 4);
            }
            0xA000..=0xAFFF => {
                self.irq_latch = (self.irq_latch & 0xF0FF) | (((value as u16) & 0x0F) << 8);
            }
            0xB000..=0xBFFF => {
                self.irq_latch = (self.irq_latch & 0x0FFF) | (((value as u16) & 0x0F) << 12);
            }
            0xC000..=0xCFFF => {
                // IRQ control: bit0=A (enable after ack), bit1=E (enable now)
                self.irq_after_ack = (value & 0x01) != 0;
                self.irq_enabled = (value & 0x02) != 0;
                if !self.irq_enabled {
                    self.irq_pending = false;
                }
            }
            0xD000..=0xDFFF => {
                // IRQ acknowledge: clear pending; reload counter; A→E
                self.irq_pending = false;
                self.irq_counter = self.irq_latch;
                self.irq_enabled = self.irq_after_ack;
            }
            0xE000..=0xEFFF => {
                self.bank_select = value & 0x07;
            }
            0xF000..=0xFFFF => {
                // Bank data write (primary)
                match self.bank_select {
                    1 => self.prg_reg[0] = value & 0x0F,
                    2 => self.prg_reg[1] = value & 0x0F,
                    3 => self.prg_reg[2] = value & 0x0F,
                    _ => {}
                }

                // Superimposed register: PRG A17 ($F000-$F3FF, mask $FC03)
                if (addr & 0xFC00) == 0xF000 {
                    let slot = (addr & 0x0003) as usize;
                    if slot < 4 {
                        self.prg_a17[slot] = (value >> 3) & 0x01;
                    }
                }

                // Superimposed register: Mirroring ($F800-$FBFF, mask $FC00)
                if (addr & 0xFC00) == 0xF800 {
                    self.mirroring = if (value & 0x01) != 0 {
                        NametableLayout::Vertical
                    } else {
                        NametableLayout::Horizontal
                    };
                }

                // Superimposed register: CHR banks ($FC00-$FC07, mask $FC07)
                if (addr & 0xFC00) == 0xFC00 {
                    let slot = (addr & 0x0007) as usize;
                    self.chr_regs[slot] = value & 0x7F;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = (addr as usize) / Self::CHR_BANK_SIZE;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        let bank = self.chr_regs[slot & 7] as usize;
        let chr_count = self.chr_memory.size() / Self::CHR_BANK_SIZE;
        if chr_count == 0 {
            return self.chr_memory.read(addr);
        }
        let safe_bank = bank % chr_count;
        self.chr_memory
            .read_at_index(safe_bank * Self::CHR_BANK_SIZE + offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        8192
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled || self.irq_counter == 0 {
            return;
        }
        self.irq_counter -= 1;
        if self.irq_counter == 0 {
            self.irq_pending = true;
            self.irq_enabled = false; // counter stops, like VRC3
        }
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.to_vec()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(self.prg_ram.len());
        self.prg_ram[..len].copy_from_slice(&data[..len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.mirroring {
            NametableLayout::Vertical => 1u8,
            _ => 0u8,
        };
        let irq_flags = (self.irq_enabled as u8)
            | ((self.irq_pending as u8) << 1)
            | ((self.irq_after_ack as u8) << 2);
        let mut v = vec![
            self.prg_reg[0],
            self.prg_reg[1],
            self.prg_reg[2],
            self.prg_a17[0],
            self.prg_a17[1],
            self.prg_a17[2],
            self.prg_a17[3],
            mirror_byte,
            self.bank_select,
            irq_flags,
            (self.irq_latch & 0xFF) as u8,
            (self.irq_latch >> 8) as u8,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
        ];
        v.extend_from_slice(&self.chr_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 22 {
            return;
        }
        self.prg_reg[0] = data[0];
        self.prg_reg[1] = data[1];
        self.prg_reg[2] = data[2];
        self.prg_a17[0] = data[3];
        self.prg_a17[1] = data[4];
        self.prg_a17[2] = data[5];
        self.prg_a17[3] = data[6];
        self.mirroring = if data[7] != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
        self.bank_select = data[8];
        self.irq_enabled = (data[9] & 1) != 0;
        self.irq_pending = (data[9] & 2) != 0;
        self.irq_after_ack = (data[9] & 4) != 0;
        self.irq_latch = (data[10] as u16) | ((data[11] as u16) << 8);
        self.irq_counter = (data[12] as u16) | ((data[13] as u16) << 8);
        self.chr_regs.copy_from_slice(&data[14..22]);
    }

    fn reset(&mut self) {
        self.prg_reg = [0; 3];
        self.prg_a17 = [1; 4];
        self.chr_regs = [0; 8];
        self.mirroring = NametableLayout::Vertical;
        self.bank_select = 0;
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_after_ack = false;
        self.irq_pending = false;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16; // 128 KiB
    const CHR_BANKS: usize = 64; // 64 KiB

    fn make_mapper() -> Mapper56 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        Mapper56::new(prg, chr, NametableLayout::Horizontal)
    }

    #[test]
    fn mapper_56_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            56,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 56 must be registered");
    }

    // --- PRG banking via $E000/$F000 ---

    #[test]
    fn prg_bank_select_via_e000_and_f000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 1); // select bank reg for $8000
        mapper.write_prg(0xF000, 5); // set bank 5; prg_a17[0] = (5>>3)&1 = 0
        assert_eq!(mapper.prg_reg[0], 5 & 0x0F, "PRG reg0 must be 5");
        // Effective bank = (0 << 4) | 5 = 5
        assert_eq!(mapper.read_prg(0x8000), 5, "PRG $8000 must map to bank 5");
    }

    #[test]
    fn prg_a17_set_by_superimposed_f000_write() {
        let mut mapper = make_mapper();
        // PRG A17 register: addr $F000, bit 3 = A17 for bank 0
        // Writing value = 0x08 (bit3=1) to $F000 sets prg_a17[0]=1
        mapper.write_prg(0xE000, 1);
        mapper.write_prg(0xF000, 0x08); // prg_a17[0] = 1, prg_reg[0] = 8
        assert_eq!(mapper.prg_a17[0], 1, "PRG A17 bit must be 1");
        // Effective bank for $8000 = (1<<4)|(8&0xF) = 16+8 = 24
        assert_eq!(
            mapper.read_prg(0x8000),
            24 % PRG_BANKS as u8,
            "PRG bank includes A17"
        );
    }

    #[test]
    fn prg_e000_fixed_last_bank() {
        let mapper = make_mapper();
        // prg_a17[3] = 1 on power-on; fixed last = highest in block starting at A17=1
        // Block starts at bank 16, size 16 → bank 31; but PRG_BANKS=16, so bank 15
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000-$FFFF must be fixed to last bank"
        );
    }

    // --- CHR banking via $FC00-$FC07 ---

    #[test]
    fn chr_banks_via_fc00() {
        let mut mapper = make_mapper();
        for slot in 0..8u16 {
            mapper.write_prg(0xFC00 + slot, (slot * 7) as u8 & 0x3F);
        }
        for slot in 0..8u16 {
            let bank = (slot * 7) as usize & 0x3F;
            let expected = (bank % CHR_BANKS) as u8;
            assert_eq!(
                mapper.read_chr(slot * 1024),
                expected,
                "CHR slot {slot} wrong bank"
            );
        }
    }

    // --- Mirroring via $F800 ---

    #[test]
    fn mirroring_horizontal_via_f800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF800, 0x00); // 0 = horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_vertical_via_f800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF800, 0x01); // 1 = vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_by_default() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_latch_nibbles() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // nibble 0 = 1
        mapper.write_prg(0x9000, 0x02); // nibble 1 = 2 → bits 7:4 = 0x20
        mapper.write_prg(0xA000, 0x03); // nibble 2 = 3 → bits 11:8 = 0x300
        mapper.write_prg(0xB000, 0x04); // nibble 3 = 4 → bits 15:12 = 0x4000
        assert_eq!(mapper.irq_latch, 0x4321, "IRQ latch must be 0x4321");
    }

    #[test]
    fn irq_fires_after_n_cycles() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3); // latch = 3
        mapper.write_prg(0x9000, 0);
        mapper.write_prg(0xA000, 0);
        mapper.write_prg(0xB000, 0);
        // A=1, E=1: enable now AND keep enabled after acknowledge/reload
        mapper.write_prg(0xC000, 0x03);
        mapper.write_prg(0xD000, 0x00); // reload counter from latch (= 3)
        for _ in 0..2 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // 3rd → counter=0 → IRQ
        assert!(mapper.irq_pending(), "IRQ must fire after 3 cycles");
    }

    #[test]
    fn irq_acknowledge_clears_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 1); // latch = 1
        // A=1 (enable after ack), E=1 (enable now)
        mapper.write_prg(0xC000, 0x03);
        mapper.write_prg(0xD000, 0); // reload counter = 1; irq_enabled = A = true
        mapper.cpu_cycle(); // counter → 0 → IRQ
        assert!(mapper.irq_pending());
        mapper.write_prg(0xD000, 0); // acknowledge
        assert!(!mapper.irq_pending());
    }
}
