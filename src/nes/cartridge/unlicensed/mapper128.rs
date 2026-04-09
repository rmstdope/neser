//! Mapper 128 - T-262 multicart (BMC-T-262)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_128>
//! - Detail: <https://www.nesdev.org/wiki/NES_2.0_Mapper_265>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 128 - T-262 multicart (BMC-T-262)
///
/// Hardware: address latch + data latch at $8000, CHR-RAM only
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_128>
/// - Detail: <https://www.nesdev.org/wiki/NES_2.0_Mapper_265>
///
/// Address latch ($8000-$FFFF, latched from address bus):
///   `A~[1.L. ..OO POO. ..MN]`
///   - L (A13): Lock bit — prevents further address latch changes
///   - O (A9,A8,A6,A5): 128 KiB outer PRG-ROM bank
///   - P (A7): PRG mode — 0=UNROM (fixed bank 7 at $C000), 1=NROM-128
///   - M (A1): Mirroring — 0=Vertical, 1=Horizontal
///   - N (A0): NROM-256 flag — replaces PRG A14 with CPU A14
///
/// Data latch ($8000-$FFFF, latched from data bus):
///   `.... .PPP` — 16 KiB inner PRG-ROM bank at $8000-$BFFF
///
/// PRG modes:
///   - UNROM (P=0, N=0): $8000 = inner, $C000 = fixed bank 7
///   - NROM-128 (P=1, N=0): both halves = inner bank
///   - NROM-256 (P=1, N=1): $8000 = inner & ~1, $C000 = inner | 1
///   - P=0, N=1: UNROM with A14 replacement (uncommon)
///
/// CHR: 8 KiB CHR-RAM, no banking
pub struct Mapper128 {
    base: BaseMapper,
    locked: bool,
    outer_bank: u8,
    prg_mode: bool,
    nrom_256: bool,
    inner_bank: u8,
}

impl Mapper128 {
    const MAPPER_NUMBER: u16 = 128;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = Self::capabilities_for_mapper();
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x4000); // 16 KiB slots
        let mut mapper = Self {
            base,
            locked: false,
            outer_bank: 0,
            prg_mode: false,
            nrom_256: false,
            inner_bank: 0,
        };
        mapper.update_prg_banks();
        mapper
    }

    fn capabilities_for_mapper() -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 0,
            ..Default::default()
        }
    }

    fn update_prg_banks(&mut self) {
        let base_bank = (self.outer_bank as i16) * 8;

        if self.prg_mode {
            if self.nrom_256 {
                // NROM-256: consecutive 32 KiB
                let even = base_bank + (self.inner_bank as i16 & 0x06);
                self.base.select_prg_page(0, even);
                self.base.select_prg_page(1, even | 1);
            } else {
                // NROM-128: same 16 KiB in both halves
                let bank = base_bank + self.inner_bank as i16;
                self.base.select_prg_page(0, bank);
                self.base.select_prg_page(1, bank);
            }
        } else if self.nrom_256 {
            // UNROM with A14 replacement
            let inner = base_bank + (self.inner_bank as i16 & 0x06);
            self.base.select_prg_page(0, inner);
            self.base.select_prg_page(1, base_bank + 7);
        } else {
            // UNROM: inner at $8000, fixed 7 at $C000
            let bank = base_bank + self.inner_bank as i16;
            self.base.select_prg_page(0, bank);
            self.base.select_prg_page(1, base_bank + 7);
        }
    }
}

