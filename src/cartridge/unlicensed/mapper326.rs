//! Mapper 326 – bootleg Contra/Gryzor board
//!
//! Specifications:
//! - NesDev wiki: <https://www.nesdev.org/wiki/NES_2.0_Mapper_326>  (Cloudflare 403 at build
//!   time; archived copy from 2024-12-09 consulted)
//! - Primary source: NesDev forum post by krzysiobal (2018-05-18), linked from the wiki.
//!   URL: <https://forums.nesdev.org/viewtopic.php?f=9&t=17352&p=218722#p218722>
//! - Mesen2 `MapperFactory.cpp`: no `case 326` entry (mapper not implemented).
//! - FCEUX: no board-specific `.cpp` found for mapper 326.
//!
//! # Hardware overview
//!
//! Used for a bootleg version of _Contra_ (_Gryzor_).
//! The board provides:
//! - 128 KiB PRG-ROM in four 8 KiB windows ($8000/$A000/$C000 switchable, $E000 fixed last)
//! - 128 KiB CHR-ROM in eight 1 KiB windows ($0000–$1C00, each independently switchable)
//! - Per-nametable mirroring: each of the four NES nametable slots ($2000/$2400/$2800/$2C00)
//!   independently selects VRAM page 0 or 1 (one bit per nametable).
//! - No bus conflicts, no IRQ, no PRG-RAM.
//!
//! # Register map
//!
//! All writes are to $8000–$FFFF.  Two orthogonal register families share that space
//! and are distinguished by address bit A4:
//!
//! ## PRG bank registers (A4 = 0, mask $E010)
//!
//! | `addr & $E010` | Effect                                |
//! |----------------|---------------------------------------|
//! | `$8000`        | Set PRG bank for $8000–$9FFF (slot 0) |
//! | `$A000`        | Set PRG bank for $A000–$BFFF (slot 1) |
//! | `$C000`        | Set PRG bank for $C000–$DFFF (slot 2) |
//! | `$E000`        | (fixed last bank; writes ignored)     |
//!
//! The data byte selects an 8 KiB PRG-ROM bank (0–15 for 128 KiB ROM).
//! Bits above the valid bank range are ignored (automatic wrapping).
//!
//! ## CHR bank and mirroring registers (A4 = 1, mask $801F)
//!
//! | `addr & $801F` | Effect                               |
//! |----------------|--------------------------------------|
//! | `$8010`        | Set CHR bank for slot 0 ($0000–$03FF)|
//! | `$8011`        | Set CHR bank for slot 1 ($0400–$07FF)|
//! | `$8012`        | Set CHR bank for slot 2 ($0800–$0BFF)|
//! | `$8013`        | Set CHR bank for slot 3 ($0C00–$0FFF)|
//! | `$8014`        | Set CHR bank for slot 4 ($1000–$13FF)|
//! | `$8015`        | Set CHR bank for slot 5 ($1400–$17FF)|
//! | `$8016`        | Set CHR bank for slot 6 ($1800–$1BFF)|
//! | `$8017`        | Set CHR bank for slot 7 ($1C00–$1FFF)|
//! | `$8018`        | NT0 ($2000) VRAM page: `data & 1`    |
//! | `$8019`        | NT1 ($2400) VRAM page: `data & 1`    |
//! | `$801A`        | NT2 ($2800) VRAM page: `data & 1`    |
//! | `$801B`        | NT3 ($2C00) VRAM page: `data & 1`    |
//!
//! The game writes to non-canonical addresses (e.g., $F018, $C003) — the masks ensure
//! these still hit the correct registers.
//!
//! # Known limitations
//! - Per-nametable VRAM control is translated to the nearest standard `NametableLayout`
//!   (Horizontal, Vertical, SingleScreenLower, SingleScreenUpper).  Exotic combinations
//!   where adjacent nametables select different pages are mapped to Horizontal as a
//!   fallback.  The only known ROM uses horizontal mirroring, so this is non-limiting.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Number of independently switchable PRG windows (excluding the fixed last slot).
const PRG_SWITCHABLE_SLOTS: usize = 3;

/// Number of independently switchable CHR 1 KiB windows.
const CHR_SLOTS: usize = 8;

