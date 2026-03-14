//! Mapper 264 – Cheapocabra (homebrew)
//!
//! Specifications:
//! - Primary: NesDev wiki (mapper page unavailable during this session; researched via Mesen2)
//! - Fallback: Mesen2 `Core/NES/Mappers/Homebrew/Cheapocabra.h`
//!
//! Register (`$5000–$5FFF` and `$7000–$7FFF` writes): `[-- N C PPPP]`
//! - `PPPP` (bits 3–0): 32 KB PRG bank at `$8000–$FFFF`
//! - `C`    (bit 4):    8 KB CHR-RAM bank (0 or 1) at `$0000–$1FFF`
//! - `N` / `NT` (bit 5): nametable set (0 = banks 0–7, 1 = banks 8–15)
//! - bits 7–6: unused
//!
//! PRG-ROM: up to 16 × 32 KB banks (512 KB).
//! CHR-RAM: 16 KB (two 8 KB banks).
//! Nametable RAM: 16 KB (16 × 1 KB banks).
//!   - NT slots 0–3 map to consecutive 1 KB banks starting at base (0 or 8).
//!
//! `$8000–$FFFF` writes target the on-board SST39SF040 flash chip.
//! Flash programming is **not emulated**; writes in that window are silently ignored.
//!
//! Known Limitations:
//! - SST39SF040 flash programming / self-flash is not emulated.
//! - Open-bus register read side-effect is not emulated.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::console::RamInitMode;

pub struct Mapper264 {
    base: BaseMapper,
    register: u8,
    nametable_ram: [u8; Self::NAMETABLE_RAM_SIZE],
}

impl Mapper264 {
    const NAMETABLE_BANK_SIZE: usize = 0x0400; // 1 KB
    const NAMETABLE_BANK_COUNT: usize = 16;
    const NAMETABLE_RAM_SIZE: usize = Self::NAMETABLE_BANK_COUNT * Self::NAMETABLE_BANK_SIZE;
    const CHR_RAM_SIZE: usize = 16 * 1024;

    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.set_chr_memory(ChrMemory::new_ram(Self::CHR_RAM_SIZE));
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);

        let mut mapper = Self {
            base,
            register: 0,
            nametable_ram: [0; Self::NAMETABLE_RAM_SIZE],
        };
        mapper.apply_register();
        mapper
    }

    fn apply_register(&mut self) {
        let prg_bank = (self.register & 0x0F) as i16;
        let chr_bank = ((self.register >> 4) & 0x01) as i16;
        self.base.select_prg_page(0, prg_bank);
        self.base.select_chr_page(0, chr_bank);
    }

    fn nametable_base(&self) -> usize {
        if (self.register & 0x20) != 0 { 8 } else { 0 }
    }

    fn nametable_index(&self, addr: u16) -> usize {
        let addr = (addr & 0x2FFF) as usize;
        let slot = (addr >> 10) & 0x03;
        let offset = addr & (Self::NAMETABLE_BANK_SIZE - 1);
        (self.nametable_base() + slot) * Self::NAMETABLE_BANK_SIZE + offset
    }
}

