//! Mapper 207 - Taito X1-005 Alternate (Fudou Myouou Den)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_207>
//! - Based on: <https://www.nesdev.org/wiki/INES_Mapper_080> (Taito X1-005)
//!
//! This mapper is identical to Mapper 80 (Taito X1-005) except:
//! - CHR registers `$7EF0` and `$7EF1` each contain a mirroring bit (bit 7):
//!   - `$7EF0` bit 7 controls which VRAM bank nametables 0 and 1 use.
//!   - `$7EF1` bit 7 controls which VRAM bank nametables 2 and 3 use.
//! - Registers `$7EF6–$7EF7` are present but ignored (no H/V mirroring control).
//! - The mapper manages its own 2 KB CIRAM to implement per-pair nametable routing.
//!
//! Register map:
//! - `$7EF0`: `[MCCC CCCC]` — M=nametable select for NT0/NT1, C=CHR bank bits 0-6 for $0000–$07FF
//! - `$7EF1`: `[MCCC CCCC]` — M=nametable select for NT2/NT3, C=CHR bank bits 0-6 for $0800–$0FFF
//! - `$7EF2–$7EF5`: CHR banks for `$1000–$1FFF` (1 KB each)
//! - `$7EF6–$7EF7`: ignored
//! - `$7EF8–$7EF9`: RAM permission (`$A3` enables `$7F00–$7FFF`)
//! - `$7EFA–$7EFB`: PRG bank at `$8000–$9FFF`
//! - `$7EFC–$7EFD`: PRG bank at `$A000–$BFFF`
//! - `$7EFE–$7EFF`: PRG bank at `$C000–$DFFF` (`$E000–$FFFF` fixed to last)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::trace_mapper;

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;
const CHR_REG_START: u16 = 0x7EF0;
const CHR_REG_END: u16 = 0x7EF5;
const PRG0_REG_START: u16 = 0x7EFA;
const PRG0_REG_END: u16 = 0x7EFB;
const PRG1_REG_START: u16 = 0x7EFC;
const PRG1_REG_END: u16 = 0x7EFD;
const PRG2_REG_START: u16 = 0x7EFE;
const PRG2_REG_END: u16 = 0x7EFF;
const PRG_REG_COUNT: usize = 3;
const CHR_REG_COUNT: usize = 6;
const RAM_START: u16 = 0x7F00;
const RAM_END: u16 = 0x7FFF;
const RAM_SIZE: usize = 0x100;
const PRG_RAM_START: u16 = 0x6000;
const PRG_RAM_END: u16 = 0x7EEF;
const PRG_RAM_SIZE: usize = (PRG_RAM_END - PRG_RAM_START + 1) as usize;
const RAM_ENABLE_VALUE: u8 = 0xA3;
const CIRAM_SIZE: usize = 2 * 1024;

/// Snapshot size: 3 PRG banks + 6 CHR banks + 1 NT select byte + 1 RAM permission + 2KB CIRAM
const SNAPSHOT_REGS_SIZE: usize = PRG_REG_COUNT + CHR_REG_COUNT + 1 + 1;
const SNAPSHOT_SIZE: usize = SNAPSHOT_REGS_SIZE + CIRAM_SIZE;

const DEFAULT_PRG_BANKS: [u8; PRG_REG_COUNT] = [0, 1, 2];
const DEFAULT_CHR_BANKS: [u8; CHR_REG_COUNT] = [0, 2, 4, 5, 6, 7];

/// Mapper 207 - Taito X1-005 with per-pair nametable select (alternate mirroring)
pub struct TaitoX1005_207Mapper {
    base: BaseMapper,
    prg_banks: [u8; PRG_REG_COUNT],
    /// CHR bank values (full byte as written, bit 7 is also the nametable select bit for regs 0/1)
    chr_banks: [u8; CHR_REG_COUNT],
    /// Nametable select: bit 0 = NT0/NT1 bank (from $7EF0 bit 7), bit 1 = NT2/NT3 bank (from $7EF1 bit 7)
    nt_select: u8,
    ram_permission: u8,
    ram: [u8; RAM_SIZE],
    prg_ram: [u8; PRG_RAM_SIZE],
    ciram: [u8; CIRAM_SIZE],
    unhandled_write_trace_budget: u16,
}

