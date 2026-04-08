//! Mapper 108 — Whirlwind Manu / Bubble Bobble FDS conversion (iNES "Bb")
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_108>
//! - Mesen2 reference: `Core/NES/Mappers/Unlicensed/Bb.h`
//!
//! Memory map:
//! - CPU `$6000-$7FFF`: Switchable 8 KB PRG-ROM bank (controlled by the PRG register)
//! - CPU `$8000-$FFFF`: Fixed last 32 KB PRG-ROM (4 × 8 KB banks at indices -4, -3, -2, -1)
//! - PPU `$0000-$1FFF`: Switchable 8 KB CHR bank (CHR-ROM or CHR-RAM)
//!
//! Register write decoding (all writes to `$8000-$FFFF`):
//! - If `(addr & 0x9000) == 0x8000` **or** `addr >= $F000`:
//!   both the PRG bank (`$6000-$7FFF`) and the CHR bank (`$0000-$1FFF`) are set to `value`.
//!   - This covers `$8000-$8FFF`, `$A000-$AFFF`, `$C000-$CFFF`, `$E000-$EFFF`, `$F000-$FFFF`.
//!   - The `$F000-$FFFF` overlap handles the version of Bubble Bobble that writes only there.
//! - Otherwise (bit 12 set, `$9000-$9FFF`, `$B000-$BFFF`, `$D000-$DFFF`):
//!   only the CHR bank is set to `value & 0x01` (ProWres compatibility).
//!
//! Mirroring: hard-wired from the iNES header; no software control.
//!
//! Known Limitations:
//! - No IRQ or expansion audio behavior; the board does not expose either.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 108;
const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper108 {
    base: BaseMapper,
    /// Current 8 KB PRG bank index mapped to `$6000-$7FFF`.
    /// Stored as `u8`; wraps modulo the available bank count.
    /// Initialized to `0xFF` so that typical power-of-two ROM sizes map to the last bank.
    prg_reg: u8,
    /// Current 8 KB CHR bank index mapped to `$0000-$1FFF`.
    chr_reg: u8,
}

impl Mapper108 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        base.configure_prg_6000_banking();

        let mut mapper = Self {
            base,
            prg_reg: 0xFF,
            chr_reg: 0,
        };
        mapper.apply_state();
        mapper
    }

    /// Apply the current register values to the banking hardware.
    fn apply_state(&mut self) {
        // $8000-$FFFF: fixed last 32 KB (four 8 KB banks, counting from the end).
        self.base.select_prg_page(0, -4);
        self.base.select_prg_page(1, -3);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // $6000-$7FFF: switchable 8 KB PRG bank.
        self.base.select_prg_6000_page(self.prg_reg as i16);

        // $0000-$1FFF: switchable 8 KB CHR bank.
        self.base.select_chr_page(0, self.chr_reg as i16);
    }
}

impl Mapper for Mapper108 {
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

        if addr < 0x8000 {
            return;
        }

