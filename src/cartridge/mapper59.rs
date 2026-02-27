//! Mapper 059 - BMC-T3H53 / BMC-D1038
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_059>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 059 – BMC-T3H53 / BMC-D1038
///
/// Hardware: BMC-T3H53 / BMC-D1038 multicart board
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_059>
/// - PRG-ROM: Up to 256 KiB (16 × 16 KiB banks or 8 × 32 KiB blocks)
/// - CHR: Up to 64 KiB (8 × 8 KiB banks)
/// - PRG-RAM: None
/// - Mirroring: Programmable (H/V)
/// - Bus conflicts: None
///
/// The "register" is encoded in the CPU ADDRESS written to $8000-$FFFF,
/// not the data value.  Address bit layout:
///
/// ```text
/// A~FEDC BA98 7654 3210
///   1... ..LD SPPp MCCC
/// ```
/// - A9  = L: Lock bit (1 = ignore further writes until hard reset)
/// - A8  = D: Jumper read mode (1 = reads return jumper value 0x00)
/// - A7  = S: PRG size (0 = NROM-256/32KB, 1 = NROM-128/16KB)
/// - A6:A5 = PP: high 2 bits of PRG block/bank
/// - A4  = p: low PRG bank bit (only meaningful in NROM-128 mode)
/// - A3  = M: Mirroring (0 = Vertical, 1 = Horizontal)
/// - A2:A0 = CCC: 8 KiB CHR bank
///
/// PRG banking:
/// - NROM-128 (S=1): 16 KB bank = PPp; same bank at $8000–$BFFF and $C000–$FFFF
/// - NROM-256 (S=0): 32 KB window; $8000→PP<<1 (16KB), $C000→(PP<<1)+1 (16KB)
pub struct Mapper59 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    chr_bank: u8,       // 3 bits: CCC
    prg_pp: u8,         // 2 bits: high PRG bits
    prg_p: u8,          // 1 bit: low PRG bank bit (NROM-128 only)
    prg_mode_128: bool, // S: true = NROM-128 (16KB), false = NROM-256 (32KB)
    jumper_mode: bool,  // D: true = return jumper value (0x00) on reads
    locked: bool,       // L: true = ignore further writes
}

impl Mapper59 {
    const MAPPER_NUMBER: u8 = 59;
    const PRG_BANK_SIZE: usize = 0x4000; // 16 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            chr_bank: 0,
            prg_pp: 0,
            prg_p: 0,
            prg_mode_128: false,
            jumper_mode: false,
            locked: false,
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn read_prg_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.prg_bank_count();
        if count == 0 {
            return 0;
        }
        let b = bank % count;
        self.prg_rom
            .get(b * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }
}

