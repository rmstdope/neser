//! Mapper 286 – BS-5 (Waixing multicart)
//!
//! Specifications:
//! - Primary source: NesDev wiki (inaccessible; HTTP 403 via Cloudflare)
//! - Fallback: Mesen2 `Core/NES/Mappers/Waixing/Bs5.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Waixing/Bs5.h>
//!
//! # Hardware overview
//!
//! Used by Waixing BS-5 multicart boards.
//!
//! - PRG-ROM: 4 × 8 KiB independently-bankable slots covering $8000–$FFFF.
//!   Bank selected by address bits `[11:10]`; bank index from address bits `[3:0]`.
//! - CHR-ROM: 4 × 2 KiB independently-bankable slots covering $0000–$1FFF.
//!   Bank selected by address bits `[11:10]`; bank index from address bits `[4:0]`.
//! - Mirroring: fixed per iNES header (no runtime control).
//! - No IRQ, no expansion audio, no PRG-RAM.
//! - DIP switches: 2 (treated as 0 in this implementation; no hardware interface).
//!
//! # Register map (write to PRG space)
//!
//! | Address range | Effect                                                                   |
//! |---------------|--------------------------------------------------------------------------|
//! | $8000–$8FFF   | CHR slot = `(addr >> 10) & 3`; CHR bank = `addr & 0x1F`                |
//! | $A000–$AFFF   | PRG slot = `(addr >> 10) & 3`; PRG bank = `addr & 0x0F`; only if bit 4 set |
//!
//! PRG writes are conditional: the bank is updated only when `addr & (1 << (dip + 4))` is set.
//! With `dip = 0` (default), this means **address bit 4 must be 1** for the write to take effect.
//!
//! # Power-on / reset state
//!
//! All 4 PRG slots and all 4 CHR slots map to the **last** bank (index −1).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 286;
const PRG_BANK_SIZE_BYTES: usize = 8 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 2 * 1024;
const PRG_SLOTS: usize = 4;
const CHR_SLOTS: usize = 4;

/// Mapper 286 – BS-5 Waixing multicart
///
/// 4 × 8 KiB PRG slots + 4 × 2 KiB CHR slots.  Banks selected via write address.
pub struct Mapper286 {
    base: BaseMapper,
    /// Current PRG bank index for each slot (−1 = last bank).
    prg_banks: [i16; PRG_SLOTS],
    /// Current CHR bank index for each slot (−1 = last bank).
    chr_banks: [i16; CHR_SLOTS],
}

impl Mapper286 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_banks: [-1; PRG_SLOTS],
            chr_banks: [-1; CHR_SLOTS],
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        for slot in 0..PRG_SLOTS {
            self.base.select_prg_page(slot, self.prg_banks[slot]);
        }
        for slot in 0..CHR_SLOTS {
            self.base.select_chr_page(slot, self.chr_banks[slot]);
        }
    }
}

