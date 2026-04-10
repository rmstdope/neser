//! Mapper 235 – "Golden Game" 150-in-1 multicart (BMC-235 board)
//!
//! Specifications:
//! - Primary source: NESdev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_235.xhtml>
//! - Implementation reference: Mesen2 `Core/NES/Mappers/Unlicensed/Bmc235.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Bmc235.h>
//!
//! ## Hardware behavior
//!
//! Banking is controlled by writes to any address in `$8000–$FFFF`. The data
//! byte is ignored; the **write address** carries the register contents:
//!
//! ```text
//! A15 A14 A13 A12 A11 A10 A9 A8 A7 A6 A5 A4 A3 A2 A1 A0
//!  1   x   M   P   R   N   B  B  x  x  x  A  A  A  A  A
//! ```
//!
//! - `A` (bits 4:0): 32K-page address within the selected ROM chip
//! - `B` (bits 9:8): ROM chip select (2 bits, only specific values valid per ROM size)
//! - `N` (bit 10):   0 = 2-screen mirroring  / 1 = single-screen (lower VRAM)
//! - `R` (bit 11):   0 = 32K mode            / 1 = 16K mode
//! - `P` (bit 12):   16K page half (0 = lower / 1 = upper) — 16K mode only
//! - `M` (bit 13):   0 = Vertical mirroring   / 1 = Horizontal (when N=0)
//!
//! **PRG banking — 32K mode (R=0):**
//!   - `$8000–$BFFF` = 16K bank `base + 2·A`
//!   - `$C000–$FFFF` = 16K bank `base + 2·A + 1`
//!
//! **PRG banking — 16K mode (R=1):**
//!   - Both `$8000–$BFFF` and `$C000–$FFFF` = 16K bank `base + 2·A + P`
//!
//! **ROM chip offsets** (base = chip_offset + A, in 32K units):
//!
//! | ROM size |  B=00 |  B=01 |  B=10 |  B=11 |
//! |----------|-------|-------|-------|-------|
//! | ≤ 512 KB |  +0   | OB    | OB    | OB    |
//! |   1 MB   |  +0   | OB    | +32   | OB    |
//! |   2 MB   |  +0   | OB    | +32   | +64   |
//! |  ≥ 4 MB  |  +0   | +32   | +64   | +96   |
//!
//! (OB = open bus; in this implementation these cases wrap via ROM mirroring.)
//!
//! **CHR:** CHR-RAM only (8 KiB, not banked).
//! **Power-on/reset:** 32K mode, page 0 (A=0, R=0, B=0, N=0, M=0).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 235;
const PRG_BANK_SIZE: usize = 16 * 1024;

/// ROM-chip base offsets (in 32K page units) indexed by [mode][B bits 0-3].
/// A `None` entry means the chip is not present (open bus).
const CHIP_OFFSETS: [[Option<u8>; 4]; 4] = [
    // mode 0: ≤ 512 KB (32 32K-pages)
    [Some(0x00), None, None, None],
    // mode 1: 1 MB  (64 32K-pages)
    [Some(0x00), None, Some(0x20), None],
    // mode 2: 2 MB (128 32K-pages)
    [Some(0x00), None, Some(0x20), Some(0x40)],
    // mode 3: ≥ 4 MB (256 32K-pages)
    [Some(0x00), Some(0x20), Some(0x40), Some(0x60)],
];

fn rom_mode(prg_rom_len: usize) -> usize {
    if prg_rom_len <= 512 * 1024 {
        0 // ≤ 512 KB
    } else if prg_rom_len <= 1024 * 1024 {
        1 // ≤ 1 MB
    } else if prg_rom_len <= 2 * 1024 * 1024 {
        2 // ≤ 2 MB
    } else {
        3 // > 2 MB
    }
}

/// Mapper 235 – "Golden Game" 150-in-1 multicart.
pub struct Mapper235 {
    base: BaseMapper,
    mode: usize,
    /// Last written register value captured from the write address.
    reg: u16,
    /// True when the current B-bits select a chip not present in this ROM size.
    chip_absent: bool,
}