impl Mapper for Mapper128 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.base.read_prg_rom(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }

        // Data latch is always writable
        self.inner_bank = value & 0x07;

        // Address latch is blocked when locked
        if !self.locked {
            self.locked = (addr & 0x2000) != 0;
            self.outer_bank = (((addr >> 6) & 0x0C) | ((addr >> 5) & 0x03)) as u8;
            self.prg_mode = (addr & 0x0080) != 0;
            self.nrom_256 = (addr & 0x0001) != 0;

            let mirroring = if (addr & 0x0002) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
            self.base.set_mirroring(mirroring);
        }

        self.update_prg_banks();
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(
            (if self.locked { 0x80 } else { 0 })
                | (self.outer_bank << 3)
                | (if self.prg_mode { 0x04 } else { 0 })
                | (if self.nrom_256 { 0x02 } else { 0 })
                | (self.inner_bank & 0x01), // save bit 0 of inner_bank here
        );
        snap.push(self.inner_bank);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            let (base_data, extra) = data.split_at(data.len() - 2);
            let flags = extra[0];
            self.locked = (flags & 0x80) != 0;
            self.outer_bank = (flags >> 3) & 0x0F;
            self.prg_mode = (flags & 0x04) != 0;
            self.nrom_256 = (flags & 0x02) != 0;
            self.inner_bank = extra[1] & 0x07;
            self.base.restore_banking(base_data);
            self.update_prg_banks();
        } else {
            self.base.restore_banking(data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        Self::capabilities_for_mapper()
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 128 PRG 16K banks = 2 MB (16 outer banks × 8 inner banks)
    const PRG_16K_BANKS: usize = 128;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(16 * 1024, PRG_16K_BANKS);
        let chr = vec![]; // Empty = 8 KiB CHR-RAM (allocated by BaseMapper)
        create_mapper(MapperContext::new_for_test(
            128,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 128 should be implemented")
    }

    /// Write to the mapper with specific address and data latch values.
    fn latch_write(mapper: &mut Box<dyn Mapper>, addr: u16, value: u8) {
        mapper.write_prg(addr, value);
    }

    /// Build address from T-262 fields.
    fn build_addr(lock: bool, outer: u8, prg_mode: bool, mirror_h: bool, nrom256: bool) -> u16 {
        let mut addr: u16 = 0x8000;
        if lock {
            addr |= 0x2000;
        }
        // outer bits: {A9,A8,A6,A5}
        addr |= ((outer as u16 & 0x0C) << 6) | ((outer as u16 & 0x03) << 5);
        if prg_mode {
            addr |= 0x0080;
        }
        if mirror_h {
            addr |= 0x0002;
        }
        if nrom256 {
            addr |= 0x0001;
        }
        addr
    }

    // --- Factory ---

    #[test]
    fn mapper_128_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            128,
            banked_data(16 * 1024, PRG_16K_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 128 must be registered in the factory"
        );
    }

    // --- UNROM mode (default: P=0, N=0) ---

    #[test]
    fn unrom_default_c000_is_fixed_bank_7() {
        // Default: outer=0, inner=0 → $C000 = bank 7
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must be fixed to inner bank 7 in UNROM mode"
        );
    }

    #[test]
    fn unrom_inner_bank_selects_8000() {
        let mut mapper = make_mapper();
        // Write inner bank 3 (data latch), no address latch changes
        latch_write(&mut mapper, 0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 must follow inner bank PPP in UNROM mode"
        );
    }

    #[test]
    fn unrom_c000_stays_fixed_when_inner_changes() {
        let mut mapper = make_mapper();
        latch_write(&mut mapper, 0x8000, 5);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must remain fixed bank 7 regardless of inner bank"
        );
    }

    // --- Outer bank ---

    #[test]
    fn outer_bank_shifts_both_halves() {
        let mut mapper = make_mapper();
        // outer = 2 → base bank = 16. Inner = 0 → $8000 = 16, $C000 = 16+7 = 23
        let addr = build_addr(false, 2, false, false, false);
        latch_write(&mut mapper, addr, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "$8000 in outer bank 2 = bank 16"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            23,
            "$C000 in outer bank 2 = bank 23 (fixed 7)"
        );
    }

    #[test]
    fn outer_bank_inner_bank_combine() {
        let mut mapper = make_mapper();
        // outer = 3 → base = 24. Inner = 5 → $8000 = 29
        let addr = build_addr(false, 3, false, false, false);
        latch_write(&mut mapper, addr, 5);
        assert_eq!(mapper.read_prg(0x8000), 29, "outer=3, inner=5 → bank 29");
    }

    // --- NROM-128 mode (P=1, N=0) ---

    #[test]
    fn nrom128_mirrors_inner_bank() {
        let mut mapper = make_mapper();
        let addr = build_addr(false, 0, true, false, false);
        latch_write(&mut mapper, addr, 4);
        assert_eq!(mapper.read_prg(0x8000), 4, "NROM-128 $8000 = inner 4");
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "NROM-128 $C000 must mirror inner bank"
        );
    }

    // --- NROM-256 mode (P=1, N=1) ---

    #[test]
    fn nrom256_maps_consecutive_32k() {
        let mut mapper = make_mapper();
        // P=1, N=1, inner = 4 → even = 4, odd = 5
        let addr = build_addr(false, 0, true, false, true);
        latch_write(&mut mapper, addr, 4);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "NROM-256 $8000 = inner & ~1 = 4"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "NROM-256 $C000 = (inner & ~1) | 1 = 5"
        );
    }

    #[test]
    fn nrom256_clears_inner_bit0() {
        let mut mapper = make_mapper();
        // inner = 5 (odd) → even = 4, odd = 5
        let addr = build_addr(false, 0, true, false, true);
        latch_write(&mut mapper, addr, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "NROM-256 must clear bit 0: inner 5 → bank 4"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "NROM-256 $C000 = 5");
    }

    // --- Locking ---

    #[test]
    fn lock_prevents_address_latch_change() {
        let mut mapper = make_mapper();
        // Set outer=1 with lock
        let addr = build_addr(true, 1, false, false, false);
        latch_write(&mut mapper, addr, 0);
        assert_eq!(mapper.read_prg(0x8000), 8, "Outer=1 → bank 8");

        // Try to change outer to 2 — should be blocked
        let addr2 = build_addr(false, 2, false, false, false);
        latch_write(&mut mapper, addr2, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "Address latch must be locked: outer stays 1"
        );
    }

    #[test]
    fn lock_allows_data_latch_change() {
        let mut mapper = make_mapper();
        // Lock with outer=0, inner=0
        let addr = build_addr(true, 0, false, false, false);
        latch_write(&mut mapper, addr, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Change inner bank while locked
        latch_write(&mut mapper, 0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Data latch (inner bank) must remain changeable when locked"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_defaults_to_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Default mirroring must be vertical"
        );
    }

    #[test]
    fn mirroring_switches_to_horizontal() {
        let mut mapper = make_mapper();
        let addr = build_addr(false, 0, false, true, false);
        latch_write(&mut mapper, addr, 0);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "M=1 must set horizontal mirroring"
        );
    }

    // --- CHR-RAM ---

    #[test]
    fn chr_ram_is_writable_and_readable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be readable/writable"
        );
    }

    // --- Save state ---

    #[test]
    fn snapshot_roundtrip_preserves_state() {
        let mut mapper = make_mapper();
        let addr = build_addr(true, 5, true, true, true);
        latch_write(&mut mapper, addr, 6);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // Verify PRG mapping matches
        assert_eq!(
            mapper.read_prg(0x8000),
            mapper2.read_prg(0x8000),
            "Snapshot round-trip must preserve PRG mapping"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            mapper2.read_prg(0xC000),
            "Snapshot round-trip must preserve $C000 mapping"
        );
        assert_eq!(
            mapper2.base().mirroring(),
            NametableLayout::Horizontal,
            "Snapshot must preserve mirroring"
        );
    }
}
