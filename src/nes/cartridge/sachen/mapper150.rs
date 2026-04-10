//! Mapper 150 — Sachen SA-015 / SA-630 (74LS374N)
//!
//! Specifications:
//! - NESdev wiki: <https://www.nesdev.org/wiki/INES_Mapper_150>
//!
//! Memory map:
//! - CPU `$8000-$FFFF`: 32 KiB switchable PRG-ROM bank (R5[1:0])
//! - PPU `$0000-$1FFF`: 8 KiB switchable CHR-ROM bank
//!
//! Register interface (same ASIC as mapper 243 / SA-020A, but wired differently):
//! - **Index** (`addr & $C101 == $4100`, write): bits 2:0 select register R0–R7
//! - **Data**  (`addr & $C101 == $4101`, read/write): bits 2:0 are the register value
//!
//! Register assignments:
//! - R4[0] → CHR A15 (bit 2 of the 3-bit CHR bank)
//! - R5[1:0] → PRG A16:A15 (32 KiB bank select, 4 × 32 KiB = 128 KiB PRG)
//! - R6[1:0] → CHR A14:A13 (bits 1:0 of the 3-bit CHR bank)
//! - R7[2:1] → Nametable mirroring (0=S0×3+S1, 1=H, 2=V, 3=SingleScreenA)
//!
//! CHR bank = `(R4[0] << 2) | R6[1:0]`
//!
//! Mirroring mode 0 (`S0-S0-S0-S1`, "vertically-flipped L") is approximated as
//! `SingleScreenLower` because neser has no four-nametable custom arrangement.
//!
//! Known limitations:
//! - Solder-pad DIP switch (Shōgi Gakuen only) is not emulated; the default
//!   (pin 14 connected to CPU D2) behaviour is always used.
//! - No IRQ, no expansion audio.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::ines::NametableLayout;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 150;

pub struct Mapper150 {
    base: BaseMapper,
    /// Currently selected register index (0–7).
    command: u8,
    /// Eight 3-bit registers.
    regs: [u8; 8],
}