impl Mapper235 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mode = rom_mode(ctx.prg_rom.len());
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        let mut mapper = Self {
            base,
            mode,
            reg: 0,
            chip_absent: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let addr = self.reg;
        let b_bits = ((addr >> 8) & 0x03) as usize;
        let a_bits = (addr & 0x1F) as i16;
        let r_mode = (addr & 0x0800) != 0;
        let p_bit = ((addr >> 12) & 0x01) as i16;
        let n_bit = (addr & 0x0400) != 0;
        let m_bit = (addr & 0x2000) != 0;

        // Mirroring: N=1 → single-screen; N=0, M=1 → Horizontal; else Vertical.
        if n_bit {
            self.base.set_mirroring(NametableLayout::SingleScreenLower);
        } else {
            self.base.set_mirroring_hv(m_bit);
        }

        // Chip offset: if chip is absent, set open-bus flag and use offset 0 as dummy.
        let chip_entry = CHIP_OFFSETS[self.mode][b_bits];
        self.chip_absent = chip_entry.is_none();
        let chip_offset = chip_entry.unwrap_or(0x00) as i16;

        if r_mode {
            // 16K mode: both pages map to the same 16K bank = chip_offset*2 + 2*A + P
            let bank_16k = chip_offset * 2 + a_bits * 2 + p_bit;
            self.base.select_prg_page(0, bank_16k);
            self.base.select_prg_page(1, bank_16k);
        } else {
            // 32K mode: sequential 16K pair = chip_offset*2 + 2*A, chip_offset*2 + 2*A + 1
            let bank_32k_base = chip_offset * 2 + a_bits * 2;
            self.base.select_prg_page(0, bank_32k_base);
            self.base.select_prg_page(1, bank_32k_base + 1);
        }
    }
}

impl Mapper for Mapper235 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if self.chip_absent && addr >= 0x8000 {
            return open_bus;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if addr < 0x8000 {
            return;
        }
        self.reg = addr;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![(self.reg & 0xFF) as u8, (self.reg >> 8) as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.reg = u16::from(data[0]) | (u16::from(data[1]) << 8);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg = 0;
        self.update_banks();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 2 MB ROM = 128 × 16 KB pages → mode 2 (B=0 valid, B=2 valid at offset 32)
    const PRG_16K_BANKS: usize = 128;

    fn make_mapper() -> Mapper235 {
        Mapper235::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_16K_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    fn make_mapper_1mb() -> Mapper235 {
        Mapper235::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, 64), // 1 MB
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_235_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_16K_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 235 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to 16K bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_is_bank_1() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must map to 16K bank 1 at power-on (32K mode)"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must be Vertical (M=0)"
        );
    }

    // ── 32K mode (R=0, bit 11 clear) ──────────────────────────────────────────

