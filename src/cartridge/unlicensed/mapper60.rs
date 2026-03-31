//! Mapper 060 - Reset-based NROM-128 4-in-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_060>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 060 - Reset-based NROM-128 4-in-1
///
/// Hardware: Multicart PCB with reset counter.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_060>
/// - PRG-ROM: 64 KiB total (4 × 16 KiB, one per game)
/// - CHR: 32 KiB total (4 × 8 KiB, one per game)
/// - Mirroring: Fixed from header
///
/// Each of the 4 games is NROM-128:
/// - PRG: 16 KiB bank = game_select (same page at $8000 and $C000)
/// - CHR: 8 KiB bank = game_select
///
/// The game is selected by a 2-bit internal counter that increments on every
/// reset and wraps from 3 back to 0. On hard reset (power-on), the counter
/// is pre-set so that the boot-time reset always selects game 0.
pub struct Mapper60 {
    base: BaseMapper,
    game_select: u8, // 2-bit counter, incremented on reset
}

impl Mapper60 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            has_chr_banking: true,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        // 16KB PRG banking: 2 slots, both start at bank 0
        // NROM-128: same bank at $8000 and $C000
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        Self {
            base,
            game_select: 0,
        }
    }

    fn apply_game_select(&mut self) {
        let bank = self.game_select as i16;
        self.base.select_prg_page(0, bank);
        self.base.select_prg_page(1, bank);
        self.base.select_chr_page(0, bank);
    }
}

impl Mapper for Mapper60 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // No writable registers
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.game_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&v) = data.first() {
            self.game_select = v & 0x03;
            self.apply_game_select();
        }
    }

    /// On reset, advance to the next game (2-bit counter).
    fn reset(&mut self) {
        self.game_select = (self.game_select + 1) & 0x03;
        self.apply_game_select();
    }

    /// On hard reset, pre-set `game_select` to 3 so that the subsequent
    /// `reset()` call during hard reset advances to 0 (power-on game).
    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.game_select = 3;
        self.apply_game_select();
        self.base_mut().initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper60 {
        let prg = banked_data(16 * 1024, 4);
        let chr = banked_data(8 * 1024, 4);
        Mapper60::new(MapperContext::new_for_test(
            60,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_60_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            60,
            banked_data(16 * 1024, 4),
            banked_data(8 * 1024, 4),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 60 must be registered");
    }

    #[test]
    fn mapper_343_is_mapper_60_compatible_alias() {
        let mut mapper = create_mapper(MapperContext::new_for_test(
            343,
            banked_data(16 * 1024, 4),
            banked_data(8 * 1024, 4),
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 343 must be registered as a Mapper 60-compatible alias");

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn default_is_game_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn prg_mirrors_same_bank_at_8000_and_c000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            mapper.read_prg(0xC000),
            "NROM-128: $8000 and $C000 are the same bank"
        );
    }

    #[test]
    fn reset_advances_game_select() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.game_select, 0);
        mapper.reset();
        assert_eq!(mapper.game_select, 1);
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.reset();
        assert_eq!(mapper.game_select, 2);
        mapper.reset();
        assert_eq!(mapper.game_select, 3);
        mapper.reset();
        assert_eq!(mapper.game_select, 0, "counter wraps from 3 to 0");
    }

    #[test]
    fn chr_advances_with_game_select() {
        let mut mapper = make_mapper();
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 1);
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn registers_snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.reset();
        mapper.reset(); // game_select = 2
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), 2);
        assert_eq!(r.read_chr(0x0000), 2);
    }

    #[test]
    fn hard_reset_starts_at_game_0() {
        let mut mapper = make_mapper();
        mapper.reset(); // reset -> game 1
        mapper.reset(); // reset -> game 2
        // Simulate hard reset: initialize_ram then reset
        mapper.initialize_ram(crate::console::RamInitMode::Zero);
        mapper.reset();
        assert_eq!(mapper.game_select, 0, "hard reset must start at game 0");
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }
}