impl Mapper150 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            command: 0,
            regs: [0; 8],
        };
        mapper.apply_state();
        mapper
    }

    fn chr_bank(&self) -> usize {
        ((self.regs[4] & 1) as usize) << 2 | (self.regs[6] & 3) as usize
    }

    fn prg_bank(&self) -> usize {
        (self.regs[5] & 3) as usize
    }

    fn apply_state(&mut self) {
        self.base.select_prg_page(0, self.prg_bank() as i16);
        self.base.select_chr_page(0, self.chr_bank() as i16);
        let mirroring = match (self.regs[7] >> 1) & 3 {
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::Vertical,
            3 => NametableLayout::SingleScreenUpper,
            _ => NametableLayout::SingleScreenLower, // mode 0: S0-S0-S0-S1 approximation
        };
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper150 {
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
        // Data register read: return low 3 bits of current register
        if (addr & 0xC101) == 0x4101 {
            return (open_bus & 0xF8) | (self.regs[self.command as usize] & 0x07);
        }
        open_bus
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (addr & 0xC101) == 0x4100 {
            self.command = value & 0x07;
        } else if (addr & 0xC101) == 0x4101 {
            self.regs[self.command as usize] = value & 0x07;
            self.apply_state();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut v = vec![self.command];
        v.extend_from_slice(&self.regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.command = data[0] & 0x07;
            self.regs.copy_from_slice(&data[1..9]);
            for r in &mut self.regs {
                *r &= 0x07;
            }
            self.apply_state();
        }
    }

    fn reset(&mut self) {
        self.command = 0;
        self.regs = [0; 8];
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 128 KiB PRG (4 × 32 KB) + 64 KiB CHR (8 × 8 KB) matches the NESdev spec.
    const PRG_BANKS_32K: usize = 4;
    const CHR_BANKS_8K: usize = 8;

    fn make_mapper() -> Mapper150 {
        Mapper150::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_150_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(32 * 1024, PRG_BANKS_32K),
            banked_data(8 * 1024, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 150 must be creatable via factory");
    }

    // ── Reset / power-on state ────────────────────────────────────────────────

    #[test]
    fn power_on_maps_prg_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 on reset");
    }

    #[test]
    fn power_on_maps_chr_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 on reset");
    }

    // ── PRG banking via R5 ────────────────────────────────────────────────────

    #[test]
    fn r5_selects_prg_bank() {
        let mut mapper = make_mapper();

        // Select register 5, write bank 2
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 2);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank 2 via R5=2");

        // Switch to bank 3
        mapper.write_prg(0x4101, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank 3 via R5=3");
    }

    #[test]
    fn r5_only_uses_low_2_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 0xFF); // only bits 1:0 matter → bank 3
        assert_eq!(mapper.read_prg(0x8000), 3, "R5 masked to 2 bits → bank 3");
    }

    // ── CHR banking via R4 / R6 ───────────────────────────────────────────────

    #[test]
    fn r6_selects_chr_bank_low_bits() {
        let mut mapper = make_mapper();
        // R6[1:0] = CHR A14:A13 (bits 1:0 of CHR bank)
        mapper.write_prg(0x4100, 6);
        mapper.write_prg(0x4101, 3); // bits 1:0 = 3 → CHR bank 3
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank 3 via R6=3");
    }

    #[test]
    fn r4_sets_chr_bank_high_bit() {
        let mut mapper = make_mapper();
        // R4[0] = CHR A15 (bit 2 of CHR bank)
        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 1); // bit 0 set → CHR bank bit 2 set
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR bank bit 2 set via R4[0]=1");
    }

    #[test]
    fn chr_bank_combines_r4_and_r6() {
        let mut mapper = make_mapper();
        // R4[0] = 1, R6[1:0] = 3 → CHR bank = (1<<2) | 3 = 7
        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 1);
        mapper.write_prg(0x4100, 6);
        mapper.write_prg(0x4101, 3);
        assert_eq!(mapper.read_chr(0x0000), 7, "CHR bank 7 = R4[0]=1, R6=3");
    }

    // ── Index register ────────────────────────────────────────────────────────

    #[test]
    fn index_register_uses_only_low_3_bits() {
        let mut mapper = make_mapper();
        // Writing 0xFF to $4100 should select register 7 (low 3 bits of 0xFF = 7)
        mapper.write_prg(0x4100, 0xFF);
        // register 7 → mirroring; just ensure it doesn't panic
        mapper.write_prg(0x4101, 0);
    }

    // ── Data register read-back ───────────────────────────────────────────────

    #[test]
    fn data_register_is_readable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 2);
        // Read back: open_bus=0 → should return 2
        let val = mapper.read_prg_open_bus(0x4101, 0x00);
        assert_eq!(
            val & 0x07,
            2,
            "register read-back must return written value"
        );
    }

    // ── Mirroring via R7 ──────────────────────────────────────────────────────

    #[test]
    fn r7_mode1_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 7);
        mapper.write_prg(0x4101, 0x02); // bits 2:1 = 01 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "R7[2:1]=1 → Horizontal"
        );
    }

    #[test]
    fn r7_mode2_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 7);
        mapper.write_prg(0x4101, 0x04); // bits 2:1 = 10 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "R7[2:1]=2 → Vertical"
        );
    }

    #[test]
    fn r7_mode3_sets_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 7);
        mapper.write_prg(0x4101, 0x06); // bits 2:1 = 11 → SingleScreenA
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "R7[2:1]=3 → SingleScreenUpper"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 3);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_round_trips_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 5);
        mapper.write_prg(0x4101, 2); // PRG bank 2
        mapper.write_prg(0x4100, 4);
        mapper.write_prg(0x4101, 1); // CHR high bit
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 2, "restored PRG bank must be 2");
        assert_eq!(restored.read_chr(0x0000), 4, "restored CHR bank must be 4");
    }
}