impl TaitoX1005_207Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_banks: DEFAULT_PRG_BANKS,
            chr_banks: DEFAULT_CHR_BANKS,
            nt_select: 0,
            ram_permission: 0,
            ram: [0; RAM_SIZE],
            prg_ram: [0; PRG_RAM_SIZE],
            ciram: [0; CIRAM_SIZE],
            unhandled_write_trace_budget: 128,
        };
        mapper.apply_banks();
        trace_mapper!(1; "[207] initialized (PRG=8KB pages, CHR=1KB pages, alternate mirroring)");
        mapper
    }

    fn decode_control_register(addr: u16) -> Option<u16> {
        if (0x7EF0..=0x7EFF).contains(&addr) {
            return Some(addr);
        }
        // Mirror at $7E70–$7E7F (bit 7 of address byte ignored, same as mapper 80)
        if (0x7E70..=0x7E7F).contains(&addr) {
            return Some(addr | 0x0080);
        }
        None
    }

    fn apply_banks(&mut self) {
        for (slot, &bank) in self.prg_banks.iter().enumerate() {
            self.base.select_prg_page(slot, bank as i16);
        }
        self.base.select_prg_page(3, -1);

        // CHR banks 0/1 use bits 0-6; bit 7 is the nametable select bit
        let chr0 = (self.chr_banks[0] & 0x7F) as i16;
        self.base.select_chr_page(0, chr0);
        self.base.select_chr_page(1, chr0 + 1);

        let chr1 = (self.chr_banks[1] & 0x7F) as i16;
        self.base.select_chr_page(2, chr1);
        self.base.select_chr_page(3, chr1 + 1);

        self.base.select_chr_page(4, self.chr_banks[2] as i16);
        self.base.select_chr_page(5, self.chr_banks[3] as i16);
        self.base.select_chr_page(6, self.chr_banks[4] as i16);
        self.base.select_chr_page(7, self.chr_banks[5] as i16);
    }

    fn ram_enabled(&self) -> bool {
        self.ram_permission == RAM_ENABLE_VALUE
    }

    fn ram_index(addr: u16) -> usize {
        (addr & 0x00FF) as usize
    }

    fn prg_ram_index(addr: u16) -> usize {
        (addr - PRG_RAM_START) as usize
    }

    /// Map nametable address to CIRAM offset using per-pair nametable select.
    ///
    /// Layout:
    /// - NT0 ($2000) and NT1 ($2400) use VRAM bank selected by `nt_select` bit 0
    /// - NT2 ($2800) and NT3 ($2C00) use VRAM bank selected by `nt_select` bit 1
    fn ciram_offset(&self, addr: u16) -> usize {
        let nt = ((addr >> 10) & 3) as usize; // 0–3
        let offset = (addr & 0x3FF) as usize;
        let bank = if nt < 2 {
            (self.nt_select & 0x01) as usize
        } else {
            ((self.nt_select >> 1) & 0x01) as usize
        };
        bank * 0x400 + offset
    }
}

