//! Mapper 257 - PEC-586 FC-Based Computer
//!
//! Specifications:
//! - Primary source: FCEUX pec-586.cpp by CaH4e3 (CaH4e3, GPL v2)
//! - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_257>
//!
//! Known Limitations:
//! - PEC586Hack (PC display mode vs NES display mode) is not emulated.
//! - 512KB custom PRG unscrambling for the NES-mode-disabled case is only partially implemented (via `unscrambled_prg_index`/`read_prg_512k`) and may not match all hardware.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 257 - PEC-586 FC-Based Computer (DUNDA)
///
/// Hardware: PEC-586 board with register-based PRG bank switching.
///
/// Specifications:
/// - Primary source: FCEUX pec-586.cpp by CaH4e3
/// - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_257>
/// - PRG-ROM: Up to 512KB; 16KB × 2 switchable (non-512KB) or 32KB/8KB direct (512KB)
/// - CHR: 8KB fixed CHR bank 0 (ROM or RAM)
/// - Mirroring: Vertical fixed (non-512KB) or H/V dynamic (512KB)
/// - PRG-RAM: 8KB WRAM at $6000-$7FFF
///
/// Register writes: $5000-$5FFF → `reg[(addr & 0x700) >> 8] = value`
///
/// Non-512KB PRG mode (most ROMs):
/// - $8000: 16KB bank = BS_TBL[reg[0] & 0x7F] >> 4
/// - $C000: 16KB bank = BS_TBL[reg[0] & 0x7F] & 0x0F
/// - Mirroring: always Vertical
/// - Power-on: reg[0] = 0x0E
///
/// 512KB PRG mode:
/// - bit 4 (0x10) set: 32KB bank at $8000, bank = reg[0] & 7
/// - bit 6 (0x40) set: 8KB bank at $8000–$9FFF, bank = (reg[0] & 0x0F) | 0x20 | ((reg[0] & 0x20) >> 1)
/// - Mirroring: (reg[0] & 0x18) == 0x18 → Horizontal, else Vertical
/// - Power-on: reg[0] = 0x00
pub struct Mapper257 {
    base: BaseMapper,
    reg: [u8; 8],
    prg_size: usize,
}

impl Mapper257 {
    /// PRG bank-select table for non-512KB mode (128 entries, each = packed two 4-bit bank numbers).
    /// Each entry: high nibble = bank for $8000, low nibble = bank for $C000.
    const BS_TBL: [u8; 128] = [
        0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23,
        0x33, // 0x00
        0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45,
        0x67, // 0x10
        0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23, 0x33, 0x03, 0x13, 0x23,
        0x33, // 0x20
        0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47,
        0x67, // 0x30
        0x02, 0x12, 0x22, 0x32, 0x02, 0x12, 0x22, 0x32, 0x02, 0x12, 0x22, 0x32, 0x02, 0x12, 0x22,
        0x32, // 0x40
        0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45, 0x67, 0x45,
        0x67, // 0x50
        0x02, 0x12, 0x22, 0x32, 0x02, 0x12, 0x22, 0x32, 0x02, 0x12, 0x22, 0x32, 0x00, 0x10, 0x20,
        0x30, // 0x60
        0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47, 0x67, 0x47,
        0x67, // 0x70
    ];

    /// Read-response table for $5000-$5FFF, indexed by `reg[4] >> 4`.
    const BR_TBL: [u8; 16] = [
        0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02,
    ];

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_size = ctx.prg_rom.len();

        let mut reg = [0u8; 8];
        reg[0] = if prg_size == 512 * 1024 { 0x00 } else { 0x0E };

        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        // Both modes start Vertical; 512KB can switch later via update_mirroring().
        base.set_mirroring_hv(false);

