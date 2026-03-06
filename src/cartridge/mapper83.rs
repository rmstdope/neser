//! Mapper 083 – Caltron 6-in-1 / bootleg multicart
//!
//! Specifications:
//! - Fallback: Mesen2 `Mapper83.h` (NesDev unavailable)
//!
//! Known Limitations:
//! - DIP switch read at $5000 always returns 0x00; DIP bits are not emulated.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 083 – Caltron 6-in-1 / bootleg multicart
///
/// Hardware: Discrete logic board
///
/// Specifications:
/// - Fallback: Mesen2 `Mapper83.h` (NesDev unavailable)
/// - PRG-ROM: Up to 256 KiB; 4 × 8 KiB switchable slots ($8000–$FFFF)
/// - CHR: Up to 2048 KiB; 8 × 1 KiB switchable slots ($0000–$1FFF)
/// - Mirroring: Programmable (V/H/1A/1B) via $8100 bits[1:0]
/// - IRQ: 16-bit CPU-cycle down-counter; fires on reach-0, auto-disables.
///
/// PRG layout (8KB mode):
/// ```text
///   $8000  $A000  $C000  $E000  $FFFF
/// +------+------+------+------+
/// | r[8] | r[9] |r[10] | last |
/// +------+------+------+------+
/// ```
///
/// PRG layout (32KB mode, set by $8000 write or $B000/$B0FF/$B1FF):
/// ```text
///   $8000          $C000          $FFFF
/// +-------------+-------------+
/// |  bank*2 &   | last-group  |
/// |   bank*2+1  |   pair      |
/// +-------------+-------------+
/// ```
///
/// CHR layout (1KB mode, default):
/// ```text
///   $0000 $0400 ... $1C00 $1FFF
/// +------+------+...+------+
/// | r[0] | r[1] |   | r[7] |
/// +------+------+...+------+
/// ```
///
/// CHR layout (2KB mode, active when $8000 written and no $8312-$8315 write):
/// ```text
///   $0000    $0800   $1000   $1800   $1FFF
/// +--------+--------+--------+--------+
/// | r[0]*2 | r[1]*2 | r[6]*2 | r[7]*2 |
/// +--------+--------+--------+--------+
/// ```
///
/// Register map:
/// - `$5100–$5103`: ex_regs (read/write pass-through)
/// - `$8000`:       bank register; enables 32KB mode; enables 2KB CHR mode
/// - `$8100`:       mode register (bit[7]=IRQ-enable-latch, bit[6]=32KB-mode, bit[1:0]=mirror)
/// - `$8200`:       IRQ counter low byte; clears pending IRQ
/// - `$8201`:       IRQ counter high byte; enables IRQ when mode bit 7 is set
/// - `$8300–$8302`: PRG 8KB banks (r[8], r[9], r[10]); clears 32KB mode
/// - `$8310–$8317`: CHR 1KB banks (r[0]–r[7]); $8312–$8315 also clears 2KB mode
/// - `$B000/$B0FF/$B1FF`: alias for $8000 (Dragon Ball Z Party [p1] BMC)
pub struct Mapper83 {
    base: BaseMapper,
    /// r[0..7] = CHR 1KB bank indices; r[8..10] = PRG 8KB bank indices.
    regs: [u8; 11],
    /// Pass-through registers at $5100–$5103.
    ex_regs: [u8; 4],
    /// Mode register:
    /// - bit[7]: IRQ enable latch (enables IRQ on next $8201 write)
    /// - bit[6]: 32KB PRG mode
    /// - bit[1:0]: mirroring (0=V, 1=H, 2=1A, 3=1B)
    mode: u8,
    /// PRG bank for 32KB mode (written by $8000 / $B000 / $B0FF / $B1FF).
    bank: u8,
    /// Set by any $8000 write; enables 2KB CHR banking when `!is_not_2k_bank`.
    is_2k_bank: bool,
    /// Set by writes to $8312–$8315; forces 1KB CHR banking.
    is_not_2k_bank: bool,
    /// 16-bit countdown IRQ counter.
    irq_counter: u16,
    /// Whether the IRQ counter is currently decrementing.
    irq_enabled: bool,
    /// Whether an IRQ is asserted.
    irq_pending: bool,
}

impl Mapper83 {
    const PRG_PAGE_SIZE: usize = 0x2000; // 8 KiB
    const CHR_PAGE_SIZE: usize = 0x0400; // 1 KiB

