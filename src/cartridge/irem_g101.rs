//! Mapper 032 - Irem G-101
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//!
//! Specification: <https://www.nesdev.org/wiki/INES_Mapper_032>

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 032 - Irem G-101
///
/// Hardware: Irem's G-101 mapper IC (52-pin DIP)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_032>
/// - PRG-ROM: Up to 512KB (32 8KB banks)
/// - CHR: Up to 256KB (256 1KB banks) or CHR-RAM
/// - Mirroring: Switchable (vertical/horizontal) or hardwired 1-screen (submapper 1)
///
/// Known games: Image Fight, Major League, Kaiketsu Yanchamaru 2
///
/// NES 2.0 Submappers:
/// - Submapper 0: Standard (switchable mirroring, switchable PRG mode)
/// - Submapper 1: Major League (hardwired 1-screen upper, $9000 disabled, PRG mode always 0)
///
/// Register map (Range $8000-$BFFF, Mask $F007):
/// - $8000-$8007: PRG Reg 0 (bits 4:0 = 8KB bank number for $8000 window)
/// - $9000-$9007: Control (bit 1 = PRG mode, bit 0 = mirroring; ignored on submapper 1)
/// - $A000-$A007: PRG Reg 1 (bits 4:0 = 8KB bank number for $A000 window)
/// - $B000-$B007: CHR Regs (8 registers, one per 1KB CHR window; low 3 bits of addr select reg)
///
/// PRG banking:
/// - Mode 0: [$8000]=reg0, [$A000]=reg1, [$C000]={-2}, [$E000]={-1}
/// - Mode 1: [$8000]={-2}, [$A000]=reg1, [$C000]=reg0, [$E000]={-1}
pub struct IremG101Mapper {
    base: BaseMapper,
    prg_regs: [u8; 2],
    chr_regs: [u8; 8],
    prg_mode: bool,
    submapper: u8,
}

impl IremG101Mapper {
    // Control register ($9000) bit masks
    const CONTROL_PRG_MODE_BIT: u8 = 0b0000_0010;
    const CONTROL_MIRROR_BIT: u8 = 0b0000_0001;
    // CHR register select: low 3 bits of address in $B000 range
    const CHR_REG_SELECT_MASK: u16 = 0x0007;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: submapper != 1,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        if submapper == 1 {
            base.set_mirroring(NametableLayout::SingleScreenUpper);
        }
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
        let mut mapper = Self {
            base,
            prg_regs: [0; 2],
            chr_regs: [0; 8],
            prg_mode: false,
            submapper,
        };
        mapper.update_banks();
        mapper
    }

    #[cfg(test)]
    pub fn new_internal(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        let ctx = super::mapper::MapperContext::new_for_test(32, prg_rom, chr_rom, mirroring);
        Self::new(ctx)
    }

    fn update_banks(&mut self) {
        // PRG banking
        let r0 = self.prg_regs[0] as i16;
        let r1 = self.prg_regs[1] as i16;
        if !self.prg_mode {
            self.base.select_prg_page(0, r0);
            self.base.select_prg_page(1, r1);
            self.base.select_prg_page(2, -2);
            self.base.select_prg_page(3, -1);
        } else {
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(1, r1);
            self.base.select_prg_page(2, r0);
            self.base.select_prg_page(3, -1);
        }

        // CHR banking
        for (slot, &bank) in self.chr_regs.iter().enumerate() {
            self.base.select_chr_page(slot, bank as i16);
        }
    }

    /// Decodes the mirroring control bit (bit 0 of $9000) to a `NametableLayout`.
    fn mirroring_from_bit(bit: u8) -> NametableLayout {
        if bit != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        }
    }

    /// Encodes the current mirroring mode as a single bit for snapshot storage.
    fn mirroring_to_bit(mirroring: NametableLayout) -> u8 {
        if mirroring == NametableLayout::Horizontal {
            1
        } else {
            0
        }
    }
}

impl Mapper for IremG101Mapper {
    fn base(&self) -> Option<&BaseMapper> {
        Some(&self.base)
    }

