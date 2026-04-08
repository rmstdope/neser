//! Mapper 121 - JY Company MMC3 variant

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper121 {
    mmc3: MMC3Mapper,
    ex_regs: [u8; Self::EX_REGS_SIZE],
}

impl Mapper121 {
    const LOOKUP: [u8; 4] = [0x83, 0x83, 0x42, 0x00];
    const EX_REGS_SIZE: usize = 8;
    // Minimum accepted MMC3 restore payload length in mmc3.rs:
    // bank_select (1) + regs[0..7] (8) + irq_latch (1) + irq_counter (1) +
    // flags (1) + mirroring (1) = 13 bytes. Used to distinguish legacy
    // MMC3-only snapshots from extended Mapper121 snapshots.
    const MMC3_MIN_SNAPSHOT_SIZE: usize = 13;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mut mapper = Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            ex_regs: [0; Self::EX_REGS_SIZE],
        };
        mapper.reset_ex_regs();
        mapper
    }

    fn reset_ex_regs(&mut self) {
        self.ex_regs = [0; Self::EX_REGS_SIZE];
        self.ex_regs[3] = 0x80;
    }

    fn reverse_low_6_bits(value: u8) -> u8 {
        ((value & 0x01) << 5)
            | ((value & 0x02) << 3)
            | ((value & 0x04) << 1)
            | ((value & 0x08) >> 1)
            | ((value & 0x10) >> 3)
            | ((value & 0x20) >> 5)
    }

    fn update_ex_regs(&mut self) {
        match self.ex_regs[5] & 0x3F {
            0x20 | 0x29 | 0x2B | 0x3C | 0x3F => {
                self.ex_regs[7] = 1;
                self.ex_regs[0] = self.ex_regs[6];
            }
            0x26 => {
                self.ex_regs[7] = 0;
                self.ex_regs[0] = self.ex_regs[6];
            }
            0x2C => {
                self.ex_regs[7] = 1;
                if self.ex_regs[6] != 0 {
                    self.ex_regs[0] = self.ex_regs[6];
                }
            }
            0x28 => {
                self.ex_regs[7] = 0;
                self.ex_regs[1] = self.ex_regs[6];
            }
            0x2A => {
                self.ex_regs[7] = 0;
                self.ex_regs[2] = self.ex_regs[6];
            }
            0x2F => {}
            _ => self.ex_regs[5] = 0,
        }
    }

    fn outer_prg_bit(&self) -> usize {
        ((self.ex_regs[3] & 0x80) >> 2) as usize
    }

    fn mapped_prg_bank(&self, addr: u16) -> usize {
        let outer = self.outer_prg_bit();

        if (self.ex_regs[5] & 0x3F) != 0 {
            match addr {
                0xA000..=0xBFFF => return self.ex_regs[2] as usize | outer,
                0xC000..=0xDFFF => return self.ex_regs[1] as usize | outer,
                0xE000..=0xFFFF => return self.ex_regs[0] as usize | outer,
                _ => {}
            }
        }

        (self.mmc3.mapped_prg_bank(addr) & 0x1F) | outer
    }
}