        Self {
            base,
            reg,
            prg_size,
        }
    }

    #[inline]
    fn is_512k(&self) -> bool {
        self.prg_size == 512 * 1024
    }

    fn read_prg_rom_at(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        if self.is_512k() {
            self.read_prg_512k(addr, prg)
        } else {
            self.read_prg_non512k(addr, prg)
        }
    }

    fn read_prg_512k(&self, addr: u16, prg: &[u8]) -> u8 {
        let a = addr as usize;
        let r0 = self.reg[0];

        if r0 & 0x10 != 0 {
            // 32KB banking: bank = reg[0] bits 0-2
            let bank = (r0 & 7) as usize;
            let offset = a - 0x8000;
            prg.get(bank * 0x8000 + offset).copied().unwrap_or(0)
        } else if r0 & 0x40 != 0 && addr < 0xA000 {
            // 8KB banking at $8000-$9FFF
            let bank = ((r0 & 0x0F) | 0x20 | ((r0 & 0x20) >> 1)) as usize;
            let offset = a - 0x8000;
            prg.get(bank * 0x2000 + offset).copied().unwrap_or(0)
        } else {
            prg.get(Self::unscrambled_prg_index(a))
                .copied()
                .unwrap_or(0)
        }
    }

    /// Compute the raw PRG-ROM byte index for the PEC-586 unscrambled address decode.
    fn unscrambled_prg_index(addr: usize) -> usize {
        ((0x0107 | ((addr >> 7) & 0x0F8)) << 10) | (addr & 0x3FF)
    }

    fn read_prg_non512k(&self, addr: u16, prg: &[u8]) -> u8 {
        let tbl_idx = (self.reg[0] & 0x7F) as usize;
        let entry = Self::BS_TBL[tbl_idx];
        let (bank, offset) = if addr < 0xC000 {
            ((entry >> 4) as usize, addr as usize - 0x8000)
        } else {
            ((entry & 0x0F) as usize, addr as usize - 0xC000)
        };
        let prg_len = prg.len();
        if prg_len == 0 {
            return 0;
        }
        prg.get((bank * 0x4000 + offset) % prg_len)
            .copied()
            .unwrap_or(0)
    }

    fn update_mirroring(&mut self) {
        if self.is_512k() {
            let horizontal = (self.reg[0] & 0x18) == 0x18;
            self.base.set_mirroring_hv(horizontal);
        }
        // non-512KB: always Vertical (set at construction)
    }
}

