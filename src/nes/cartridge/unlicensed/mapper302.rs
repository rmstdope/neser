//! Mapper 302 – Kaiser7057
//!
//! Specifications:
//! - Primary: NesDev wiki
//! - Fallback: Mesen2 `Kaiser7057.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 302 – Kaiser7057
///
/// Hardware: Kaiser KS7057
///
/// Specifications:
/// - Primary: NesDev wiki; Fallback: Mesen2 `Kaiser7057.h`
/// - PRG-ROM: 128 KiB (64 × 2 KiB banks); four 2 KiB switchable at $8000–$9FFF,
///   four 2 KiB switchable at $6000–$7FFF, rest fixed
/// - CHR: 8 KiB ROM (single bank, unbanked)
/// - Mirroring: H / V programmable via writes to $8000–$9FFF (bit 0)
/// - IRQ: None
/// - Bus conflicts: None
///
/// Memory map:
/// ```text
///  CPU $6000-$67FF  PRG-ROM 2 KiB switchable (register regs[4])
///  CPU $6800-$6FFF  PRG-ROM 2 KiB switchable (register regs[5])
///  CPU $7000-$77FF  PRG-ROM 2 KiB switchable (register regs[6])
///  CPU $7800-$7FFF  PRG-ROM 2 KiB switchable (register regs[7])
///  CPU $8000-$87FF  PRG-ROM 2 KiB switchable (register regs[0])
///  CPU $8800-$8FFF  PRG-ROM 2 KiB switchable (register regs[1])
///  CPU $9000-$97FF  PRG-ROM 2 KiB switchable (register regs[2])
///  CPU $9800-$9FFF  PRG-ROM 2 KiB switchable (register regs[3])
///  CPU $A000-$BFFF  PRG-ROM 8 KiB fixed (banks 0x34–0x37)
///  CPU $C000-$DFFF  PRG-ROM 8 KiB fixed (banks 0x38–0x3B)
///  CPU $E000-$FFFF  PRG-ROM 8 KiB fixed (banks 0x3C–0x3F, i.e. last 8 KiB)
/// ```
///
/// Register map:
/// - `$8000–$9FFF` (write): bit 0 controls mirroring (0 = Horizontal, 1 = Vertical)
/// - `$B000` / `$B001`: regs[0] low / high nibble (slot $8000–$87FF)
/// - `$B002` / `$B003`: regs[1] low / high nibble (slot $8800–$8FFF)
/// - `$C000` / `$C001`: regs[2] low / high nibble (slot $9000–$97FF)
/// - `$C002` / `$C003`: regs[3] low / high nibble (slot $9800–$9FFF)
/// - `$D000` / `$D001`: regs[4] low / high nibble (slot $6000–$67FF)
/// - `$D002` / `$D003`: regs[5] low / high nibble (slot $6800–$6FFF)
/// - `$E000` / `$E001`: regs[6] low / high nibble (slot $7000–$77FF)
/// - `$E002` / `$E003`: regs[7] low / high nibble (slot $7800–$7FFF)
///
/// Nibble protocol: even address → write low nibble; odd address → write high nibble.
///
/// Power-on state: all registers zero, horizontal mirroring.
pub struct Mapper302 {
    base: BaseMapper,
    /// Eight 8-bit registers:
    /// - regs[0..4]: bank selectors for $8000–$9FFF (four 2 KiB slots)
    /// - regs[4..8]: bank selectors for $6000–$7FFF (four 2 KiB slots)
    regs: [u8; 8],
}

const PRG_PAGE_SIZE: usize = 0x800; // 2 KiB
const LOWER_NIBBLE_MASK: u8 = 0x0F;
const UPPER_NIBBLE_MASK: u8 = 0xF0;
const REGISTER_ADDR_MASK: u16 = 0xF002;
const FIXED_A_BASE: i16 = 0x34; // $A000–$BFFF fixed bank start
const FIXED_B_BASE: i16 = 0x38; // $C000–$DFFF fixed bank start
const FIXED_C_BASE: i16 = 0x3C; // $E000–$FFFF fixed bank start

