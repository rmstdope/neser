//! Mapper 196 – MMC3 with address-scrambled registers and 32 KiB PRG override
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_196>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_196.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 196 is an MMC3 clone used in several pirate game boards.  It differs
//! from standard MMC3 in two ways:
//!
//! 1. **Address bit scrambling** – The register address line A0 is replaced by a
//!    logical OR of higher address bits before the write reaches the MMC3 logic:
//!    - `$8000–$BFFF`: new A0 = A1 | A2 | A3
//!    - `$C000–$FFFF`: new A0 = A2 | A3
//!
//! 2. **32 KiB PRG override** – A write to `$6000–$6FFF` activates a fixed
//!    32 KiB PRG bank: both the "bank" nibbles of the written byte are OR-combined
//!    to form a 4-bit block index, and the four consecutive 8 KiB banks at
//!    `block × 4` through `block × 4 + 3` fill `$8000–$FFFF`.  Once activated
//!    this override persists until a power cycle.
//!
//! CHR, IRQ, and mirroring behave as in standard MMC3 (mediated by the scrambled
//! addresses).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 196;
const PRG_BANK_SIZE: usize = 0x2000;
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;

/// Mapper 196 – MMC3 with scrambled register addresses and 32 KiB PRG override.
///
/// See the module-level documentation for hardware details.
pub struct Mapper196 {
    inner: MMC3Mapper,
    /// Set to `true` when a `$6000–$6FFF` write activates the 32 KiB PRG mode.
    prg_locked: bool,
    /// Block index for the 32 KiB PRG override (four consecutive 8 KiB banks at
    /// `prg_block * 4`).
    prg_block: u8,
}

impl Mapper196 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            inner: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            prg_locked: false,
            prg_block: 0,
        }
    }

    /// Compute the 8 KiB PRG bank number for the given CPU address when the
    /// 32 KiB PRG override is active.
    fn locked_prg_bank(&self, addr: u16) -> usize {
        let base = (self.prg_block as usize) << 2;
        let slot = ((addr as usize).saturating_sub(0x8000) >> 13) & 0x03;
        base | slot
    }

    /// Scramble address bit A0 for an MMC3 register write in `$8000–$BFFF`:
    /// new A0 = A1 | A2 | A3.
    fn scramble_lo(addr: u16) -> u16 {
        (addr & 0xFFFE) | ((addr >> 1) & 1) | ((addr >> 2) & 1) | ((addr >> 3) & 1)
    }

    /// Scramble address bit A0 for an MMC3 register write in `$C000–$FFFF`:
    /// new A0 = A2 | A3.
    fn scramble_hi(addr: u16) -> u16 {
        (addr & 0xFFFE) | ((addr >> 2) & 1) | ((addr >> 3) & 1)
    }
}