    #[test]
    fn write_32k_mode_a3_selects_32k_page_3() {
        let mut mapper = make_mapper();
        // addr = 0x8003 → A=3, R=0, B=0 → 32K page 3 → 16K banks 6 and 7
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "$8000 must be 16K bank 6 for 32K page 3"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must be 16K bank 7 for 32K page 3"
        );
    }

    #[test]
    fn write_32k_mode_pages_sequential() {
        let mut mapper = make_mapper();
        for a in 0u16..=4 {
            mapper.write_prg(0x8000 | a, 0); // A = a, R=0
            assert_eq!(
                mapper.read_prg(0x8000),
                (a * 2) as u8,
                "32K page {a} lower: 16K bank {}",
                a * 2
            );
            assert_eq!(
                mapper.read_prg(0xC000),
                (a * 2 + 1) as u8,
                "32K page {a} upper: 16K bank {}",
                a * 2 + 1
            );
        }
    }

    // ── 16K mode (R=1, bit 11 set) ────────────────────────────────────────────

    #[test]
    fn write_16k_mode_p0_both_pages_same_bank() {
        let mut mapper = make_mapper();
        // addr = 0x8803 → A=3, R=1 (0x800), P=0 → 16K bank = 2*3+0 = 6
        mapper.write_prg(0x8803, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "$8000 must be 16K bank 6 in 16K mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 must also be 16K bank 6 in 16K mode (P=0)"
        );
    }

    #[test]
    fn write_16k_mode_p1_both_pages_odd_bank() {
        let mut mapper = make_mapper();
        // addr = 0x9803 → A=3, R=1 (0x800), P=1 (0x1000) → 16K bank = 2*3+1 = 7
        mapper.write_prg(0x9803, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "$8000 must be 16K bank 7 in 16K mode (P=1)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must also be 16K bank 7 in 16K mode (P=1)"
        );
    }

    // ── ROM chip select (B bits, mode 2 = 2 MB) ───────────────────────────────

    #[test]
    fn b0_maps_to_first_mb() {
        let mut mapper = make_mapper();
        // addr = 0x8005 → A=5, B=0, R=0 → 16K banks 10, 11
        mapper.write_prg(0x8005, 0);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 11);
    }

    #[test]
    fn b2_maps_to_second_mb_offset_32() {
        let mut mapper = make_mapper();
        // addr = 0x8205 → A=5, B=2 (bits 9:8 = 10b), R=0 → base 32K offset 32 → 16K banks 64+10=74, 75
        // Wait: chip_offset = 0x20 = 32 (in 32K), 16K bank = 32*2 + 5*2 = 64+10 = 74
        mapper.write_prg(0x8205, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            74,
            "B=2 A=5: 16K bank lower must be 74"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            75,
            "B=2 A=5: 16K bank upper must be 75"
        );
    }

    #[test]
    fn absent_chip_b1_returns_open_bus_for_2mb_rom() {
        let mapper = make_mapper(); // 2 MB = mode 2; CHIP_OFFSETS[2][1] = None
        // Write addr = 0x8100 → B=1 (bits 9:8 = 01b), A=0 → absent chip for mode 2
        let mut mapper = mapper;
        mapper.write_prg(0x8100, 0);
        let open_bus: u8 = 0xAB;
        assert_eq!(
            mapper.read_prg_open_bus(0x8000, open_bus),
            open_bus,
            "B=1 in mode 2 selects absent chip; $8000 must return open bus"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0xC000, open_bus),
            open_bus,
            "B=1 in mode 2 selects absent chip; $C000 must return open bus"
        );
    }

    #[test]
    fn present_chip_b0_is_not_open_bus() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // B=0 → chip present in all modes
        let open_bus: u8 = 0xAB;
        assert_ne!(
            mapper.read_prg_open_bus(0x8000, open_bus),
            open_bus,
            "B=0 selects present chip; must not return open bus"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn m0_n0_vertical_mirroring() {
        let mut mapper = make_mapper();
        // addr = 0x8000 → M=0, N=0 → Vertical
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn m1_n0_horizontal_mirroring() {
        let mut mapper = make_mapper();
        // addr = 0xA000 → M=1 (bit 13 = 0x2000), N=0 → Horizontal
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn n1_single_screen_mirroring() {
        let mut mapper = make_mapper();
        // addr = 0x8400 → N=1 (bit 10 = 0x400) → SingleScreenLower
        mapper.write_prg(0x8400, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn n1_overrides_m1_for_mirroring() {
        let mut mapper = make_mapper();
        // addr has both M=1 and N=1 → N takes priority
        mapper.write_prg(0xA400, 0); // M=1, N=1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "N=1 must override M for mirroring"
        );
    }

    // ── Data byte is ignored ──────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0x00);
        let bank_00 = mapper.read_prg(0x8000);
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            bank_00,
            "Data byte must be ignored; banking from address only"
        );
    }

    // ── Writes below $8000 ignored ────────────────────────────────────────────

    #[test]
    fn write_below_8000_does_nothing() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Write below $8000 must not change banking"
        );
    }

    // ── 1 MB ROM (mode 1: B=0 and B=2 valid) ─────────────────────────────────

    #[test]
    fn mode_1_b0_valid() {
        let mut mapper = make_mapper_1mb();
        mapper.write_prg(0x8001, 0); // A=1, B=0
        assert_eq!(mapper.read_prg(0x8000), 2); // 16K bank 2
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_page_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA005, 0); // some arbitrary state
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must be bank 1 after reset (32K mode)"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB, "CHR-RAM must be writable");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9803, 0); // 16K mode, P=1, A=3

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG bank at $8000 must survive snapshot round-trip"
        );
        assert_eq!(
            mapper2.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring must survive snapshot round-trip"
        );
    }
}
