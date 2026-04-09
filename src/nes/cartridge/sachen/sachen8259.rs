//! Mappers 135, 137, 138, 139 – Sachen 8259 family (variants A, D, B, C)
//!
//! All four mappers share the same register interface but differ in CHR bank
//! granularity, CHR bank shift/or values, and nametable mirroring polarity.
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_137>
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_138>
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_139>
//! - Errata: <https://www.nesdev.org/wiki/NES_2.0_submappers/Sachen_8259#Mapper_135>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Two 1 KiB nametable pages backed by mapper-internal CIRAM.
const CIRAM_SIZE: usize = 0x800;

/// Which hardware variant the mapper implements.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Variant {
    /// Mapper 135 – 8259A, 2 KiB CHR banks, CHR shift = 1.
    A,
    /// Mapper 137 – 8259D, 1 KiB CHR banks, reversed mirroring polarity.
    D,
    /// Mapper 138 – 8259B, 2 KiB CHR banks, no shift or OR values.
    B,
    /// Mapper 139 – 8259C, 2 KiB CHR banks, CHR shift = 2, OR values applied.
    C,
}

pub struct Sachen8259 {
    base: BaseMapper,
    variant: Variant,
    /// Index of the register selected by the last write to 0x4100.
    reg_select: u8,
    /// Eight 3-bit registers.
    regs: [u8; 8],
    /// Internal CIRAM: two 1 KiB pages shared across all four nametable slots.
    ciram: [u8; CIRAM_SIZE],
}

impl Sachen8259 {
    pub fn new(ctx: MapperContext) -> Self {
        let variant = match ctx.mapper {
            135 => Variant::A,
            137 => Variant::D,
            138 => Variant::B,
            139 => Variant::C,
            n => panic!("unsupported mapper {n} for Sachen8259"),
        };

        let chr_kb = if variant == Variant::D { 1 } else { 2 };

        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: chr_kb,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(chr_kb * 1024);

        let mut mapper = Self {
            base,
            variant,
            reg_select: 0,
            regs: [0; 8],
            ciram: [0; CIRAM_SIZE],
        };
        mapper.apply_banks();
        mapper
    }

    /// True when `addr` falls in the 8259 register window ($4100/$4101).
    fn in_register_window(addr: u16) -> bool {
        addr & 0xC101 == 0x4100 || addr & 0xC101 == 0x4101
    }

    fn is_simple_mode(&self) -> bool {
        self.regs[7] & 0x04 != 0
    }

    fn mirroring_select(&self) -> u8 {
        self.regs[7] & 0x03
    }

    /// Nametable page (0 or 1) for the given quadrant index (0–3).
    fn nt_page_for_quadrant(&self, nt_index: usize) -> usize {
        let mm = if self.is_simple_mode() {
            0
        } else {
            self.mirroring_select()
        };

        match mm {
            0 => {
                // Mapper D: Horizontal; A/B/C: Vertical
                if self.variant == Variant::D {
                    usize::from(nt_index >= 2) // Horizontal (0,0,1,1)
                } else {
                    usize::from(nt_index == 1 || nt_index == 3) // Vertical (0,1,0,1)
                }
            }
            1 => {
                // Mapper D: Vertical; A/B/C: Horizontal
                if self.variant == Variant::D {
                    usize::from(nt_index == 1 || nt_index == 3) // Vertical
                } else {
                    usize::from(nt_index >= 2) // Horizontal
                }
            }
            2 => {
                // (0,1,1,1) – NT0 uses page 0; NT1/NT2/NT3 use page 1
                usize::from(nt_index > 0)
            }
            _ => {
                // Single screen, all use page 0
                0
            }
        }
    }

    fn apply_banks(&mut self) {
        self.apply_prg();
        self.apply_chr();
    }

    fn apply_prg(&mut self) {
        self.base.select_prg_page(0, (self.regs[5] & 0x07) as i16);
    }

    fn apply_chr(&mut self) {
        match self.variant {
            Variant::D => self.apply_chr_d(),
            _ => self.apply_chr_abc(),
        }
    }