impl Mapper for Mapper196 {
    fn base(&self) -> &BaseMapper {
        &self.inner.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.inner.read_prg(addr),
            0x8000..=0xFFFF => {
                if self.prg_locked {
                    let bank = self.locked_prg_bank(addr);
                    let offset = (addr as usize) & PRG_BANK_MASK;
                    self.inner.read_prg_at_bank(bank, offset)
                } else {
                    let bank = self.inner.mapped_prg_bank(addr);
                    let offset = (addr as usize) & PRG_BANK_MASK;
                    self.inner.read_prg_at_bank(bank, offset)
                }
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.inner.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            // $6000–$6FFF: activate 32 KiB PRG override (takes priority over PRG-RAM).
            0x6000..=0x6FFF => {
                self.prg_locked = true;
                self.prg_block = (value & 0x0F) | (value >> 4);
            }
            // $7000–$7FFF: PRG-RAM (standard MMC3 path).
            0x7000..=0x7FFF => {
                self.inner.write_prg(addr, value);
            }
            // $8000–$BFFF: MMC3 register write with A0 = A1|A2|A3.
            0x8000..=0xBFFF => {
                self.inner.write_prg(Self::scramble_lo(addr), value);
            }
            // $C000–$FFFF: MMC3 register write with A0 = A2|A3.
            0xC000..=0xFFFF => {
                self.inner.write_prg(Self::scramble_hi(addr), value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.inner.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.inner.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.inner.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & CHR_BANK_MASK;
        self.inner.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.push(self.prg_locked as u8);
        snap.push(self.prg_block);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        let (mmc3_data, tail) = data.split_at(data.len() - 2);
        self.inner.restore_registers(mmc3_data);
        self.prg_locked = tail[0] != 0;
        self.prg_block = tail[1];
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 16;
    const CHR_1K_BANKS: usize = 64;

    fn make_mapper() -> Mapper196 {
        Mapper196::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_196_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 196 must be registered in the factory"
        );
    }

    // ── Address scrambling ($8000–$BFFF) ─────────────────────────────────────

    #[test]
    fn scramble_lo_a0_cleared_stays_zero() {
        // $8000: A1=A2=A3=0 → new A0=0 → bank select
        assert_eq!(Mapper196::scramble_lo(0x8000), 0x8000);
    }

    #[test]
    fn scramble_lo_a1_set_gives_a0_one() {
        // $8002: A1=1 → new A0=1 → bank data
        assert_eq!(Mapper196::scramble_lo(0x8002), 0x8003);
    }

    #[test]
    fn scramble_lo_a2_set_gives_a0_one() {
        // $8004: A2=1 → new A0=1
        assert_eq!(Mapper196::scramble_lo(0x8004), 0x8005);
    }

    #[test]
    fn scramble_lo_a3_set_gives_a0_one() {
        // $8008: A3=1 → new A0=1
        assert_eq!(Mapper196::scramble_lo(0x8008), 0x8009);
    }

    #[test]
    fn scramble_hi_a0_cleared_stays_zero() {
        // $C000: A2=A3=0 → new A0=0
        assert_eq!(Mapper196::scramble_hi(0xC000), 0xC000);
    }

    #[test]
    fn scramble_hi_a2_set_gives_a0_one() {
        // $C004: A2=1 → new A0=1
        assert_eq!(Mapper196::scramble_hi(0xC004), 0xC005);
    }

    #[test]
    fn scramble_hi_a1_does_not_affect_a0() {
        // $C002: A1=1 but A2=A3=0 → new A0=0 for $C000–$FFFF range
        assert_eq!(Mapper196::scramble_hi(0xC002), 0xC002);
    }

    // ── MMC3 register access via scrambled addresses ──────────────────────────

    #[test]
    fn mmc3_bank_select_via_scrambled_8000() {
        let mut mapper = make_mapper();
        // Write bank select (reg 6 = PRG bank 0) to $8000
        mapper.write_prg(0x8000, 0b0000_0110); // select reg 6
        // Write bank data to scrambled $8001 equivalent: $8002 (A1=1 → A0=1)
        mapper.write_prg(0x8002, 3); // bank 3
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000–$9FFF must read from bank 3"
        );
    }

    #[test]
    fn mmc3_mirroring_via_scrambled_a000() {
        let mut mapper = make_mapper();
        // $A000 scrambled: A1=A2=A3=0 → A0=0 → mirroring register
        mapper.write_prg(0xA000, 0x01); // horizontal
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must update via scrambled $A000"
        );
    }

    // ── 32 KiB PRG override ($6000–$6FFF write) ───────────────────────────────

    #[test]
    fn prg_override_activates_on_6000_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // block = (0x01 & 0xF) | (0x01 >> 4) = 1
        assert!(
            mapper.prg_locked,
            "PRG override must activate on $6000 write"
        );
    }

    #[test]
    fn prg_override_maps_four_8k_banks() {
        let mut mapper = make_mapper();
        // block 1 → banks 4,5,6,7
        mapper.write_prg(0x6000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 must read bank 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 must read bank 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 must read bank 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 must read bank 7");
    }

    #[test]
    fn prg_override_block_combines_nibbles() {
        let mut mapper = make_mapper();
        // value = 0x23: lower nibble = 3, upper nibble = 2, OR = 3
        mapper.write_prg(0x6000, 0x23);
        assert_eq!(
            mapper.prg_block, 3,
            "Block must be (lower) | (upper>>4) = 3"
        );
    }

    #[test]
    fn prg_override_block_from_upper_nibble() {
        let mut mapper = make_mapper();
        // value = 0x20: lower = 0, upper = 2, OR = 2
        mapper.write_prg(0x6000, 0x20);
        assert_eq!(
            mapper.prg_block, 2,
            "Block must be derivable from upper nibble"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 must map to bank 8 (block 2 × 4)"
        );
    }

    #[test]
    fn prg_override_6fff_also_activates() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6FFF, 0x02); // anywhere in $6000–$6FFF
        assert!(mapper.prg_locked);
        assert_eq!(mapper.prg_block, 2);
    }

    // ── Standard MMC3 PRG mapping (no override) ───────────────────────────────

    #[test]
    fn power_on_prg_e000_is_last_bank() {
        let mapper = make_mapper();
        // MMC3 power-on: $E000–$FFFF always fixed to last bank
        let last = (PRG_8K_BANKS - 1) as u8;
        assert_eq!(mapper.read_prg(0xE000), last);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips_no_override() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8002, 5); // bank 5 in PRG slot 0

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG $8000 must match after restore"
        );
        assert!(
            !restored.prg_locked,
            "prg_locked must be false after restore"
        );
    }

    #[test]
    fn registers_snapshot_round_trips_with_override() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // activate override, block 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert!(restored.prg_locked, "prg_locked must be restored");
        assert_eq!(restored.prg_block, 3, "prg_block must be restored");
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG $8000 must match after restore"
        );
    }

    // ── CHR banking (standard MMC3) ───────────────────────────────────────────

    #[test]
    fn chr_bank_select_via_scrambled_address() {
        let mut mapper = make_mapper();
        // Select CHR reg 0 (bank_select = 0b0000_0000)
        mapper.write_prg(0x8000, 0b0000_0000);
        // Write bank data via scrambled $8002 (A1=1 → A0=1)
        mapper.write_prg(0x8002, 10);
        assert_eq!(
            mapper.read_chr(0x0000),
            10,
            "CHR $0000 must map to 1K bank 10"
        );
    }

    // ── No IRQ by default ─────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }
}