impl Mapper for Mapper286 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        let slot = ((addr >> 10) & 0x03) as usize;
        match addr & 0xF000 {
            0x8000 => {
                // CHR banking: 5-bit bank from address bits [4:0]
                self.chr_banks[slot] = (addr & 0x1F) as i16;
                self.base.select_chr_page(slot, self.chr_banks[slot]);
            }
            // PRG banking: 4-bit bank from address bits [3:0], gated by bit 4
            0xA000 if addr & 0x10 != 0 => {
                self.prg_banks[slot] = (addr & 0x0F) as i16;
                self.base.select_prg_page(slot, self.prg_banks[slot]);
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(16);
        data.extend(self.prg_banks.iter().flat_map(|b| b.to_le_bytes()));
        data.extend(self.chr_banks.iter().flat_map(|b| b.to_le_bytes()));
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 16 {
            return;
        }
        for i in 0..PRG_SLOTS {
            self.prg_banks[i] = i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        }
        for i in 0..CHR_SLOTS {
            self.chr_banks[i] = i16::from_le_bytes([data[8 + i * 2], data[8 + i * 2 + 1]]);
        }
        self.apply_banks();
    }

    fn reset(&mut self) {
        self.prg_banks = [-1; PRG_SLOTS];
        self.chr_banks = [-1; CHR_SLOTS];
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS_8K: usize = 11; // 11 × 8 KiB
    const CHR_BANKS_2K: usize = 13; // 13 × 2 KiB

    fn make_mapper() -> Mapper286 {
        Mapper286::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_2K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_286_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_2K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 286 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────
    // All 4 PRG slots and all 4 CHR slots must map to the last bank on power-on.

    #[test]
    fn power_on_prg_all_slots_map_to_last_bank() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte N; last bank = PRG_BANKS_8K - 1 = 10
        let last_prg = (PRG_BANKS_8K - 1) as u8;
        assert_eq!(
            mapper.read_prg(0x8000),
            last_prg,
            "PRG slot 0 ($8000) must be last bank at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            last_prg,
            "PRG slot 1 ($A000) must be last bank at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            last_prg,
            "PRG slot 3 ($E000) must be last bank at power-on"
        );
    }

    #[test]
    fn power_on_chr_all_slots_map_to_last_bank() {
        let mut mapper = make_mapper();
        // Last CHR bank = CHR_BANKS_2K - 1 = 12
        let last_chr = (CHR_BANKS_2K - 1) as u8;
        assert_eq!(
            mapper.read_chr(0x0000),
            last_chr,
            "CHR slot 0 ($0000) must be last bank at power-on"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            last_chr,
            "CHR slot 1 ($0800) must be last bank at power-on"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            last_chr,
            "CHR slot 3 ($1800) must be last bank at power-on"
        );
    }

    // ── CHR banking ($8000–$8FFF) ─────────────────────────────────────────────
    // Slot = (addr >> 10) & 3; bank = addr & 0x1F

    #[test]
    fn chr_slot_selected_by_addr_bits_11_10() {
        let mut mapper = make_mapper();
        // Slot 0: $8000–$83FF, Slot 1: $8400–$87FF,
        // Slot 2: $8800–$8BFF, Slot 3: $8C00–$8FFF
        // Set each slot to a distinct bank and verify only that slot changed.
        mapper.write_prg(0x8001, 0x00); // slot 0, bank = addr & 0x1F = 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR slot 0 ($0000) should be bank 1"
        );

        mapper.write_prg(0x8402, 0x00); // slot 1 ($8400), bank = 2
        assert_eq!(
            mapper.read_chr(0x0800),
            2,
            "CHR slot 1 ($0800) should be bank 2"
        );

        mapper.write_prg(0x8803, 0x00); // slot 2 ($8800), bank = 3
        assert_eq!(
            mapper.read_chr(0x1000),
            3,
            "CHR slot 2 ($1000) should be bank 3"
        );

        mapper.write_prg(0x8C04, 0x00); // slot 3 ($8C00), bank = 4
        assert_eq!(
            mapper.read_chr(0x1800),
            4,
            "CHR slot 3 ($1800) should be bank 4"
        );
    }

    #[test]
    fn chr_bank_encoded_in_addr_low_5_bits() {
        let mut mapper = make_mapper();
        // Bank 7 = 0b00111, use slot 0 ($8000), addr & 0x1F = 7
        mapper.write_prg(0x8007, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 7, "CHR bank 7 from addr 0x8007");

        // Bank 12 = 0b01100, addr 0x800C
        mapper.write_prg(0x800C, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 12, "CHR bank 12 from addr 0x800C");
    }

    #[test]
    fn chr_write_ignores_written_value() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8005, 0xFF); // bank = addr & 0x1F = 5; value ignored
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank must come from address, not written value"
        );
    }

    #[test]
    fn chr_slots_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0); // slot 0 → bank 1
        mapper.write_prg(0x8405, 0); // slot 1 → bank 5
        mapper.write_prg(0x880A, 0); // slot 2 → bank 10
        mapper.write_prg(0x8C02, 0); // slot 3 → bank 2

        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x0800), 5);
        assert_eq!(mapper.read_chr(0x1000), 10);
        assert_eq!(mapper.read_chr(0x1800), 2);
    }

    // ── PRG banking ($A000–$AFFF) ─────────────────────────────────────────────
    // Slot = (addr >> 10) & 3; bank = addr & 0x0F; only when addr bit 4 set.

    #[test]
    fn prg_write_requires_bit_4_set() {
        let mut mapper = make_mapper();
        let last_bank = (PRG_BANKS_8K - 1) as u8;

        // $A001: bit 4 clear → PRG must NOT change
        mapper.write_prg(0xA001, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            last_bank,
            "PRG slot 0 must stay as last bank when addr bit 4 is clear"
        );

        // $A011: bit 4 set → PRG bank = addr & 0x0F = 1
        mapper.write_prg(0xA011, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG slot 0 should switch to bank 1 when addr bit 4 is set"
        );
    }

    #[test]
    fn prg_slot_selected_by_addr_bits_11_10() {
        let mut mapper = make_mapper();
        // Slot 0: $A000–$A3FF, Slot 1: $A400–$A7FF,
        // Slot 2: $A800–$ABFF, Slot 3: $AC00–$AFFF
        mapper.write_prg(0xA011, 0); // slot 0, bank 1 (bit4=1, bank=0x11&0xF=1)
        mapper.write_prg(0xA412, 0); // slot 1, bank 2
        mapper.write_prg(0xA813, 0); // slot 2, bank 3
        mapper.write_prg(0xAC14, 0); // slot 3, bank 4

        assert_eq!(mapper.read_prg(0x8000), 1, "PRG slot 0 ($8000) bank 1");
        assert_eq!(mapper.read_prg(0xA000), 2, "PRG slot 1 ($A000) bank 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "PRG slot 2 ($C000) bank 3");
        assert_eq!(mapper.read_prg(0xE000), 4, "PRG slot 3 ($E000) bank 4");
    }

    #[test]
    fn prg_bank_encoded_in_addr_low_4_bits() {
        let mut mapper = make_mapper();
        // Bank 5 = 0b0101, addr = $A015 (slot0, bit4=1, bank=5)
        mapper.write_prg(0xA015, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5, "PRG bank 5 from addr 0xA015");

        // Bank 9, addr = $A019
        mapper.write_prg(0xA019, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 9, "PRG bank 9 from addr 0xA019");
    }

    #[test]
    fn prg_write_ignores_written_value() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA013, 0xFF); // bank = addr & 0x0F = 3; value ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank must come from address, not written value"
        );
    }

    #[test]
    fn prg_slots_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA011, 0); // slot 0 → bank 1
        mapper.write_prg(0xA415, 0); // slot 1 → bank 5
        mapper.write_prg(0xA817, 0); // slot 2 → bank 7
        mapper.write_prg(0xAC19, 0); // slot 3 → bank 9

        assert_eq!(mapper.read_prg(0x8000), 1, "PRG slot 0 = bank 1");
        assert_eq!(mapper.read_prg(0xA000), 5, "PRG slot 1 = bank 5");
        assert_eq!(mapper.read_prg(0xC000), 7, "PRG slot 2 = bank 7");
        assert_eq!(mapper.read_prg(0xE000), 9, "PRG slot 3 = bank 9");
    }

    // ── 8 KiB PRG window layout ───────────────────────────────────────────────
    // $8000–$9FFF = slot 0, $A000–$BFFF = slot 1,
    // $C000–$DFFF = slot 2, $E000–$FFFF = slot 3.

    #[test]
    fn prg_each_slot_spans_8kb() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA012, 0); // slot 0 → bank 2

        // All addresses in $8000–$9FFF should read bank 2
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0x9FFF), 2);
        // $A000 is slot 1 (still last bank)
        let last_bank = (PRG_BANKS_8K - 1) as u8;
        assert_eq!(mapper.read_prg(0xA000), last_bank);
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_dynamic_mirroring, "no dynamic mirroring");
        assert!(caps.has_chr_banking, "CHR banking enabled");
        assert_eq!(caps.prg_bank_size_kb, 8, "8 KiB PRG banks");
        assert_eq!(caps.chr_bank_size_kb, 2, "2 KiB CHR banks");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 286 must never assert IRQ");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip_preserves_all_banks() {
        let mut mapper = make_mapper();
        // Configure distinct banks in all slots
        mapper.write_prg(0xA011, 0); // PRG slot 0 → bank 1
        mapper.write_prg(0xA415, 0); // PRG slot 1 → bank 5
        mapper.write_prg(0xA817, 0); // PRG slot 2 → bank 7
        mapper.write_prg(0xAC19, 0); // PRG slot 3 → bank 9
        mapper.write_prg(0x8002, 0); // CHR slot 0 → bank 2
        mapper.write_prg(0x8406, 0); // CHR slot 1 → bank 6
        mapper.write_prg(0x880B, 0); // CHR slot 2 → bank 11
        mapper.write_prg(0x8C03, 0); // CHR slot 3 → bank 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 1, "PRG slot 0 restored");
        assert_eq!(restored.read_prg(0xA000), 5, "PRG slot 1 restored");
        assert_eq!(restored.read_prg(0xC000), 7, "PRG slot 2 restored");
        assert_eq!(restored.read_prg(0xE000), 9, "PRG slot 3 restored");
        assert_eq!(restored.read_chr(0x0000), 2, "CHR slot 0 restored");
        assert_eq!(restored.read_chr(0x0800), 6, "CHR slot 1 restored");
        assert_eq!(restored.read_chr(0x1000), 11, "CHR slot 2 restored");
        assert_eq!(restored.read_chr(0x1800), 3, "CHR slot 3 restored");
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA011, 0); // PRG slot 0 → bank 1
        // Snapshot has 16 bytes; providing fewer should be ignored
        mapper.restore_registers(&[0u8; 8]);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "state must be unchanged after short restore"
        );
    }

    #[test]
    fn snapshot_restore_preserves_power_on_last_bank_state() {
        let mapper = make_mapper();
        let snap = mapper.registers_snapshot();
        let last_prg = (PRG_BANKS_8K - 1) as u8;
        let last_chr = (CHR_BANKS_2K - 1) as u8;

        let mut restored = make_mapper();
        // Modify state first, then restore to power-on
        restored.write_prg(0xA011, 0);
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            last_prg,
            "PRG slot 0 restored to last bank"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            last_chr,
            "CHR slot 0 restored to last bank"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA011, 0); // PRG slot 0 → bank 1
        mapper.write_prg(0x8005, 0); // CHR slot 0 → bank 5
        mapper.reset();

        let last_prg = (PRG_BANKS_8K - 1) as u8;
        let last_chr = (CHR_BANKS_2K - 1) as u8;
        assert_eq!(
            mapper.read_prg(0x8000),
            last_prg,
            "PRG slot 0 must be last bank after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            last_chr,
            "CHR slot 0 must be last bank after reset"
        );
    }

    // ── PRG window address mapping ────────────────────────────────────────────

    #[test]
    fn prg_slot_boundaries_are_correct() {
        let mut mapper = make_mapper();
        // Set each slot to a unique bank
        mapper.write_prg(0xA010, 0); // slot 0 → bank 0
        mapper.write_prg(0xA411, 0); // slot 1 → bank 1
        mapper.write_prg(0xA812, 0); // slot 2 → bank 2
        mapper.write_prg(0xAC13, 0); // slot 3 → bank 3

        // Verify slot boundaries
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = slot 0");
        assert_eq!(mapper.read_prg(0x9FFF), 0, "$9FFF = slot 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 = slot 1");
        assert_eq!(mapper.read_prg(0xBFFF), 1, "$BFFF = slot 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 = slot 2");
        assert_eq!(mapper.read_prg(0xDFFF), 2, "$DFFF = slot 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 = slot 3");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF = slot 3");
    }

    // ── CHR window address mapping ────────────────────────────────────────────

    #[test]
    fn chr_slot_boundaries_are_correct() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0); // CHR slot 0 → bank 1
        mapper.write_prg(0x8402, 0); // CHR slot 1 → bank 2
        mapper.write_prg(0x8803, 0); // CHR slot 2 → bank 3
        mapper.write_prg(0x8C04, 0); // CHR slot 3 → bank 4

        assert_eq!(mapper.read_chr(0x0000), 1, "$0000 = CHR slot 0");
        assert_eq!(mapper.read_chr(0x07FF), 1, "$07FF = CHR slot 0");
        assert_eq!(mapper.read_chr(0x0800), 2, "$0800 = CHR slot 1");
        assert_eq!(mapper.read_chr(0x0FFF), 2, "$0FFF = CHR slot 1");
        assert_eq!(mapper.read_chr(0x1000), 3, "$1000 = CHR slot 2");
        assert_eq!(mapper.read_chr(0x17FF), 3, "$17FF = CHR slot 2");
        assert_eq!(mapper.read_chr(0x1800), 4, "$1800 = CHR slot 3");
        assert_eq!(mapper.read_chr(0x1FFF), 4, "$1FFF = CHR slot 3");
    }
}
