//! Mapper 270 – OneBus Multicart (Family Pocket, Game Prince RS-16, Bittboy)
//!
//! Specifications:
//! - Primary: NesDev wiki returned 403; archived copy and backup mirror both
//!   returned 404 for mapper 270 specifically.
//! - Fallback: FCEUX `src/mappers/mapper270.c` and `src/mappers/hw/onebus.c`
//!   from `negativeExponent/libretro-fceumm_next`.
//!
//! ## Hardware summary
//!
//! Mapper 270 wraps an OneBus SOC (VT02/VT03) that is a NES-compatible chip
//! with extended registers. In default mode the inner PRG/CHR banking is
//! mathematically equivalent to MMC3 (standard IRQ scanline counter). A single
//! outer-bank register at `$412C` extends the physical address with two extra
//! bits (A24/A25 in terms of byte address, i.e. 8 KiB PRG-bank index bits 11
//! and 12). The exact bit assignment depends on the NES 2.0 submapper.
//!
//! ## Register map
//!
//! ```text
//! $8000–$FFFF  MMC3-compatible PRG/CHR/mirroring/IRQ registers (inner core)
//!
//! $412C – Outer Bank Register (write; read returns DIP state in bit 3 for submapper 2)
//! 7654 3210  (unused bits are ignored on write)
//!   Submapper 0 (combination):
//!     bit 2 → PRG/CHR A25  bit 1 → PRG/CHR A24  bit 0 → PRG/CHR A25 (alternative)
//!   Submapper 1 (Game Prince RS-16):
//!     bit 1 → PRG/CHR A24
//!   Submapper 2 (Family Pocket 638-in-1):
//!     bit 1 → PRG/CHR A24   bit 0 → PRG/CHR A25
//!     Read: bit 3 = DIP switch state
//!   Submapper 3 (Bittboy 300-in-1):
//!     bit 2 → PRG/CHR A24
//!
//! $4242 – CHR memory control (write)
//!   bit 0: 1 = enable unbanked 8 KiB CHR-RAM; 0 = use CHR-ROM via MMC3 banking
//! ```
//!
//! ## PRG banking
//!
//! The OneBus PRG banking in default mode is equivalent to MMC3 (inner range
//! 64 banks of 8 KiB). The outer register extends the bank index with bits 11+
//! via a simple OR:
//!
//! ```text
//! mblock      = submapper-dependent function of $412C (see above)
//! prg_bank    = mblock | inner_page   (upper bits from outer, lower 6 from inner)
//! ```
//!
//! ## CHR banking
//!
//! When CHR-RAM is not enabled, the 1 KiB CHR bank index is extended with the
//! same `mblock` value shifted left by 3 (scaling from 8 KiB to 1 KiB banks):
//!
//! ```text
//! chr_bank = (mblock << 3) | inner_1k_page
//! ```
//!
//! When CHR-RAM is enabled (`$4242` bit 0), all CHR accesses go to an unbanked
//! 8 KiB internal RAM buffer regardless of the bank registers.
//!
//! ## DIP switch
//!
//! A 1-bit DIP switch value is toggled on every soft/hard reset. For submapper
//! 2 only, reading `$412C` returns the open-bus value with bit 3 replaced by
//! the current DIP state. All other submappers return the open-bus value
//! unchanged.
//!
//! ## Mirroring / IRQ / PRG-RAM
//!
//! Standard MMC3 mirroring (`$A000`), scanline IRQ (`$C000–$E001`), and 8 KiB
//! PRG-RAM at `$6000–$7FFF`.
//!
//! ## Known Limitations
//!
//! - OneBus extended PPU registers (palette extensions, sprite enhancements)
//!   are not implemented.
//! - CHR data interleaving difference between OneBus and standard NES PPU bus
//!   is not emulated (same simplification as mapper 296).
//! - Source: FCEUX mapper270/onebus. No known discrepancies for the features
//!   that are implemented.

use crate::cartridge::Mapper;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;

