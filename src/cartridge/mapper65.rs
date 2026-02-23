//! Mapper 065 - Irem H3001
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_065>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 065 - Irem H3001
///
/// Hardware: Irem H3001 ASIC
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_065>
/// - PRG-ROM: Up to 256 KiB (32 × 8 KiB banks)
/// - CHR: Up to 256 KiB (256 × 1 KiB banks)
/// - Mirroring: Programmable (H/V/1-screen-A)
/// - IRQ: 16-bit CPU-cycle counter, fires on reach-0, does not wrap.
///
/// Register map:
/// - $8000: PRG bank 0 (8KB at $8000 or $C000, depending on $9000 mode)
/// - $A000: PRG bank 1 (8KB at $A000-$BFFF)
/// - $B000-$B007: CHR banks 0-7 (8 × 1KB banks for PPU $0000-$1FFF)
/// - $9000 [X... ....]: PRG mode
///   - 0: reg0 → $8000, fixed 0x3E → $C000
///   - 1: reg0 → $C000, fixed 0x3E → $8000
/// - $9001 [MM.. ....]: Mirroring: 00=Vert, 10=Horz, 01/11=1-screen A
/// - $9003 [E... ....]: IRQ Enable; also acknowledges pending IRQ
/// - $9004 [.... ....]: Reload IRQ counter; also acknowledges pending IRQ
/// - $9005 [IIII IIII]: High 8 bits of IRQ reload value
/// - $9006 [IIII IIII]: Low 8 bits of IRQ reload value
///
/// $E000-$FFFF: always fixed to last bank.
///
/// Power-on state: prg[0]=0x00, prg[1]=0x01, IRQ disabled.
pub struct Mapper65 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    prg_regs: [u8; 2], // reg0 ($8000), reg1 ($A000)
    chr_regs: [u8; 8], // $B000-$B007
    prg_mode: bool,    // $9000 bit7: false=mode0, true=mode1
    mirroring: NametableLayout,
    irq_enabled: bool,
    irq_pending: bool,
    irq_counter: u16,
    irq_reload: u16,
}

impl Mapper65 {
    const MAPPER_NUMBER: u8 = 65;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        let _ = mirroring; // mirroring is programmable; header value ignored
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            prg_regs: [0x00, 0x01], // power-on state
            chr_regs: [0; 8],
            prg_mode: false,
            mirroring: NametableLayout::Vertical,
            irq_enabled: false,
            irq_pending: false,
            irq_counter: 0,
            irq_reload: 0,
        }
    }

    fn num_prg_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn prg_bank_read(&self, bank: usize, offset: usize) -> u8 {
        let count = self.num_prg_banks();
        if count == 0 {
            return 0;
        }
        let b = bank % count;
        self.prg_rom
            .get(b * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn resolve_prg_addr(&self, addr: u16) -> u8 {
        let count = self.num_prg_banks();
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        match addr {
            0x8000..=0x9FFF => {
                if self.prg_mode {
                    // mode1: $8000 = fixed 0x3E (second to last)
                    let bank = count.saturating_sub(2);
                    self.prg_bank_read(bank, offset)
                } else {
                    // mode0: $8000 = reg0
                    self.prg_bank_read(self.prg_regs[0] as usize, offset)
                }
            }
            0xA000..=0xBFFF => self.prg_bank_read(self.prg_regs[1] as usize, offset),
            0xC000..=0xDFFF => {
                if self.prg_mode {
                    // mode1: $C000 = reg0
                    self.prg_bank_read(self.prg_regs[0] as usize, offset)
                } else {
                    // mode0: $C000 = fixed 0x3E (second to last)
                    let bank = count.saturating_sub(2);
                    self.prg_bank_read(bank, offset)
                }
            }
            0xE000..=0xFFFF => {
                // Always last bank
                let bank = count.saturating_sub(1);
                self.prg_bank_read(bank, offset)
            }
            _ => 0,
        }
    }

    fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
    }
}