impl Mapper for Mapper257 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(v) = self.base.try_read_prg_ram(addr) {
            return v;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.read_prg_rom_at(addr);
        }
        0
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x5000..=0x5FFF).contains(&addr) {
            let idx = (self.reg[4] >> 4) as usize;
            return (open_bus & 0xD8) | Self::BR_TBL[idx];
        }
        self.base()
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if (0x5000..=0x5FFF).contains(&addr) {
            let idx = ((addr & 0x700) >> 8) as usize;
            self.reg[idx] = value;
            self.update_mirroring();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Snapshot only the mapper's register state. Mirroring is derived from registers
        // via `update_mirroring()` on restore, so we do not serialize it separately here.
        self.reg.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 8 {
            self.reg.copy_from_slice(&data[..8]);
        }
        self.update_mirroring();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper257(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            257, prg_rom, chr_rom, mirroring,
        ))
    }

    fn prg_16kb(banks: usize) -> Vec<u8> {
        banked_data(16 * 1024, banks)
    }

    fn prg_32kb(banks: usize) -> Vec<u8> {
        banked_data(32 * 1024, banks)
    }

    fn chr_8kb() -> Vec<u8> {
        banked_data(8 * 1024, 1)
    }

    #[test]
    fn test_factory_creates_mapper_257() {
        let mapper = create_mapper257(prg_16kb(16), chr_8kb(), NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 257 should be creatable via factory");
    }

    #[test]
    fn test_register_writes_decoded_by_address_bits() {
        // reg[(addr & 0x700) >> 8] = value
        // $5000-$50FF → reg[0], $5100-$51FF → reg[1], etc.
        let mut mapper =
            create_mapper257(prg_16kb(16), chr_8kb(), NametableLayout::Vertical).unwrap();

        // Write different values to different registers
        mapper.write_prg(0x5000, 0x0E); // reg[0]
        mapper.write_prg(0x5100, 0xAA); // reg[1]
        mapper.write_prg(0x5200, 0xBB); // reg[2]
        mapper.write_prg(0x5400, 0xCC); // reg[4]

        // Verify reg[4] is used for $5000 reads: BR_TBL[0xCC >> 4] = BR_TBL[0x0C] = 0x00
        // open_bus = 0xFF → (0xFF & 0xD8) | 0x00 = 0xD8 | 0x00 = 0xD8
        let open_bus = 0xFF;
        let expected_idx = 0xCC >> 4; // = 0x0C
        let expected = (open_bus & 0xD8) | Mapper257::BR_TBL[expected_idx];
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, open_bus),
            expected,
            "reg[4] (written via $5400) should control $5000 read response"
        );
    }

    #[test]
    fn test_wram_at_6000_7fff() {
        let mut mapper =
            create_mapper257(prg_16kb(16), chr_8kb(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x7FFF, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
        assert_eq!(mapper.read_prg(0x7FFF), 0xAB);
    }

    #[test]
    fn test_chr_rom_readable() {
        let chr = banked_data(8 * 1024, 3);
        let mut mapper = create_mapper257(prg_16kb(16), chr, NametableLayout::Vertical).unwrap();
        // CHR is fixed to bank 0; bank 0 starts with value 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    // ── Non-512KB PRG banking (bs_tbl lookup) ──────────────────────────────

    #[test]
    fn test_non_512k_power_on_state_reg0_is_0x0e() {
        // With 3 x 16KB banks (48KB PRG → not 512KB), power-on reg[0] = 0x0E.
        // bs_tbl[0x0E] = 0x23 → $8000 = bank 2, $C000 = bank 3
        // Use 13 banks to avoid modulo coincidence with bank 2 or 3.
        let prg = prg_16kb(13);
        let mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        // bs_tbl[0x0E] = 0x23 → bank 2 at $8000, bank 3 at $C000
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "power-on: $8000 should be bs_tbl[0x0E] >> 4 = 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "power-on: $C000 should be bs_tbl[0x0E] & 0x0F = 3"
        );
    }

    #[test]
    fn test_non_512k_prg_banking_via_bs_tbl() {
        // reg[0] = 0x00: bs_tbl[0x00] = 0x03 → $8000 = bank 0, $C000 = bank 3
        // Use 13 banks to avoid wrap coincidence
        let prg = prg_16kb(13);
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x5000, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "bs_tbl[0x00] >> 4 = 0: $8000 should be bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "bs_tbl[0x00] & 0x0F = 3: $C000 should be bank 3"
        );
    }

    #[test]
    fn test_non_512k_prg_banking_index_1() {
        // reg[0] = 0x01: bs_tbl[0x01] = 0x13 → $8000 = bank 1, $C000 = bank 3
        let prg = prg_16kb(13);
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x5000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 should be bank 1");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should be bank 3");
    }

    #[test]
    fn test_non_512k_mirroring_always_vertical() {
        let prg = prg_16kb(4);
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Horizontal).unwrap();

        // Write to reg[0] – mirroring must stay vertical
        mapper.write_prg(0x5000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Non-512KB mapper 257 always uses Vertical mirroring"
        );
    }

    // ── 512KB PRG banking ─────────────────────────────────────────────────

    #[test]
    fn test_512k_power_on_state_reg0_is_0x00() {
        // 512KB = 32 x 16KB banks
        let prg = prg_16kb(32); // 512KB
        let mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        // reg[0] = 0x00 at power-on, neither bit4 nor bit6 set → custom read
        // Custom: ((0x0107 | ((0x8000 >> 7) & 0x0F8)) << 10) | (0x8000 & 0x3FF)
        // 0x8000 >> 7 = 0x100, & 0x0F8 = 0xF8, 0x0107 | 0xF8 = 0x01FF, << 10 = 0x7FC00
        // | (0x8000 & 0x3FF) = | 0 = 0x7FC00
        // That's 522240 bytes into a 524288-byte (512KB) ROM
        // This just checks we don't panic, not the specific value
        let _ = mapper.read_prg(0x8000);
    }

    #[test]
    fn test_512k_32kb_mode_when_bit4_set() {
        // 32 x 16KB = 512KB PRG, but for 32KB banking we need 32KB-aligned banks
        // Use 16 x 32KB = 512KB
        let prg = prg_32kb(16); // 512KB
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        // Set bit4 (0x10) and bank = 3: reg[0] = 0x13
        mapper.write_prg(0x5000, 0x13);

        // 32KB bank 3: $8000 = bank*0x8000 + offset
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "32KB mode: $8000 should read from bank 3"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "32KB mode: $C000 (within same 32KB bank) should also read bank 3"
        );
    }

    #[test]
    fn test_512k_8kb_mode_when_bit6_set() {
        // 512KB ROM: 64 x 8KB banks
        // reg[0] bit6 (0x40) set → 8KB at $8000-$9FFF
        // bank = (reg[0] & 0x0F) | 0x20 | ((reg[0] & 0x20) >> 1)
        // reg[0] = 0x40 → bank = (0x00) | 0x20 | 0x00 = 0x20 = 32
        let prg = banked_data(8 * 1024, 64); // 512KB, 64 x 8KB banks
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x5000, 0x40);
        // 8KB bank 32 at $8000-$9FFF
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "8KB mode: $8000 should be bank 32"
        );
        assert_eq!(
            mapper.read_prg(0x9FFF),
            32,
            "8KB mode: $9FFF should still be bank 32"
        );
    }

    #[test]
    fn test_512k_mirroring_horizontal_when_bits_3_4_set() {
        let prg = prg_16kb(32); // 512KB
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        // (reg[0] & 0x18) == 0x18 → Horizontal
        mapper.write_prg(0x5000, 0x18);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "512KB mode: reg[0]=0x18 → Horizontal mirroring"
        );
    }

    #[test]
    fn test_512k_mirroring_vertical_when_bits_3_4_not_both_set() {
        let prg = prg_16kb(32); // 512KB
        let mut mapper = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();

        // (reg[0] & 0x18) != 0x18 (only one bit set) → Vertical
        mapper.write_prg(0x5000, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "512KB mode: reg[0]=0x08 → Vertical mirroring"
        );
    }

    // ── $5000 read (BR_TBL) ──────────────────────────────────────────────

    #[test]
    fn test_5000_read_uses_br_tbl_with_reg4() {
        let mut mapper =
            create_mapper257(prg_16kb(4), chr_8kb(), NametableLayout::Vertical).unwrap();

        // reg[4] = 0x70 → BR_TBL[7] = 0x20
        mapper.write_prg(0x5400, 0x70);
        let open_bus = 0xF7_u8;
        // (0xF7 & 0xD8) | 0x20 = 0xD0 | 0x20 = 0xD0 (wait: 0xF7 & 0xD8 = 0xD0; 0xD0 | 0x20 = 0xF0)
        // Let me recalculate: 0xF7 = 1111_0111, 0xD8 = 1101_1000
        // 0xF7 & 0xD8 = 1101_0000 = 0xD0
        // 0xD0 | 0x20 = 1111_0000 = 0xF0
        let expected = (open_bus & 0xD8) | 0x20;
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, open_bus),
            expected,
            "$5000 read should return (open_bus & 0xD8) | BR_TBL[reg4 >> 4]"
        );
    }

    #[test]
    fn test_5000_read_br_tbl_index_1() {
        let mut mapper =
            create_mapper257(prg_16kb(4), chr_8kb(), NametableLayout::Vertical).unwrap();

        // reg[4] = 0x10 → BR_TBL[1] = 0x09
        mapper.write_prg(0x5400, 0x10);
        let open_bus = 0x00;
        let expected = (open_bus & 0xD8) | 0x09;
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, open_bus),
            expected,
            "BR_TBL[1] = 0x09"
        );
    }

    // ── Save state ───────────────────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg = prg_16kb(13);
        let mut mapper =
            create_mapper257(prg.clone(), chr_8kb(), NametableLayout::Vertical).unwrap();

        // Set a known state: bs_tbl[0x02] = 0x23 → $8000=bank2, $C000=bank3
        mapper.write_prg(0x5000, 0x02);
        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper257(prg, chr_8kb(), NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 2, "Restored: $8000 = bank 2");
        assert_eq!(restored.read_prg(0xC000), 3, "Restored: $C000 = bank 3");
    }
    // existing tests above

    #[test]
    fn snapshot_and_restore_only_use_register_bytes() {
        // 32KB PRG, 8KB CHR, initial vertical mirroring (value shouldn't matter)
        let prg_rom = vec![0u8; 0x8000];
        let chr_rom = vec![0u8; 0x2000];
        let mut mapper = create_mapper257(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("failed to create mapper")
            .downcast::<Mapper257>()
            .expect("not a Mapper257");

        // Take an initial snapshot and confirm it only contains the 8 register bytes.
        let snapshot = mapper.registers_snapshot();
        assert_eq!(snapshot.len(), 8);

        // Mutate registers via PRG writes that the mapper interprets as register writes.
        mapper.write_prg(0x5000, 0x12);
        mapper.write_prg(0x5100, 0x34);

        // Restore from the original snapshot; this should restore reg[] and mirroring
        // based solely on those 8 bytes.
        mapper.restore_registers(&snapshot);

        // After restore, the live snapshot should match the original snapshot bytes.
        let snapshot_after = mapper.registers_snapshot();
        assert_eq!(snapshot_after, snapshot);
    }
}
