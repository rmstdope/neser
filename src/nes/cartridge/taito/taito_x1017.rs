//! Mapper 082 – Taito X1-017
//!
//! Specifications:
//! - Fallback: Mesen2 `TaitoX1017.h` (NesDev wiki returns 403 from sandbox)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 082 – Taito X1-017
///
/// Hardware: Taito X1-017
///
/// Specifications:
/// - Fallback: Mesen2 `TaitoX1017.h` (NesDev sandbox blocks direct wiki access)
/// - PRG-ROM: Up to 512 KiB (64 × 8 KiB banks); three 8 KiB switchable + last fixed
/// - CHR: Up to 256 KiB; six 1 KiB registers controlling 2×2 KB + 4×1 KB (mode-dependent)
/// - Mirroring: H / V programmable via register $7EF6 bit 0
/// - IRQ: None
/// - PRG-RAM: 5 KiB (5 × 1 KiB pages) with password-based enable per region
///
/// Memory map:
/// ```text
///  CPU $6000-$63FF  1 KB PRG-RAM (bank 0; enabled when $7EF7 == $CA)
///  CPU $6400-$67FF  1 KB PRG-RAM (bank 1; enabled when $7EF7 == $CA)
///  CPU $6800-$6BFF  1 KB PRG-RAM (bank 2; enabled when $7EF8 == $69)
///  CPU $6C00-$6FFF  1 KB PRG-RAM (bank 3; enabled when $7EF8 == $69)
///  CPU $7000-$73FF  1 KB PRG-RAM (bank 4; enabled when $7EF9 == $84)
///  CPU $7400-$7EEF  Open bus
///  CPU $7EF0-$7EFF  Registers (write-only; reads return open bus)
///  CPU $8000-$9FFF  PRG page 0 (switchable via $7EFA, bank = value >> 2)
///  CPU $A000-$BFFF  PRG page 1 (switchable via $7EFB, bank = value >> 2)
///  CPU $C000-$DFFF  PRG page 2 (switchable via $7EFC, bank = value >> 2)
///  CPU $E000-$FFFF  PRG page 3 (fixed to last bank)
/// ```
///
/// CHR layout depends on `chr_mode` (bit 1 of register $7EF6):
///
/// Mode 0 (default):
/// ```text
///  PPU $0000-$07FF  2 KB → chr_regs[0] & 0xFE
///  PPU $0800-$0FFF  2 KB → chr_regs[1] & 0xFE
///  PPU $1000-$13FF  1 KB → chr_regs[2]
///  PPU $1400-$17FF  1 KB → chr_regs[3]
///  PPU $1800-$1BFF  1 KB → chr_regs[4]
///  PPU $1C00-$1FFF  1 KB → chr_regs[5]
/// ```
///
/// Mode 1:
/// ```text
///  PPU $0000-$03FF  1 KB → chr_regs[2]
///  PPU $0400-$07FF  1 KB → chr_regs[3]
///  PPU $0800-$0BFF  1 KB → chr_regs[4]
///  PPU $0C00-$0FFF  1 KB → chr_regs[5]
///  PPU $1000-$17FF  2 KB → chr_regs[0] & 0xFE
///  PPU $1800-$1FFF  2 KB → chr_regs[1] & 0xFE
/// ```
///
/// Register map ($7EF0-$7EFF):
/// - `$7EF0`–`$7EF5`: CHR bank registers 0–5
/// - `$7EF6` bit 0: mirroring (0 = Horizontal, 1 = Vertical)
/// - `$7EF6` bit 1: CHR mode (0 = mode0, 1 = mode1)
/// - `$7EF7`: PRG-RAM enable for $6000-$67FF (password = $CA)
/// - `$7EF8`: PRG-RAM enable for $6800-$6FFF (password = $69)
/// - `$7EF9`: PRG-RAM enable for $7000-$73FF (password = $84)
/// - `$7EFA`: PRG bank 0 (bits[7:2]) for $8000-$9FFF
/// - `$7EFB`: PRG bank 1 (bits[7:2]) for $A000-$BFFF
/// - `$7EFC`: PRG bank 2 (bits[7:2]) for $C000-$DFFF
///
/// Power-on state: all registers zero, RAM disabled, page 3 fixed to last bank.
pub struct TaitoX1017Mapper {
    base: BaseMapper,
    /// PRG bank selectors for pages 0–2 (bits[7:2] of write value).
    prg_regs: [u8; 3],
    /// CHR bank registers 0–5 (written via $7EF0–$7EF5).
    chr_regs: [u8; 6],
    /// CHR mode: false = mode 0, true = mode 1.
    chr_mode: bool,
    /// 5 KiB internal save RAM (5 × 1 KiB pages at offsets 0x000–0x13FF).
    ram: [u8; 0x1400],
    /// Password registers for RAM enable ($7EF7, $7EF8, $7EF9).
    ram_perm: [u8; 3],
}