impl Mapper for Mapper121 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        121
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x5FFF => self.ex_regs[4],
            0x8000..=0xFFFF => {
                let bank = self.mapped_prg_bank(addr);
                let offset = (addr as usize) & 0x1FFF;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => self.mmc3.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000..=0x5FFF => {
                self.ex_regs[4] = Self::LOOKUP[(value & 0x03) as usize];
                if (addr & 0x5180) == 0x5180 {
                    self.ex_regs[3] = value;
                }
            }
            0x8000..=0x9FFF => {
                if (addr & 0x03) == 0x03 {
                    self.ex_regs[5] = value;
                    self.update_ex_regs();
                    self.mmc3.write_prg(0x8000, value);
                } else if (addr & 0x01) != 0 {
                    self.ex_regs[6] = Self::reverse_low_6_bits(value);
                    if self.ex_regs[7] == 0 {
                        self.update_ex_regs();
                    }
                    self.mmc3.write_prg(0x8001, value);
                } else {
                    self.mmc3.write_prg(0x8000, value);
                }
            }
            _ => self.mmc3.write_prg(addr, value),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x5000..=0x5FFF).contains(&addr) {
            self.read_prg(addr)
        } else {
            self.mmc3.read_prg_open_bus(addr, open_bus)
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = self.mmc3.registers_snapshot();
        snapshot.extend_from_slice(&self.ex_regs);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= Self::EX_REGS_SIZE {
            let split = data.len() - Self::EX_REGS_SIZE;
            if split >= Self::MMC3_MIN_SNAPSHOT_SIZE {
                self.mmc3.restore_registers(&data[..split]);
                self.ex_regs.copy_from_slice(&data[split..]);
                return;
            }
        }
        self.mmc3.restore_registers(data);
        self.reset_ex_regs();
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.reset_ex_regs();
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.mmc3.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper121;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const MAPPER_ID: u16 = 121;
    const PRG_BANKS_8K: usize = 48;
    const CHR_BANKS_1K: usize = 300;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            MAPPER_ID,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 121 must be created")
    }

    fn write_mmc3_reg(mapper: &mut dyn Mapper, reg: u8, value: u8) {
        mapper.write_prg(0x8000, reg & 0x07);
        mapper.write_prg(0x8001, value);
    }

    #[test]
    fn mapper_121_is_registered() {
        let _mapper = make_mapper();
    }

    #[test]
    fn mapper_121_initial_state_has_last_prg_bank_fixed_at_e000() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS_8K - 1) as u8);
    }

    #[test]
    fn mapper_121_prg_bank_switching_uses_protection_index_register() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8001, 0x15);
        mapper.write_prg(0x8003, 0x2A);

        assert_eq!(mapper.read_prg(0xA000), 0x2A);
    }

    #[test]
    fn mapper_121_chr_bank_switching_uses_mmc3_registers() {
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut *mapper, 0x02, 0x21);
        assert_eq!(mapper.read_chr(0x1000), 0x21);
    }

    #[test]
    fn mapper_121_mirroring_follows_a000_register() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mapper_121_irq_enable_acknowledge_and_trigger() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);

        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);

        assert!(mapper.irq_pending());

        mapper.write_prg(0xE000, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn mapper_121_preserves_mmc3_prg_ram_semantics_at_6000_7fff() {
        let mut mapper = make_mapper();

        assert_eq!(mapper.read_prg(0x5000), 0);

        mapper.write_prg(0x6000, 0x82);
        assert_eq!(mapper.read_prg(0x6000), 0x82);
        assert_eq!(mapper.read_prg(0x5000), 0);

        mapper.write_prg(0xA001, 0x00);
        mapper.write_prg(0x6000, 0x33);
        assert_eq!(mapper.read_prg(0x6000), 0);

        mapper.write_prg(0xA001, 0x80);
        assert_eq!(mapper.read_prg(0x6000), 0x82);

        mapper.write_prg(0xA001, 0xC0);
        mapper.write_prg(0x6000, 0x44);
        assert_eq!(mapper.read_prg(0x6000), 0x82);
    }

    #[test]
    fn mapper_121_restores_old_mmc3_only_snapshots_without_truncation() {
        let mapper = make_mapper();

        let snapshot = mapper.registers_snapshot();
        let legacy_mmc3_snapshot = snapshot[..snapshot.len() - Mapper121::EX_REGS_SIZE].to_vec();

        let mut restored = Mapper121::new(MapperContext::new_for_test(
            MAPPER_ID,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&legacy_mmc3_snapshot);

        assert_eq!(restored.read_prg(0x5000), 0);
        assert_eq!(restored.read_prg(0xE000), (PRG_BANKS_8K - 1) as u8);
        assert_eq!(restored.ex_regs[3], 0x80);
    }
}
