//! Mapper 221 – NTDEC N625092 multicart board
//!
//! Specifications:
//! - Primary source: Mesen2 `Core/NES/Mappers/Ntdec/Mapper221.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Ntdec/Mapper221.h>
//! - No NESdev wiki page available.
//!
//! ## Hardware behavior
//!
//! ### Mode register (`$8000–$BFFF` write)
//! The write **address** (not the data byte) is stored as the 16-bit mode register.
//!
//! ```text
//! mode = addr (full 16-bit write address)
//!   A0    → mirroring: 1 = Horizontal, 0 = Vertical
//!   A1    → PRG mode enable: 0 = NROM-128, 1 = NROM-256 or UNROM
//!   A7:A2 → outer PRG bank (6 bits)
//!   A8    → UNROM sub-mode (when A1=1): 0 = NROM-256, 1 = UNROM
//! ```
//!
//! ### Inner PRG register (`$C000–$FFFF` write)
//! Write address bits A2:A0 are stored as the 3-bit inner PRG bank register.
//! The data byte is ignored.
//!
//! ### PRG banking (16 KB pages)
//! `outerBank = (mode & 0xFC) >> 2`
//!
//! | A1 (mode & 0x02) | A8 (mode & 0x0100) | Mode       | Slot 0                    | Slot 1                    |
//! |---|---|---|---|---|
//! | 0 | — | NROM-128   | outerBank \| prgReg       | outerBank \| prgReg       |
//! | 1 | 0 | NROM-256   | outerBank \| (prgReg & 6) | outerBank \| (prgReg & 6) + 1 |
//! | 1 | 1 | UNROM      | outerBank \| prgReg       | outerBank \| 0x07         |
//!
//! ### CHR
//! Single 8 KB page (CHR-RAM; no CHR-ROM banking).
//!
//! ### Mirroring
//! Controlled by mode bit A0: 1 → Horizontal, 0 → Vertical.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on / reset state: mode = 0, prgReg = 0 (NROM-128, bank 0, Vertical).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 221;
const PRG_BANK_SIZE: usize = 16 * 1024;

/// Mapper 221 – NTDEC N625092 multicart board.
///
/// See the module-level documentation for hardware details.
pub struct Mapper221 {
    base: BaseMapper,
    /// Full 16-bit write address latched from the last $8000–$BFFF write.
    mode: u16,
    /// 3-bit inner PRG bank from the last $C000–$FFFF write (addr bits A2:A0).
    prg_reg: u8,
}

impl Mapper221 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        let mut mapper = Self {
            base,
            mode: 0,
            prg_reg: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let outer_bank = ((self.mode & 0x00FC) >> 2) as i16;
        let prg = self.prg_reg as i16;

        if self.mode & 0x02 == 0 {
            // NROM-128: both 16KB slots map to the same bank.
            let bank = outer_bank | prg;
            self.base.select_prg_page(0, bank);
            self.base.select_prg_page(1, bank);
        } else if self.mode & 0x0100 == 0 {
            // NROM-256: two consecutive 16KB banks form a 32KB page.
            // Use (prgReg & 6) to align to 32KB boundary.
            let base_bank = outer_bank | (prg & 0x06);
            self.base.select_prg_page(0, base_bank);
            self.base.select_prg_page(1, base_bank + 1);
        } else {
            // UNROM: slot 0 switches, slot 1 fixed to outer|0x07.
            self.base.select_prg_page(0, outer_bank | prg);
            self.base.select_prg_page(1, outer_bank | 0x07);
        }

        // Mirroring: A0 = 1 → Horizontal, else Vertical.
        self.base.set_mirroring_hv(self.mode & 0x01 != 0);
    }
}