    fn apply_chr_abc(&mut self) {
        // CHR bank shift and OR values per variant.
        let (shift, chr_or): (u8, [u8; 3]) = match self.variant {
            Variant::A => (1, [1, 0, 1]),
            Variant::B => (0, [0, 0, 0]),
            Variant::C => (2, [1, 2, 3]),
            Variant::D => unreachable!(),
        };

        let chr_high = (self.regs[4] & 0x07) << 3;
        let simple = self.is_simple_mode();

        for i in 0..4usize {
            let inner_reg = if simple && i > 0 {
                self.regs[0]
            } else {
                self.regs[i]
            };
            let inner = (chr_high | inner_reg) << shift;
            let or_val = if i == 0 { 0 } else { chr_or[i - 1] };
            self.base.select_chr_page(i, (inner | or_val) as i16);
        }
    }

    fn apply_chr_d(&mut self) {
        let simple = self.is_simple_mode();
        let r = self.regs;

        // Slots 0-3: 1 KiB switchable
        let slot0 = r[0];
        let slot1 = ((r[4] & 0x01) << 4) | if simple { r[0] } else { r[1] };
        let slot2 = ((r[4] & 0x02) << 3) | if simple { r[0] } else { r[2] };
        let slot3 = ((r[4] & 0x04) << 2) | ((r[6] & 0x01) << 3) | if simple { r[0] } else { r[3] };

        self.base.select_chr_page(0, slot0 as i16);
        self.base.select_chr_page(1, slot1 as i16);
        self.base.select_chr_page(2, slot2 as i16);
        self.base.select_chr_page(3, slot3 as i16);

        // Slots 4-7: fixed to the last 4 KiB of CHR-ROM.
        for i in 0..4usize {
            self.base.select_chr_page(4 + i, -4 + i as i16);
        }
    }
}