/// Mapper 326 – bootleg Contra/Gryzor
///
/// See module-level documentation for the full hardware description.
pub struct Mapper326 {
    base: BaseMapper,
    /// Bank index for each of the three switchable PRG slots.
    /// Slot 3 ($E000–$FFFF) is always fixed to the last bank.
    prg_banks: [u8; PRG_SWITCHABLE_SLOTS],
    /// Bank index for each of the eight 1 KiB CHR windows.
    chr_banks: [u8; CHR_SLOTS],
    /// VRAM-page select bit (0 or 1) for each of the four nametable slots.
    nt_banks: [u8; 4],
}

impl Mapper326 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
        let mut mapper = Self {
            base,
            prg_banks: [0; PRG_SWITCHABLE_SLOTS],
            chr_banks: [0; CHR_SLOTS],
            nt_banks: [0; 4],
        };
        mapper.sync_banks();
        mapper
    }

    /// Push all bank registers into `BaseMapper`.
    fn sync_banks(&mut self) {
        for (slot, &bank) in self.prg_banks.iter().enumerate() {
            self.base.select_prg_page(slot, bank as i16);
        }
        // Slot 3 is always the last 8 KiB bank.
        self.base.select_prg_page(3, -1);

        for (slot, &bank) in self.chr_banks.iter().enumerate() {
            self.base.select_chr_page(slot, bank as i16);
        }

        self.base.set_mirroring(self.derive_mirroring());
    }

    /// Translate the four per-nametable VRAM-page bits to the nearest standard
    /// `NametableLayout`.
    fn derive_mirroring(&self) -> NametableLayout {
        match self.nt_banks {
            [0, 0, 0, 0] => NametableLayout::SingleScreenLower,
            [1, 1, 1, 1] => NametableLayout::SingleScreenUpper,
            [0, 0, 1, 1] => NametableLayout::Horizontal,
            [0, 1, 0, 1] => NametableLayout::Vertical,
            _ => NametableLayout::Horizontal,
        }
    }
}

