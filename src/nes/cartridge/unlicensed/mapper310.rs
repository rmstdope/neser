//! Mapper 310 — K-1053 "1200-in-1 New World Multi" multicart
//!
//! # Specification sources
//!
//! - **Primary source**: NESdev Wiki — NES 2.0 Mapper 310
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_310>
//!   (accessed via nes.science mirror)
//!
//! # Hardware overview
//!
//! Mapper 310 is the K-1053 circuit board used for the *1200-in-1 New World Multi*
//! multicart. It is an extension of iNES Mapper 015 that supports more than 1 MiB
//! of PRG ROM and 32 KiB rather than 8 KiB of CHR RAM.
//!
//! # Banks
//!
//! | Bus             | Window             | Contents                          |
//! |-----------------|--------------------|-----------------------------------|
//! | CPU $8000–$FFFF | Switchable 8/16/32 KiB | PRG-ROM (4 × 8 KiB slots)    |
//! | PPU $0000–$1FFF | Switchable 8 KiB   | CHR-RAM (selected from 32 KiB)    |
//!
//! # PRG banking modes
//!
//! The banking mode (`SS`) is in address bits A1:A0 when writing to $C000–$FFFF:
//!
//! | SS | Mode    | Behavior                                          |
//! |----|---------|---------------------------------------------------|
//! | 0  | NROM-256 | 32 KiB bank: 4 sequential 8 KiB slots           |
//! | 1  | UNROM   | Lower 16 KiB switchable; upper 16 KiB fixed A16:A15:A14=111 |
//! | 2  | NROM-64 | All 4 slots map to same 8 KiB page (p=A13)       |
//! | 3  | NROM-128 | 16 KiB bank mirrored across $8000–$FFFF          |
//!
//! # CHR banking
//!
//! Writing to $C000–$FFFF: data bits D1:D0 select which 8 KiB window (CC = CHR A14:A13)
//! of the 32 KiB CHR RAM is visible at PPU $0000–$1FFF.
//!
//! CHR-RAM is **write-protected** in modes 0 and 3, **write-enabled** in modes 1 and 2.
//!
//! # Registers
//!
//! ## Data Latch `$8000–$BFFF` (write, mask `$C000`)
//!
//! ```text
//! D~7654 3210
//! ---------
//! pMPP PPPP
//! ||++-++++- PRG A19..A14
//! |+-------- Nametable mirroring (0=vertical, 1=horizontal)
//! +--------- PRG A13 if SS=2, ignored otherwise
//! ```
//!
//! ## Address and Data Latch `$C000–$FFFF` (write, mask `$C000`)
//!
//! ```text
//! A~FEDC BA98 7654 3210   D~7654 3210
//!   -------------------     ---------
//!   11.. .... .... PPSS     .... ..CC
//!                  ||||            ++- CHR A14..A13
//!                  ||++-------------- PRG banking mode (SS)
//!                  ++---------------- PRG A21..A20 (PP)
//! ```
//!
//! Power-on/reset value: all bits clear.

use crate::cartridge::BaseMapper;
use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;

const CHR_RAM_SIZE: usize = 32 * 1024; // 32 KiB

/// Mapper 310 — K-1053 multicart board.
pub struct Mapper310 {
    base: BaseMapper,
    /// PRG A19..A14 from the $8000–$BFFF data latch (bits 5..0 of written byte).
    prg_bank: u8,
    /// PRG A13 override for mode 2 (bit 7 of $8000–$BFFF data latch).
    p_bit: u8,
    /// Current nametable mirroring (bit 6 of $8000–$BFFF data latch).
    mirroring: NametableLayout,
    /// PRG A21..A20 from address bits A3:A2 of $C000–$FFFF writes.
    prg_high: u8,
    /// PRG banking mode SS from address bits A1:A0 of $C000–$FFFF writes.
    mode: u8,
    /// CHR A14..A13 from data bits D1:D0 of $C000–$FFFF writes.
    chr_bank: u8,
}

