//! Mapper 225 – ET-4310 / K-1010 multicart (60-in-1, 64-in-1, etc.)
//!
//! # Specifications
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_225.xhtml>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper225.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper225.h>
//!
//! # Hardware overview
//!
//! Used by ET-4310 (60-pin) and K-1010 (72-pin) multicart boards (e.g.
//! 52-in-1, 58-in-1, 64-in-1).
//!
//! A write to **any address in `$8000–$FFFF`** latches banking information
//! from the **address bus** (the data byte is ignored):
//!
//! ```text
//! CPU address: [. H M O  PP PPPP  CC CCCC]
//!               15 14 13 12  11..6   5..0
//! ```
//!
//! | Bit(s) | Source       | Function                                      |
//! |--------|--------------|-----------------------------------------------|
//! | A14    | H            | High bit: OR'd as bit 6 of PRG and CHR pages  |
//! | A13    | M            | Mirroring: 0 = Vertical, 1 = Horizontal       |
//! | A12    | O            | PRG mode: 0 = 32 KB, 1 = 16 KB mirrored       |
//! | A11–A6 | P[5:0]       | Lower 6 bits of PRG page                      |
//! | A5–A0  | C[5:0]       | Lower 6 bits of CHR page                      |
//!
//! **7-bit PRG page** = `((addr >> 6) & 0x3F) | ((addr >> 8) & 0x40)`
//!
//! **7-bit CHR page** = `(addr & 0x3F) | ((addr >> 8) & 0x40)`
//!
//! # PRG banking (16 KiB pages)
//!
//! - **O = 0** (32 KB): `$8000–$BFFF` → `page & 0xFE`, `$C000–$FFFF` → `(page & 0xFE) + 1`
//! - **O = 1** (16 KB mirrored): both windows → `page`
//!
//! # CHR banking
//!
//! 8 KiB CHR bank at `$0000–$1FFF`, selected by the 7-bit CHR page.
//!
//! # Mirroring
//!
//! A13 = 0 → Vertical, A13 = 1 → Horizontal.
//!
//! # $5800–$5FFF RAM (not implemented)
//!
//! The PCB includes a 74'670 register file providing 4 × 4-bit (16-bit total)
//! readable/writable RAM at `$5800–$5FFF`.  This feature is omitted by
//! Nestopia and FCEUX ≥ 2.2.1, and by this implementation.  Games are
//! slightly buggy without it but still run.
//!
//! # Power-on / reset state
//!
//! 32 KB mode, PRG banks 0 + 1, CHR bank 0, Vertical mirroring.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 225;

/// Mapper 225 – ET-4310 / K-1010 multicart.
///
/// See module-level documentation for hardware details.
pub struct Mapper225 {
    base: BaseMapper,
}

impl Mapper225 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base };
        mapper.apply_write(0x8000);
        mapper
    }

    /// Decode a write-address and apply banking / mirroring.
    fn apply_write(&mut self, addr: u16) {
        // H = A14; kept at bit-6 position by the shift below.
        let high_bit = (addr >> 8) & 0x40;
        let prg_page = (((addr >> 6) & 0x3F) | high_bit) as i16;
        let chr_page = ((addr & 0x3F) | high_bit) as i16;

        if addr & 0x1000 != 0 {
            // O = 1: 16 KB mirrored
            self.base.select_prg_page(0, prg_page);
            self.base.select_prg_page(1, prg_page);
        } else {
            // O = 0: 32 KB contiguous
            self.base.select_prg_page(0, prg_page & !1);
            self.base.select_prg_page(1, (prg_page & !1) + 1);
        }

        self.base.select_chr_page(0, chr_page);

        self.base.set_mirroring(if addr & 0x2000 != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        });
    }
}