impl Mapper302 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 2,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_PAGE_SIZE);
        let mut mapper = Self { base, regs: [0; 8] };
        mapper.update_state();
        mapper
    }

    /// Apply all registers to the PRG page tables and mirroring.
    ///
    /// - Slots 0..3 ($8000–$9FFF): switchable via regs[0..3]
    /// - Slots 4..7 ($A000–$BFFF): fixed to banks 0x34..0x37
    /// - Slots 8..11 ($C000–$DFFF): fixed to banks 0x38..0x3B
    /// - Slots 12..15 ($E000–$FFFF): fixed to banks 0x3C..0x3F
    /// - $6000–$7FFF: handled in read_prg via regs[4..7] (no page table slot)
    fn update_state(&mut self) {
        for i in 0..4 {
            self.base.select_prg_page(i, self.regs[i] as i16);
        }
        for i in 0..4usize {
            self.base.select_prg_page(4 + i, FIXED_A_BASE + i as i16);
        }
        for i in 0..4usize {
            self.base.select_prg_page(8 + i, FIXED_B_BASE + i as i16);
        }
        for i in 0..4usize {
            self.base.select_prg_page(12 + i, FIXED_C_BASE + i as i16);
        }
    }

    /// Nibble-split write to a register.
    ///
    /// - `low = true`: update lower nibble from `value & 0x0F`
    /// - `low = false`: update upper nibble from `(value & 0x0F) << 4`
    fn write_prg_reg(&mut self, index: usize, value: u8, low: bool) {
        if low {
            self.regs[index] = (self.regs[index] & UPPER_NIBBLE_MASK) | (value & LOWER_NIBBLE_MASK);
        } else {
            self.regs[index] =
                (self.regs[index] & LOWER_NIBBLE_MASK) | ((value & LOWER_NIBBLE_MASK) << 4);
        }
        self.update_state();
    }
}

