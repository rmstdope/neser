//! Mapper 208 – MMC3 with hardware protection LUT and fixed 32 KiB PRG
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_208>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_208.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 208 is an MMC3 clone with a copy-protection mechanism based on a
//! 256-byte LUT and a set of shadow registers.  The PRG banking ignores the
//! MMC3 PRG registers entirely and always maps a fixed 32 KiB block selected
//! by an extra "PRG block" register (`exRegs[5]`).
//!
//! ## Extra Register Map
//!
//! | Address           | Access | Effect                                          |
//! |-------------------|--------|-------------------------------------------------|
//! | `$4800–$4FFF`     | Write  | PRG block: `exRegs[5] = (val & 1) | ((val >> 3) & 2)` |
//! | `$5000–$57FF`     | Write  | LUT index: `exRegs[4] = val`                    |
//! | `$5800–$5FFF`     | Write  | Shadow reg: `exRegs[addr & 3] = val ^ LUT[exRegs[4]]` |
//! | `$5800–$5FFF`     | Read   | `exRegs[addr & 3]`                              |
//! | `$6800–$6FFF`     | Write  | PRG block (same as `$4800–$4FFF`)               |
//! | `$8000–$FFFF`     | R/W    | Standard MMC3 CHR / IRQ / mirroring registers   |
//!
//! ## PRG Banking
//!
//! Always maps a fixed 32 KiB block: the four 8 KiB slots at `$8000–$FFFF`
//! come from banks `exRegs[5] * 4 + 0` through `exRegs[5] * 4 + 3`.
//! `exRegs[5]` is initialised to **3** at power-on.
//!
//! ## CHR, IRQ, and Mirroring
//!
//! Handled identically to standard MMC3 via `$8000–$FFFF` register writes.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 208;
const PRG_BANK_SIZE: usize = 0x2000;
const PRG_BANK_MASK: usize = PRG_BANK_SIZE - 1;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;

/// 256-byte protection LUT used to XOR shadow-register writes.
const PROTECTION_LUT: [u8; 256] = [
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x49, 0x19, 0x09, 0x59, 0x49, 0x19, 0x09,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x51, 0x41, 0x11, 0x01, 0x51, 0x41, 0x11, 0x01,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x49, 0x19, 0x09, 0x59, 0x49, 0x19, 0x09,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x51, 0x41, 0x11, 0x01, 0x51, 0x41, 0x11, 0x01,
    0x00, 0x10, 0x40, 0x50, 0x00, 0x10, 0x40, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x08, 0x18, 0x48, 0x58, 0x08, 0x18, 0x48, 0x58, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x10, 0x40, 0x50, 0x00, 0x10, 0x40, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x08, 0x18, 0x48, 0x58, 0x08, 0x18, 0x48, 0x58, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x58, 0x48, 0x18, 0x08, 0x58, 0x48, 0x18, 0x08,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x50, 0x40, 0x10, 0x00, 0x50, 0x40, 0x10, 0x00,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x58, 0x48, 0x18, 0x08, 0x58, 0x48, 0x18, 0x08,
    0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x59, 0x50, 0x40, 0x10, 0x00, 0x50, 0x40, 0x10, 0x00,
    0x01, 0x11, 0x41, 0x51, 0x01, 0x11, 0x41, 0x51, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x09, 0x19, 0x49, 0x59, 0x09, 0x19, 0x49, 0x59, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x11, 0x41, 0x51, 0x01, 0x11, 0x41, 0x51, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x09, 0x19, 0x49, 0x59, 0x09, 0x19, 0x49, 0x59, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Mapper 208 – MMC3 with protection LUT and fixed 32 KiB PRG override.
///
/// See the module-level documentation for hardware details.
pub struct Mapper208 {
    inner: MMC3Mapper,
    /// Shadow registers [0..3]: readable from $5800–$5FFF.
    /// [4]: LUT index for shadow-register writes.
    /// [5]: PRG block (init = 3); selects 32 KiB block at $8000–$FFFF.
    ex_regs: [u8; 6],
}

impl Mapper208 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mut mapper = Self {
            inner: MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
                ctx.prg_rom,
                ctx.chr_rom,
                ctx.mirroring,
                false,
                1,
            ),
            ex_regs: [0; 6],
        };
        mapper.ex_regs[5] = 3; // PRG block initialised to 3 at power-on
        mapper
    }

    fn prg_bank_for(&self, addr: u16) -> usize {
        let base = (self.ex_regs[5] as usize) << 2;
        let slot = ((addr as usize).saturating_sub(0x8000) >> 13) & 0x03;
        base + slot
    }

    fn set_prg_block(&mut self, value: u8) {
        self.ex_regs[5] = (value & 0x01) | ((value >> 3) & 0x02);
    }
}

