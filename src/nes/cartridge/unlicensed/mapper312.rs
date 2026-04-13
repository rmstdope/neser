//! Mapper 312
//!
//! # Specification sources
//!
//! - **Primary source**: NESdev Wiki — NES 2.0 Mapper 312
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_312>
//!
//! # Hardware overview
//!
//! Mapper 312 is a simple unlicensed board with a 16 KiB switchable PRG-ROM
//! window at `$8000–$BFFF`, a fixed last-bank 16 KiB window at `$C000–$FFFF`,
//! an 8 KiB CHR window (ROM or RAM, no banking), and software-controlled H/V
//! mirroring.
//!
//! # PRG banking
//!
//! | CPU address     | Window size | Bank                               |
//! |-----------------|-------------|------------------------------------|
//! | `$8000–$BFFF`   | 16 KiB      | Switchable — reg[0] (written at `$6000–$7FFF`) |
//! | `$C000–$FFFF`   | 16 KiB      | Fixed — last 16 KiB bank           |
//!
//! # Registers
//!
//! | CPU write address | Effect                                        |
//! |-------------------|-----------------------------------------------|
//! | `$6000–$7FFF`     | PRG bank select: selects 16 KiB page at `$8000–$BFFF` |
//! | `$8000–$FFFF`     | Mirroring: bit 0 = 1 → horizontal, 0 → vertical |
//!
//! # CHR
//!
//! Single 8 KiB window. Uses CHR-ROM if present; otherwise allocates 8 KiB
//! CHR-RAM.
//!
//! # Power-on / reset state
//!
//! Both registers initialise to `0x00`: first 16 KiB at `$8000`, vertical mirroring.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 312;
const PRG_16K_BANK_SIZE: usize = 0x4000;
const CHR_RAM_SIZE: usize = 8 * 1024;

/// Mapper 312 — simple unlicensed board with switchable lower 16 KiB PRG bank
/// and fixed upper 16 KiB PRG bank, plus software-controlled H/V mirroring.
pub struct Mapper312 {
    base: BaseMapper,
    /// PRG bank register written at `$6000–$7FFF`.
    prg_bank: u8,
    /// Current nametable mirroring (bit 0 of writes to `$8000–$FFFF`).
    mirroring: NametableLayout,
}