impl Mapper for Sachen8259 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_rom(addr);
        }
        0
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::in_register_window(addr) {
            if addr & 0x01 == 0 {
                // $4100: select register
                self.reg_select = value & 0x07;
            } else {
                // $4101: write to selected register (3 bits)
                self.regs[self.reg_select as usize] = value & 0x07;
                self.apply_banks();
            }
        }
    }

    fn reset(&mut self) {
        self.reg_select = 0;
        self.regs = [0; 8];
        self.ciram = [0; CIRAM_SIZE];
        self.apply_banks();
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }
        let nt_index = ((addr - 0x2000) >> 10) as usize;
        let page = self.nt_page_for_quadrant(nt_index);
        let offset = (addr as usize & 0x3FF) + page * 0x400;
        Some(self.ciram[offset])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }
        let nt_index = ((addr - 0x2000) >> 10) as usize;
        let page = self.nt_page_for_quadrant(nt_index);
        let offset = (addr as usize & 0x3FF) + page * 0x400;
        self.ciram[offset] = value;
        true
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(9 + CIRAM_SIZE);
        snap.push(self.reg_select);
        snap.extend_from_slice(&self.regs);
        snap.extend_from_slice(&self.ciram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 9 + CIRAM_SIZE {
            return;
        }
        self.reg_select = data[0] & 0x07;
        self.regs.copy_from_slice(&data[1..9]);
        self.ciram.copy_from_slice(&data[9..9 + CIRAM_SIZE]);
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 4; // 4 × 32 KiB = 128 KiB PRG-ROM
    const CHR_BANKS_2K: usize = 8; // 8 × 2 KiB = 16 KiB CHR-ROM
    const CHR_BANKS_1K: usize = 16; // 16 × 1 KiB = 16 KiB CHR-ROM (mapper D)

    fn make_mapper(number: u8, chr_banks: usize, chr_bank_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(32 * 1024, PRG_BANKS_32K);
        let chr_rom = banked_data(chr_bank_kb * 1024, chr_banks);
        create_mapper(MapperContext::new_for_test(
            number.into(),
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("Sachen8259 mapper should be creatable")
    }

    fn make_mapper_abc(number: u8) -> Box<dyn Mapper> {
        make_mapper(number, CHR_BANKS_2K, 2)
    }

    fn make_mapper_d() -> Box<dyn Mapper> {
        make_mapper(137, CHR_BANKS_1K, 1)
    }

    /// Write reg_select then value to the 8259 register interface.
    fn write_reg(m: &mut dyn Mapper, reg: u8, value: u8) {
        m.write_prg(0x4100, reg);
        m.write_prg(0x4101, value);
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_135_is_registered() {
        assert_eq!(make_mapper_abc(135).mapper_number(), 135);
    }

    #[test]
    fn mapper_137_is_registered() {
        assert_eq!(make_mapper_d().mapper_number(), 137);
    }

    #[test]
    fn mapper_138_is_registered() {
        assert_eq!(make_mapper_abc(138).mapper_number(), 138);
    }

    #[test]
    fn mapper_139_is_registered() {
        assert_eq!(make_mapper_abc(139).mapper_number(), 139);
    }

    // ── PRG banking (common) ─────────────────────────────────────────────────

    #[test]
    fn prg_bank_selected_by_reg5() {
        for num in [135u8, 137, 138, 139] {
            let chr_banks = if num == 137 {
                CHR_BANKS_1K
            } else {
                CHR_BANKS_2K
            };
            let chr_kb = if num == 137 { 1 } else { 2 };
            let mut m = make_mapper(num, chr_banks, chr_kb);

            write_reg(m.as_mut(), 5, 3);
            assert_eq!(m.read_prg(0x8000), 3, "mapper {num}: reg5=3 → PRG bank 3");

            write_reg(m.as_mut(), 5, 0);
            assert_eq!(m.read_prg(0x8000), 0, "mapper {num}: reg5=0 → PRG bank 0");
        }
    }

    // ── CHR banking – variant B (mapper 138, shift=0, no OR) ─────────────────

    #[test]
    fn mapper138_chr_reg0_selects_2k_bank_slot0() {
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 0, 3); // CHR slot 0 → bank 3
        assert_eq!(m.read_chr(0x0000), 3, "2 KiB slot 0 from reg[0]");
    }

    #[test]
    fn mapper138_chr_reg1_selects_bank_slot1() {
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 1, 5);
        assert_eq!(m.read_chr(0x0800), 5, "2 KiB slot 1 from reg[1]");
    }

    #[test]
    fn mapper138_simple_mode_forces_all_chr_slots_from_reg0() {
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 0, 2);
        write_reg(m.as_mut(), 1, 6); // would normally set slot 1 to bank 6
        write_reg(m.as_mut(), 7, 0b100); // simple mode on
        // In simple mode all slots use reg[0] = 2 (no shift, no OR for variant B)
        assert_eq!(m.read_chr(0x0800), 2, "simple mode: slot 1 uses reg[0]");
    }

    #[test]
    fn mapper138_reg4_extends_chr_high_bits() {
        // With variant B (shift=0, no OR): bank = (reg4 << 3) | regN
        // 8 × 2 KiB banks means (reg4 & 7)<<3 selects the upper 3 bits.
        // Our test ROM only has 8 banks (0-7), so use reg4=0, reg0=7.
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 4, 0);
        write_reg(m.as_mut(), 0, 7);
        assert_eq!(m.read_chr(0x0000), 7);
    }

    // ── CHR banking – variant A (mapper 135, shift=1, OR=[1,0,1]) ─────────────

    #[test]
    fn mapper135_chr_slot0_shifted_left_1() {
        // slot 0: bank = (reg4<<3 | reg0) << 1; reg0=2 → bank 4
        let mut m = make_mapper_abc(135);
        write_reg(m.as_mut(), 0, 2);
        assert_eq!(m.read_chr(0x0000), 4, "variant A: slot 0 = inner<<1 = 4");
    }

    #[test]
    fn mapper135_chr_slot1_shifted_and_orred() {
        // slot 1: bank = (inner << 1) | 1; reg1=1 → inner=1, (1<<1)|1 = 3
        let mut m = make_mapper_abc(135);
        write_reg(m.as_mut(), 1, 1);
        assert_eq!(m.read_chr(0x0800), 3, "variant A: slot 1 = (inner<<1)|1");
    }

    // ── CHR banking – variant C (mapper 139, shift=2, OR=[1,2,3]) ─────────────

    #[test]
    fn mapper139_chr_slot0_shifted_left_2() {
        // slot 0: bank = reg0 << 2; reg0=1 → bank 4
        let mut m = make_mapper_abc(139);
        write_reg(m.as_mut(), 0, 1);
        assert_eq!(m.read_chr(0x0000), 4, "variant C: slot 0 = reg0<<2 = 4");
    }

    #[test]
    fn mapper139_chr_slot2_shifted_and_orred() {
        // slot 2: bank = (inner << 2) | 2; reg2=1 → (1<<2)|2 = 6
        let mut m = make_mapper_abc(139);
        write_reg(m.as_mut(), 2, 1);
        assert_eq!(m.read_chr(0x1000), 6, "variant C: slot 2 = (1<<2)|2 = 6");
    }

    // ── CHR banking – variant D (mapper 137, 1 KiB banks) ────────────────────

    #[test]
    fn mapper137_chr_slot3_uses_reg6_bit3() {
        // slot 3: ((reg4&4)<<2) | ((reg6&1)<<3) | reg3
        // reg3=1, reg6=1 → 0|8|1 = 9
        let mut m = make_mapper_d();
        write_reg(m.as_mut(), 3, 1);
        write_reg(m.as_mut(), 6, 1);
        assert_eq!(m.read_chr(0x0C00), 9, "slot 3 uses reg6 bit 3");
    }

    #[test]
    fn mapper137_slots_4_to_7_fixed_to_last_4k() {
        let mut m = make_mapper_d();
        // 16 × 1 KiB banks (0-15); last 4 KiB = banks 12-15
        assert_eq!(
            m.read_chr(0x1000),
            12,
            "slot 4 = second-to-last-4K: bank 12"
        );
        assert_eq!(m.read_chr(0x1C00), 15, "slot 7 = last 1 KiB bank");
    }

    // ── Nametable mirroring – variant A/B/C ───────────────────────────────────

    #[test]
    fn mapper138_mm0_is_vertical_mirroring() {
        // mm=0, not D → Vertical: NT0&NT2 = page 0, NT1&NT3 = page 1
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 7, 0b000); // mm=0, simple=off

        m.write_nametable(0x2000, 0xAA); // NT0 → page 0
        m.write_nametable(0x2400, 0xBB); // NT1 → page 1

        assert_eq!(m.read_nametable(0x2000), Some(0xAA), "NT0 page 0");
        assert_eq!(m.read_nametable(0x2800), Some(0xAA), "NT2 mirrors page 0");
        assert_eq!(m.read_nametable(0x2400), Some(0xBB), "NT1 page 1");
        assert_eq!(m.read_nametable(0x2C00), Some(0xBB), "NT3 mirrors page 1");
    }

    #[test]
    fn mapper138_mm1_is_horizontal_mirroring() {
        // mm=1, not D → Horizontal: NT0&NT1 = page 0, NT2&NT3 = page 1
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 7, 0b001);

        m.write_nametable(0x2000, 0x11); // NT0 → page 0
        m.write_nametable(0x2800, 0x22); // NT2 → page 1

        assert_eq!(m.read_nametable(0x2000), Some(0x11));
        assert_eq!(m.read_nametable(0x2400), Some(0x11), "NT1 mirrors page 0");
        assert_eq!(m.read_nametable(0x2800), Some(0x22));
        assert_eq!(m.read_nametable(0x2C00), Some(0x22), "NT3 mirrors page 1");
    }

    #[test]
    fn mapper138_mm2_assigns_nt0_to_page0_rest_to_page1() {
        // mm=2 → (0,1,1,1): NT0→page0, NT1/NT2/NT3→page1
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 7, 0b010);

        m.write_nametable(0x2000, 0xAA); // NT0 → page 0
        m.write_nametable(0x2400, 0xBB); // NT1 → page 1

        assert_eq!(m.read_nametable(0x2000), Some(0xAA), "NT0 → page 0");
        assert_eq!(m.read_nametable(0x2400), Some(0xBB), "NT1 → page 1");
        assert_eq!(m.read_nametable(0x2800), Some(0xBB), "NT2 mirrors page 1");
        assert_eq!(m.read_nametable(0x2C00), Some(0xBB), "NT3 mirrors page 1");
    }

    #[test]
    fn mapper138_mm3_is_single_screen_all_page0() {
        // mm=3 → all four NTs map to page 0
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 7, 0b011);

        m.write_nametable(0x2000, 0xCC);
        // All quadrants should read the same page-0 value.
        assert_eq!(m.read_nametable(0x2000), Some(0xCC));
        assert_eq!(m.read_nametable(0x2400), Some(0xCC));
        assert_eq!(m.read_nametable(0x2800), Some(0xCC));
        assert_eq!(m.read_nametable(0x2C00), Some(0xCC));
    }

    #[test]
    fn mapper138_simple_mode_forces_mm0_vertical() {
        // simple mode (bit 2 of reg7) ignores mm bits and forces mm=0 for A/B/C → Vertical
        let mut m = make_mapper_abc(138);
        write_reg(m.as_mut(), 7, 0b101); // simple=1, mm=1 (would normally be Horizontal)

        m.write_nametable(0x2000, 0xDD); // NT0 → page 0
        m.write_nametable(0x2400, 0xEE); // NT1 → page 1
        // Should behave as Vertical (mm=0)
        assert_eq!(
            m.read_nametable(0x2800),
            Some(0xDD),
            "NT2 mirrors NT0 (Vertical)"
        );
        assert_eq!(
            m.read_nametable(0x2C00),
            Some(0xEE),
            "NT3 mirrors NT1 (Vertical)"
        );
    }

    // ── Nametable mirroring – variant D (mapper 137) ──────────────────────────

    #[test]
    fn mapper137_mm0_is_horizontal_mirroring() {
        // mm=0 for D → Horizontal
        let mut m = make_mapper_d();
        write_reg(m.as_mut(), 7, 0b000);

        m.write_nametable(0x2000, 0x11); // NT0 → page 0
        m.write_nametable(0x2800, 0x22); // NT2 → page 1

        assert_eq!(m.read_nametable(0x2000), Some(0x11));
        assert_eq!(m.read_nametable(0x2400), Some(0x11), "NT1 mirrors page 0");
        assert_eq!(m.read_nametable(0x2800), Some(0x22));
        assert_eq!(m.read_nametable(0x2C00), Some(0x22), "NT3 mirrors page 1");
    }

    #[test]
    fn mapper137_mm1_is_vertical_mirroring() {
        // mm=1 for D → Vertical
        let mut m = make_mapper_d();
        write_reg(m.as_mut(), 7, 0b001);

        m.write_nametable(0x2000, 0xAA); // NT0 → page 0
        m.write_nametable(0x2400, 0xBB); // NT1 → page 1

        assert_eq!(
            m.read_nametable(0x2800),
            Some(0xAA),
            "NT2 mirrors page 0 (Vertical)"
        );
        assert_eq!(
            m.read_nametable(0x2C00),
            Some(0xBB),
            "NT3 mirrors page 1 (Vertical)"
        );
    }

    // ── Reset and snapshot ────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_state() {
        for num in [135u8, 137, 138, 139] {
            let chr_banks = if num == 137 {
                CHR_BANKS_1K
            } else {
                CHR_BANKS_2K
            };
            let chr_kb = if num == 137 { 1 } else { 2 };
            let mut m = make_mapper(num, chr_banks, chr_kb);

            write_reg(m.as_mut(), 5, 2);
            write_reg(m.as_mut(), 0, 3);
            m.write_nametable(0x2000, 0xFF);
            m.reset();

            assert_eq!(m.read_prg(0x8000), 0, "mapper {num}: PRG bank reset");
            assert_eq!(m.read_chr(0x0000), 0, "mapper {num}: CHR bank reset");
            assert_eq!(
                m.read_nametable(0x2000),
                Some(0x00),
                "mapper {num}: CIRAM cleared"
            );
        }
    }

    #[test]
    fn snapshot_and_restore_preserve_full_state() {
        let mut m = make_mapper_abc(138);

        write_reg(m.as_mut(), 5, 2); // PRG bank 2
        write_reg(m.as_mut(), 0, 3); // CHR slot 0 → bank 3
        write_reg(m.as_mut(), 7, 0b001); // Horizontal mirroring
        m.write_nametable(0x2000, 0x42);

        let snap = m.registers_snapshot();
        m.reset();
        m.restore_registers(&snap);

        assert_eq!(m.read_prg(0x8000), 2);
        assert_eq!(m.read_chr(0x0000), 3);
        assert_eq!(m.read_nametable(0x2000), Some(0x42));
    }
}