    const MIRRORING_MASK: u8 = 0x03; // mode bits [1:0]
    const MODE_32KB_PRG: u8 = 0x40; // mode bit 6
    const MODE_IRQ_LATCH: u8 = 0x80; // mode bit 7
    const BANK_GROUP_MASK: u8 = 0x30; // bank bits [5:4] used for CHR ext and PRG group
    const PRG_BANK_MASK: u8 = 0x3F; // bank bits [5:0] for 32KB base bank
    const PRG_LAST_GROUP_MASK: u8 = 0x0F; // last bank within a 32KB group
    const IRQ_COUNTER_RESET: u16 = 0xFFFF;
    const SNAPSHOT_SIZE: usize = 22; // 7 header + 11 regs + 4 ex_regs

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_PAGE_SIZE);
        base.configure_chr_banking(Self::CHR_PAGE_SIZE);

        let mut mapper = Self {
            base,
            regs: [0u8; 11],
            ex_regs: [0u8; 4],
            mode: 0,
            bank: 0,
            is_2k_bank: false,
            is_not_2k_bank: false,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.update_state();
        mapper
    }

    fn apply_mirroring(&mut self) {
        self.base
            .set_mirroring(match self.mode & Self::MIRRORING_MASK {
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                2 => NametableLayout::SingleScreenLower,
                _ => NametableLayout::SingleScreenUpper,
            });
    }

    fn apply_chr_banking(&mut self) {
        if self.is_2k_bank && !self.is_not_2k_bank {
            // 2KB CHR mode: regs[0], regs[1], regs[6], regs[7] each select a 2KB page
            let r0 = (self.regs[0] as i16) << 1;
            let r1 = (self.regs[1] as i16) << 1;
            let r6 = (self.regs[6] as i16) << 1;
            let r7 = (self.regs[7] as i16) << 1;
            self.base.select_chr_page(0, r0);
            self.base.select_chr_page(1, r0 + 1);
            self.base.select_chr_page(2, r1);
            self.base.select_chr_page(3, r1 + 1);
            self.base.select_chr_page(4, r6);
            self.base.select_chr_page(5, r6 + 1);
            self.base.select_chr_page(6, r7);
            self.base.select_chr_page(7, r7 + 1);
        } else {
            // 1KB CHR mode: regs[0..7] with bank extension bits from `bank`
            let ext = ((self.bank & Self::BANK_GROUP_MASK) as i16) << 4;
            for i in 0..8usize {
                self.base.select_chr_page(i, (self.regs[i] as i16) | ext);
            }
        }
    }

    fn apply_prg_banking(&mut self) {
        if self.mode & Self::MODE_32KB_PRG != 0 {
            // 32KB mode
            let base_bank = ((self.bank & Self::PRG_BANK_MASK) as i16) << 1;
            let last_bank =
                (((self.bank & Self::BANK_GROUP_MASK) | Self::PRG_LAST_GROUP_MASK) as i16) << 1;
            self.base.select_prg_page(0, base_bank);
            self.base.select_prg_page(1, base_bank + 1);
            self.base.select_prg_page(2, last_bank);
            self.base.select_prg_page(3, last_bank + 1);
        } else {
            // 8KB mode
            self.base.select_prg_page(0, self.regs[8] as i16);
            self.base.select_prg_page(1, self.regs[9] as i16);
            self.base.select_prg_page(2, self.regs[10] as i16);
            self.base.select_prg_page(3, -1);
        }
    }

    fn update_state(&mut self) {
        self.apply_mirroring();
        self.apply_chr_banking();
        self.apply_prg_banking();
    }
}