impl Mapper312 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_16K_BANK_SIZE);
        if ctx.chr_rom.is_empty() {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        }
        let mut mapper = Self {
            base,
            prg_bank: 0,
            mirroring: NametableLayout::Vertical,
        };
        mapper.apply_banking();
        mapper
    }

    fn apply_banking(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, -1);
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for Mapper312 {
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
        match addr {
            0x6000..=0x7FFF => {
                self.prg_bank = value;
                self.apply_banking();
            }
            0x8000..=0xFFFF => {
                self.mirroring = if (value & 0x01) != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                };
                self.apply_banking();
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.mirroring = NametableLayout::Vertical;
        self.apply_banking();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.mirroring.to_snapshot_byte()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        self.prg_bank = data[0];
        self.mirroring = NametableLayout::from_snapshot_byte(data[1]);
        self.apply_banking();
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.base.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    const PRG_BANK_SIZE: usize = PRG_16K_BANK_SIZE;

    /// Create a PRG-ROM with N 16 KiB banks, each filled with its bank index value.
    fn make_prg_rom(num_banks: usize) -> Vec<u8> {
        let mut rom = vec![0u8; num_banks * PRG_BANK_SIZE];
        for bank in 0..num_banks {
            let start = bank * PRG_BANK_SIZE;
            let end = start + PRG_BANK_SIZE;
            rom[start..end].fill(bank as u8);
        }
        rom
    }

    fn make_mapper(num_banks: usize) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            make_prg_rom(num_banks),
            vec![],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 312 must be registered")
    }

    fn make_mapper_direct(num_banks: usize) -> Mapper312 {
        Mapper312::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                make_prg_rom(num_banks),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_312_is_registered_in_factory() {
        assert_eq!(make_mapper(4).mapper_number(), MAPPER_NUMBER);
    }

    // ── PRG $8000 lower window — switchable ──────────────────────────────────

    #[test]
    fn prg_lower_defaults_to_bank_0_on_power_on() {
        let mapper = make_mapper(4);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must read bank 0 on power-on"
        );
    }

    #[test]
    fn prg_lower_switches_to_bank_1_after_6000_write() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x6000, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must reflect bank 1 after writing 1 to $6000"
        );
    }

    #[test]
    fn prg_lower_switches_to_bank_2_after_6000_write() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x6000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must reflect bank 2 after writing 2 to $6000"
        );
    }

    #[test]
    fn prg_lower_can_switch_back_to_bank_0() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x6000, 2);
        mapper.write_prg(0x6000, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must reflect bank 0 after switching back"
        );
    }

    #[test]
    fn prg_lower_bfff_mirrors_same_bank() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x6000, 1);
        assert_eq!(
            mapper.read_prg(0xBFFF),
            1,
            "$BFFF must mirror the same bank as $8000"
        );
    }

    #[test]
    fn prg_lower_write_to_7fff_also_selects_bank() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x7FFF, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Writing to $7FFF must also select the PRG bank"
        );
    }

    // ── PRG $C000 upper window — fixed last bank ─────────────────────────────

    #[test]
    fn prg_upper_always_reads_last_bank() {
        // 4 banks: last bank = 3
        let mapper = make_mapper(4);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must always read the last PRG bank"
        );
    }

    #[test]
    fn prg_upper_does_not_change_when_lower_switches() {
        let mut mapper = make_mapper(4);
        mapper.write_prg(0x6000, 1);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must remain fixed at last bank after $6000 write"
        );
    }

    #[test]
    fn prg_upper_ffff_also_reads_last_bank() {
        let mapper = make_mapper(4);
        assert_eq!(
            mapper.read_prg(0xFFFF),
            3,
            "$FFFF must also read the last PRG bank"
        );
    }

    // ── Mirroring via $8000 writes ───────────────────────────────────────────

    #[test]
    fn mirroring_defaults_to_vertical_on_power_on() {
        let mapper = make_mapper_direct(4);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical on power-on"
        );
    }

    #[test]
    fn mirroring_bit0_one_selects_horizontal() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit 0 = 1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_bit0_zero_selects_vertical() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x8000, 0x01); // set horizontal first
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit 0 = 0 must select Vertical mirroring"
        );
    }

    #[test]
    fn mirroring_upper_bits_of_8000_write_are_ignored() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x8000, 0xFE); // bit 0 = 0 → vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Upper bits of $8000 write must be ignored for mirroring"
        );
    }

    #[test]
    fn mirroring_write_to_ffff_also_takes_effect() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0xFFFF, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Write to $FFFF must also control mirroring"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_prg_bank_to_0() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x6000, 2);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must read bank 0 after reset"
        );
    }

    #[test]
    fn reset_restores_mirroring_to_vertical() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x8000, 0x01); // horizontal
        mapper.reset();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    #[test]
    fn reset_does_not_affect_upper_bank() {
        let mut mapper = make_mapper_direct(4);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must still read last bank after reset"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_report_dynamic_mirroring() {
        let mapper = make_mapper(4);
        assert!(
            mapper.capabilities().has_dynamic_mirroring,
            "Mapper 312 must report has_dynamic_mirroring = true"
        );
    }

    #[test]
    fn capabilities_report_no_irq() {
        let mapper = make_mapper(4);
        assert!(
            !mapper.capabilities().has_irq,
            "Mapper 312 must report has_irq = false"
        );
    }

    #[test]
    fn capabilities_report_no_chr_banking() {
        let mapper = make_mapper(4);
        assert!(
            !mapper.capabilities().has_chr_banking,
            "Mapper 312 must report has_chr_banking = false"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_bank() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x6000, 2);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_direct(4);
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            2,
            "Restored PRG bank must match saved state"
        );
    }

    #[test]
    fn snapshot_restore_preserves_mirroring() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x8000, 0x01); // horizontal
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_direct(4);
        restored.restore_registers(&snap);

        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "Restored mirroring must match saved state"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper_direct(4);
        mapper.write_prg(0x6000, 2);
        let original_bank = mapper.prg_bank;

        mapper.restore_registers(&[]);

        assert_eq!(
            mapper.prg_bank, original_bank,
            "Short restore must not change state"
        );
    }

    // ── CHR RAM allocation ───────────────────────────────────────────────────

    #[test]
    fn chr_ram_writable_when_no_chr_rom() {
        let mut mapper = make_mapper(4); // no CHR ROM → CHR RAM should be allocated
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR RAM must be writable when no CHR ROM is present"
        );
    }
}