const CHR_RAM_SIZE: usize = 8 * 1024;
const PRG_BANK_MASK: usize = 0x1FFF; // 8 KiB
const CHR_1K_BANK_MASK: usize = 0x03FF; // 1 KiB

/// Mapper 270 – OneBus Multicart
pub struct Mapper270 {
    mmc3: MMC3Mapper,
    /// NES 2.0 submapper (0–3) determines $412C bit assignments.
    submapper: u8,
    /// $412C outer bank register.
    outer_reg: u8,
    /// True when $4242 bit 0 is set → use unbanked 8 KiB CHR-RAM.
    chr_ram_enabled: bool,
    /// DIP switch state; toggled on every reset.
    dip: bool,
    /// 8 KiB CHR-RAM buffer (used when chr_ram_enabled).
    chr_ram: Vec<u8>,
}

impl Mapper270 {
    const MAPPER_NUMBER: u16 = 270;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let mmc3 = MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false);
        Self {
            mmc3,
            submapper,
            outer_reg: 0,
            chr_ram_enabled: false,
            dip: false,
            chr_ram: vec![0u8; CHR_RAM_SIZE],
        }
    }

    /// Compute the `mblock` outer-bank offset (in 8 KiB PRG-bank units) from
    /// the current `$412C` register and submapper.
    fn mblock(&self) -> usize {
        let r = self.outer_reg as usize;
        match self.submapper {
            1 => (r & 0x02) << 10,
            2 => ((r & 0x02) << 10) | ((r & 0x01) << 12),
            3 => (r & 0x04) << 10,
            _ => ((r & 0x06) << 10) | ((r & 0x01) << 12), // submapper 0
        }
    }

    /// Apply the outer PRG bank to an MMC3 inner 8 KiB page number.
    fn apply_prg_outer(&self, inner_page: usize) -> usize {
        self.mblock() | inner_page
    }

    /// Apply the outer CHR bank to an MMC3 inner 1 KiB page number.
    fn apply_chr_outer(&self, inner_1k_page: usize) -> usize {
        (self.mblock() << 3) | inner_1k_page
    }
}