impl Mapper for Mapper326 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        if addr & 0x0010 == 0 {
            // A4 = 0 → PRG bank register.  Slot is bits 14:13.
            let slot = ((addr >> 13) & 3) as usize;
            if slot < PRG_SWITCHABLE_SLOTS {
                self.prg_banks[slot] = value;
                self.sync_banks();
            }
            // slot == 3 → fixed last bank; writes ignored.
        } else if addr & 0x0008 == 0 {
            // A4 = 1, A3 = 0 → CHR bank register.  Slot is bits 2:0.
            let slot = (addr & 0x0007) as usize;
            self.chr_banks[slot] = value;
            self.sync_banks();
        } else {
            // A4 = 1, A3 = 1 → nametable page register.  NT index is bits 1:0.
            let nt = (addr & 0x0003) as usize;
            self.nt_banks[nt] = value & 1;
            self.sync_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(PRG_SWITCHABLE_SLOTS + CHR_SLOTS + 4);
        snap.extend_from_slice(&self.prg_banks);
        snap.extend_from_slice(&self.chr_banks);
        snap.extend_from_slice(&self.nt_banks);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected = PRG_SWITCHABLE_SLOTS + CHR_SLOTS + 4;
        if data.len() != expected {
            return;
        }
        self.prg_banks
            .copy_from_slice(&data[..PRG_SWITCHABLE_SLOTS]);
        self.chr_banks
            .copy_from_slice(&data[PRG_SWITCHABLE_SLOTS..PRG_SWITCHABLE_SLOTS + CHR_SLOTS]);
        self.nt_banks
            .copy_from_slice(&data[PRG_SWITCHABLE_SLOTS + CHR_SLOTS..]);
        self.sync_banks();
    }

    fn reset(&mut self) {
        self.prg_banks = [0; PRG_SWITCHABLE_SLOTS];
        self.chr_banks = [0; CHR_SLOTS];
        self.nt_banks = [0; 4];
        self.sync_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 6; // 6 × 8 KiB = 48 KiB PRG
    const CHR_BANKS: usize = 9; // 9 × 1 KiB  = 9 KiB CHR

    fn make_mapper() -> Mapper326 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        Mapper326::new(MapperContext::new_for_test(
            326,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // ─── Registration ───────────────────────────────────────────────────────

    #[test]
    fn mapper_326_is_registered() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            326,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 326 must be registered in the factory"
        );
    }

    // ─── Power-on PRG state ─────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_a000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "$A000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_e000_reads_last_bank() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        // Last bank = bank (PRG_BANKS-1), filled with byte (PRG_BANKS-1).
        let expected = (PRG_BANKS - 1) as u8;
        let mapper = Mapper326::new(MapperContext::new_for_test(
            326,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_prg(0xE000),
            expected,
            "$E000 must map to the last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_prg_ffff_reads_last_bank_last_byte() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let last_bank_offset = (PRG_BANKS - 1) * 8 * 1024 + (8 * 1024 - 1);
        let expected = prg[last_bank_offset];
        let mapper = Mapper326::new(MapperContext::new_for_test(
            326,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_prg(0xFFFF),
            expected,
            "$FFFF must read last byte of last PRG bank"
        );
    }

    // ─── PRG bank switching ─────────────────────────────────────────────────

    #[test]
    fn write_prg_8000_selects_bank_for_8000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 2); // select bank 2 for $8000-$9FFF
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must map to bank 2 after write to $8000"
        );
    }

    #[test]
    fn write_prg_a000_selects_bank_for_a000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 3); // select bank 3 for $A000-$BFFF
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "$A000 must map to bank 3 after write to $A000"
        );
    }

    #[test]
    fn write_prg_c000_selects_bank_for_c000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 4); // select bank 4 for $C000-$DFFF
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must map to bank 4 after write to $C000"
        );
    }

    #[test]
    fn write_prg_e000_does_not_change_e000_window() {
        // $E000 slot is fixed to the last bank; writes must be ignored.
        let mut mapper = make_mapper();
        let before = mapper.read_prg(0xE000);
        mapper.write_prg(0xE000, 0); // attempt to switch fixed slot
        assert_eq!(
            mapper.read_prg(0xE000),
            before,
            "$E000 window must remain at last bank after write"
        );
    }

    #[test]
    fn prg_slot_0_independent_from_slot_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 3);
        assert_eq!(mapper.read_prg(0x8000), 2, "slot 0 must be bank 2");
        assert_eq!(mapper.read_prg(0xA000), 3, "slot 1 must be bank 3");
    }

    #[test]
    fn prg_mask_e010_canonical_address_c003_hits_slot_2() {
        // $C003 & $E010 == $C000 → PRG slot 2 register (addr bit A4=0, bits 14:13 = 0b10).
        let mut mapper = make_mapper();
        mapper.write_prg(0xC003, 5);
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "non-canonical address $C003 must still write PRG slot 2"
        );
    }

    // ─── Power-on CHR state ─────────────────────────────────────────────────

    #[test]
    fn power_on_chr_0000_reads_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "$0000 must map to CHR bank 0");
    }

    #[test]
    fn power_on_chr_1c00_reads_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x1C00), 0, "$1C00 must map to CHR bank 0");
    }

    // ─── CHR bank switching ─────────────────────────────────────────────────

    #[test]
    fn write_8010_selects_chr_slot_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 3); // CHR slot 0 → bank 3
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR slot 0 must map to bank 3 after write to $8010"
        );
    }

    #[test]
    fn write_8017_selects_chr_slot_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8017, 5); // CHR slot 7 → bank 5
        assert_eq!(
            mapper.read_chr(0x1C00),
            5,
            "CHR slot 7 must map to bank 5 after write to $8017"
        );
    }

    #[test]
    fn chr_slots_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 1);
        mapper.write_prg(0x8011, 2);
        mapper.write_prg(0x8012, 3);
        assert_eq!(mapper.read_chr(0x0000), 1, "slot 0 must be bank 1");
        assert_eq!(mapper.read_chr(0x0400), 2, "slot 1 must be bank 2");
        assert_eq!(mapper.read_chr(0x0800), 3, "slot 2 must be bank 3");
    }

    #[test]
    fn chr_mask_801f_non_canonical_a010_hits_chr_slot_0() {
        // $A010 & $801F = $8010 → CHR slot 0 (A4=1, A3=0, addr[2:0]=0).
        let mut mapper = make_mapper();
        mapper.write_prg(0xA010, 4);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "non-canonical address $A010 must write CHR slot 0"
        );
    }

    #[test]
    fn chr_mask_801f_non_canonical_f018_hits_mirroring_nt0() {
        // $F018 & $801F = $8018 → NT0 mirror register.
        let mut mapper = make_mapper();
        mapper.write_prg(0xF018, 1); // NT0 → page 1
        // After NT0=1, NT1=0, NT2=0, NT3=0 → no standard match → Horizontal fallback.
        // But we can verify the nt_banks[0] was set via the registers snapshot.
        let snap = mapper.registers_snapshot();
        // snap = [prg0,prg1,prg2, chr0..chr7, nt0,nt1,nt2,nt3]
        assert_eq!(
            snap[PRG_SWITCHABLE_SLOTS + CHR_SLOTS],
            1,
            "NT0 bank must be 1 after $F018 write"
        );
    }

    // ─── Mirroring control ──────────────────────────────────────────────────

    #[test]
    fn all_nt_banks_zero_gives_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8018, 0);
        mapper.write_prg(0x8019, 0);
        mapper.write_prg(0x801A, 0);
        mapper.write_prg(0x801B, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn all_nt_banks_one_gives_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8018, 1);
        mapper.write_prg(0x8019, 1);
        mapper.write_prg(0x801A, 1);
        mapper.write_prg(0x801B, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn nt_banks_0011_gives_horizontal_mirroring() {
        // NT0=0, NT1=0, NT2=1, NT3=1 → Horizontal
        let mut mapper = make_mapper();
        mapper.write_prg(0x8018, 0);
        mapper.write_prg(0x8019, 0);
        mapper.write_prg(0x801A, 1);
        mapper.write_prg(0x801B, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn nt_banks_0101_gives_vertical_mirroring() {
        // NT0=0, NT1=1, NT2=0, NT3=1 → Vertical
        let mut mapper = make_mapper();
        mapper.write_prg(0x8018, 0);
        mapper.write_prg(0x8019, 1);
        mapper.write_prg(0x801A, 0);
        mapper.write_prg(0x801B, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit_mask_only_bit_0_matters() {
        // Writing data=0xFF → bit 0 = 1; data=0xFE → bit 0 = 0.
        let mut mapper = make_mapper();
        mapper.write_prg(0x8018, 0xFF); // NT0 page = 0xFF & 1 = 1
        mapper.write_prg(0x8019, 0xFE); // NT1 page = 0xFE & 1 = 0
        mapper.write_prg(0x801A, 0xFF); // NT2 page = 0xFF & 1 = 1
        mapper.write_prg(0x801B, 0xFE); // NT3 page = 0xFE & 1 = 0
        // [1,0,1,0] = no standard match → Horizontal fallback
        // Mainly verifies masking; check nt_banks via snapshot.
        let snap = mapper.registers_snapshot();
        let nt = &snap[PRG_SWITCHABLE_SLOTS + CHR_SLOTS..];
        assert_eq!(
            nt,
            &[1, 0, 1, 0],
            "NT bank bits must be masked to bit 0 only"
        );
    }

    // ─── Snapshot / restore ─────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_has_correct_length() {
        let mapper = make_mapper();
        let snap = mapper.registers_snapshot();
        assert_eq!(
            snap.len(),
            PRG_SWITCHABLE_SLOTS + CHR_SLOTS + 4,
            "snapshot must contain all bank registers"
        );
    }

    #[test]
    fn restore_registers_restores_prg_and_chr_and_mirroring() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let mut mapper = Mapper326::new(MapperContext::new_for_test(
            326,
            prg.clone(),
            chr.clone(),
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0xC000, 4);
        mapper.write_prg(0x8010, 1);
        mapper.write_prg(0x8018, 0);
        mapper.write_prg(0x8019, 0);
        mapper.write_prg(0x801A, 1);
        mapper.write_prg(0x801B, 1);
        let snap = mapper.registers_snapshot();

        let mut mapper2 = Mapper326::new(MapperContext::new_for_test(
            326,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        mapper2.restore_registers(&snap);

        assert_eq!(mapper2.read_prg(0x8000), 2, "PRG slot 0 must be restored");
        assert_eq!(mapper2.read_prg(0xA000), 3, "PRG slot 1 must be restored");
        assert_eq!(mapper2.read_prg(0xC000), 4, "PRG slot 2 must be restored");
        assert_eq!(mapper2.read_chr(0x0000), 1, "CHR slot 0 must be restored");
        assert_eq!(
            mapper2.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring must be restored"
        );
    }

    // ─── Reset ──────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_prg_banks_to_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0xC000, 4);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must be 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "PRG slot 1 must be 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG slot 2 must be 0 after reset"
        );
    }

    #[test]
    fn reset_preserves_fixed_e000_bank() {
        let mut mapper = make_mapper();
        let expected = mapper.read_prg(0xE000);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0xE000),
            expected,
            "$E000 window must remain at last bank after reset"
        );
    }

    #[test]
    fn reset_clears_chr_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8010, 5);
        mapper.reset();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 must be 0 after reset"
        );
    }

    // ─── No IRQ ─────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 326 must never assert IRQ");
    }
}