impl Mapper for Mapper208 {
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
            0x5800..=0x5FFF => self.ex_regs[(addr & 0x03) as usize],
            0x6000..=0x7FFF => self.inner.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for(addr);
                let offset = (addr as usize) & PRG_BANK_MASK;
                self.inner.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5800..=0x5FFF => self.ex_regs[(addr & 0x03) as usize],
            0x6000..=0x7FFF => self.inner.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            // PRG block register (two mirrored address ranges)
            0x4800..=0x4FFF | 0x6800..=0x6FFF => {
                self.set_prg_block(value);
            }
            // LUT index register
            0x5000..=0x57FF => {
                self.ex_regs[4] = value;
            }
            // Shadow registers (write XOR'd with LUT entry)
            0x5800..=0x5FFF => {
                let lut_val = PROTECTION_LUT[self.ex_regs[4] as usize];
                self.ex_regs[(addr & 0x03) as usize] = value ^ lut_val;
            }
            // Standard PRG-RAM ($6000–$6FFF not in register range, $7000–$7FFF)
            0x6000..=0x6FFF | 0x7000..=0x7FFF => {
                self.inner.write_prg(addr, value);
            }
            // Standard MMC3 registers (CHR banking, IRQ, mirroring)
            0x8000..=0xFFFF => {
                self.inner.write_prg(addr, value);
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
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.extend_from_slice(&self.ex_regs);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected_len = self.inner.registers_snapshot().len() + self.ex_regs.len();

        if data.len() == expected_len {
            let (mmc3_data, tail) = data.split_at(data.len() - self.ex_regs.len());
            self.inner.restore_registers(mmc3_data);
            self.ex_regs.copy_from_slice(tail);
        } else {
            // Legacy MMC3-only snapshot: restore MMC3 and reset extra regs.
            self.inner.restore_registers(data);
            self.ex_regs = [0; 6];
            self.ex_regs[5] = 3; // PRG block initialised to 3 at power-on
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.inner.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 32;
    const CHR_1K_BANKS: usize = 64;

    fn make_mapper() -> Mapper208 {
        Mapper208::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_208_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_8K_BANKS),
            banked_data(CHR_1K_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 208 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_block_is_3() {
        let mapper = make_mapper();
        assert_eq!(mapper.ex_regs[5], 3, "PRG block must be 3 at power-on");
        // Block 3 → banks 12,13,14,15
        assert_eq!(mapper.read_prg(0x8000), 12, "$8000 must map to bank 12");
        assert_eq!(mapper.read_prg(0xA000), 13, "$A000 must map to bank 13");
        assert_eq!(mapper.read_prg(0xC000), 14, "$C000 must map to bank 14");
        assert_eq!(mapper.read_prg(0xE000), 15, "$E000 must map to bank 15");
    }

    // ── PRG block register ($4800–$4FFF and $6800–$6FFF) ─────────────────────

    #[test]
    fn prg_block_write_4800_extracts_bits_0_and_4() {
        let mut mapper = make_mapper();
        // set_prg_block: ex_regs[5] = (value & 0x01) | ((value >> 3) & 0x02)
        // With value 0x01, only bit 0 is set, so ex_regs[5] becomes 1.
        mapper.write_prg(0x4800, 0x01);
        assert_eq!(
            mapper.ex_regs[5], 1,
            "Bit 0 of value sets bit 0 of PRG block"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 must be bank 4 (block 1×4)"
        );
    }

    #[test]
    fn prg_block_write_4800_bit4_sets_bit1() {
        let mut mapper = make_mapper();
        // formula: (value >> 3) & 0x02 → bit 4 of value maps to bit 1 of PRG block
        // value = 0x10: bit4=1, bit0=0 → exRegs[5] = 0 | 2 = 2
        mapper.write_prg(0x4800, 0x10);
        assert_eq!(
            mapper.ex_regs[5], 2,
            "Bit 4 of value sets bit 1 of PRG block"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "$8000 must be bank 8 (block 2×4)"
        );
    }

    #[test]
    fn prg_block_6800_mirrors_4800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6800, 0x01);
        assert_eq!(mapper.ex_regs[5], 1, "$6800 must have same effect as $4800");
    }

    #[test]
    fn prg_block_maps_four_consecutive_8k_banks() {
        let mut mapper = make_mapper();
        // value = 0x10: bit4=1, bit0=0 → exRegs[5] = 2 → banks 8,9,10,11
        mapper.write_prg(0x4800, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 8);
        assert_eq!(mapper.read_prg(0xA000), 9);
        assert_eq!(mapper.read_prg(0xC000), 10);
        assert_eq!(mapper.read_prg(0xE000), 11);
    }

    // ── Protection registers ($5000–$5FFF) ───────────────────────────────────

    #[test]
    fn write_5000_sets_lut_index() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x42);
        assert_eq!(mapper.ex_regs[4], 0x42, "$5000 write must set LUT index");
    }

    #[test]
    fn write_5800_stores_value_xor_lut() {
        let mut mapper = make_mapper();
        // LUT[0] = 0x59, so writing value=0 to $5800 → exRegs[0] = 0 ^ 0x59 = 0x59
        mapper.write_prg(0x5000, 0x00); // LUT index = 0
        mapper.write_prg(0x5800, 0x00); // exRegs[0] = 0 ^ LUT[0] = 0x59
        assert_eq!(mapper.ex_regs[0], 0x59);
    }

    #[test]
    fn write_5800_addr_selects_shadow_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00); // LUT[0] = 0x59
        mapper.write_prg(0x5801, 0x00); // exRegs[1] = 0 ^ 0x59
        assert_eq!(mapper.ex_regs[1], 0x59);
        mapper.write_prg(0x5802, 0x00); // exRegs[2]
        assert_eq!(mapper.ex_regs[2], 0x59);
        mapper.write_prg(0x5803, 0x00); // exRegs[3]
        assert_eq!(mapper.ex_regs[3], 0x59);
    }

    #[test]
    fn read_5800_returns_shadow_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00); // LUT index 0
        mapper.write_prg(0x5800, 0xFF); // exRegs[0] = 0xFF ^ 0x59 = 0xA6
        assert_eq!(
            mapper.read_prg(0x5800),
            0xA6,
            "$5800 read must return exRegs[0]"
        );
        assert_eq!(
            mapper.read_prg(0x5801),
            0,
            "$5801 read must return exRegs[1] (unchanged)"
        );
    }

    #[test]
    fn protection_lut_sample_entry() {
        // Verify a known LUT entry: LUT[9] = 0x49 (from the constant)
        assert_eq!(PROTECTION_LUT[9], 0x49);
    }

    // ── Standard MMC3 CHR via $8000 ──────────────────────────────────────────

    #[test]
    fn mmc3_chr_reachable_via_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // select CHR reg 2
        mapper.write_prg(0x8001, 5); // reg[2] = 5
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "CHR $1000 must map to 1K bank 5"
        );
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_round_trips_prg_block_and_shadow_regs() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x01); // block 1
        mapper.write_prg(0x5000, 0x00); // LUT index 0
        mapper.write_prg(0x5800, 0xFF); // exRegs[0] = 0xFF ^ 0x59 = 0xA6

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.ex_regs[5], 1, "PRG block must be restored");
        assert_eq!(restored.ex_regs[0], 0xA6, "Shadow reg must be restored");
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG read must match"
        );
    }
}
