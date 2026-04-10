//! Mapper 188 – Bandai Karaoke Studio
//!
//! Known Limitations:
//! - Microphone ADC input is not emulated (always returns "not pressed / no signal").

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 188;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;
/// Number of internal ROM 16 KiB banks (128 KiB cartridge ROM = 8 banks)
const INTERNAL_ROM_BANKS: i16 = 8;
/// Fixed page (last internal bank)
const LAST_INTERNAL_BANK: i16 = INTERNAL_ROM_BANKS - 1;

/// Mapper 188 – Bandai Karaoke Studio (Karaoke Studio cartridge)
///
/// Hardware: Bandai FCG-1 / FCG-2 based Karaoke cartridge.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_188>
/// - PRG-ROM: Up to 256 KiB internal + 256 KiB on expansion cartridge (32 × 16 KiB total)
/// - PRG-ROM page at $8000–$BFFF: switchable 16 KiB bank
/// - PRG-ROM page at $C000–$FFFF: fixed to last bank of internal ROM (bank 7)
/// - CHR: 8 KiB CHR-RAM, fixed at bank 0
/// - Mirroring: software-controlled Vertical / Horizontal
/// - Bus conflicts: yes
/// - $6000–$7FFF: read returns microphone / button status (not writable)
///
/// Register layout (write to $8000–$FFFF):
///
/// ```text
/// 7  bit  0
/// ---- ----
/// .LXR BBBB
/// ||||||||
/// ||||++++— PRG bank select for $8000–$BFFF (3 bits used: B2..B0)
/// |||+————— R: 0 = external ROM chip, 1 = internal ROM chip
/// ||+—————— X: CIRAM A10 — 0 = PPU A10 (Vertical), 1 = PPU A11 (Horizontal)
/// |+——————— L: 1-bit latch, present but unused
/// +———————— Unused
/// ```
///
/// Microphone register ($6000–$7FFF, read-only):
///
/// ```text
/// 7  bit  0
/// ---- ----
/// xxxx xMBA
///       |||
///       ||+— A button: 0 = pressed
///       |+—— B button: 0 = pressed
///       +——— Microphone ADC input
/// ```
///
/// This emulator returns 0x03 (A + B not pressed, no microphone signal).
pub struct Mapper188 {
    base: BaseMapper,
    prg_bank: u8,
    internal_selected: bool,
    mirroring_horizontal: bool,
    has_expansion_rom: bool,
}

impl Mapper188 {
    pub fn new(ctx: MapperContext) -> Self {
        let has_expansion_rom = ctx.prg_rom.len() >= 2 * 128 * 1024;
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_RAM_SIZE / 1024,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        base.set_bus_conflicts(true);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            internal_selected: true,
            mirroring_horizontal: false,
            has_expansion_rom,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let bank_8000 = if self.internal_selected {
            self.prg_bank as i16
        } else if self.has_expansion_rom {
            (self.prg_bank as i16) | INTERNAL_ROM_BANKS
        } else {
            // No expansion ROM — $8000-$BFFF is open bus; use a dummy mapping (intercepted in read)
            0
        };
        self.base.select_prg_page(0, bank_8000);
        self.base.select_prg_page(1, LAST_INTERNAL_BANK);
        self.base.set_mirroring_hv(self.mirroring_horizontal);
    }

    /// Returns true when reads from $8000–$BFFF should return open bus.
    fn is_8000_open_bus(&self) -> bool {
        !self.internal_selected && !self.has_expansion_rom
    }
}