impl TaitoX1017Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB

    /// Passwords required to enable each RAM region.
    const RAM_PASS: [u8; 3] = [0xCA, 0x69, 0x84];

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_regs: [0; 3],
            chr_regs: [0; 6],
            chr_mode: false,
            ram: [0; 0x1400],
            ram_perm: [0; 3],
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.update_prg_banks();
        self.update_chr_banks();
    }

    fn update_prg_banks(&mut self) {
        for i in 0..3 {
            self.base.select_prg_page(i, self.prg_regs[i] as i16);
        }
        self.base.select_prg_page(3, -1); // $E000 fixed to last bank
    }

    fn update_chr_banks(&mut self) {
        if self.chr_mode {
            self.apply_chr_mode1();
        } else {
            self.apply_chr_mode0();
        }
    }

    /// CHR mode 0: lower $0000–$0FFF = 2×2 KB (regs 0–1), upper $1000–$1FFF = 4×1 KB (regs 2–5).
    fn apply_chr_mode0(&mut self) {
        let b0 = (self.chr_regs[0] & 0xFE) as i16;
        let b1 = (self.chr_regs[1] & 0xFE) as i16;
        self.base.select_chr_page(0, b0);
        self.base.select_chr_page(1, b0 + 1);
        self.base.select_chr_page(2, b1);
        self.base.select_chr_page(3, b1 + 1);
        self.base.select_chr_page(4, self.chr_regs[2] as i16);
        self.base.select_chr_page(5, self.chr_regs[3] as i16);
        self.base.select_chr_page(6, self.chr_regs[4] as i16);
        self.base.select_chr_page(7, self.chr_regs[5] as i16);
    }

    /// CHR mode 1: lower $0000–$0FFF = 4×1 KB (regs 2–5), upper $1000–$1FFF = 2×2 KB (regs 0–1).
    fn apply_chr_mode1(&mut self) {
        self.base.select_chr_page(0, self.chr_regs[2] as i16);
        self.base.select_chr_page(1, self.chr_regs[3] as i16);
        self.base.select_chr_page(2, self.chr_regs[4] as i16);
        self.base.select_chr_page(3, self.chr_regs[5] as i16);
        let b0 = (self.chr_regs[0] & 0xFE) as i16;
        let b1 = (self.chr_regs[1] & 0xFE) as i16;
        self.base.select_chr_page(4, b0);
        self.base.select_chr_page(5, b0 + 1);
        self.base.select_chr_page(6, b1);
        self.base.select_chr_page(7, b1 + 1);
    }

    /// Returns whether the RAM region covering `addr` is unlocked by its password.
    fn ram_enabled(&self, addr: u16) -> bool {
        match addr {
            0x6000..=0x67FF => self.ram_perm[0] == Self::RAM_PASS[0],
            0x6800..=0x6FFF => self.ram_perm[1] == Self::RAM_PASS[1],
            0x7000..=0x73FF => self.ram_perm[2] == Self::RAM_PASS[2],
            _ => false,
        }
    }

    /// Reads a byte from internal RAM at `addr`. Returns `None` if the region is locked.
    fn read_ram(&self, addr: u16) -> Option<u8> {
        if self.ram_enabled(addr) {
            Some(self.ram[(addr - 0x6000) as usize])
        } else {
            None
        }
    }

    /// Decodes the nametable layout from bit 0 of a register value.
    fn decode_mirroring(value: u8) -> NametableLayout {
        if value & 0x01 != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        }
    }
}