impl Mapper for Mapper83 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000 => {
                // DIP switch bits not emulated; always return 0x00
                0x00
            }
            0x5100..=0x5103 => self.ex_regs[(addr & 0x03) as usize],
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5100..=0x5103 => {
                self.ex_regs[(addr & 0x03) as usize] = value;
            }
            0x8000 | 0xB000 | 0xB0FF | 0xB1FF => {
                self.is_2k_bank = true;
                self.bank = value;
                self.mode |= Self::MODE_32KB_PRG;
                self.update_state();
            }
            0x8100 => {
                self.mode = value | (self.mode & Self::MODE_32KB_PRG);
                self.update_state();
            }
            0x8200 => {
                self.irq_counter = (self.irq_counter & 0xFF00) | (value as u16);
                self.irq_pending = false;
            }
            0x8201 => {
                self.irq_enabled = (self.mode & Self::MODE_IRQ_LATCH) != 0;
                self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
            }
            0x8300..=0x8302 => {
                self.mode &= !Self::MODE_32KB_PRG; // clear 32KB mode
                self.regs[(addr - 0x8300 + 8) as usize] = value;
                self.update_state();
            }
            0x8310..=0x8317 => {
                let idx = (addr - 0x8310) as usize;
                self.regs[idx] = value;
                if (0x8312..=0x8315).contains(&addr) {
                    self.is_not_2k_bank = true;
                }
                self.update_state();
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
            if self.irq_counter == 0 {
                self.irq_enabled = false;
                self.irq_counter = Self::IRQ_COUNTER_RESET;
                self.irq_pending = true;
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout: [mode, bank, is_2k_bank, is_not_2k_bank, irq_flags, irq_lo, irq_hi, regs×11, ex_regs×4]
        let irq_flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        let mut v = vec![
            self.mode,
            self.bank,
            self.is_2k_bank as u8,
            self.is_not_2k_bank as u8,
            irq_flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
        ];
        v.extend_from_slice(&self.regs);
        v.extend_from_slice(&self.ex_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < Self::SNAPSHOT_SIZE {
            return;
        }
        self.mode = data[0];
        self.bank = data[1];
        self.is_2k_bank = data[2] != 0;
        self.is_not_2k_bank = data[3] != 0;
        self.irq_enabled = (data[4] & 1) != 0;
        self.irq_pending = (data[4] & 2) != 0;
        self.irq_counter = (data[5] as u16) | ((data[6] as u16) << 8);
        self.regs.copy_from_slice(&data[7..18]);
        self.ex_regs.copy_from_slice(&data[18..22]);
        self.update_state();
    }

    fn reset(&mut self) {
        self.regs = [0u8; 11];
        self.ex_regs = [0u8; 4];
        self.mode = 0;
        self.bank = 0;
        self.is_2k_bank = false;
        self.is_not_2k_bank = false;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 33; // 33 × 8KB = 264KB
    const CHR_BANKS: usize = 33; // 33 × 1KB = 33KB

    fn make_mapper() -> Mapper83 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        Mapper83::new(MapperContext::new_for_test(
            83,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_83_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            83,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 83 must be registered in the factory"
        );
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg_slot0_is_bank0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must start at bank 0"
        );
    }

    #[test]
    fn power_on_prg_slot3_is_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "PRG slot 3 must be fixed to last bank at power-on"
        );
    }

    #[test]
    fn power_on_chr_slot0_is_bank0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 must start at bank 0"
        );
    }

    #[test]
    fn power_on_irq_not_pending() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn power_on_mirroring_from_header() {
        let mapper = make_mapper();
        // mode=0 → Vertical mirroring
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- PRG 8KB mode banking ---

    #[test]
    fn prg_8kb_mode_reg8_controls_slot0() {
        let mut mapper = make_mapper();
        // Write bank 5 to $8300 (reg[8] = 5), clears 32KB mode
        mapper.write_prg(0x8300, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "PRG slot 0 must reflect reg[8] in 8KB mode"
        );
    }

    #[test]
    fn prg_8kb_mode_reg9_controls_slot1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8301, 7);
        assert_eq!(
            mapper.read_prg(0xA000),
            7,
            "PRG slot 1 must reflect reg[9] in 8KB mode"
        );
    }

    #[test]
    fn prg_8kb_mode_reg10_controls_slot2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8302, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "PRG slot 2 must reflect reg[10] in 8KB mode"
        );
    }

    #[test]
    fn prg_8kb_mode_slot3_always_last() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8300, 5);
        mapper.write_prg(0x8301, 7);
        mapper.write_prg(0x8302, 3);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "PRG slot 3 must always be fixed to last bank in 8KB mode"
        );
    }

    #[test]
    fn prg_8300_write_clears_32kb_mode() {
        let mut mapper = make_mapper();
        // First enable 32KB mode via $8000
        mapper.write_prg(0x8000, 1); // 32KB mode
        // Then write $8300 to switch back to 8KB mode with reg[8]=2
        mapper.write_prg(0x8300, 2);
        // Slot 3 should be last bank (8KB mode), not part of 32KB bank pair
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Writing $8300 must clear 32KB mode and fix slot 3 to last bank"
        );
    }

    // --- PRG 32KB mode banking ---

    #[test]
    fn prg_32kb_mode_set_by_8000_write() {
        let mut mapper = make_mapper();
        // Write bank=2 to $8000: mode |= 0x40
        // base_bank = (2 & 0x3F) * 2 = 4
        // Slot 0 = bank 4, slot 1 = bank 5
        mapper.write_prg(0x8000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "PRG slot 0 in 32KB mode must be (bank & 0x3F)*2"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "PRG slot 1 in 32KB mode must be (bank & 0x3F)*2 + 1"
        );
    }

    #[test]
    fn prg_32kb_mode_slots_2_3_from_last_group() {
        let mut mapper = make_mapper();
        // bank=2: (bank & 0x30) = 0, (0 | 0x0F) = 15
        // last_bank = 15 * 2 = 30, slot2 = 30, slot3 = 31
        mapper.write_prg(0x8000, 2);
        assert_eq!(
            mapper.read_prg(0xC000),
            30,
            "PRG slot 2 in 32KB mode must be last-group bank"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "PRG slot 3 in 32KB mode must be last-group bank + 1"
        );
    }

    #[test]
    fn prg_32kb_mode_b000_alias() {
        let mut mapper = make_mapper();
        // $B000 is an alias for $8000
        mapper.write_prg(0xB000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$B000 must behave like $8000 for bank selection"
        );
    }

    // --- CHR 1KB mode banking ---

    #[test]
    fn chr_1kb_reg0_controls_slot0() {
        let mut mapper = make_mapper();
        // Force 1KB mode by writing to $8312 first (sets is_not_2k_bank)
        mapper.write_prg(0x8312, 0);
        mapper.write_prg(0x8310, 7);
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "CHR slot 0 must reflect reg[0] in 1KB mode"
        );
    }

    #[test]
    fn chr_1kb_reg7_controls_slot7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8312, 0);
        mapper.write_prg(0x8317, 5);
        assert_eq!(
            mapper.read_chr(0x1C00),
            5,
            "CHR slot 7 must reflect reg[7] in 1KB mode"
        );
    }

    #[test]
    fn chr_1kb_all_slots_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8312, 0); // force 1KB mode
        for i in 0..8u16 {
            mapper.write_prg(0x8310 + i, (i * 4) as u8);
        }
        for i in 0..8u16 {
            let bank = (i * 4) as u8 % PRG_BANKS as u8;
            assert_eq!(
                mapper.read_chr(i * 0x400),
                bank,
                "CHR slot {i} must be independently selectable"
            );
        }
    }

    // --- CHR 2KB mode banking ---

    #[test]
    fn chr_2kb_mode_enabled_by_8000_write() {
        let mut mapper = make_mapper();
        // Write $8000 with bank=0 (enables is_2k_bank, no $8312-$8315 written)
        // Then set reg[0]=3 via $8310: slot 0 = 3*2=6, slot 1 = 7
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8310, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            6,
            "CHR slot 0 in 2KB mode must be reg[0]*2"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            7,
            "CHR slot 1 in 2KB mode must be reg[0]*2 + 1"
        );
    }

    #[test]
    fn chr_2kb_mode_reg1_controls_slots_2_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8311, 4); // reg[1]=4 → slots 2,3 = 8,9
        assert_eq!(
            mapper.read_chr(0x0800),
            8,
            "CHR slot 2 in 2KB mode must be reg[1]*2"
        );
        assert_eq!(
            mapper.read_chr(0x0C00),
            9,
            "CHR slot 3 in 2KB mode must be reg[1]*2 + 1"
        );
    }

    #[test]
    fn chr_8312_write_disables_2kb_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // enable 2KB mode
        mapper.write_prg(0x8310, 3); // reg[0]=3 (would give 6 in 2KB mode)
        // Now write $8312 → sets is_not_2k_bank, forcing 1KB mode
        mapper.write_prg(0x8312, 0);
        // In 1KB mode, slot 0 uses reg[0]=3 directly
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "Writing $8312 must disable 2KB mode; slot 0 must use reg[0] directly"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical_from_mode_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x00); // bits[1:0] = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_from_mode_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x01); // bits[1:0] = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_single_screen_a_from_mode_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x02); // bits[1:0] = 2 → SingleScreenLower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn mirroring_single_screen_b_from_mode_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x03); // bits[1:0] = 3 → SingleScreenUpper
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn mirroring_8100_preserves_32kb_mode_bit() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 2); // enable 32KB mode (sets mode bit 6)
        mapper.write_prg(0x8100, 0x01); // set mirroring; mode = 0x01 | (old mode & 0x40)
        // 32KB mode must still be active
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "32KB mode must persist after $8100 mirroring write"
        );
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not fire at power-on");
    }

    #[test]
    fn irq_fires_after_counter_reaches_zero() {
        let mut mapper = make_mapper();
        // Set mode bit 7 to enable IRQ latch
        mapper.write_prg(0x8100, 0x80);
        // Counter low = 3, high = 0 → counter = 3
        mapper.write_prg(0x8200, 3);
        mapper.write_prg(0x8201, 0);
        // 3 cycles: counter → 2 → 1 → 0 → fires
        for _ in 0..2 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // counter → 0 → IRQ
        assert!(mapper.irq_pending(), "IRQ must fire when counter reaches 0");
    }

    #[test]
    fn irq_write_8200_clears_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x80);
        mapper.write_prg(0x8200, 1);
        mapper.write_prg(0x8201, 0);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
        mapper.write_prg(0x8200, 5); // clears IRQ
        assert!(
            !mapper.irq_pending(),
            "Writing $8200 must clear the pending IRQ"
        );
    }

    #[test]
    fn irq_auto_disables_after_fire() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x80);
        mapper.write_prg(0x8200, 1);
        mapper.write_prg(0x8201, 0);
        mapper.cpu_cycle(); // fires
        assert!(mapper.irq_pending());
        // Acknowledge
        mapper.write_prg(0x8200, 10);
        // Additional cycles should not fire again (IRQ disabled after firing)
        for _ in 0..20 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not re-fire after auto-disable"
        );
    }

    #[test]
    fn irq_counter_resets_to_ffff_after_fire() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8100, 0x80);
        mapper.write_prg(0x8200, 1);
        mapper.write_prg(0x8201, 0);
        mapper.cpu_cycle(); // fires; counter should reset to 0xFFFF
        assert_eq!(
            mapper.irq_counter, 0xFFFF,
            "IRQ counter must reset to 0xFFFF after firing"
        );
    }

    #[test]
    fn irq_not_enabled_without_mode_bit7() {
        let mut mapper = make_mapper();
        // Mode bit 7 NOT set → IRQ must not enable
        mapper.write_prg(0x8100, 0x00); // bit 7 = 0
        mapper.write_prg(0x8200, 3);
        mapper.write_prg(0x8201, 0); // triggers IRQ enable check → should NOT enable
        for _ in 0..10 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire when mode bit 7 is clear"
        );
    }

    // --- ex_regs ($5100–$5103) ---

    #[test]
    fn ex_regs_read_write_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5100, 0xAB);
        mapper.write_prg(0x5101, 0xCD);
        mapper.write_prg(0x5102, 0xEF);
        mapper.write_prg(0x5103, 0x12);
        assert_eq!(mapper.read_prg(0x5100), 0xAB);
        assert_eq!(mapper.read_prg(0x5101), 0xCD);
        assert_eq!(mapper.read_prg(0x5102), 0xEF);
        assert_eq!(mapper.read_prg(0x5103), 0x12);
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Set up a distinctive state
        mapper.write_prg(0x8300, 5); // PRG reg[8] = 5
        mapper.write_prg(0x8301, 7); // PRG reg[9] = 7
        mapper.write_prg(0x8312, 3); // CHR reg[2] = 3 (also sets is_not_2k_bank)
        mapper.write_prg(0x8100, 0x81); // mode: bit[7]=IRQ latch, bit[0]=Horizontal
        mapper.write_prg(0x8200, 0x34);
        mapper.write_prg(0x8201, 0x12);
        mapper.write_prg(0x5101, 0x55);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG slot 0 must be restored"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            mapper.read_prg(0xA000),
            "PRG slot 1 must be restored"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring must be restored"
        );
        assert_eq!(
            restored.irq_counter, mapper.irq_counter,
            "IRQ counter must be restored"
        );
        assert_eq!(
            restored.read_prg(0x5101),
            mapper.read_prg(0x5101),
            "ex_reg[1] must be restored"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3); // 32KB mode, bank=3
        mapper.write_prg(0x8100, 0x83);
        mapper.write_prg(0x8200, 10);
        mapper.write_prg(0x8201, 0);
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "PRG slot 3 must be last bank after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
        assert!(!mapper.irq_pending(), "IRQ must not be pending after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must reset to Vertical"
        );
    }

    // --- CHR-RAM fallback ---

    #[test]
    fn chr_ram_writable_when_no_chr_rom() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let mut mapper = Mapper83::new(MapperContext::new_for_test(
            83,
            prg,
            vec![],
            NametableLayout::Vertical,
        ));
        mapper.write_chr(0x0200, 0xBE);
        assert_eq!(
            mapper.read_chr(0x0200),
            0xBE,
            "CHR-RAM must be writable when no CHR-ROM"
        );
    }
}