        // Decode per the Bb board: when bit 15 is set and bit 12 is clear, or the
        // address falls in $F000-$FFFF, both PRG and CHR banks are updated.
        // Otherwise only the CHR bank is updated (bit 0 only — ProWres compatibility).
        if (addr & 0x9000) == 0x8000 || addr >= 0xF000 {
            self.prg_reg = value;
            self.chr_reg = value;
        } else {
            self.chr_reg = value & 0x01;
        }

        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg, self.chr_reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_reg = data[0];
            self.chr_reg = data[1];
            self.apply_state();
        } else if let Some(&v) = data.first() {
            self.prg_reg = v;
            self.chr_reg = v;
            self.apply_state();
        }
    }

    fn reset(&mut self) {
        self.prg_reg = 0xFF;
        self.chr_reg = 0;
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use a power-of-two bank count so 0xFF wraps to the last bank, matching
    // Mesen's initialisation behaviour for typical Bubble Bobble ROM sizes.
    const TEST_PRG_BANKS_8K: usize = 8; // 64 KB
    const TEST_CHR_BANKS_8K: usize = 8; // 64 KB

    fn make_mapper() -> Mapper108 {
        Mapper108::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_108_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE, TEST_CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 108 must be creatable via factory");
    }

    // ── Reset / power-on state ────────────────────────────────────────────────

    #[test]
    fn reset_maps_fixed_prg_rom_to_last_32kb_at_8000_ffff() {
        let mapper = make_mapper();

        // $8000-$9FFF → bank 4, $A000-$BFFF → bank 5,
        // $C000-$DFFF → bank 6, $E000-$FFFF → bank 7
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should read bank 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should read bank 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 should read bank 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should read bank 7");
    }

    #[test]
    fn reset_maps_prg_6000_to_last_bank_for_power_of_two_rom() {
        let mapper = make_mapper();
        // 0xFF % 8 = 7, so the last 8 KB bank is mapped to $6000-$7FFF.
        assert_eq!(
            mapper.read_prg(0x6000),
            7,
            "$6000 should read last bank (7) on reset"
        );
    }

    #[test]
    fn reset_maps_chr_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank 0 should be selected on reset"
        );
    }

    #[test]
    fn default_mirroring_is_preserved_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "header mirroring must be preserved at reset"
        );
    }

    // ── PRG / CHR banking via $8000-range register ($F000 variant — Bubble Bobble) ──

    #[test]
    fn write_to_f000_selects_prg_bank_at_6000() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xF000, 3);
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "bank 3 should be mapped to $6000"
        );

        mapper.write_prg(0xFFFF, 5);
        assert_eq!(
            mapper.read_prg(0x7FFF),
            5,
            "bank 5 should be mapped to $7FFF"
        );
    }

    #[test]
    fn write_to_f000_also_selects_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xF000, 2);
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank 2 should be selected");
    }

    #[test]
    fn write_to_8000_selects_prg_and_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 1);
        assert_eq!(
            mapper.read_prg(0x6000),
            1,
            "PRG bank 1 at $6000 after $8000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank 1 after $8000 write");
    }

    #[test]
    fn write_to_a000_selects_prg_and_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 2);
        assert_eq!(
            mapper.read_prg(0x6000),
            2,
            "PRG bank 2 at $6000 after $A000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank 2 after $A000 write");
    }

    #[test]
    fn write_to_c000_selects_prg_and_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xC000, 3);
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "PRG bank 3 at $6000 after $C000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank 3 after $C000 write");
    }

    #[test]
    fn write_to_e000_selects_prg_and_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xE000, 4);
        assert_eq!(
            mapper.read_prg(0x6000),
            4,
            "PRG bank 4 at $6000 after $E000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR bank 4 after $E000 write");
    }

    // ── CHR-only banking via odd-page addresses (ProWres compatibility) ───────

    #[test]
    fn write_to_9000_updates_only_chr_and_masks_to_bit_0() {
        let mut mapper = make_mapper();

        // Set a known PRG bank first
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x6000), 3, "precondition: PRG bank 3");

        // Write to $9000 with value 0x05 → CHR = 0x05 & 0x01 = 1; PRG unchanged
        mapper.write_prg(0x9000, 0x05);
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "PRG bank must not change on $9000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank must be value & 0x01");
    }

    #[test]
    fn write_to_b000_updates_only_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xB000, 0x03); // CHR = 0x03 & 0x01 = 1; PRG = 2
        assert_eq!(
            mapper.read_prg(0x6000),
            2,
            "PRG must be unchanged after $B000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR must be 0x03 & 0x01 = 1");
    }

    #[test]
    fn write_to_d000_updates_only_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xD000, 0x02); // CHR = 0x02 & 0x01 = 0; PRG = 5
        assert_eq!(
            mapper.read_prg(0x6000),
            5,
            "PRG must be unchanged after $D000 write"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be 0x02 & 0x01 = 0");
    }

    // ── Fixed $8000-$FFFF region is unaffected by register writes ────────────

    #[test]
    fn fixed_prg_window_at_8000_does_not_change_after_register_write() {
        let mut mapper = make_mapper();
        let before = (
            mapper.read_prg(0x8000),
            mapper.read_prg(0xA000),
            mapper.read_prg(0xC000),
            mapper.read_prg(0xE000),
        );

        mapper.write_prg(0xF000, 0);

        assert_eq!(
            mapper.read_prg(0x8000),
            before.0,
            "bank at $8000 must be fixed"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            before.1,
            "bank at $A000 must be fixed"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            before.2,
            "bank at $C000 must be fixed"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            before.3,
            "bank at $E000 must be fixed"
        );
    }

    // ── Mirroring is hard-wired ───────────────────────────────────────────────

    #[test]
    fn mirroring_is_unchanged_after_register_writes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xF000, 0xFF);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x9000, 0x01);

        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring must remain hard-wired to the header value"
        );
    }

    // ── Soft reset ────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_initial_prg_and_chr_banks() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xF000, 2);
        assert_eq!(mapper.read_prg(0x6000), 2, "precondition: PRG bank 2");
        assert_eq!(mapper.read_chr(0x0000), 2, "precondition: CHR bank 2");

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x6000),
            7,
            "reset must restore PRG bank 7 (last) at $6000"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "reset must restore CHR bank 0");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_round_trips_prg_and_chr_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 5);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x6000), 5, "restored PRG bank must be 5");
        assert_eq!(restored.read_chr(0x0000), 5, "restored CHR bank must be 5");
    }

    #[test]
    fn snapshot_restore_preserves_independent_prg_and_chr_registers() {
        let mut mapper = make_mapper();
        // Put PRG and CHR into different banks via a $9000 write
        mapper.write_prg(0x8000, 6); // sets prg_reg=6, chr_reg=6
        mapper.write_prg(0x9000, 0x01); // updates chr_reg=1, prg_reg stays 6
        assert_eq!(mapper.read_prg(0x6000), 6, "PRG=6 before snapshot");
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR=1 before snapshot");

        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x6000), 6, "restored PRG must be 6");
        assert_eq!(restored.read_chr(0x0000), 1, "restored CHR must be 1");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_mapper_hardware() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();

        assert!(caps.has_chr_banking, "CHR banking must be reported");
        assert!(!caps.has_irq, "no IRQ on this board");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_dynamic_mirroring, "mirroring is hard-wired");
        assert_eq!(caps.prg_bank_size_kb, 8, "PRG bank size is 8 KB");
        assert_eq!(caps.chr_bank_size_kb, 8, "CHR bank size is 8 KB");
    }
}