impl Mapper for Mapper225 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0x8000 {
            let _ = value; // data byte is ignored; banking is determined by address lines
            self.apply_write(addr);
        }
    }

    fn reset(&mut self) {
        self.apply_write(0x8000);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.base.banking_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Non-power-of-two bank counts prevent modulo-wrap false positives.
    /// 131 × 16 KiB covers the full 7-bit PRG page range (0–127) and is odd.
    const PRG_BANKS: usize = 131;
    /// 131 × 8 KiB for CHR.
    const CHR_BANKS: usize = 131;
    const PRG_BANK_SIZE: usize = 16 * 1024;
    const CHR_BANK_SIZE: usize = 8 * 1024;

    fn make_mapper() -> Mapper225 {
        Mapper225::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_225_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 225 must be registered in the factory"
        );
        assert_eq!(result.unwrap().mapper_number(), MAPPER_NUMBER);
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_lower_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_upper_window_is_bank_1() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must map to PRG bank 1 at power-on (32 KB mode)"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical at power-on (A13=0)"
        );
    }

    // ── PRG 32 KB mode (O=0, A12=0) ──────────────────────────────────────────

    #[test]
    fn mode_32k_lower_gets_even_bank_upper_gets_odd() {
        let mut mapper = make_mapper();
        // addr=$8000: highBit=0, prgPage=0, O=0 → lower=0, upper=1
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn mode_32k_even_page_from_addr_bits() {
        let mut mapper = make_mapper();
        // addr=$8080: A[11:6] = (0x80 >> 6 not right)
        // addr=0x8080: binary: 1000_0000_1000_0000
        // A14=0, A13=0, A12=0, A11..A6 = 0b00_0010 = 2 → prgPage=2, O=0
        // lower = 2 & 0xFE = 2, upper = 3
        mapper.write_prg(0x8080, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "lower must be bank 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "upper must be bank 3");
    }

    #[test]
    fn mode_32k_odd_prg_page_aligns_to_even_pair() {
        let mut mapper = make_mapper();
        // addr=$8040: A[11:6] = 1 → prgPage=1, O=0
        // lower = 1 & 0xFE = 0, upper = 1
        mapper.write_prg(0x8040, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "lower must align to even bank 0"
        );
        assert_eq!(mapper.read_prg(0xC000), 1, "upper must be bank 1");
    }

    // ── PRG 16 KB mirrored mode (O=1, A12=1) ─────────────────────────────────

    #[test]
    fn mode_16k_both_windows_same_bank() {
        let mut mapper = make_mapper();
        // addr=$9040: A12=1 (O=1), A[11:6]=1 → prgPage=1, mode=16KB
        // both windows → bank 1
        mapper.write_prg(0x9040, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must be bank 1 (16 KB mode)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror bank 1 (16 KB mode)"
        );
    }

    #[test]
    fn mode_16k_odd_page_not_aligned() {
        let mut mapper = make_mapper();
        // addr=$9040: prgPage=1, O=1 → both windows bank 1 (no alignment)
        mapper.write_prg(0x9040, 0);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn mode_16k_large_bank() {
        let mut mapper = make_mapper();
        // addr=$9100: A12=1 (O=1), highBit=(0x9100>>8)&0x40 = 0x91&0x40 = 0
        // actually 0x9100 in hex: A14=0, A13=0, A12=1, A11..A6 = bits 11-6 of 0x9100
        // 0x9100 = 1001_0001_0000_0000
        //  A14=0, A13=0, A12=1, A11=0, A10=0, A9=0, A8=1, A7=0, A6=0 → prgPage = (0x9100>>6)&0x3F = 0x244&0x3F = 0x04 = 4
        // highBit = (0x9100>>8)&0x40 = 0x91&0x40 = 0
        // prgPage = 4, O=1 → both windows bank 4
        mapper.write_prg(0x9100, 0);
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    // ── High bit (A14) extends PRG and CHR to 7-bit ───────────────────────────

    #[test]
    fn high_bit_a14_extends_prg_to_bit6() {
        let mut mapper = make_mapper();
        // addr=$C000: A14=1 → highBit=0x40=64, A[11:6]=0, O=0
        // prgPage = 0 | 64 = 64, 32KB: lower=64, upper=65
        mapper.write_prg(0xC000, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            64,
            "$8000 = bank 64 (A14 sets bit 6)"
        );
        assert_eq!(mapper.read_prg(0xC000), 65, "$C000 = bank 65");
    }

    #[test]
    fn high_bit_a14_extends_chr_to_bit6() {
        let mut mapper = make_mapper();
        // addr=$C000: A14=1 → highBit=0x40=64, A[5:0]=0
        // chrPage = 0 | 64 = 64
        mapper.write_prg(0xC000, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            64,
            "CHR bank must include A14 as bit 6"
        );
    }

    #[test]
    fn high_bit_combines_with_low_prg_bits() {
        let mut mapper = make_mapper();
        // addr=$C100: A14=1, highBit=64; A[11:6]=(0xC100>>6)&0x3F = 0x304&0x3F = 4
        // 0xC100 = 1100_0001_0000_0000
        // A14=1, A13=0, A12=0, A11=0, A10=0, A9=0, A8=1, A7=0, A6=0
        // (addr>>6)&0x3F = (0xC100>>6)&0x3F = 0x304&0x3F = 4
        // prgPage = 4 | 64 = 68, O=0 → lower=68, upper=69
        mapper.write_prg(0xC100, 0);
        assert_eq!(mapper.read_prg(0x8000), 68);
        assert_eq!(mapper.read_prg(0xC000), 69);
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_from_a5_to_a0() {
        let mut mapper = make_mapper();
        // addr=$8003: A[5:0]=3, highBit=0 → chrPage=3
        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank must be 3");
        assert_eq!(mapper.read_chr(0x1FFF), 3, "CHR bank covers full 8 KB");
    }

    #[test]
    fn chr_bank_independent_of_prg_mode() {
        let mut mapper = make_mapper();
        // addr=$9005: O=1 (16KB), A[5:0]=5, highBit=0 → chrPage=5
        mapper.write_prg(0x9005, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank must be 5 regardless of PRG mode"
        );
    }

    #[test]
    fn chr_max_low_bits() {
        let mut mapper = make_mapper();
        // addr=$803F: A[5:0]=0x3F=63, highBit=0 → chrPage=63
        mapper.write_prg(0x803F, 0);
        assert_eq!(mapper.read_chr(0x0000), 63);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn a13_zero_gives_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // A13=0 → Vertical
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "A13=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn a13_one_gives_horizontal_mirroring() {
        let mut mapper = make_mapper();
        // addr=$A000: A13=1 → Horizontal
        mapper.write_prg(0xA000, 0);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "A13=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_changes_independently_of_banking() {
        let mut mapper = make_mapper();
        // addr=$8080: A13=0, prgPage=2, O=0 → lower=2, upper=3, Vertical
        mapper.write_prg(0x8080, 0);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);

        // addr=$A080: A13=1, prgPage=2, O=0 → lower=2, upper=3, Horizontal
        mapper.write_prg(0xA080, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank must not change");
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must change to Horizontal"
        );
    }

    // ── Data byte ignored ─────────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8080, 0x00);
        let bank_a = mapper.read_prg(0x8000);
        mapper.write_prg(0x8080, 0xFF);
        let bank_b = mapper.read_prg(0x8000);
        assert_eq!(
            bank_a, bank_b,
            "Data byte must be ignored; only address matters"
        );
    }

    // ── Writes below $8000 have no effect ────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_affect_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes below $8000 must not affect PRG banking"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes below $8000 must not affect CHR banking"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 225 must never assert IRQ");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
        assert_eq!(caps.max_prg_ram_kb, 0, "Must have no PRG-RAM");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        // Change state: 16 KB mode, bank 10, Horizontal
        mapper.write_prg(0xA280, 0); // A13=1→H, A12=0→32KB... let me use 0xB280 (A12=1)
        // Actually just set to some non-default state
        mapper.write_prg(0xD040, 0); // A14=1,A13=0,A12=1(O=1),prgPage=64|1=65
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "PRG $C000 must be bank 1 after reset (32 KB mode)"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper225::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM present"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_round_trips_32k_mode() {
        let mut mapper = make_mapper();
        // addr=$A080: A13=1 (Horizontal), A12=0 (32KB), prgPage=2, chrPage=0
        mapper.write_prg(0xA080, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored $8000 must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored $C000 must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR must match"
        );
        assert_eq!(
            restored.base().mirroring(),
            mapper.base().mirroring(),
            "Restored mirroring must match"
        );
    }

    #[test]
    fn snapshot_round_trips_16k_mode() {
        let mut mapper = make_mapper();
        // addr=$9040: A12=1 (16KB), prgPage=1, Vertical
        mapper.write_prg(0x9040, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored $8000 must match in 16 KB mode"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored $C000 must match in 16 KB mode"
        );
        assert_eq!(
            restored.base().mirroring(),
            mapper.base().mirroring(),
            "Restored mirroring must match"
        );
    }
}