    fn base_mut(&mut self) -> Option<&mut BaseMapper> {
        Some(&mut self.base)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xF000 {
            0x8000 => {
                self.prg_regs[0] = value & 0x1F;
                self.update_banks();
            }
            0x9000 => {
                if self.submapper != 1 {
                    self.prg_mode = (value & Self::CONTROL_PRG_MODE_BIT) != 0;
                    self.base
                        .set_mirroring(Self::mirroring_from_bit(value & Self::CONTROL_MIRROR_BIT));
                    self.update_banks();
                }
            }
            0xA000 => {
                self.prg_regs[1] = value & 0x1F;
                self.update_banks();
            }
            0xB000 => {
                let slot = (addr & Self::CHR_REG_SELECT_MASK) as usize;
                self.chr_regs[slot] = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base.read_chr_banked(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base.write_chr_banked(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(12);
        snap.extend_from_slice(&self.prg_regs);
        snap.extend_from_slice(&self.chr_regs);
        snap.push(self.prg_mode as u8);
        snap.push(Self::mirroring_to_bit(self.base.mirroring()));
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 12 {
            self.prg_regs.copy_from_slice(&data[0..2]);
            self.chr_regs.copy_from_slice(&data[2..10]);
            self.prg_mode = data[10] != 0;
            if self.submapper != 1 {
                self.base.set_mirroring(Self::mirroring_from_bit(data[11]));
            }
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IremG101Mapper;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper(prg_banks: usize, chr_banks_1k: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(8 * 1024, prg_banks);
        let chr_rom = banked_data(1024, chr_banks_1k);
        create_mapper(MapperContext::new_for_test(
            32,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 32 should be implemented")
    }

    fn make_mapper_submapper(
        prg_banks: usize,
        chr_banks_1k: usize,
        submapper: u8,
    ) -> Box<dyn Mapper> {
        let prg_rom = banked_data(8 * 1024, prg_banks);
        let chr_rom = banked_data(1024, chr_banks_1k);
        create_mapper(
            MapperContext::new_for_test(32, prg_rom, chr_rom, NametableLayout::Vertical)
                .with_submapper(submapper),
        )
        .expect("Mapper 32 should be implemented")
    }

    // --- PRG Mode 0 ---

    #[test]
    fn prg_mode0_reg0_selects_bank_at_8000() {
        // Use 6 banks to avoid power-of-two wrapping false-passes
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x8000, 2); // PRG reg 0 = bank 2
        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn prg_mode0_reg1_selects_bank_at_a000() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0xA000, 3); // PRG reg 1 = bank 3
        assert_eq!(mapper.read_prg(0xA000), 3);
    }

    #[test]
    fn prg_mode0_c000_is_fixed_second_to_last_bank() {
        let mapper = make_mapper(6, 8);
        // Second-to-last bank = bank 4 (of 6 banks: 0-5, so -2 = 4)
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    #[test]
    fn prg_mode0_e000_is_fixed_last_bank() {
        let mapper = make_mapper(6, 8);
        // Last bank = bank 5
        assert_eq!(mapper.read_prg(0xE000), 5);
    }

    #[test]
    fn prg_mode0_reg0_does_not_affect_c000() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x8000, 1); // change reg0, $C000 must stay fixed to {-2}
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    // --- PRG Mode 1 ---

    #[test]
    fn prg_mode1_8000_is_fixed_second_to_last_bank() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x9000, 0b0000_0010); // set PRG mode 1 (bit 1)
        assert_eq!(mapper.read_prg(0x8000), 4); // {-2} = bank 4
    }

    #[test]
    fn prg_mode1_c000_uses_reg0() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x8000, 1); // PRG reg 0 = bank 1
        mapper.write_prg(0x9000, 0b0000_0010); // set PRG mode 1
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn prg_mode1_a000_still_uses_reg1() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0xA000, 2); // PRG reg 1 = bank 2
        mapper.write_prg(0x9000, 0b0000_0010); // set PRG mode 1
        assert_eq!(mapper.read_prg(0xA000), 2);
    }

    #[test]
    fn prg_mode1_e000_is_still_fixed_last_bank() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x9000, 0b0000_0010); // PRG mode 1
        assert_eq!(mapper.read_prg(0xE000), 5); // last = bank 5
    }

    #[test]
    fn prg_mode_can_switch_back_to_mode0() {
        let mut mapper = make_mapper(6, 8);
        mapper.write_prg(0x8000, 1);
        mapper.write_prg(0x9000, 0b0000_0010); // mode 1
        mapper.write_prg(0x9000, 0b0000_0000); // back to mode 0
        assert_eq!(mapper.read_prg(0x8000), 1); // reg0 back at $8000
        assert_eq!(mapper.read_prg(0xC000), 4); // {-2} back at $C000
    }

    // --- CHR Banking ---

    #[test]
    fn chr_regs_select_1kb_banks() {
        // Use 11 CHR banks (non-power-of-two to prevent false-pass wrapping)
        let mut mapper = make_mapper(4, 11);
        for i in 0u8..8 {
            mapper.write_prg(0xB000 | i as u16, i); // CHR reg i = bank i
        }
        for i in 0u16..8 {
            assert_eq!(
                mapper.read_chr(i * 0x400),
                i as u8,
                "CHR window {} should map to bank {}",
                i,
                i
            );
        }
    }

    #[test]
    fn chr_regs_are_independently_selectable() {
        let mut mapper = make_mapper(4, 11);
        mapper.write_prg(0xB002, 5); // CHR reg 2 = bank 5
        mapper.write_prg(0xB005, 9); // CHR reg 5 = bank 9
        assert_eq!(mapper.read_chr(0x0800), 5); // $0800 is window 2
        assert_eq!(mapper.read_chr(0x1400), 9); // $1400 is window 5
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_bit0_0_is_vertical() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0x9000, 0b0000_0000); // bit 0 = 0 → vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit0_1_is_horizontal() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0x9000, 0b0000_0001); // bit 0 = 1 → horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- Submapper 1 (Major League) ---

    #[test]
    fn submapper1_hardwired_1screen_upper_mirroring() {
        let mapper = make_mapper_submapper(4, 8, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn submapper1_9000_write_does_not_change_mirroring() {
        let mut mapper = make_mapper_submapper(4, 8, 1);
        mapper.write_prg(0x9000, 0b0000_0001); // try to set horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn submapper1_9000_write_does_not_enable_prg_mode1() {
        let mut mapper = make_mapper_submapper(6, 8, 1);
        mapper.write_prg(0x8000, 1); // reg0 = bank 1
        mapper.write_prg(0x9000, 0b0000_0010); // try to set PRG mode 1
        // PRG mode must remain 0: reg0 must still be at $8000, not $C000
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xC000), 4); // still second-to-last
    }

    // --- Registers snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 6);
        let chr_rom = banked_data(1024, 11);
        let mut mapper = IremG101Mapper::new_internal(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        );

        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0xB003, 7);
        mapper.write_prg(0x9000, 0b0000_0011); // PRG mode 1, horizontal

        let snap = mapper.registers_snapshot();

        let mut restored =
            IremG101Mapper::new_internal(prg_rom, chr_rom, NametableLayout::Vertical);
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0xC000), 2); // mode 1: reg0 at $C000
        assert_eq!(restored.read_prg(0xA000), 3);
        assert_eq!(restored.read_chr(0x0C00), 7); // window 3 = bank 7
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }
}