impl Mapper for TaitoX1017Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x73FF => self.read_ram(addr).unwrap_or(0), // disabled RAM reads as 0
            0x7400..=0x7FFF => 0, // open bus (unmapped / write-only registers)
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x73FF => self.read_ram(addr).unwrap_or(open_bus),
            0x7400..=0x7FFF => open_bus,
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x73FF => {
                if self.ram_enabled(addr) {
                    self.ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x7EF0..=0x7EF5 => {
                self.chr_regs[(addr - 0x7EF0) as usize] = value;
                self.update_banks();
            }
            0x7EF6 => {
                self.base.set_mirroring(Self::decode_mirroring(value));
                self.chr_mode = (value & 0x02) != 0;
                self.update_banks();
            }
            0x7EF7..=0x7EF9 => {
                self.ram_perm[(addr - 0x7EF7) as usize] = value;
            }
            0x7EFA..=0x7EFC => {
                self.prg_regs[(addr - 0x7EFA) as usize] = value >> 2;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let is_vertical = self.base.mirroring() == NametableLayout::Vertical;
        let mut snapshot = vec![is_vertical as u8, self.chr_mode as u8];
        snapshot.extend_from_slice(&self.prg_regs);
        snapshot.extend_from_slice(&self.chr_regs);
        snapshot.extend_from_slice(&self.ram_perm);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const SNAPSHOT_SIZE: usize = 14; // 2 flags + 3 prg + 6 chr + 3 ram_perm
        if data.len() < SNAPSHOT_SIZE {
            return;
        }
        self.base.set_mirroring(Self::decode_mirroring(data[0]));
        self.chr_mode = data[1] != 0;
        self.prg_regs.copy_from_slice(&data[2..5]);
        self.chr_regs.copy_from_slice(&data[5..11]);
        self.ram_perm.copy_from_slice(&data[11..14]);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 3];
        self.chr_regs = [0; 6];
        self.chr_mode = false;
        self.ram_perm = [0; 3];
        self.base.set_mirroring(NametableLayout::Horizontal);
        self.update_banks();
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        crate::nes::console::initialize_ram(&mut self.ram, mode);
        self.base.initialize_ram(mode);
    }

    fn wram_size(&self) -> usize {
        self.ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.ram.to_vec()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(self.ram.len());
        self.ram[..len].copy_from_slice(&data[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two sizes to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 11; // 11 × 8 KB
    const CHR_BANKS: usize = 13; // 13 × 1 KB

    fn make_mapper() -> TaitoX1017Mapper {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        TaitoX1017Mapper::new(MapperContext::new_for_test(
            82,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_82_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            82,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 82 must be registered");
    }

    // --- Power-on PRG state ---

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_a000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "$A000 must be PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must be PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_e000_is_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must be fixed to last PRG bank at power-on"
        );
    }

    // --- PRG bank switching ---

    #[test]
    fn prg_page0_switches_via_7efa() {
        let mut mapper = make_mapper();
        // value >> 2 = bank; write 8 << 2 = 0x20 to select bank 8
        mapper.write_prg(0x7EFA, 8 << 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 must reflect bank written to $7EFA"
        );
    }

    #[test]
    fn prg_page1_switches_via_7efb() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EFB, 5 << 2);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 must reflect bank written to $7EFB"
        );
    }

    #[test]
    fn prg_page2_switches_via_7efc() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EFC, 3 << 2);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must reflect bank written to $7EFC"
        );
    }

    #[test]
    fn prg_page0_bank_uses_bits_7_to_2_only() {
        let mut mapper = make_mapper();
        // value = 0b0010_0011: bits[7:2] = 0b001000 = 8, bits[1:0] ignored
        mapper.write_prg(0x7EFA, (8 << 2) | 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "PRG bank must use bits[7:2] only"
        );
    }

    #[test]
    fn prg_e000_stays_fixed_after_register_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EFA, 5 << 2);
        mapper.write_prg(0x7EFB, 5 << 2);
        mapper.write_prg(0x7EFC, 5 << 2);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must remain fixed to last bank"
        );
    }

    // --- CHR banking mode 0 ---

    #[test]
    fn chr_mode0_lower_2kb_from_reg0() {
        let mut mapper = make_mapper();
        // reg0 = 4 → 2KB at $0000 covers CHR banks 4 and 5 (LSB cleared)
        mapper.write_prg(0x7EF0, 4);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR $0000 must come from reg0 bank"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "CHR $0400 must come from reg0 bank+1"
        );
    }

    #[test]
    fn chr_mode0_reg0_lsb_is_cleared() {
        let mut mapper = make_mapper();
        // reg0 = 5 (odd) → LSB cleared → 2KB at banks 4,5
        mapper.write_prg(0x7EF0, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR $0000 must use reg0 with LSB cleared"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5,
            "CHR $0400 must use reg0+1 with LSB cleared"
        );
    }

    #[test]
    fn chr_mode0_lower_2kb_from_reg1() {
        let mut mapper = make_mapper();
        // reg1 = 6 → 2KB at $0800 covers CHR banks 6 and 7
        mapper.write_prg(0x7EF1, 6);
        assert_eq!(
            mapper.read_chr(0x0800),
            6,
            "CHR $0800 must come from reg1 bank"
        );
        assert_eq!(
            mapper.read_chr(0x0C00),
            7,
            "CHR $0C00 must come from reg1 bank+1"
        );
    }

    #[test]
    fn chr_mode0_upper_1kb_slots() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF2, 2);
        mapper.write_prg(0x7EF3, 3);
        mapper.write_prg(0x7EF4, 4);
        mapper.write_prg(0x7EF5, 5);
        assert_eq!(mapper.read_chr(0x1000), 2, "CHR $1000 must come from reg2");
        assert_eq!(mapper.read_chr(0x1400), 3, "CHR $1400 must come from reg3");
        assert_eq!(mapper.read_chr(0x1800), 4, "CHR $1800 must come from reg4");
        assert_eq!(mapper.read_chr(0x1C00), 5, "CHR $1C00 must come from reg5");
    }

    // --- CHR banking mode 1 ---

    #[test]
    fn chr_mode1_lower_1kb_slots_from_regs_2_to_5() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF6, 0x02); // set chr_mode = 1
        mapper.write_prg(0x7EF2, 2);
        mapper.write_prg(0x7EF3, 3);
        mapper.write_prg(0x7EF4, 4);
        mapper.write_prg(0x7EF5, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "mode1: CHR $0000 must come from reg2"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            3,
            "mode1: CHR $0400 must come from reg3"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            4,
            "mode1: CHR $0800 must come from reg4"
        );
        assert_eq!(
            mapper.read_chr(0x0C00),
            5,
            "mode1: CHR $0C00 must come from reg5"
        );
    }

    #[test]
    fn chr_mode1_upper_2kb_from_reg0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF6, 0x02); // chr_mode = 1
        // reg0 = 8 → 2KB at $1000 covers banks 8,9
        mapper.write_prg(0x7EF0, 8);
        assert_eq!(
            mapper.read_chr(0x1000),
            8,
            "mode1: CHR $1000 must come from reg0 bank"
        );
        assert_eq!(
            mapper.read_chr(0x1400),
            9,
            "mode1: CHR $1400 must come from reg0+1"
        );
    }

    #[test]
    fn chr_mode1_upper_2kb_from_reg1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF6, 0x02); // chr_mode = 1
        mapper.write_prg(0x7EF1, 10);
        assert_eq!(
            mapper.read_chr(0x1800),
            10,
            "mode1: CHR $1800 must come from reg1 bank"
        );
        assert_eq!(
            mapper.read_chr(0x1C00),
            11,
            "mode1: CHR $1C00 must come from reg1+1"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_bit0_0_sets_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF6, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit 0 = 0 must select horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_bit0_1_sets_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF6, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit 0 = 1 must select vertical mirroring"
        );
    }

    // --- RAM enable / disable ---

    #[test]
    fn ram_group0_disabled_by_default() {
        let mapper = make_mapper();
        // Default: ram_perm[0] = 0 ≠ 0xCA → reads return open bus / 0
        let val = mapper.read_prg(0x6000);
        // When disabled, read_prg returns 0 (open bus approximation)
        assert_eq!(val, 0, "RAM group 0 must be inaccessible when disabled");
    }

    #[test]
    fn ram_group0_enabled_with_correct_password() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA); // correct password
        mapper.write_prg(0x6000, 0xAB); // write to RAM
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "RAM group 0 must be readable/writable when enabled with password $CA"
        );
    }

    #[test]
    fn ram_group0_disabled_with_wrong_password() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA); // enable first
        mapper.write_prg(0x6000, 0xAB); // write data
        mapper.write_prg(0x7EF7, 0x00); // disable
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "RAM group 0 must return 0 after password is removed"
        );
    }

    #[test]
    fn ram_group0_bank1_at_6400() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x6400, 0x55);
        assert_eq!(
            mapper.read_prg(0x6400),
            0x55,
            "RAM bank 1 at $6400 must be accessible when group 0 enabled"
        );
    }

    #[test]
    fn ram_group1_enabled_with_correct_password() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF8, 0x69); // correct password
        mapper.write_prg(0x6800, 0xCC);
        assert_eq!(
            mapper.read_prg(0x6800),
            0xCC,
            "RAM group 1 must be accessible with password $69"
        );
    }

    #[test]
    fn ram_group1_bank3_at_6c00() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF8, 0x69);
        mapper.write_prg(0x6C00, 0x77);
        assert_eq!(
            mapper.read_prg(0x6C00),
            0x77,
            "RAM bank 3 at $6C00 must be accessible when group 1 enabled"
        );
    }

    #[test]
    fn ram_group2_enabled_with_correct_password() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF9, 0x84); // correct password
        mapper.write_prg(0x7000, 0xDD);
        assert_eq!(
            mapper.read_prg(0x7000),
            0xDD,
            "RAM bank 4 at $7000 must be accessible with password $84"
        );
    }

    #[test]
    fn ram_group2_top_boundary_73ff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF9, 0x84);
        mapper.write_prg(0x73FF, 0xEE);
        assert_eq!(
            mapper.read_prg(0x73FF),
            0xEE,
            "RAM top boundary $73FF must be accessible"
        );
    }

    #[test]
    fn ram_groups_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x6000, 0x11);
        // Group 1 not enabled → $6800 should return 0
        assert_eq!(
            mapper.read_prg(0x6800),
            0,
            "Enabling group 0 must not enable group 1"
        );
    }

    // --- Open-bus region ---

    #[test]
    fn open_bus_region_7400_to_7eef() {
        let mapper = make_mapper();
        // $7400-$7EEF should return open bus (0 from read_prg)
        assert_eq!(mapper.read_prg(0x7400), 0, "$7400 must return 0 (open bus)");
        assert_eq!(mapper.read_prg(0x7EEF), 0, "$7EEF must return 0 (open bus)");
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "Mapper 82 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EFA, 5 << 2);
        mapper.write_prg(0x7EFB, 3 << 2);
        mapper.write_prg(0x7EFC, 2 << 2);
        mapper.write_prg(0x7EF0, 6);
        mapper.write_prg(0x7EF1, 8);
        mapper.write_prg(0x7EF6, 0x03); // vertical + mode1
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x7EF8, 0x69);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "snapshot: $8000"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            mapper.read_prg(0xA000),
            "snapshot: $A000"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "snapshot: $C000"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "snapshot: mirroring"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "snapshot: chr $0000"
        );
    }

    // --- WRAM snapshot ---

    #[test]
    fn wram_snapshot_preserves_ram_contents() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x6100, 0x99);

        let snap = mapper.wram_snapshot();
        assert_eq!(snap[0x000], 0x42, "wram snapshot byte 0 wrong");
        assert_eq!(snap[0x100], 0x99, "wram snapshot byte 0x100 wrong");
    }

    #[test]
    fn wram_snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x6200, 0xAB);

        let snap = mapper.wram_snapshot();
        let mut restored = make_mapper();
        restored.write_prg(0x7EF7, 0xCA); // enable so we can read back
        restored.load_wram_snapshot(&snap);

        assert_eq!(
            restored.read_prg(0x6200),
            0xAB,
            "wram snapshot restore must recover RAM contents"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_returns_prg_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EFA, 7 << 2);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 after reset"
        );
    }

    #[test]
    fn reset_disables_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7EF7, 0xCA);
        mapper.write_prg(0x6000, 0x55);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "RAM must be disabled after reset"
        );
    }
}