impl Mapper for Mapper221 {
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
        // Data byte is ignored entirely; banking is determined by address lines.
        let _ = value;
        match addr & 0xC000 {
            0x8000 => {
                self.mode = addr;
                self.update_banks();
            }
            0xC000 => {
                self.prg_reg = (addr & 0x0007) as u8;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // banking snapshot first (base-managed PRG/CHR pages + mirroring),
        // then mode lo, mode hi, prg_reg.
        let mut snap = self.base.banking_snapshot();
        snap.push((self.mode & 0xFF) as u8);
        snap.push((self.mode >> 8) as u8);
        snap.push(self.prg_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_len = self.base.banking_snapshot().len();
        if data.len() >= banking_len + 3 {
            self.base.restore_banking(&data[..banking_len]);
            let rest = &data[banking_len..];
            self.mode = u16::from(rest[0]) | (u16::from(rest[1]) << 8);
            self.prg_reg = rest[2] & 0x07;
            self.update_banks();
        } else if data.len() >= 3 {
            // Legacy snapshot without banking prefix.
            self.mode = u16::from(data[0]) | (u16::from(data[1]) << 8);
            self.prg_reg = data[2] & 0x07;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.mode = 0;
        self.prg_reg = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to detect modulo-wrapping bugs.
    const PRG_BANKS: usize = 11;

    fn make_mapper() -> Mapper221 {
        Mapper221::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_221_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 221 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_both_slots_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must be Vertical (A0=0)"
        );
    }

    // ── NROM-128 mode (A1=0) ──────────────────────────────────────────────────

    #[test]
    fn nrom128_both_slots_mirror_same_bank() {
        let mut mapper = make_mapper();
        // addr=$8002: A1=1 → mode enable, but set mode with A1=0 (addr=$8000)
        // addr=$C004: A2:A0=4 → prgReg=4; outerBank=0; bank=4
        mapper.write_prg(0x8000, 0); // mode: A1=0 → NROM-128
        mapper.write_prg(0xC004, 0); // prgReg = 4
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 must be bank 4 in NROM-128 mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must mirror bank 4 in NROM-128 mode"
        );
    }

    #[test]
    fn nrom128_outer_bank_bits_are_applied() {
        let mut mapper = make_mapper();
        // addr=$8008: A3=1 → outer_bank = (0x0008 & 0xFC) >> 2 = 2; A1=0 → NROM-128
        // prgReg=0 → bank = 2
        mapper.write_prg(0x8008, 0);
        mapper.write_prg(0xC000, 0); // prgReg = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "outerBank=2 with prgReg=0 → bank 2"
        );
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 must mirror bank 2");
    }

    // ── NROM-256 mode (A1=1, A8=0) ───────────────────────────────────────────

    #[test]
    fn nrom256_slots_are_consecutive_32kb_pages() {
        let mut mapper = make_mapper();
        // addr=$8002: A1=1, A8=0 → NROM-256
        // prgReg via $C000 addr: A2:A0=0 → aligned base = 0; slot0=0, slot1=1
        mapper.write_prg(0x8002, 0); // mode: NROM-256, outerBank=0
        mapper.write_prg(0xC000, 0); // prgReg = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 in NROM-256"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must be bank 1 in NROM-256"
        );
    }

    #[test]
    fn nrom256_prg_reg_aligned_to_32kb() {
        let mut mapper = make_mapper();
        // prgReg=3 (odd) → aligned to 2 → slot0=2, slot1=3
        mapper.write_prg(0x8002, 0); // NROM-256
        mapper.write_prg(0xC003, 0); // prgReg = 3
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "prgReg=3 must align to base=2 in NROM-256"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must be base+1=3 in NROM-256"
        );
    }

    // ── UNROM mode (A1=1, A8=1) ───────────────────────────────────────────────

    #[test]
    fn unrom_slot1_fixed_to_outer_or_7() {
        let mut mapper = make_mapper();
        // addr=$8102: A1=1, A8=1 → UNROM; outerBank = (0x0102 & 0xFC) >> 2 = 0
        // prgReg=0; slot0=0|0=0, slot1=0|7=7
        mapper.write_prg(0x8102, 0); // UNROM, outerBank=0
        mapper.write_prg(0xC000, 0); // prgReg=0
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must be bank 0 in UNROM");
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must be fixed bank 7 (outer|7) in UNROM"
        );
    }

    #[test]
    fn unrom_slot0_switches_while_slot1_stays_fixed() {
        let mut mapper = make_mapper();
        // UNROM: outerBank=0, prgReg varies
        mapper.write_prg(0x8102, 0); // UNROM
        mapper.write_prg(0xC002, 0); // prgReg=2 → slot0=2, slot1=7
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 7);

        mapper.write_prg(0xC005, 0); // prgReg=5 → slot0=5, slot1=7
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mode_a0_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0); // A0=1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "A0=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mode_a0_0_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0); // set Horizontal first
        mapper.write_prg(0x8000, 0); // A0=0 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "A0=0 must select Vertical mirroring"
        );
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0010, 0xAB);
        assert_eq!(mapper.read_chr(0x0010), 0xAB, "CHR-RAM must be writable");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8103, 0); // UNROM, Horizontal
        mapper.write_prg(0xC005, 0); // prgReg=5
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8103, 0); // UNROM + A0=1 (Horizontal), outerBank=0
        mapper.write_prg(0xC003, 0); // prgReg=3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Snapshot must restore $8000 PRG"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Snapshot must restore $C000 PRG"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Snapshot must restore mirroring"
        );
    }
}