impl Mapper for Mapper188 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => 0x03,
            0x8000..=0xBFFF if self.is_8000_open_bus() => 0,
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => 0x03,
            0x8000..=0xBFFF if self.is_8000_open_bus() => open_bus,
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            let effective = self.base.apply_bus_conflict(addr, value);
            self.prg_bank = effective & 0x07;
            self.internal_selected = (effective & 0x10) != 0;
            self.mirroring_horizontal = (effective & 0x20) != 0;
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank,
            self.internal_selected as u8,
            self.mirroring_horizontal as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        self.prg_bank = data[0] & 0x07;
        self.internal_selected = data[1] != 0;
        self.mirroring_horizontal = data[2] != 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};

    /// Bank ID offset within a 16 KiB bank used for identification in reads.
    const BANK_ID_OFFSET: u16 = 0x100;

    /// Build a PRG-ROM with all bytes set to 0xFF (so bus-conflict AND is transparent).
    /// The bank index (mod 256) is stored at offset BANK_ID_OFFSET within each bank.
    fn make_prg_rom(bank_count: usize) -> Vec<u8> {
        let mut rom = vec![0xFF_u8; PRG_BANK_SIZE * bank_count];
        for bank in 0..bank_count {
            rom[bank * PRG_BANK_SIZE + BANK_ID_OFFSET as usize] = bank as u8;
        }
        rom
    }

    fn make_internal_only_mapper() -> Mapper188 {
        let ctx = MapperContext::new_for_test(
            MAPPER_NUMBER,
            make_prg_rom(8),
            vec![],
            NametableLayout::Vertical,
        )
        .with_prg_ram_banks(0);
        Mapper188::new(ctx)
    }

    fn make_with_expansion_mapper() -> Mapper188 {
        // 256 KiB ROM: 8 banks internal + 8 banks expansion
        let ctx = MapperContext::new_for_test(
            MAPPER_NUMBER,
            make_prg_rom(16),
            vec![],
            NametableLayout::Vertical,
        )
        .with_prg_ram_banks(0);
        Mapper188::new(ctx)
    }

    /// Read bank identifier from a 16 KiB window at window_base.
    fn read_bank_id(mapper: &Mapper188, window_base: u16) -> u8 {
        mapper.read_prg(window_base + BANK_ID_OFFSET)
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_188_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            make_prg_rom(8),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 188 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_page0_is_bank0_internal() {
        let mapper = make_internal_only_mapper();
        assert_eq!(read_bank_id(&mapper, 0x8000), 0, "$8000 must map to bank 0");
    }

    #[test]
    fn power_on_page1_fixed_to_last_internal_bank() {
        let mapper = make_internal_only_mapper();
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "$C000 must map to bank 7 (last internal)"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_internal_only_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Internal ROM bank switching ───────────────────────────────────────────

    #[test]
    fn write_selects_internal_bank() {
        let mut mapper = make_internal_only_mapper();
        // bit4=1 = internal ROM, bits2:0 = bank 3; all bytes are 0xFF so bus conflict is transparent
        mapper.write_prg(0x8000, 0x13);
        assert_eq!(read_bank_id(&mapper, 0x8000), 3);
    }

    #[test]
    fn page1_stays_fixed_after_bank_switch() {
        let mut mapper = make_internal_only_mapper();
        mapper.write_prg(0x8000, 0x15); // bank 5, internal
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "page 1 must stay at last internal bank"
        );
    }

    #[test]
    fn internal_banks_0_through_7_accessible() {
        let mut mapper = make_internal_only_mapper();
        for bank in 0u8..8 {
            mapper.write_prg(0x8000, 0x10 | bank); // bit4=1 = internal
            assert_eq!(
                read_bank_id(&mapper, 0x8000),
                bank,
                "internal bank {bank} not accessible"
            );
        }
    }

    // ── External ROM ──────────────────────────────────────────────────────────

    #[test]
    fn external_bank_selection_with_expansion_rom() {
        let mut mapper = make_with_expansion_mapper();
        // bit4=0 = external ROM, bits2:0 = bank 2 → absolute bank 10 (8+2)
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(read_bank_id(&mapper, 0x8000), 10);
    }

    #[test]
    fn page1_fixed_to_internal_even_with_expansion_selected() {
        let mut mapper = make_with_expansion_mapper();
        mapper.write_prg(0x8000, 0x03); // external, bank 3
        assert_eq!(
            read_bank_id(&mapper, 0xC000),
            7,
            "page 1 must remain at last internal bank"
        );
    }

    #[test]
    fn external_open_bus_without_expansion_rom() {
        let mut mapper = make_internal_only_mapper();
        // Select external ROM (bit4=0) but no expansion ROM present
        mapper.write_prg(0x8000, 0x00);
        // read_prg_open_bus must return open_bus value
        assert_eq!(mapper.read_prg_open_bus(0x8000, 0xAB), 0xAB);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn write_bit5_set_selects_horizontal_mirroring() {
        let mut mapper = make_internal_only_mapper();
        // bit5=1 → horizontal, bit4=1 → internal
        mapper.write_prg(0x8000, 0x30);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_bit5_clear_selects_vertical_mirroring() {
        let mut mapper = make_internal_only_mapper();
        mapper.write_prg(0x8000, 0x30); // set horizontal first
        mapper.write_prg(0x8000, 0x10); // clear bit5 → vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Bus conflicts ─────────────────────────────────────────────────────────

    #[test]
    fn bus_conflict_masks_write_value_with_rom() {
        // Build ROM where first byte of bank 0 is 0x07 (not 0xFF)
        let mut prg = make_prg_rom(8);
        prg[0] = 0x07; // $8000 in bank 0 has 0x07
        let ctx =
            MapperContext::new_for_test(MAPPER_NUMBER, prg, vec![], NametableLayout::Vertical)
                .with_prg_ram_banks(0);
        let mut mapper = Mapper188::new(ctx);
        // Write 0xFF; bus conflict: 0xFF & 0x07 = 0x07; bit4=0 → external; bits2:0 = 7
        // No expansion ROM → open bus for $8000-$BFFF
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_prg_open_bus(0x8000, 0xAB),
            0xAB,
            "bus conflict should yield external mode"
        );
    }

    // ── Microphone register ───────────────────────────────────────────────────

    #[test]
    fn read_6000_returns_microphone_idle() {
        let mapper = make_internal_only_mapper();
        assert_eq!(mapper.read_prg(0x6000), 0x03, "A+B not pressed, no mic");
        assert_eq!(mapper.read_prg(0x7FFF), 0x03);
    }

    #[test]
    fn open_bus_read_6000_returns_microphone_not_open_bus() {
        let mapper = make_internal_only_mapper();
        // Microphone register overrides open bus
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0xDE), 0x03);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_internal_only_mapper();
        mapper.write_prg(0x8000, 0x35); // bit5=1 horizontal, bit4=1 internal, bank 5
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_internal_only_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(
            read_bank_id(&restored, 0x8000),
            read_bank_id(&mapper, 0x8000)
        );
        assert_eq!(
            read_bank_id(&restored, 0xC000),
            read_bank_id(&mapper, 0xC000)
        );
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }

    #[test]
    fn restore_with_too_short_data_is_ignored() {
        let mut mapper = make_internal_only_mapper();
        mapper.write_prg(0x8000, 0x15); // bank 5, internal
        let bank_before = read_bank_id(&mapper, 0x8000);
        mapper.restore_registers(&[0x02]); // too short
        assert_eq!(
            read_bank_id(&mapper, 0x8000),
            bank_before,
            "state must be unchanged after short restore"
        );
    }
}