impl Mapper for Mapper302 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let prg = self.base.prg_rom();
                let bank_count = prg.len() / PRG_PAGE_SIZE;
                if bank_count == 0 {
                    return 0;
                }
                let slot = (addr - 0x6000) as usize / PRG_PAGE_SIZE;
                let bank = self.regs[4 + slot] as usize % bank_count;
                let offset = (addr - 0x6000) as usize % PRG_PAGE_SIZE;
                let index = bank * PRG_PAGE_SIZE + offset;
                prg.get(index).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg(addr),
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        let low = (addr & 0x01) == 0x00;
        match addr & REGISTER_ADDR_MASK {
            0x8000 | 0x8002 | 0x9000 | 0x9002 => {
                self.base.set_mirroring(if (value & 0x01) != 0 {
                    NametableLayout::Vertical
                } else {
                    NametableLayout::Horizontal
                });
            }
            0xB000 => self.write_prg_reg(0, value, low),
            0xB002 => self.write_prg_reg(1, value, low),
            0xC000 => self.write_prg_reg(2, value, low),
            0xC002 => self.write_prg_reg(3, value, low),
            0xD000 => self.write_prg_reg(4, value, low),
            0xD002 => self.write_prg_reg(5, value, low),
            0xE000 => self.write_prg_reg(6, value, low),
            0xE002 => self.write_prg_reg(7, value, low),
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.regs.to_vec();
        snap.push(matches!(self.base.mirroring(), NametableLayout::Vertical) as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 8 {
            return;
        }
        self.regs.copy_from_slice(&data[0..8]);
        if let Some(&mir) = data.get(8) {
            self.base.set_mirroring(if mir != 0 {
                NametableLayout::Vertical
            } else {
                NametableLayout::Horizontal
            });
        }
        self.update_state();
    }

    fn reset(&mut self) {
        self.regs = [0; 8];
        self.base.set_mirroring(NametableLayout::Horizontal);
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};

    // 64 × 2 KiB = 128 KiB. Bank indices 0x00..0x3F are all valid, including
    // the fixed windows (0x34–0x3F). 64 is 2^6 — to avoid modulo-wrapping
    // false-passes in the switchable $8000–$9FFF window tests we use bank
    // values that cannot coincidentally wrap to the same slot.
    const PRG_BANKS: usize = 64;
    const PRG_BANK_SIZE: usize = 2 * 1024; // 2 KiB
    const CHR_SIZE: usize = 8 * 1024; // 8 KiB single CHR-ROM bank

    /// Build a 128 KiB PRG ROM where offset 0x100 in each 2 KiB bank stores
    /// the bank index (0–63) for identification.
    fn make_prg_rom() -> Vec<u8> {
        let mut rom = vec![0u8; PRG_BANKS * PRG_BANK_SIZE];
        for bank in 0..PRG_BANKS {
            rom[bank * PRG_BANK_SIZE + 0x100] = bank as u8;
        }
        rom
    }

    fn make_mapper() -> Mapper302 {
        Mapper302::new(MapperContext::new_for_test(
            302,
            make_prg_rom(),
            vec![0u8; CHR_SIZE],
            NametableLayout::Horizontal,
        ))
    }

    /// Read the 2 KiB bank marker from a window at the given base address.
    fn read_bank(mapper: &Mapper302, base: u16) -> u8 {
        mapper.read_prg(base + 0x100)
    }

    // --- Factory registration ---

    #[test]
    fn mapper_302_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            302,
            make_prg_rom(),
            vec![0u8; CHR_SIZE],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 302 must be registered in the factory"
        );
    }

    // --- Power-on state: switchable windows ---

    #[test]
    fn power_on_8000_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x8000),
            0,
            "$8000 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_8800_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x8800),
            0,
            "$8800 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_9000_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x9000),
            0,
            "$9000 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_9800_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x9800),
            0,
            "$9800 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_6000_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x6000),
            0,
            "$6000 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_6800_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x6800),
            0,
            "$6800 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_7000_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x7000),
            0,
            "$7000 must map bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_7800_window_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0x7800),
            0,
            "$7800 must map bank 0 at power-on"
        );
    }

    // --- Power-on state: fixed windows ---

    #[test]
    fn power_on_a000_fixed_to_bank_0x34() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xA000),
            0x34,
            "$A000 must be fixed to bank 0x34"
        );
    }

    #[test]
    fn power_on_a800_fixed_to_bank_0x35() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xA800),
            0x35,
            "$A800 must be fixed to bank 0x35"
        );
    }

    #[test]
    fn power_on_b000_fixed_to_bank_0x36() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xB000),
            0x36,
            "$B000 must be fixed to bank 0x36"
        );
    }

    #[test]
    fn power_on_b800_fixed_to_bank_0x37() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xB800),
            0x37,
            "$B800 must be fixed to bank 0x37"
        );
    }

    #[test]
    fn power_on_c000_fixed_to_bank_0x38() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xC000),
            0x38,
            "$C000 must be fixed to bank 0x38"
        );
    }

    #[test]
    fn power_on_c800_fixed_to_bank_0x39() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xC800),
            0x39,
            "$C800 must be fixed to bank 0x39"
        );
    }

    #[test]
    fn power_on_d000_fixed_to_bank_0x3a() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xD000),
            0x3A,
            "$D000 must be fixed to bank 0x3A"
        );
    }

    #[test]
    fn power_on_d800_fixed_to_bank_0x3b() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xD800),
            0x3B,
            "$D800 must be fixed to bank 0x3B"
        );
    }

    #[test]
    fn power_on_e000_fixed_to_bank_0x3c() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xE000),
            0x3C,
            "$E000 must be fixed to bank 0x3C"
        );
    }

    #[test]
    fn power_on_e800_fixed_to_bank_0x3d() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xE800),
            0x3D,
            "$E800 must be fixed to bank 0x3D"
        );
    }

    #[test]
    fn power_on_f000_fixed_to_bank_0x3e() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xF000),
            0x3E,
            "$F000 must be fixed to bank 0x3E"
        );
    }

    #[test]
    fn power_on_f800_fixed_to_bank_0x3f() {
        let mapper = make_mapper();
        assert_eq!(
            read_bank(&mapper, 0xF800),
            0x3F,
            "$F800 must be fixed to bank 0x3F"
        );
    }

    // --- PRG bank switching: $8000–$9FFF (regs[0..3]) ---

    #[test]
    fn reg0_low_nibble_write_selects_bank_at_8000() {
        let mut mapper = make_mapper();
        // Write low nibble = 5 to reg[0]: bank = 0x05
        mapper.write_prg(0xB000, 0x05); // low nibble of reg[0]
        assert_eq!(
            read_bank(&mapper, 0x8000),
            5,
            "reg[0] low nibble 5 → bank 5 at $8000"
        );
    }

    #[test]
    fn reg0_high_nibble_write_selects_bank_at_8000() {
        let mut mapper = make_mapper();
        // Write low nibble first: reg[0] = 0x05
        mapper.write_prg(0xB000, 0x05);
        // Write high nibble = 2 (value 0x02 → upper nibble = 0x20): reg[0] = 0x25
        mapper.write_prg(0xB001, 0x02); // high nibble of reg[0]
        assert_eq!(
            read_bank(&mapper, 0x8000),
            0x25,
            "reg[0] combined nibbles → bank 0x25 at $8000"
        );
    }

    #[test]
    fn reg1_selects_bank_at_8800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB002, 0x03); // reg[1] low nibble = 3
        mapper.write_prg(0xB003, 0x01); // reg[1] high nibble = 1 → bank 0x13
        assert_eq!(
            read_bank(&mapper, 0x8800),
            0x13,
            "reg[1] → bank 0x13 at $8800"
        );
    }

    #[test]
    fn reg2_selects_bank_at_9000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x07); // reg[2] low nibble = 7
        mapper.write_prg(0xC001, 0x00); // reg[2] high nibble = 0 → bank 0x07
        assert_eq!(
            read_bank(&mapper, 0x9000),
            0x07,
            "reg[2] → bank 0x07 at $9000"
        );
    }

    #[test]
    fn reg3_selects_bank_at_9800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC002, 0x0A); // reg[3] low nibble = A → 0x0A
        assert_eq!(
            read_bank(&mapper, 0x9800),
            0x0A,
            "reg[3] low nibble A → bank 0x0A at $9800"
        );
    }

    // --- PRG bank switching: $6000–$7FFF (regs[4..7]) ---

    #[test]
    fn reg4_selects_bank_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x06); // reg[4] low nibble = 6
        mapper.write_prg(0xD001, 0x01); // reg[4] high nibble = 1 → bank 0x16
        assert_eq!(
            read_bank(&mapper, 0x6000),
            0x16,
            "reg[4] → bank 0x16 at $6000"
        );
    }

    #[test]
    fn reg5_selects_bank_at_6800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD002, 0x08); // reg[5] low nibble = 8 → bank 0x08
        assert_eq!(
            read_bank(&mapper, 0x6800),
            0x08,
            "reg[5] low = 8 → bank 0x08 at $6800"
        );
    }

    #[test]
    fn reg6_selects_bank_at_7000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x0C); // reg[6] low nibble = C
        mapper.write_prg(0xE001, 0x02); // reg[6] high nibble = 2 → bank 0x2C
        assert_eq!(
            read_bank(&mapper, 0x7000),
            0x2C,
            "reg[6] → bank 0x2C at $7000"
        );
    }

    #[test]
    fn reg7_selects_bank_at_7800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE002, 0x09); // reg[7] low nibble = 9 → bank 0x09
        assert_eq!(
            read_bank(&mapper, 0x7800),
            0x09,
            "reg[7] low = 9 → bank 0x09 at $7800"
        );
    }

    // --- Address mask: writes to $B000–$BFFF all map reg[0] or reg[1] ---

    #[test]
    fn b100_write_affects_reg0_low_nibble() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB100, 0x0B); // $B100 & $F002 = $B000 → reg[0] low nibble
        assert_eq!(
            read_bank(&mapper, 0x8000),
            0x0B,
            "$B100 masked to $B000 → reg[0] low nibble"
        );
    }

    #[test]
    fn b103_write_affects_reg1_high_nibble() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB103, 0x03); // $B103 & $F002 = $B002, odd → reg[1] high nibble = 0x30
        assert_eq!(
            read_bank(&mapper, 0x8800),
            0x30,
            "$B103 masked to $B002 → reg[1] high nibble 3 → bank 0x30 at $8800"
        );
    }

    // --- Fixed windows are not affected by reg writes ---

    #[test]
    fn fixed_window_a000_unchanged_after_bank_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 0x0F); // change reg[0]
        assert_eq!(
            read_bank(&mapper, 0xA000),
            0x34,
            "$A000 fixed bank 0x34 unchanged after reg write"
        );
    }

    #[test]
    fn fixed_window_e000_unchanged_after_bank_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE002, 0x0F);
        assert_eq!(
            read_bank(&mapper, 0xE000),
            0x3C,
            "$E000 fixed bank 0x3C unchanged after reg[7] write"
        );
    }

    // --- Mirroring ---

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mapper 302 must use Horizontal mirroring at power-on"
        );
    }

    #[test]
    fn write_to_8000_bit0_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // bit 0 = 1 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Write 0x01 to $8000 must enable Vertical mirroring"
        );
    }

    #[test]
    fn write_to_8000_bit0_clear_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // Vertical
        mapper.write_prg(0x8000, 0x00); // back to Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Write 0x00 to $8000 must enable Horizontal mirroring"
        );
    }

    #[test]
    fn write_to_9002_also_controls_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9002, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Write to $9002 must also control mirroring"
        );
    }

    #[test]
    fn write_to_9fff_also_controls_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9FFF, 0x01); // $9FFF & $F002 = $9002 → mirroring
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Write to $9FFF ($9FFF & $F002 = $9002) must control mirroring"
        );
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 302 must never assert IRQ");
    }

    // --- Open-bus: $6000–$7FFF returns PRG-ROM (not open bus) ---

    #[test]
    fn read_prg_open_bus_at_6000_returns_prg_rom() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x05); // reg[4] = 0x05 → bank 5 at $6000
        let expected = mapper.read_prg(0x6100);
        let result = mapper.read_prg_open_bus(0x6100, 0xFF);
        assert_eq!(
            result, expected,
            "$6000–$7FFF open-bus read must return PRG-ROM, not open bus"
        );
    }

    // --- CHR ROM ---

    #[test]
    fn chr_reads_return_rom_data() {
        let mut chr = vec![0u8; CHR_SIZE];
        chr[0x0042] = 0xAB;
        let mut mapper = Mapper302::new(MapperContext::new_for_test(
            302,
            make_prg_rom(),
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_chr(0x0042),
            0xAB,
            "CHR ROM read at $0042 must return data"
        );
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips_prg_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 0x05); // reg[0] low = 5
        mapper.write_prg(0xB001, 0x02); // reg[0] high = 2 → bank 0x25
        mapper.write_prg(0xD000, 0x09); // reg[4] low = 9 → bank 0x09 at $6000

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            read_bank(&restored, 0x8000),
            0x25,
            "Snapshot round-trip: $8000 → bank 0x25"
        );
        assert_eq!(
            read_bank(&restored, 0x6000),
            0x09,
            "Snapshot round-trip: $6000 → bank 0x09"
        );
    }

    #[test]
    fn registers_snapshot_round_trips_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // Vertical

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "Snapshot round-trip: Vertical mirroring must be restored"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_clears_all_registers_and_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 0x05);
        mapper.write_prg(0xB001, 0x02); // reg[0] = 0x25
        mapper.write_prg(0xD000, 0x09); // reg[4] = 0x09
        mapper.write_prg(0x8000, 0x01); // Vertical

        mapper.reset();

        assert_eq!(
            read_bank(&mapper, 0x8000),
            0,
            "After reset $8000 must map bank 0"
        );
        assert_eq!(
            read_bank(&mapper, 0x6000),
            0,
            "After reset $6000 must map bank 0"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "After reset mirroring must be Horizontal"
        );
    }

    #[test]
    fn reset_preserves_fixed_windows() {
        let mut mapper = make_mapper();
        mapper.reset();
        assert_eq!(
            read_bank(&mapper, 0xE000),
            0x3C,
            "After reset $E000 still fixed to bank 0x3C"
        );
        assert_eq!(
            read_bank(&mapper, 0xA000),
            0x34,
            "After reset $A000 still fixed to bank 0x34"
        );
    }
}