impl Mapper for Mapper270 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) {
            return self.mmc3.read_prg(addr);
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let inner_page = self.mmc3.mapped_prg_bank(addr);
        let bank = self.apply_prg_outer(inner_page);
        let offset = (addr as usize) & PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if addr == 0x412C && self.submapper == 2 {
            return (open_bus & !0x08) | (if self.dip { 0x08 } else { 0x00 });
        }
        open_bus
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.mmc3.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x412C => self.outer_reg = value,
            0x4242 => self.chr_ram_enabled = (value & 0x01) != 0,
            0x8000..=0xFFFF => self.mmc3.write_prg(addr, value),
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.chr_ram_enabled {
            return self.chr_ram[(addr as usize) & 0x1FFF];
        }
        let inner_1k_page = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner_1k_page);
        let offset = (addr as usize) & CHR_1K_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.chr_ram_enabled {
            self.chr_ram[(addr as usize) & 0x1FFF] = value;
            return;
        }
        let inner_1k_page = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner_1k_page);
        let offset = (addr as usize) & CHR_1K_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer_reg);
        snap.push(self.chr_ram_enabled as u8);
        snap.push(self.dip as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // 16 bytes MMC3 + 3 extra; only restore extra fields when present.
        if data.len() >= 16 + 3 {
            let (mmc3_part, extra) = data.split_at(data.len() - 3);
            self.mmc3.restore_registers(mmc3_part);
            self.outer_reg = extra[0];
            self.chr_ram_enabled = extra[1] != 0;
            self.dip = extra[2] != 0;
        } else {
            self.mmc3.restore_registers(data);
            self.outer_reg = 0;
            self.chr_ram_enabled = false;
            self.dip = false;
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
        self.chr_ram = vec![0u8; CHR_RAM_SIZE];
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.dip = !self.dip;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Non-power-of-2 bank counts to avoid modulo-wrap false passes.
    const PRG_8K_BANKS: usize = 9;
    const CHR_1K_BANKS: usize = 48;

    fn make_mapper_sub(submapper: u8) -> Mapper270 {
        Mapper270::new(
            MapperContext::new_for_test(
                270,
                banked_data(8 * 1024, PRG_8K_BANKS),
                banked_data(1024, CHR_1K_BANKS),
                NametableLayout::Horizontal,
            )
            .with_submapper(submapper),
        )
    }

    fn make_mapper() -> Mapper270 {
        make_mapper_sub(0)
    }

    fn make_mapper_chr_ram() -> Mapper270 {
        Mapper270::new(MapperContext::new_for_test(
            270,
            banked_data(8 * 1024, PRG_8K_BANKS),
            vec![], // empty CHR → CHR-RAM
            NametableLayout::Horizontal,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_270_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            270,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 270 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // Default state = standard MMC3 PRG banking (outer_reg = 0)
    // -------------------------------------------------------------------------

    #[test]
    fn default_state_prg_acts_like_mmc3() {
        let mut mapper = make_mapper();
        // Select R6 (PRG $8000) = bank 3
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "default PRG must act like MMC3");
    }

    #[test]
    fn default_state_chr_acts_like_mmc3() {
        let mut mapper = make_mapper();
        // Select R2 (CHR $1000, 1 KiB) = bank 5
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x1000), 5, "default CHR must act like MMC3");
    }

    // -------------------------------------------------------------------------
    // PRG outer banking – submapper 0 (combination)
    // -------------------------------------------------------------------------

    #[test]
    fn submapper0_prg_outer_a24_from_412c_bit1() {
        let mut mapper = make_mapper_sub(0);
        // bit 1 of $412C → mblock bit 11 = 0x800 = 2048 PRG banks
        mapper.write_prg(0x412C, 0x02); // outer_reg = 0x02
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner = 0 → bank = 2048 % 9 = 5
        assert_eq!(
            mapper.read_prg(0x8000),
            (2048usize % PRG_8K_BANKS) as u8,
            "submapper 0: $412C bit1 → A24 (mblock=0x800)"
        );
    }

    #[test]
    fn submapper0_prg_outer_a25_from_412c_bit2() {
        let mut mapper = make_mapper_sub(0);
        // bit 2 of $412C → mblock bit 12 = 0x1000 = 4096
        mapper.write_prg(0x412C, 0x04); // outer_reg = 0x04
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner = 0 → bank = 4096 % 9 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            (4096usize % PRG_8K_BANKS) as u8,
            "submapper 0: $412C bit2 → A25 (mblock=0x1000)"
        );
    }

    #[test]
    fn submapper0_prg_outer_a25_alternative_from_412c_bit0() {
        let mut mapper = make_mapper_sub(0);
        // bit 0 of $412C also maps to mblock bit 12 = 0x1000 = 4096
        mapper.write_prg(0x412C, 0x01); // outer_reg = 0x01
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner = 0 → bank = 4096 % 9 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            (4096usize % PRG_8K_BANKS) as u8,
            "submapper 0: $412C bit0 → A25 alternative (mblock=0x1000)"
        );
    }

    // -------------------------------------------------------------------------
    // PRG outer banking – submapper 1
    // -------------------------------------------------------------------------

    #[test]
    fn submapper1_prg_outer_a24_from_412c_bit1() {
        let mut mapper = make_mapper_sub(1);
        mapper.write_prg(0x412C, 0x02);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            (2048usize % PRG_8K_BANKS) as u8,
            "submapper 1: $412C bit1 → A24"
        );
    }

    #[test]
    fn submapper1_prg_outer_bit0_has_no_effect() {
        let mut mapper = make_mapper_sub(1);
        mapper.write_prg(0x412C, 0x01); // only bit 0 set; submapper 1 ignores it
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner = 0 → bank = 0 % 9 = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "submapper 1: $412C bit0 must have no effect"
        );
    }

    // -------------------------------------------------------------------------
    // PRG outer banking – submapper 2
    // -------------------------------------------------------------------------

    #[test]
    fn submapper2_prg_outer_a24_from_412c_bit1() {
        let mut mapper = make_mapper_sub(2);
        mapper.write_prg(0x412C, 0x02);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            (2048usize % PRG_8K_BANKS) as u8,
            "submapper 2: $412C bit1 → A24"
        );
    }

    #[test]
    fn submapper2_prg_outer_a25_from_412c_bit0() {
        let mut mapper = make_mapper_sub(2);
        mapper.write_prg(0x412C, 0x01);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            (4096usize % PRG_8K_BANKS) as u8,
            "submapper 2: $412C bit0 → A25"
        );
    }

    // -------------------------------------------------------------------------
    // PRG outer banking – submapper 3
    // -------------------------------------------------------------------------

    #[test]
    fn submapper3_prg_outer_a24_from_412c_bit2() {
        let mut mapper = make_mapper_sub(3);
        // bit 2 of $412C → (0x04 & 0x04) << 10 = 0x04 << 10 = 0x1000 = 4096
        mapper.write_prg(0x412C, 0x04);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0); // inner = 0 → bank = 4096 % 9 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            (4096usize % PRG_8K_BANKS) as u8,
            "submapper 3: $412C bit2 → mblock=0x1000"
        );
    }

    #[test]
    fn submapper3_prg_outer_bit1_has_no_effect() {
        let mut mapper = make_mapper_sub(3);
        mapper.write_prg(0x412C, 0x02); // bit 1 set; submapper 3 ignores it
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "submapper 3: $412C bit1 must have no effect"
        );
    }

    // -------------------------------------------------------------------------
    // CHR outer banking (submapper 1, bit1 → A24)
    // -------------------------------------------------------------------------

    #[test]
    fn submapper1_chr_outer_a24_from_412c_bit1() {
        let mut mapper = make_mapper_sub(1);
        // mblock = 0x800; chr mblock << 3 = 0x4000 = 16384
        // inner_1k_page = 0 → bank = 16384 % 48 = 16
        mapper.write_prg(0x412C, 0x02);
        mapper.write_prg(0x8000, 0x02); // R2 → CHR $1000
        mapper.write_prg(0x8001, 0); // inner = 0
        let expected = (0x4000usize % CHR_1K_BANKS) as u8; // = 16
        assert_eq!(
            mapper.read_chr(0x1000),
            expected,
            "submapper 1: CHR outer A24 from $412C bit1"
        );
    }

    #[test]
    fn submapper2_chr_outer_a25_from_412c_bit0() {
        let mut mapper = make_mapper_sub(2);
        // mblock = 0x1000; chr mblock << 3 = 0x8000 = 32768
        // inner = 0 → bank = 32768 % 48 = 32
        mapper.write_prg(0x412C, 0x01);
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0);
        let expected = (0x8000usize % CHR_1K_BANKS) as u8; // = 32
        assert_eq!(
            mapper.read_chr(0x1000),
            expected,
            "submapper 2: CHR outer A25 from $412C bit0"
        );
    }

    // -------------------------------------------------------------------------
    // CHR-RAM enable via $4242
    // -------------------------------------------------------------------------

    #[test]
    fn chr_ram_enabled_by_4242_bit0() {
        let mut mapper = make_mapper_chr_ram();
        mapper.write_prg(0x4242, 0x01); // enable CHR-RAM
        mapper.write_chr(0x1234, 0xAB);
        assert_eq!(
            mapper.read_chr(0x1234),
            0xAB,
            "CHR-RAM write/read must work when $4242 bit0 is set"
        );
    }

    #[test]
    fn chr_ram_disabled_by_4242_bit0_clear() {
        let mut mapper = make_mapper_chr_ram();
        mapper.write_prg(0x4242, 0x01); // enable
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_prg(0x4242, 0x00); // disable
        // After disabling CHR-RAM, reads return CHR-ROM data (bank 0 = 0)
        // banked_data fills each 1K bank with its index; bank 0 = byte 0
        assert_ne!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM disabled: reads must return CHR-ROM data, not stale RAM"
        );
    }

    #[test]
    fn chr_rom_used_when_chr_ram_not_enabled() {
        let mut mapper = make_mapper();
        // Default: chr_ram_enabled = false; R2 = bank 7
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 7);
        assert_eq!(
            mapper.read_chr(0x1000),
            7,
            "CHR-ROM banking must work when CHR-RAM is not enabled"
        );
    }

    // -------------------------------------------------------------------------
    // DIP switch
    // -------------------------------------------------------------------------

    #[test]
    fn dip_switch_starts_false() {
        let mapper = make_mapper_sub(2);
        // For submapper 2: $412C bit3 = DIP; starts as 0
        assert_eq!(
            mapper.read_prg_open_bus(0x412C, 0xFF) & 0x08,
            0x00,
            "DIP switch must start as 0"
        );
    }

    #[test]
    fn dip_switch_toggles_on_reset() {
        let mut mapper = make_mapper_sub(2);
        assert_eq!(mapper.read_prg_open_bus(0x412C, 0x00) & 0x08, 0x00);
        mapper.reset();
        assert_eq!(
            mapper.read_prg_open_bus(0x412C, 0x00) & 0x08,
            0x08,
            "DIP switch must be 1 after first reset"
        );
        mapper.reset();
        assert_eq!(
            mapper.read_prg_open_bus(0x412C, 0x00) & 0x08,
            0x00,
            "DIP switch must toggle back to 0 after second reset"
        );
    }

    #[test]
    fn dip_switch_readable_only_for_submapper2() {
        let mut mapper0 = make_mapper_sub(0);
        let mut mapper1 = make_mapper_sub(1);
        let mut mapper3 = make_mapper_sub(3);
        mapper0.reset();
        mapper1.reset();
        mapper3.reset();
        // For submappers 0, 1, 3: reading $412C must return open_bus unchanged
        assert_eq!(
            mapper0.read_prg_open_bus(0x412C, 0x00),
            0x00,
            "submapper 0: $412C read must not expose DIP"
        );
        assert_eq!(
            mapper1.read_prg_open_bus(0x412C, 0x00),
            0x00,
            "submapper 1: $412C read must not expose DIP"
        );
        assert_eq!(
            mapper3.read_prg_open_bus(0x412C, 0x00),
            0x00,
            "submapper 3: $412C read must not expose DIP"
        );
    }

    // -------------------------------------------------------------------------
    // Save state
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trip() {
        let mut mapper = make_mapper_sub(2);
        mapper.write_prg(0x412C, 0x03);
        mapper.write_prg(0x4242, 0x01);
        mapper.reset(); // dip = true

        let snap = mapper.registers_snapshot();
        assert!(
            snap.len() >= 19,
            "snapshot must include MMC3 + 3 extra bytes"
        );

        let mut mapper2 = make_mapper_sub(2);
        mapper2.restore_registers(&snap);
        assert_eq!(mapper2.outer_reg, 0x03, "outer_reg must survive round-trip");
        assert!(
            mapper2.chr_ram_enabled,
            "chr_ram_enabled must survive round-trip"
        );
        assert!(mapper2.dip, "dip must survive round-trip");
    }

    #[test]
    fn restore_from_legacy_mmc3_snapshot_resets_extra() {
        let mut mapper = make_mapper_sub(2);
        mapper.write_prg(0x412C, 0x03);
        mapper.write_prg(0x4242, 0x01);
        mapper.reset(); // dip = true

        // Provide only 16 bytes (MMC3 snapshot, no extra fields)
        let mmc3_snap: Vec<u8> = vec![0u8; 16];
        mapper.restore_registers(&mmc3_snap);
        assert_eq!(
            mapper.outer_reg, 0,
            "outer_reg must reset for legacy snapshot"
        );
        assert!(
            !mapper.chr_ram_enabled,
            "chr_ram_enabled must reset for legacy snapshot"
        );
        assert!(!mapper.dip, "dip must reset for legacy snapshot");
    }
}