impl Mapper for Mapper59 {
    fn read_prg(&self, addr: u16) -> u8 {
        if self.jumper_mode {
            return 0x00;
        }
        let offset = addr as usize & Self::PRG_BANK_MASK;
        if self.prg_mode_128 {
            // NROM-128: 16KB bank = PP:p; same bank at $8000 and $C000
            let bank = ((self.prg_pp as usize) << 1) | (self.prg_p as usize);
            self.read_prg_bank(bank, offset)
        } else {
            // NROM-256: 32KB window; $8000→PP<<1, $C000→(PP<<1)+1
            let base = (self.prg_pp as usize) << 1;
            let bank = if addr >= 0xC000 { base + 1 } else { base };
            self.read_prg_bank(bank, offset)
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let _ = value; // data is ignored; info is in address bits
        if self.locked {
            return;
        }
        let a = addr as usize;
        // Decode address bits
        let chr_bank = (a & 0x07) as u8; // A2:A0
        let mirroring_h = (a >> 3) & 1 != 0; // A3
        let prg_p = ((a >> 4) & 1) as u8; // A4
        let prg_pp = ((a >> 5) & 3) as u8; // A6:A5
        let prg_mode_128 = (a >> 7) & 1 != 0; // A7
        let jumper_mode = (a >> 8) & 1 != 0; // A8
        let lock = (a >> 9) & 1 != 0; // A9

        self.chr_bank = chr_bank;
        self.mirroring = if mirroring_h {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        };
        self.prg_p = prg_p;
        self.prg_pp = prg_pp;
        self.prg_mode_128 = prg_mode_128;
        self.jumper_mode = jumper_mode;
        if lock {
            self.locked = true;
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let chr_count = self.chr_memory.size() / Self::CHR_BANK_SIZE;
        if chr_count == 0 {
            return self.chr_memory.read(addr);
        }
        let bank = self.chr_bank as usize % chr_count;
        let offset = addr as usize & Self::CHR_BANK_MASK;
        self.chr_memory
            .read_at_index(bank * Self::CHR_BANK_SIZE + offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.prg_mode_128 as u8)
            | ((self.jumper_mode as u8) << 1)
            | ((self.locked as u8) << 2)
            | ((if self.mirroring == NametableLayout::Horizontal {
                1u8
            } else {
                0u8
            }) << 3);
        vec![self.chr_bank, self.prg_pp, self.prg_p, flags]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.chr_bank = data[0];
            self.prg_pp = data[1];
            self.prg_p = data[2];
            let flags = data[3];
            self.prg_mode_128 = flags & 1 != 0;
            self.jumper_mode = flags & 2 != 0;
            self.locked = flags & 4 != 0;
            self.mirroring = if flags & 8 != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            has_dynamic_mirroring: true,
            ..MapperCapabilities::default()
        }
    }

    fn reset(&mut self) {
        self.locked = false;
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper59;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const CHR_BANKS: usize = 5; // non-power-of-two to catch wrapping bugs
    const PRG_BANKS: usize = 6; // 6 × 16KB = 96KB

    fn make_mapper() -> Mapper59 {
        Mapper59::new(MapperContext::new_for_test(
            59,
            banked_data(0x4000, PRG_BANKS),
            banked_data(0x2000, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory ────────────────────────────────────────────────────────────

    #[test]
    fn mapper_59_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            59,
            banked_data(0x4000, PRG_BANKS),
            banked_data(0x2000, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 59 must be registered in the factory"
        );
    }

    // ── Power-on state ─────────────────────────────────────────────────────

    #[test]
    fn power_on_state_prg_bank_0_chr_bank_0_vertical_mirroring() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 at $8000");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── CHR banking ────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_by_address_bits_2_0() {
        let mut mapper = make_mapper();
        // Write addr = 0x8000 | (bank << 0) for CCC bits
        // Select CHR bank 3: A2:A0 = 011
        mapper.write_prg(0x8003, 0);
        // bank 3 % 5 = 3; each bank is filled with its bank index
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank 3 should be selected");
    }

    // ── Mirroring ──────────────────────────────────────────────────────────

    #[test]
    fn mirroring_selected_by_address_bit_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000 | (1 << 3), 0); // A3=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 0); // A3=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── PRG NROM-128 (S=1) ─────────────────────────────────────────────────

    #[test]
    fn nrom128_mode_s1_both_halves_same_bank() {
        let mut mapper = make_mapper();
        // S=1 (bit 7), PP=0 (bits 6:5), p=1 (bit 4) → bank 1
        // addr = 0x8000 | (1<<7) | (1<<4) = 0x8090
        mapper.write_prg(0x8090, 0);
        let lo = mapper.read_prg(0x8000); // bank 1
        let hi = mapper.read_prg(0xC000); // same bank 1
        assert_eq!(lo, 1, "$8000 must read bank 1 in NROM-128 mode");
        assert_eq!(hi, 1, "$C000 must read same bank 1 in NROM-128 mode");
    }

    // ── PRG NROM-256 (S=0) ─────────────────────────────────────────────────

    #[test]
    fn nrom256_mode_s0_8000_and_c000_are_different_halves() {
        let mut mapper = make_mapper();
        // S=0, PP=1 (bits 6:5 = 01 → A6=0, A5=1) → base = PP<<1 = 2
        // addr = 0x8000 | (1<<5) = 0x8020
        mapper.write_prg(0x8020, 0);
        let lo = mapper.read_prg(0x8000); // bank 2
        let hi = mapper.read_prg(0xC000); // bank 3
        assert_eq!(lo, 2, "$8000 must read bank 2 in NROM-256 PP=1 mode");
        assert_eq!(hi, 3, "$C000 must read bank 3 in NROM-256 PP=1 mode");
    }

    // ── Lock bit ───────────────────────────────────────────────────────────

    #[test]
    fn lock_bit_prevents_further_writes() {
        let mut mapper = make_mapper();
        // Lock while simultaneously selecting CHR bank 2 (A9=1, A2:A0=2 → addr = $8202)
        mapper.write_prg(0x8000 | (1 << 9) | 2, 0); // A9=1 (lock), A2:A0=2
        mapper.write_prg(0x8004, 0); // attempt to set CHR bank 4 → ignored
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Lock must prevent further writes"
        );
    }

    #[test]
    fn lock_releases_on_reset() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000 | (1 << 9), 0); // lock
        mapper.reset();
        mapper.write_prg(0x8004, 0); // CHR bank 4
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "After reset, lock must be released"
        );
    }

    // ── Snapshot round-trip ────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut original = make_mapper();
        original.write_prg(0x8000 | (1 << 7) | (2 << 5) | (1 << 4) | (1 << 3) | 3, 0);
        // S=1, PP=2, p=1, M=1, CCC=3

        let snap = original.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), original.read_prg(0x8000));
        assert_eq!(restored.get_mirroring(), original.get_mirroring());
    }
}