impl Mapper for TaitoX1005_207Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Shadow writes at $7E70–$7E7F into PRG-RAM (same as mapper 80)
        if (0x7E70..=0x7E7F).contains(&addr) {
            self.prg_ram[Self::prg_ram_index(addr)] = value;
            trace_mapper!(2; "[207] PRG-RAM shadow write ${:04X}=${:02X}", addr, value);
        }

        if let Some(reg_addr) = Self::decode_control_register(addr) {
            match reg_addr {
                CHR_REG_START..=CHR_REG_END => {
                    let idx = (reg_addr - CHR_REG_START) as usize;
                    self.chr_banks[idx] = value;
                    // Registers $7EF0 and $7EF1 carry the nametable select bit in bit 7
                    if reg_addr == 0x7EF0 {
                        self.nt_select = (self.nt_select & !0x01) | (value >> 7);
                        trace_mapper!(1; "[207] CHR0 reg=${:02X} nt_select={}", value, self.nt_select);
                    } else if reg_addr == 0x7EF1 {
                        self.nt_select = (self.nt_select & !0x02) | ((value >> 6) & 0x02);
                        trace_mapper!(1; "[207] CHR1 reg=${:02X} nt_select={}", value, self.nt_select);
                    } else {
                        trace_mapper!(1; "[207] CHR reg ${:04X}=${:02X}", reg_addr, value);
                    }
                    self.apply_banks();
                }
                0x7EF6..=0x7EF7 => {
                    // Ignored in mapper 207
                    trace_mapper!(2; "[207] mirroring reg ${:04X}=${:02X} (ignored)", reg_addr, value);
                }
                0x7EF8..=0x7EF9 => {
                    self.ram_permission = value;
                    trace_mapper!(1; "[207] RAM permission=${:02X} (enabled={})", value, self.ram_enabled());
                }
                PRG0_REG_START..=PRG0_REG_END => {
                    self.prg_banks[0] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[207] PRG0=${:02X}", value);
                }
                PRG1_REG_START..=PRG1_REG_END => {
                    self.prg_banks[1] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[207] PRG1=${:02X}", value);
                }
                PRG2_REG_START..=PRG2_REG_END => {
                    self.prg_banks[2] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[207] PRG2=${:02X}", value);
                }
                _ => {}
            }
            return;
        }

        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    let idx = Self::ram_index(addr);
                    self.ram[idx] = value;
                    self.ram[idx ^ 0x80] = value;
                    trace_mapper!(2; "[207] RAM write ${:04X}=${:02X} (mirrored)", addr, value);
                }
            }
            PRG_RAM_START..=PRG_RAM_END => {
                self.prg_ram[Self::prg_ram_index(addr)] = value;
                trace_mapper!(2; "[207] PRG-RAM write ${:04X}=${:02X}", addr, value);
            }
            _ => {
                if (0x4020..=0xFFFF).contains(&addr) && self.unhandled_write_trace_budget > 0 {
                    trace_mapper!(1; "[207] unhandled write ${:04X}=${:02X}", addr, value);
                    self.unhandled_write_trace_budget -= 1;
                }
            }
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    self.ram[Self::ram_index(addr)]
                } else {
                    0
                }
            }
            PRG_RAM_START..=PRG_RAM_END => self.prg_ram[Self::prg_ram_index(addr)],
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    self.ram[Self::ram_index(addr)]
                } else {
                    open_bus
                }
            }
            PRG_RAM_START..=PRG_RAM_END => self.prg_ram[Self::prg_ram_index(addr)],
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let offset = self.ciram_offset(addr);
        Some(self.ciram[offset])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let offset = self.ciram_offset(addr);
        self.ciram[offset] = value;
        true
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(SNAPSHOT_SIZE);
        snapshot.extend_from_slice(&self.prg_banks);
        snapshot.extend_from_slice(&self.chr_banks);
        snapshot.push(self.nt_select);
        snapshot.push(self.ram_permission);
        snapshot.extend_from_slice(&self.ciram);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < SNAPSHOT_REGS_SIZE {
            return;
        }
        self.prg_banks.copy_from_slice(&data[0..PRG_REG_COUNT]);
        self.chr_banks
            .copy_from_slice(&data[PRG_REG_COUNT..(PRG_REG_COUNT + CHR_REG_COUNT)]);
        self.nt_select = data[PRG_REG_COUNT + CHR_REG_COUNT];
        self.ram_permission = data[PRG_REG_COUNT + CHR_REG_COUNT + 1];
        // Restore CIRAM if present (backwards-compatible with legacy snapshots)
        if data.len() >= SNAPSHOT_SIZE {
            self.ciram
                .copy_from_slice(&data[SNAPSHOT_REGS_SIZE..SNAPSHOT_REGS_SIZE + CIRAM_SIZE]);
        }
        self.apply_banks();
    }

    fn reset(&mut self) {
        self.prg_banks = DEFAULT_PRG_BANKS;
        self.chr_banks = DEFAULT_CHR_BANKS;
        self.nt_select = 0;
        self.ram_permission = 0;
        self.unhandled_write_trace_budget = 128;
        self.apply_banks();
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.ram.fill(0);
        self.base.initialize_ram(mode);
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len() + self.ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.prg_ram.len() + self.ram.len());
        data.extend_from_slice(&self.prg_ram);
        data.extend_from_slice(&self.ram);
        data
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let prg_len = data.len().min(self.prg_ram.len());
        self.prg_ram[..prg_len].copy_from_slice(&data[..prg_len]);

        if data.len() > self.prg_ram.len() {
            let ram_data = &data[self.prg_ram.len()..];
            let len = ram_data.len().min(self.ram.len());
            self.ram[..len].copy_from_slice(&ram_data[..len]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CHR_BANK_SIZE, PRG_BANK_SIZE, RAM_SIZE, RAM_START};
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 13;
    const CHR_BANKS: usize = 11;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            207,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 207 must be creatable via factory")
    }

    // -------------------------------------------------------------------------
    // Factory
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_207_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            207,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 207 must be creatable via factory");
    }

    #[test]
    fn mapper_207_is_in_supported_mappers_list() {
        use crate::nes::cartridge::mapper::supported_mappers;
        assert!(
            supported_mappers().contains(&207),
            "Mapper 207 must be in the SUPPORTED_MAPPERS list"
        );
    }

    // -------------------------------------------------------------------------
    // PRG banking — same as mapper 80
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_prg_layout_is_contiguous_with_last_bank_fixed() {
        let mapper = make_mapper();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn registers_7efa_to_7eff_select_prg_windows_with_last_bank_fixed() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EFA, 2);
        mapper.write_prg(0x7EFC, 4);
        mapper.write_prg(0x7EFE, 6);

        assert_eq!(mapper.read_prg(0x8000), (2 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xA000), (4 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xC000), (6 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    // -------------------------------------------------------------------------
    // CHR banking
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_chr_layout_is_contiguous_8kb() {
        let mut mapper = make_mapper();

        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0400), 1);
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0C00), 3);
        assert_eq!(mapper.read_chr(0x1000), 4);
        assert_eq!(mapper.read_chr(0x1400), 5);
        assert_eq!(mapper.read_chr(0x1800), 6);
        assert_eq!(mapper.read_chr(0x1C00), 7);
    }

    #[test]
    fn registers_7ef0_to_7ef5_select_chr_windows_with_two_2k_pairs() {
        let mut mapper = make_mapper();

        // Only bits 0-6 used for CHR bank in registers $7EF0/$7EF1 (bit 7 = mirroring)
        mapper.write_prg(0x7EF0, 3 & 0x7F); // bank 3, M=0
        mapper.write_prg(0x7EF1, 5 & 0x7F); // bank 5, M=0
        mapper.write_prg(0x7EF2, 7);
        mapper.write_prg(0x7EF3, 8);
        mapper.write_prg(0x7EF4, 9);
        mapper.write_prg(0x7EF5, 10);

        assert_eq!(mapper.read_chr(0x0000), (3 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0400), (4 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0800), (5 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0C00), (6 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1000), (7 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1400), (8 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1800), (9 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1C00), (10 % CHR_BANKS) as u8);
    }

    #[test]
    fn chr_banks_0_1_use_only_bits_0_to_6_for_page_number() {
        let mut mapper = make_mapper();

        // Write value 0x83 (M=1, bank=3) to $7EF0 — CHR bank should be 3 (not 0x83)
        mapper.write_prg(0x7EF0, 0x83);
        assert_eq!(
            mapper.read_chr(0x0000),
            (3 % CHR_BANKS) as u8,
            "bit 7 must not affect CHR bank selection"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            (4 % CHR_BANKS) as u8,
            "second 1KB of the pair must be bank+1"
        );
    }

    // -------------------------------------------------------------------------
    // Nametable / mirroring — the key difference from mapper 80
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_nametable_all_map_to_bank_a() {
        let mut mapper = make_mapper();

        // Write distinct data to both VRAM banks via the nametable override
        mapper.write_nametable(0x2000, 0xAA); // NT0 → bank A offset 0
        mapper.write_nametable(0x2400, 0xBB); // NT1 → bank A offset 0 (should match NT0)
        mapper.write_nametable(0x2800, 0xCC); // NT2 → bank A offset 0 (nt_select default=0)
        mapper.write_nametable(0x2C00, 0xDD); // NT3 → bank A offset 0 (should match NT2)

        // At power-on, nt_select=0 → all nametables use bank A
        // NT0/NT1 share bank A, NT2/NT3 share bank A
        // So writing to NT0 and reading NT1 should give the same value
        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0xDD),
            "NT0 should read from bank A (all share at power-on)"
        );
        assert_eq!(
            mapper.read_nametable(0x2400),
            Some(0xDD),
            "NT1 should read from bank A"
        );
        assert_eq!(
            mapper.read_nametable(0x2800),
            Some(0xDD),
            "NT2 should read from bank A"
        );
        assert_eq!(
            mapper.read_nametable(0x2C00),
            Some(0xDD),
            "NT3 should read from bank A"
        );
    }

    #[test]
    fn bit7_of_7ef0_selects_nt0_nt1_bank() {
        let mut mapper = make_mapper();

        // Set up distinct content in CIRAM banks A and B via writes before changing nt_select
        // Bank A offset 0 = 0xAA, Bank B offset 0 = 0xBB
        mapper.write_prg(0x7EF0, 0x00); // M=0 → NT0/NT1 use bank A
        mapper.write_nametable(0x2000, 0xAA); // write to bank A offset 0
        mapper.write_prg(0x7EF0, 0x80); // M=1 → NT0/NT1 use bank B
        mapper.write_nametable(0x2000, 0xBB); // write to bank B offset 0

        // Read NT0 with M=1: should read bank B
        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0xBB),
            "NT0 must read bank B when $7EF0 bit7=1"
        );
        assert_eq!(
            mapper.read_nametable(0x2400),
            Some(0xBB),
            "NT1 must also use bank B when $7EF0 bit7=1"
        );

        // Switch back to bank A
        mapper.write_prg(0x7EF0, 0x00); // M=0 → NT0/NT1 back to bank A
        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0xAA),
            "NT0 must read bank A when $7EF0 bit7=0"
        );
    }

    #[test]
    fn bit7_of_7ef1_selects_nt2_nt3_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EF1, 0x00); // M=0 → NT2/NT3 use bank A
        mapper.write_nametable(0x2800, 0xAA); // write to bank A offset 0
        mapper.write_prg(0x7EF1, 0x80); // M=1 → NT2/NT3 use bank B
        mapper.write_nametable(0x2800, 0xBB); // write to bank B offset 0

        assert_eq!(
            mapper.read_nametable(0x2800),
            Some(0xBB),
            "NT2 must read bank B when $7EF1 bit7=1"
        );
        assert_eq!(
            mapper.read_nametable(0x2C00),
            Some(0xBB),
            "NT3 must also use bank B when $7EF1 bit7=1"
        );

        mapper.write_prg(0x7EF1, 0x00);
        assert_eq!(
            mapper.read_nametable(0x2800),
            Some(0xAA),
            "NT2 must read bank A when $7EF1 bit7=0"
        );
    }

    #[test]
    fn nt_pairs_are_independent_reverse_horizontal() {
        let mut mapper = make_mapper();

        // Set M0=1 (NT0/NT1 → bank B), M1=0 (NT2/NT3 → bank A)
        mapper.write_prg(0x7EF0, 0x80); // NT0/NT1 → bank B
        mapper.write_prg(0x7EF1, 0x00); // NT2/NT3 → bank A

        mapper.write_nametable(0x2000, 0xBB); // bank B offset 0
        mapper.write_nametable(0x2800, 0xAA); // bank A offset 0

        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0xBB),
            "NT0 should read bank B (M0=1)"
        );
        assert_eq!(
            mapper.read_nametable(0x2400),
            Some(0xBB),
            "NT1 should read bank B (M0=1)"
        );
        assert_eq!(
            mapper.read_nametable(0x2800),
            Some(0xAA),
            "NT2 should read bank A (M1=0)"
        );
        assert_eq!(
            mapper.read_nametable(0x2C00),
            Some(0xAA),
            "NT3 should read bank A (M1=0)"
        );
    }

    #[test]
    fn nt_pairs_horizontal_mirroring_via_m_bits() {
        let mut mapper = make_mapper();

        // M0=0 (NT0/NT1 → bank A), M1=1 (NT2/NT3 → bank B) = standard horizontal
        mapper.write_prg(0x7EF0, 0x00);
        mapper.write_prg(0x7EF1, 0x80);

        mapper.write_nametable(0x2000, 0xAA); // bank A offset 0
        mapper.write_nametable(0x2800, 0xBB); // bank B offset 0

        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));
        assert_eq!(mapper.read_nametable(0x2400), Some(0xAA));
        assert_eq!(mapper.read_nametable(0x2800), Some(0xBB));
        assert_eq!(mapper.read_nametable(0x2C00), Some(0xBB));
    }

    #[test]
    fn registers_7ef6_and_7ef7_are_ignored() {
        let mut mapper = make_mapper();

        // Set up known state: M0=0, M1=0 → all NT use bank A
        mapper.write_prg(0x7EF0, 0x00);
        mapper.write_prg(0x7EF1, 0x00);
        mapper.write_nametable(0x2000, 0xAA);

        // Write to $7EF6/$7EF7 — should have no effect on mirroring
        mapper.write_prg(0x7EF6, 0x01); // would be V in mapper 80; must be ignored here
        mapper.write_prg(0x7EF7, 0xFF);

        // NT0 should still read from bank A
        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0xAA),
            "$7EF6/$7EF7 writes must be ignored by mapper 207"
        );
    }

    // -------------------------------------------------------------------------
    // RAM / WRAM — same as mapper 80
    // -------------------------------------------------------------------------

    #[test]
    fn ram_7f00_is_gated_by_a3_and_mirrored_every_0x80_bytes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7F00, 0x5A);
        assert_eq!(
            mapper.read_prg(0x7F00),
            0,
            "RAM should be inaccessible before permission is set"
        );

        mapper.write_prg(0x7EF8, 0xA3);
        mapper.write_prg(0x7F00, 0x5A);

        assert_eq!(mapper.read_prg(0x7F00), 0x5A);
        assert_eq!(
            mapper.read_prg(0x7F80),
            0x5A,
            "upper half should mirror lower"
        );

        mapper.write_prg(0x7F80, 0x33);
        assert_eq!(mapper.read_prg(0x7F00), 0x33);
        assert_eq!(mapper.read_prg(0x7F80), 0x33);

        mapper.write_prg(0x7EF9, 0x00);
        mapper.write_prg(0x7F00, 0x99);
        assert_eq!(
            mapper.read_prg(0x7F00),
            0,
            "RAM should be inaccessible after clearing permission"
        );
    }

    #[test]
    fn writes_to_7000_range_use_standard_prg_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7000, 0x4C);
        mapper.write_prg(0x7045, 0x8D);

        assert_eq!(mapper.read_prg(0x7000), 0x4C);
        assert_eq!(mapper.read_prg(0x7045), 0x8D);
    }

    // -------------------------------------------------------------------------
    // Snapshot / restore
    // -------------------------------------------------------------------------

    #[test]
    fn snapshot_restore_preserves_nt_select_and_banks() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EF0, 0x83); // bank=3, M=1 → NT0/NT1 bank B
        mapper.write_prg(0x7EF1, 0x05); // bank=5, M=0 → NT2/NT3 bank A
        mapper.write_prg(0x7EFA, 5);
        mapper.write_prg(0x7EF8, 0xA3);

        let snapshot = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snapshot);

        assert_eq!(mapper2.read_prg(0x8000), (5 % PRG_BANKS) as u8);
        assert_eq!(
            mapper2.read_chr(0x0000),
            (3 % CHR_BANKS) as u8,
            "CHR bank 0 after restore"
        );
        // Check NT select preserved: NT0/NT1 should use bank B
        mapper2.write_nametable(0x2000, 0xBB);
        assert_eq!(
            mapper2.read_nametable(0x2000),
            Some(0xBB),
            "NT0 must use bank B after restore"
        );
    }

    #[test]
    fn snapshot_restore_preserves_ciram_contents() {
        let mut mapper = make_mapper();

        // Put NT0/NT1 on bank A, NT2/NT3 on bank B
        mapper.write_prg(0x7EF0, 0x00); // M=0 → NT0/NT1 bank A
        mapper.write_prg(0x7EF1, 0x80); // M=1 → NT2/NT3 bank B

        // Write distinct data to each CIRAM bank
        mapper.write_nametable(0x2000, 0xAA); // bank A offset 0
        mapper.write_nametable(0x2001, 0xAB); // bank A offset 1
        mapper.write_nametable(0x2800, 0xCC); // bank B offset 0
        mapper.write_nametable(0x2801, 0xCD); // bank B offset 1

        let reg_snapshot = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&reg_snapshot);

        // CIRAM contents must survive the round-trip
        assert_eq!(
            mapper2.read_nametable(0x2000),
            Some(0xAA),
            "bank A offset 0 must be preserved after restore"
        );
        assert_eq!(
            mapper2.read_nametable(0x2001),
            Some(0xAB),
            "bank A offset 1 must be preserved after restore"
        );
        assert_eq!(
            mapper2.read_nametable(0x2800),
            Some(0xCC),
            "bank B offset 0 must be preserved after restore"
        );
        assert_eq!(
            mapper2.read_nametable(0x2801),
            Some(0xCD),
            "bank B offset 1 must be preserved after restore"
        );
    }

    // -------------------------------------------------------------------------
    // initialize_ram
    // -------------------------------------------------------------------------

    #[test]
    fn initialize_ram_zeroes_mapper_owned_ram_buffers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7004, 0x9D);
        mapper.write_prg(0x7024, 0xEE);
        mapper.initialize_ram(crate::nes::console::RamInitMode::Random);
        mapper.write_prg(0x7EF8, 0xA3);

        assert_eq!(mapper.read_prg(0x7004), 0x9D);
        assert_eq!(mapper.read_prg(0x7024), 0xEE);

        for offset in 0..RAM_SIZE {
            assert_eq!(mapper.read_prg(RAM_START + offset as u16), 0);
        }
    }
}