impl Mapper for Mapper65 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.resolve_prg_addr(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => {
                self.prg_regs[0] = value;
            }
            0xA000 => {
                self.prg_regs[1] = value;
            }
            0xB000..=0xB007 => {
                let reg = (addr & 0x0007) as usize;
                self.chr_regs[reg] = value;
            }
            0x9000 => {
                self.prg_mode = (value & 0x80) != 0;
            }
            0x9001 => {
                self.mirroring = match (value >> 6) & 0x03 {
                    0b00 => NametableLayout::Vertical,
                    0b10 => NametableLayout::Horizontal,
                    _ => NametableLayout::SingleScreenLower, // 1-screen A
                };
            }
            0x9003 => {
                self.acknowledge_irq();
                self.irq_enabled = (value & 0x80) != 0;
            }
            0x9004 => {
                self.acknowledge_irq();
                self.irq_counter = self.irq_reload;
            }
            0x9005 => {
                self.irq_reload = (self.irq_reload & 0x00FF) | ((value as u16) << 8);
            }
            0x9006 => {
                self.irq_reload = (self.irq_reload & 0xFF00) | (value as u16);
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
        0
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

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.mirroring {
            NametableLayout::Vertical => 0u8,
            NametableLayout::Horizontal => 1,
            _ => 2,
        };
        let irq_flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        let mut v = vec![
            self.prg_regs[0],
            self.prg_regs[1],
            self.prg_mode as u8,
            mirror_byte,
            irq_flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
            (self.irq_reload & 0xFF) as u8,
            (self.irq_reload >> 8) as u8,
        ];
        v.extend_from_slice(&self.chr_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 17 {
            return;
        }
        self.prg_regs[0] = data[0];
        self.prg_regs[1] = data[1];
        self.prg_mode = data[2] != 0;
        self.mirroring = match data[3] {
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            _ => NametableLayout::Vertical,
        };
        self.irq_enabled = (data[4] & 1) != 0;
        self.irq_pending = (data[4] & 2) != 0;
        self.irq_counter = (data[5] as u16) | ((data[6] as u16) << 8);
        self.irq_reload = (data[7] as u16) | ((data[8] as u16) << 8);
        self.chr_regs.copy_from_slice(&data[9..17]);
    }

    fn reset(&mut self) {
        self.prg_regs = [0x00, 0x01];
        self.chr_regs = [0; 8];
        self.prg_mode = false;
        self.mirroring = NametableLayout::Vertical;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_counter = 0;
        self.irq_reload = 0;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
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

    const PRG_BANKS: usize = 32;
    const CHR_BANKS: usize = 64;

    fn make_mapper() -> Mapper65 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        Mapper65::new(prg, chr, NametableLayout::Horizontal)
    }

    #[test]
    fn mapper_65_is_registered() {
        let result = create_mapper(MapperContext::new(
            65,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 65 must be registered");
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg0_is_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG reg0 must start at 0");
    }

    #[test]
    fn power_on_prg1_is_1() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xA000), 1, "PRG reg1 must start at 1");
    }

    #[test]
    fn power_on_c000_is_fixed_second_to_last() {
        let mapper = make_mapper();
        // mode0: $C000 = bank 30 (second to last in 32-bank ROM)
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 must be fixed to second-to-last bank"
        );
    }

    #[test]
    fn power_on_e000_is_fixed_last() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must always be last bank"
        );
    }

    // --- PRG mode ---

    #[test]
    fn prg_mode1_swaps_8000_and_c000_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5); // reg0 = 5
        mapper.write_prg(0x9000, 0x80); // mode1
        assert_eq!(
            mapper.read_prg(0x8000),
            (PRG_BANKS - 2) as u8,
            "mode1: $8000 = second-to-last"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "mode1: $C000 = reg0");
    }

    // --- CHR banking ---

    #[test]
    fn chr_bank_registers() {
        let mut mapper = make_mapper();
        for slot in 0..8u16 {
            mapper.write_prg(0xB000 + slot, (slot * 5) as u8 & 0x3F);
        }
        for slot in 0..8u16 {
            let expected = ((slot * 5) as usize * 1024) % (CHR_BANKS * 1024) / 1024;
            assert_eq!(
                mapper.read_chr(slot * 1024),
                expected as u8,
                "CHR slot {slot} wrong bank"
            );
        }
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9001, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_by_default() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_fires_after_reload_value_cycles() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00); // high = 0
        mapper.write_prg(0x9006, 5); // low = 5 → reload = 5
        mapper.write_prg(0x9004, 0); // load counter
        mapper.write_prg(0x9003, 0x80); // enable IRQ
        for _ in 0..4 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // 5th cycle → counter reaches 0 → IRQ
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after reload value cycles"
        );
    }

    #[test]
    fn irq_acknowledge_via_9003() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00);
        mapper.write_prg(0x9006, 1);
        mapper.write_prg(0x9004, 0);
        mapper.write_prg(0x9003, 0x80);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
        mapper.write_prg(0x9003, 0x00); // acknowledge (and disable)
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after write to $9003"
        );
    }

    #[test]
    fn irq_counter_stops_at_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00);
        mapper.write_prg(0x9006, 2);
        mapper.write_prg(0x9004, 0);
        mapper.write_prg(0x9003, 0x80);
        for _ in 0..10 {
            mapper.cpu_cycle();
        }
        // IRQ fires once; counter stays at 0 (no wrap)
        assert!(mapper.irq_pending(), "IRQ must remain pending");
        assert_eq!(mapper.irq_counter, 0, "Counter must stop at 0");
    }

    // --- Snapshot ---

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 7);
        mapper.write_prg(0xA000, 4);
        mapper.write_prg(0x9001, 0x80); // horizontal
        mapper.write_prg(0x9005, 0x01);
        mapper.write_prg(0x9006, 0x23);
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_prg(0xA000), mapper.read_prg(0xA000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
        assert_eq!(r.irq_reload, mapper.irq_reload);
    }
}