impl Mapper for Mapper264 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000..=0x5FFF | 0x7000..=0x7FFF => {
                self.register = value;
                self.apply_register();
            }
            // $8000-$FFFF: flash chip writes – not emulated
            _ => {}
        }
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr_masked = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&(addr_masked)) {
            return None;
        }
        let idx = self.nametable_index(addr);
        Some(self.nametable_ram[idx])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr_masked = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&(addr_masked)) {
            return false;
        }
        let idx = self.nametable_index(addr);
        self.nametable_ram[idx] = value;
        true
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(1 + Self::NAMETABLE_RAM_SIZE);
        snapshot.push(self.register);
        snapshot.extend_from_slice(&self.nametable_ram);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.register = data[0];
        if data.len() > 1 {
            let ram_len = (data.len() - 1).min(Self::NAMETABLE_RAM_SIZE);
            self.nametable_ram[..ram_len].copy_from_slice(&data[1..1 + ram_len]);
        }
        self.apply_register();
    }

    fn initialize_ram(&mut self, mode: RamInitMode) {
        self.base.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.nametable_ram, mode);
    }

    fn reset(&mut self) {
        self.register = 0;
        self.apply_register();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::create_mapper;
    use crate::cartridge::test_helpers::banked_data;

    // 3 banks (non-power-of-2 to catch modulo-wrap false passes)
    const PRG_BANKS_32K: usize = 3;

    fn make_mapper() -> Mapper264 {
        Mapper264::new(MapperContext::new_for_test(
            264,
            banked_data(32 * 1024, PRG_BANKS_32K),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_264_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            264,
            banked_data(32 * 1024, PRG_BANKS_32K),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 264 must be registered in factory");
    }

    #[test]
    fn prg_bank_0_is_mapped_at_power_on() {
        let mapper = make_mapper();
        // PRG bank 0 is filled with 0x00 by banked_data
        assert_eq!(mapper.read_prg(0x8000), 0x00);
        assert_eq!(mapper.read_prg(0xFFFF), 0x00);
    }

    #[test]
    fn prg_bank_switches_with_register_bits_0_to_3() {
        let mut mapper = make_mapper();

        // Select bank 2 (last bank available since PRG_BANKS_32K=3)
        mapper.write_prg(0x5000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xFFFF), 2);

        // Select bank 1
        mapper.write_prg(0x5000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 1);
    }

    #[test]
    fn register_write_at_7000_also_updates_prg_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn chr_bank_switches_with_register_bit_4() {
        let mut mapper = make_mapper();

        // Write to CHR bank 0, then switch to bank 1 and write different data
        mapper.write_prg(0x5000, 0x00); // CHR bank 0
        mapper.write_chr(0x0100, 0xAA);

        mapper.write_prg(0x5000, 0x10); // CHR bank 1
        mapper.write_chr(0x0100, 0xBB);

        // Switch back and verify bank 0 data unchanged
        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_chr(0x0100), 0xAA);

        // Switch to bank 1 and verify
        mapper.write_prg(0x5000, 0x10);
        assert_eq!(mapper.read_chr(0x0100), 0xBB);
    }

    #[test]
    fn chr_ram_is_16kb() {
        let mapper = make_mapper();
        assert_eq!(mapper.chr_ram_snapshot().len(), 16 * 1024);
    }

    #[test]
    fn nametable_slots_map_to_banks_0_to_3_when_bit5_is_clear() {
        let mut mapper = make_mapper();

        // bit5=0 → nametable base = 0, slots 0-3 use banks 0-3
        mapper.write_prg(0x5000, 0x00);

        // Write distinct values to each NT slot
        assert!(mapper.write_nametable(0x2000, 0x01)); // slot 0 → bank 0
        assert!(mapper.write_nametable(0x2400, 0x02)); // slot 1 → bank 1
        assert!(mapper.write_nametable(0x2800, 0x03)); // slot 2 → bank 2
        assert!(mapper.write_nametable(0x2C00, 0x04)); // slot 3 → bank 3

        assert_eq!(mapper.read_nametable(0x2000), Some(0x01));
        assert_eq!(mapper.read_nametable(0x2400), Some(0x02));
        assert_eq!(mapper.read_nametable(0x2800), Some(0x03));
        assert_eq!(mapper.read_nametable(0x2C00), Some(0x04));
    }

    #[test]
    fn nametable_slots_map_to_banks_8_to_11_when_bit5_is_set() {
        let mut mapper = make_mapper();

        // First, set bit5=0 and write to banks 0-3
        mapper.write_prg(0x5000, 0x00);
        assert!(mapper.write_nametable(0x2000, 0xAA)); // bank 0
        assert!(mapper.write_nametable(0x2400, 0xBB)); // bank 1

        // Now set bit5=1 → slots use banks 8-11
        mapper.write_prg(0x5000, 0x20);
        assert!(mapper.write_nametable(0x2000, 0x11)); // bank 8
        assert!(mapper.write_nametable(0x2400, 0x22)); // bank 9

        // Switch back to bit5=0 and verify banks 0-3 unchanged
        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));
        assert_eq!(mapper.read_nametable(0x2400), Some(0xBB));

        // Switch back to bit5=1 and verify banks 8-9
        mapper.write_prg(0x5000, 0x20);
        assert_eq!(mapper.read_nametable(0x2000), Some(0x11));
        assert_eq!(mapper.read_nametable(0x2400), Some(0x22));
    }

    #[test]
    fn nametable_mirror_region_3000_maps_same_as_2000() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x00);
        assert!(mapper.write_nametable(0x2000, 0x55));

        // $3000-$3EFF mirrors $2000-$2EFF
        assert_eq!(mapper.read_nametable(0x3000), Some(0x55));
    }

    #[test]
    fn registers_snapshot_restore_round_trips_state() {
        let mut mapper = make_mapper();

        // Set PRG=2, CHR=1, NT=1
        mapper.write_prg(0x5000, 0x32); // 0b0011_0010
        assert!(mapper.write_nametable(0x2000, 0x5A));
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.read_nametable(0x2000), Some(0x5A));
    }

    #[test]
    fn reset_restores_power_on_register_state() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x32); // PRG=2, CHR=1, NT=1
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.reset();

        // After reset, PRG bank 0 should be mapped
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn initialize_ram_zero_clears_nametable_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x00);
        assert!(mapper.write_nametable(0x2000, 0xDE));
        mapper.initialize_ram(RamInitMode::Zero);

        assert_eq!(mapper.read_nametable(0x2000), Some(0x00));
    }

    #[test]
    fn flash_writes_to_8000_are_silently_ignored() {
        let mut mapper = make_mapper();

        // PRG bank 0 reads return 0x00 (bank data)
        assert_eq!(mapper.read_prg(0x8000), 0x00);

        // Writing to $8000-$FFFF should not crash and PRG data unchanged
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xFFFF, 0xFF);

        // PRG bank still mapped to 0
        assert_eq!(mapper.read_prg(0x8000), 0x00);
    }
}