impl Mapper310 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        if ctx.chr_rom.is_empty() {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        }
        base.configure_prg_banking(0x2000); // 4 × 8 KiB slots
        base.configure_chr_banking(0x2000); // 1 × 8 KiB CHR slot
        let mut mapper = Self {
            base,
            prg_bank: 0,
            p_bit: 0,
            mirroring: NametableLayout::Vertical,
            prg_high: 0,
            mode: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    /// Return the 9-bit 8 KiB PRG page for the given slot (0..3).
    fn prg_page_for_slot(&self, slot: u8) -> i16 {
        // Combine prg_high (A21..A20) and prg_bank (A19..A14) into the base
        // 8 KiB page number. Shifting prg_bank left by 1 gives the A14=0 base.
        let base = ((self.prg_high as u16) << 7) | ((self.prg_bank as u16) << 1);

        let page = match self.mode {
            // NROM-256: four sequential 8 KiB pages, CPU A14:A13 come from slot.
            0 => base + slot as u16,

            // UNROM: lower 16 KiB switchable; upper 16 KiB fixed with A16:A15:A14=111.
            1 => {
                if slot < 2 {
                    base + slot as u16
                } else {
                    // Set bits 3:2:1 = 111 (A16:A15:A14) while preserving higher bits.
                    (base | 0x000E) + (slot as u16 - 2)
                }
            }

            // NROM-64: all four slots map to the same 8 KiB page; p selects A13.
            2 => base | (self.p_bit as u16),

            // NROM-128: 16 KiB mirrored — slots 0..1 and slots 2..3 are identical pairs.
            3 => {
                let pair_slot = slot % 2;
                base + pair_slot as u16
            }

            _ => unreachable!("Invalid banking mode"),
        };
        page as i16
    }

    fn is_chr_ram_writable(&self) -> bool {
        matches!(self.mode, 1 | 2)
    }

    fn update_banks(&mut self) {
        for slot in 0..4 {
            let page = self.prg_page_for_slot(slot);
            self.base.select_prg_page(slot as usize, page);
        }
        self.base.select_chr_page(0, self.chr_bank as i16);
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for Mapper310 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.p_bit = 0;
        self.mirroring = NametableLayout::Vertical;
        self.prg_high = 0;
        self.mode = 0;
        self.chr_bank = 0;
        self.update_banks();
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        if addr < 0x8000 {
            return;
        }

        if addr < 0xC000 {
            // $8000–$BFFF: Data Latch
            self.prg_bank = value & 0x3F; // PRG A19..A14 (bits 5..0)
            self.p_bit = (value >> 7) & 0x01; // PRG A13 for mode 2 (bit 7)
            self.mirroring = if (value & 0x40) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
        } else {
            // $C000–$FFFF: Address and Data Latch
            self.mode = (addr & 0x0003) as u8; // SS = A1:A0
            self.prg_high = ((addr >> 2) & 0x0003) as u8; // PP = A3:A2
            self.chr_bank = value & 0x03; // CC = D1:D0
        }

        self.update_banks();
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.is_chr_ram_writable() {
            self.base.write_chr(addr, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank,
            self.p_bit,
            self.mirroring.to_snapshot_byte(),
            self.prg_high,
            self.mode,
            self.chr_bank,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.prg_bank = data[0] & 0x3F;
            self.p_bit = data[1] & 0x01;
            self.mirroring = NametableLayout::from_snapshot_byte(data[2]);
            self.prg_high = data[3] & 0x03;
            self.mode = data[4] & 0x03;
            self.chr_bank = data[5] & 0x03;
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;

    const PRG_BANK_SIZE_8K: usize = 0x2000;

    /// Create a PRG-ROM where each 8 KiB bank is filled with its bank index value.
    fn make_prg_rom(num_8k_banks: usize) -> Vec<u8> {
        let mut rom = vec![0u8; num_8k_banks * PRG_BANK_SIZE_8K];
        for bank in 0..num_8k_banks {
            let start = bank * PRG_BANK_SIZE_8K;
            let end = start + PRG_BANK_SIZE_8K;
            for byte in &mut rom[start..end] {
                *byte = bank as u8;
            }
        }
        rom
    }

    fn new_mapper(num_8k_banks: usize) -> Mapper310 {
        Mapper310::new(
            MapperContext::new_for_test(
                310,
                make_prg_rom(num_8k_banks),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── RED: PRG banking mode 0 (NROM-256) ───────────────────────────────────

    #[test]
    fn test_mode0_four_sequential_8k_pages() {
        let mut m = new_mapper(128);
        // Write to $8000 (lower latch): bank_select = 3, mode is set by $C000 write
        // Set mode 0 via $C000 with SS=0, PP=0, chr_bank=0
        m.write_prg(0xC000, 0);
        // prg_bank = 3 → base = 6; slots should be 6,7,8,9
        m.write_prg(0x8000, 3);

        assert_eq!(m.read_prg(0x8000), 6, "slot 0 should be bank 6");
        assert_eq!(m.read_prg(0xA000), 7, "slot 1 should be bank 7");
        assert_eq!(m.read_prg(0xC000), 8, "slot 2 should be bank 8");
        assert_eq!(m.read_prg(0xE000), 9, "slot 3 should be bank 9");
    }

    #[test]
    fn test_mode0_bank_zero_selects_first_32k() {
        let mut m = new_mapper(64);
        m.write_prg(0xC000, 0); // mode 0
        m.write_prg(0x8000, 0); // bank 0 → base = 0

        assert_eq!(m.read_prg(0x8000), 0);
        assert_eq!(m.read_prg(0xA000), 1);
        assert_eq!(m.read_prg(0xC000), 2);
        assert_eq!(m.read_prg(0xE000), 3);
    }

    // ── RED: PRG banking mode 1 (UNROM) ──────────────────────────────────────

    #[test]
    fn test_mode1_lower_16k_switchable() {
        let mut m = new_mapper(128);
        // mode 1 via $C001 (SS=1)
        m.write_prg(0xC001, 0);
        // prg_bank = 2 → base = 4
        m.write_prg(0x8000, 2);

        assert_eq!(m.read_prg(0x8000), 4, "slot 0 should be bank 4");
        assert_eq!(m.read_prg(0xA000), 5, "slot 1 should be bank 5");
    }

    #[test]
    fn test_mode1_upper_16k_fixed_a16_a15_a14_all_ones() {
        let mut m = new_mapper(128);
        // mode 1 via $C001
        m.write_prg(0xC001, 0);
        // prg_bank = 2 (base = 4 = 0b000_0100); A16:A15:A14 should be 111 for upper slots
        // base | 0x0E = 4 | 14 = 14; base | 0x0F = 15
        m.write_prg(0x8000, 2);

        assert_eq!(m.read_prg(0xC000), 14, "slot 2 fixed to bank 14");
        assert_eq!(m.read_prg(0xE000), 15, "slot 3 fixed to bank 15");
    }

    #[test]
    fn test_mode1_fixed_bank_preserves_upper_prg_bits() {
        // With prg_bank = 0b001000 (8), base = 16 = 0b010000
        // Fixed upper: base | 0x0E = 16 | 14 = 30 (0b011110); base | 0x0F = 31
        let mut m = new_mapper(128);
        m.write_prg(0xC001, 0);
        m.write_prg(0x8000, 8); // prg_bank = 8, base = 16

        assert_eq!(m.read_prg(0x8000), 16, "slot 0 bank 16");
        assert_eq!(m.read_prg(0xA000), 17, "slot 1 bank 17");
        assert_eq!(m.read_prg(0xC000), 30, "slot 2 fixed to bank 30");
        assert_eq!(m.read_prg(0xE000), 31, "slot 3 fixed to bank 31");
    }

    // ── RED: PRG banking mode 2 (NROM-64) ────────────────────────────────────

    #[test]
    fn test_mode2_all_slots_same_page_p0() {
        let mut m = new_mapper(128);
        m.write_prg(0xC002, 0); // mode 2
        // prg_bank = 5, p_bit = 0 → page = 10 | 0 = 10
        m.write_prg(0x8000, 5);

        assert_eq!(m.read_prg(0x8000), 10, "slot 0");
        assert_eq!(m.read_prg(0xA000), 10, "slot 1");
        assert_eq!(m.read_prg(0xC000), 10, "slot 2");
        assert_eq!(m.read_prg(0xE000), 10, "slot 3");
    }

    #[test]
    fn test_mode2_p_bit_selects_a13() {
        let mut m = new_mapper(128);
        m.write_prg(0xC002, 0); // mode 2
        // prg_bank = 5, p_bit = 1 → page = 10 | 1 = 11
        m.write_prg(0x8000, 5 | 0x80); // bit 7 sets p_bit

        assert_eq!(m.read_prg(0x8000), 11, "all slots map to page 11 when p=1");
        assert_eq!(m.read_prg(0xA000), 11);
        assert_eq!(m.read_prg(0xC000), 11);
        assert_eq!(m.read_prg(0xE000), 11);
    }

    // ── RED: PRG banking mode 3 (NROM-128) ───────────────────────────────────

    #[test]
    fn test_mode3_16k_mirrored() {
        let mut m = new_mapper(128);
        m.write_prg(0xC003, 0); // mode 3
        // prg_bank = 4 → base = 8
        m.write_prg(0x8000, 4);

        assert_eq!(m.read_prg(0x8000), 8, "slot 0");
        assert_eq!(m.read_prg(0xA000), 9, "slot 1");
        assert_eq!(m.read_prg(0xC000), 8, "slot 2 mirrors slot 0");
        assert_eq!(m.read_prg(0xE000), 9, "slot 3 mirrors slot 1");
    }

    // ── RED: mirroring ────────────────────────────────────────────────────────

    #[test]
    fn test_mirroring_bit6_zero_is_vertical() {
        let mut m = new_mapper(32);
        m.write_prg(0x8000, 0x00); // bit 6 = 0 → vertical
        assert_eq!(m.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_mirroring_bit6_one_is_horizontal() {
        let mut m = new_mapper(32);
        m.write_prg(0x8000, 0x40); // bit 6 = 1 → horizontal
        assert_eq!(m.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── RED: CHR banking ─────────────────────────────────────────────────────

    #[test]
    fn test_chr_bank_selects_8k_from_32k() {
        let mut m = new_mapper(32);
        // Enable writes (mode 1)
        m.write_prg(0xC001, 0x00); // mode 1, chr_bank = 0
        // Write distinct pattern to bank 0
        m.write_chr(0x0000, 0xAA);

        // Switch to chr_bank 1 (CC=1 in data byte)
        m.write_prg(0xC001, 0x01); // chr_bank = 1
        m.write_chr(0x0000, 0xBB);

        // Switch back to bank 0 and verify
        m.write_prg(0xC001, 0x00);
        assert_eq!(m.read_chr(0x0000), 0xAA, "bank 0 should still have 0xAA");

        // Switch to bank 1 and verify
        m.write_prg(0xC001, 0x01);
        assert_eq!(m.read_chr(0x0000), 0xBB, "bank 1 should have 0xBB");
    }

    #[test]
    fn test_chr_bank_all_four_banks_independent() {
        let mut m = new_mapper(32);
        m.write_prg(0xC001, 0); // mode 1 for writes

        for bank in 0u8..4 {
            m.write_prg(0xC001, bank); // select bank
            m.write_chr(0x0000, bank * 10 + 1);
        }

        for bank in 0u8..4 {
            m.write_prg(0xC001, bank);
            assert_eq!(m.read_chr(0x0000), bank * 10 + 1);
        }
    }

    // ── RED: CHR write protection ─────────────────────────────────────────────

    #[test]
    fn test_chr_write_protected_in_mode_0() {
        let mut m = new_mapper(32);
        m.write_prg(0xC001, 0x00); // mode 1 (writeable) — pre-write
        m.write_chr(0x0000, 0xAA);
        m.write_prg(0xC000, 0x00); // switch to mode 0 (protected)
        m.write_chr(0x0000, 0xFF); // should be ignored
        assert_eq!(m.read_chr(0x0000), 0xAA, "mode 0: write ignored");
    }

    #[test]
    fn test_chr_write_protected_in_mode_3() {
        let mut m = new_mapper(32);
        m.write_prg(0xC001, 0x00); // mode 1 first
        m.write_chr(0x0000, 0xCC);
        m.write_prg(0xC003, 0x00); // switch to mode 3
        m.write_chr(0x0000, 0xFF);
        assert_eq!(m.read_chr(0x0000), 0xCC, "mode 3: write ignored");
    }

    #[test]
    fn test_chr_writable_in_mode_1() {
        let mut m = new_mapper(32);
        m.write_prg(0xC001, 0);
        m.write_chr(0x0000, 0x55);
        assert_eq!(m.read_chr(0x0000), 0x55, "mode 1: write accepted");
    }

    #[test]
    fn test_chr_writable_in_mode_2() {
        let mut m = new_mapper(32);
        m.write_prg(0xC002, 0);
        m.write_chr(0x1FFF, 0x77);
        assert_eq!(m.read_chr(0x1FFF), 0x77, "mode 2: write accepted");
    }

    // ── RED: prg_high (PP bits from $C000+ address) ───────────────────────────

    #[test]
    fn test_prg_high_extends_prg_address_space() {
        // 256 × 8 KiB = 2 MiB — need prg_high to address the upper half
        let mut m = new_mapper(256);
        // PP=1 via address $C004 (A3:A2 = 01), SS=0, chr=0
        m.write_prg(0xC004, 0); // mode 0, PP=1
        // prg_bank=0, prg_high=1 → base = 1<<7 | 0<<1 = 128
        m.write_prg(0x8000, 0);

        assert_eq!(
            m.read_prg(0x8000),
            128,
            "prg_high=1 offset should start at page 128"
        );
        assert_eq!(m.read_prg(0xA000), 129);
        assert_eq!(m.read_prg(0xC000), 130);
        assert_eq!(m.read_prg(0xE000), 131);
    }

    #[test]
    fn test_prg_high_pp_values() {
        let mut m = new_mapper(256);
        // PP=0: A11:A10 = 00 → addr $C000 (A3:A2=00)
        m.write_prg(0xC000, 0);
        m.write_prg(0x8000, 0);
        assert_eq!(m.read_prg(0x8000), 0, "PP=0, page 0");

        // PP=1: addr $C004 (A3:A2=01)
        m.write_prg(0xC004, 0);
        m.write_prg(0x8000, 0);
        assert_eq!(m.read_prg(0x8000), 128, "PP=1, page 128");

        // PP=2: addr $C008 (A3:A2=10)
        m.write_prg(0xC008, 0);
        m.write_prg(0x8000, 0);
        assert_eq!(
            m.read_prg(0x8000),
            0,
            "PP=2, wraps back to page 0 for 256-bank rom (256 mod 256)"
        );

        // PP=3: addr $C00C (A3:A2=11)
        m.write_prg(0xC00C, 0);
        m.write_prg(0x8000, 0);
        assert_eq!(
            m.read_prg(0x8000),
            128,
            "PP=3, wraps to page 128 (384 mod 256)"
        );
    }

    // ── RED: power-on state ───────────────────────────────────────────────────

    #[test]
    fn test_power_on_all_registers_zero() {
        let m = new_mapper(32);
        assert_eq!(m.prg_bank, 0);
        assert_eq!(m.p_bit, 0);
        assert_eq!(m.prg_high, 0);
        assert_eq!(m.mode, 0);
        assert_eq!(m.chr_bank, 0);
    }

    // ── RED: mode decode from address ─────────────────────────────────────────

    #[test]
    fn test_mode_decoded_from_low_address_bits() {
        let mut m = new_mapper(32);
        m.write_prg(0xC000, 0);
        assert_eq!(m.mode, 0);
        m.write_prg(0xC001, 0);
        assert_eq!(m.mode, 1);
        m.write_prg(0xC002, 0);
        assert_eq!(m.mode, 2);
        m.write_prg(0xC003, 0);
        assert_eq!(m.mode, 3);
        // Wraps: $C004 → SS=0
        m.write_prg(0xC004, 0);
        assert_eq!(m.mode, 0);
        m.write_prg(0xFFFF, 0);
        assert_eq!(m.mode, 3);
    }

    // ── RED: registers snapshot / restore ────────────────────────────────────

    #[test]
    fn test_snapshot_restore_round_trip() {
        let mut m = new_mapper(128);
        m.write_prg(0xC005, 0x02); // mode 1, PP=1, chr_bank=2
        m.write_prg(0x8000, 0x45 | 0x40); // prg_bank=5, mirroring=H, p_bit=0

        let snap = m.registers_snapshot();

        let mut m2 = new_mapper(128);
        m2.restore_registers(&snap);

        assert_eq!(m2.prg_bank, m.prg_bank);
        assert_eq!(m2.p_bit, m.p_bit);
        assert_eq!(m2.mirroring, m.mirroring);
        assert_eq!(m2.prg_high, m.prg_high);
        assert_eq!(m2.mode, m.mode);
        assert_eq!(m2.chr_bank, m.chr_bank);
        // Verify banking state was restored
        assert_eq!(m2.read_prg(0x8000), m.read_prg(0x8000));
    }

    #[test]
    fn test_snapshot_too_short_is_ignored() {
        let mut m = new_mapper(32);
        m.write_prg(0xC001, 0x02);
        m.write_prg(0x8000, 0x07);
        let saved_bank = m.prg_bank;

        m.restore_registers(&[0, 0, 0]); // too short
        assert_eq!(
            m.prg_bank, saved_bank,
            "short snapshot should not change state"
        );
    }

    // ── RED: restore_registers masks corrupted values ─────────────────────────

    #[test]
    fn test_restore_registers_masks_invalid_values() {
        let mut m = new_mapper(128);
        // Supply out-of-range values for each field
        m.restore_registers(&[
            0xFF, // prg_bank: only 6 bits valid → 0x3F
            0xFF, // p_bit: only 1 bit valid → 0x01
            NametableLayout::Horizontal.to_snapshot_byte(),
            0xFF, // prg_high: only 2 bits valid → 0x03
            0xFF, // mode: only 2 bits valid → 0x03
            0xFF, // chr_bank: only 2 bits valid → 0x03
        ]);
        assert_eq!(m.prg_bank, 0x3F);
        assert_eq!(m.p_bit, 0x01);
        assert_eq!(m.prg_high, 0x03);
        assert_eq!(m.mode, 0x03);
        assert_eq!(m.chr_bank, 0x03);
    }

    // ── RED: reset clears all registers ───────────────────────────────────────

    #[test]
    fn test_reset_clears_all_registers() {
        let mut m = new_mapper(128);
        m.write_prg(0xC005, 0x02); // mode 1, PP=1, chr_bank=2
        m.write_prg(0x8000, 0xC5); // prg_bank=5, mirroring=H, p_bit=1

        m.reset();

        assert_eq!(m.prg_bank, 0);
        assert_eq!(m.p_bit, 0);
        assert_eq!(m.mirroring, NametableLayout::Vertical);
        assert_eq!(m.prg_high, 0);
        assert_eq!(m.mode, 0);
        assert_eq!(m.chr_bank, 0);
    }

    // ── RED: power-on mirroring is vertical per spec ─────────────────────────

    #[test]
    fn test_power_on_mirroring_is_vertical_regardless_of_header() {
        // Even if header says Horizontal, power-on state spec says all-clear = vertical
        let m = Mapper310::new(
            MapperContext::new_for_test(310, make_prg_rom(32), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0),
        );
        assert_eq!(
            m.mirroring,
            NametableLayout::Vertical,
            "power-on mirroring should be vertical per spec (bit 6 = 0)"
        );
    }
}
