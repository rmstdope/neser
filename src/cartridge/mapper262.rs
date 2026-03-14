//! Mapper 262 – Street Heroes (MMC3 variant with CHR-RAM and soft-reset switch)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_StreetHeroes.h`
//!   (NesDev wiki page unavailable at time of implementation)
//!
//! Hardware: Unlicensed board used by "Street Heroes" (街道英雄).
//!
//! PRG banking: Standard MMC3 8 KiB banking with scanline IRQ (A12 rising-edge).
//!
//! Register at `$4100` (read/write):
//! - Write: sets ex_reg (`value`); controls CHR outer bank and CHR-RAM mode.
//! - Read:  returns `reset_switch` (toggled on every soft reset).
//!
//! CHR banking (when ex_reg bit 6 = 0):
//! - MMC3 supplies a 1 KiB base bank for each slot; ex_reg contributes bit 8
//!   of the resulting 9-bit bank index depending on the PPU address slot:
//!   - PPU $0000–$07FF (slots 0–1): outer bit from ex_reg bit 3
//!   - PPU $0800–$0FFF (slots 2–3): outer bit from ex_reg bit 2
//!   - PPU $1000–$17FF (slots 4–5): outer bit from ex_reg bit 0
//!   - PPU $1800–$1FFF (slots 6–7): outer bit from ex_reg bit 1
//!
//! CHR-RAM mode (when ex_reg bit 6 = 1):
//! - The entire 8 KiB CHR window maps to the on-board 8 KiB CHR-RAM.
//! - CHR-ROM bank registers are ignored.
//!
//! Soft-reset switch:
//! - On every soft reset the `reset_switch` byte toggles between `0x00` and `0xFF`.
//! - Reading `$4100` returns the current `reset_switch` value.
//!
//! PRG-RAM: 8 KiB at `$6000–$7FFF` (standard MMC3).
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!   Source: Mesen2 `MMC3_StreetHeroes.h`. No known deltas.

use crate::cartridge::Mapper;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;

/// Mapper 262 – Street Heroes (MMC3 variant with CHR-RAM and soft-reset switch)
pub struct Mapper262 {
    mmc3: MMC3Mapper,
    ex_reg: u8,
    reset_switch: u8,
    chr_ram: [u8; Self::CHR_RAM_SIZE],
}

impl Mapper262 {
    const MAPPER_NUMBER: u16 = 262;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            ex_reg: 0,
            reset_switch: 0,
            chr_ram: [0; Self::CHR_RAM_SIZE],
        }
    }

    /// Return the outer bank bit (bit 8 of the 1 KiB CHR bank index) contributed
    /// by the current `ex_reg` value for the given PPU address.
    ///
    /// The outer bit depends on which 2 KiB PPU CHR group the address falls into:
    /// - Slots 0–1 ($0000–$07FF): ex_reg bit 3 → bank bit 8
    /// - Slots 2–3 ($0800–$0FFF): ex_reg bit 2 → bank bit 8
    /// - Slots 4–5 ($1000–$17FF): ex_reg bit 0 → bank bit 8
    /// - Slots 6–7 ($1800–$1FFF): ex_reg bit 1 → bank bit 8
    fn chr_outer_bit(&self, ppu_addr: u16) -> usize {
        let slot = ((ppu_addr & 0x1FFF) >> 10) as usize;
        match slot {
            0 | 1 => ((self.ex_reg as usize) & 0x08) << 5, // bit3 → bit8
            2 | 3 => ((self.ex_reg as usize) & 0x04) << 6, // bit2 → bit8
            4 | 5 => ((self.ex_reg as usize) & 0x01) << 8, // bit0 → bit8
            _ => ((self.ex_reg as usize) & 0x02) << 7,     // bit1 → bit8
        }
    }
}

impl Mapper for Mapper262 {
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
        if addr == 0x4100 {
            return self.reset_switch;
        }
        self.mmc3.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if addr == 0x4100 {
            return self.reset_switch;
        }
        self.mmc3.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr == 0x4100 {
            self.ex_reg = value;
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.ex_reg & 0x40 != 0 {
            self.chr_ram[(addr & 0x1FFF) as usize]
        } else {
            let bank = self.mmc3.raw_chr_1k_bank(addr) | self.chr_outer_bit(addr);
            let offset = (addr as usize) & Self::CHR_BANK_MASK;
            self.mmc3.read_chr_1k_at(bank, offset)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.ex_reg & 0x40 != 0 {
            self.chr_ram[(addr & 0x1FFF) as usize] = value;
        } else {
            let bank = self.mmc3.raw_chr_1k_bank(addr) | self.chr_outer_bit(addr);
            let offset = (addr as usize) & Self::CHR_BANK_MASK;
            self.mmc3.write_chr_1k_at(bank, offset, value);
        }
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

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.to_vec()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let len = data.len().min(Self::CHR_RAM_SIZE);
        self.chr_ram[..len].copy_from_slice(&data[..len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap.push(self.reset_switch);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const MMC3_SNAP_LEN: usize = 16;
        if data.len() >= MMC3_SNAP_LEN {
            self.mmc3.restore_registers(&data[..MMC3_SNAP_LEN]);
        }
        if data.len() > MMC3_SNAP_LEN {
            self.ex_reg = data[MMC3_SNAP_LEN];
        }
        if data.len() > MMC3_SNAP_LEN + 1 {
            self.reset_switch = data[MMC3_SNAP_LEN + 1];
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn reset(&mut self) {
        self.reset_switch ^= 0xFF;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;
    use crate::console::RamInitMode;

    // Use a non-power-of-2 CHR bank count to catch modulo-wrap false passes.
    const CHR_1K_BANKS: usize = 48;

    fn make_mapper() -> Mapper262 {
        Mapper262::new(MapperContext::new_for_test(
            262,
            banked_data(8 * 1024, 8),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_262_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            262,
            banked_data(8 * 1024, 8),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 262 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // PRG banking (standard MMC3)
    // -------------------------------------------------------------------------

    #[test]
    fn prg_banking_uses_standard_mmc3() {
        let mut mapper = make_mapper();

        // Select PRG bank 3 at $8000
        mapper.write_prg(0x8000, 6); // select R6
        mapper.write_prg(0x8001, 3); // R6 = bank 3

        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank 3 should be at $8000");
    }

    // -------------------------------------------------------------------------
    // $4100 register write sets ex_reg; read returns reset_switch
    // -------------------------------------------------------------------------

    #[test]
    fn read_from_4100_returns_reset_switch_at_power_on() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x4100),
            0x00,
            "$4100 read must return reset_switch (0x00 at power-on)"
        );
    }

    #[test]
    fn write_to_4100_sets_ex_reg_without_changing_reset_switch() {
        let mut mapper = make_mapper();

        // Trigger soft reset first so reset_switch = 0xFF
        mapper.reset();

        // Write ex_reg via $4100
        mapper.write_prg(0x4100, 0x40);

        // read_prg($4100) must still return reset_switch (0xFF), not ex_reg
        assert_eq!(
            mapper.read_prg(0x4100),
            0xFF,
            "$4100 read must return reset_switch, not the value just written"
        );
    }

    #[test]
    fn read_prg_open_bus_4100_returns_reset_switch() {
        let mut mapper = make_mapper();
        mapper.reset(); // reset_switch = 0xFF

        assert_eq!(
            mapper.read_prg_open_bus(0x4100, 0xAA),
            0xFF,
            "read_prg_open_bus($4100) must return reset_switch"
        );
    }

    // -------------------------------------------------------------------------
    // Soft-reset switch
    // -------------------------------------------------------------------------

    #[test]
    fn soft_reset_toggles_reset_switch_to_ff() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x4100), 0x00);

        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x4100),
            0xFF,
            "First soft reset must set reset_switch to 0xFF"
        );
    }

    #[test]
    fn second_soft_reset_toggles_reset_switch_back_to_00() {
        let mut mapper = make_mapper();
        mapper.reset();
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x4100),
            0x00,
            "Second soft reset must toggle reset_switch back to 0x00"
        );
    }

    // -------------------------------------------------------------------------
    // CHR-RAM mode (ex_reg bit 6 = 1)
    // -------------------------------------------------------------------------

    #[test]
    fn chr_ram_mode_enables_writes_to_chr_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x40); // enable CHR-RAM mode
        mapper.write_chr(0x0000, 0xAB);

        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM mode: write must be readable back"
        );
    }

    #[test]
    fn chr_ram_mode_full_8k_window_is_writable() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x40); // enable CHR-RAM mode
        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x0FFF, 0x22);
        mapper.write_chr(0x1000, 0x33);
        mapper.write_chr(0x1FFF, 0x44);

        assert_eq!(mapper.read_chr(0x0000), 0x11);
        assert_eq!(mapper.read_chr(0x0FFF), 0x22);
        assert_eq!(mapper.read_chr(0x1000), 0x33);
        assert_eq!(mapper.read_chr(0x1FFF), 0x44);
    }

    #[test]
    fn disabling_chr_ram_mode_switches_back_to_chr_rom() {
        let mut mapper = make_mapper();

        // Write something in CHR-RAM
        mapper.write_prg(0x4100, 0x40); // CHR-RAM mode
        mapper.write_chr(0x0000, 0xFF);

        // Disable CHR-RAM mode (ex_reg bit 6 = 0)
        mapper.write_prg(0x4100, 0x00);

        // With MMC3 bank regs at default (all 0), CHR-ROM bank 0 is mapped at $0000.
        // banked_data fills each 1KB bank with the bank index; bank 0 = all 0x00.
        assert_eq!(
            mapper.read_chr(0x0000),
            0x00,
            "After disabling CHR-RAM mode, reads must come from CHR-ROM"
        );
    }

    // -------------------------------------------------------------------------
    // CHR outer bank bits (ex_reg bit 6 = 0)
    // -------------------------------------------------------------------------

    #[test]
    fn chr_slots_0_1_use_ex_reg_bit3_as_outer_bank_bit() {
        let mut mapper = make_mapper();

        // Map CHR bank 0 via R0 (slots 0-1 in MMC3 mode 0)
        mapper.write_prg(0x8000, 0x00); // bank_select = 0 → select R0
        mapper.write_prg(0x8001, 0); // R0 = 0 → slots 0-1 use bank 0 (even-aligned)

        // ex_reg bit3 = 0 → outer bit = 0 → effective bank = 0
        mapper.write_prg(0x4100, 0x00);
        // bank 0 from banked_data is all 0x00
        assert_eq!(
            mapper.read_chr(0x0000),
            0x00,
            "slot 0 with outer=0 → bank 0"
        );

        // ex_reg bit3 = 1 → outer bit = 0x100 = 256 → effective bank = 256
        // With CHR_1K_BANKS=48, bank 256 wraps to 256 % 48 = 16.
        // banked_data fills bank 16 with value 16.
        mapper.write_prg(0x4100, 0x08); // bit3 set
        assert_eq!(
            mapper.read_chr(0x0000),
            16,
            "slot 0 with outer=1, R0=0 → effective bank 256 % 48 = 16"
        );
    }

    #[test]
    fn chr_slots_2_3_use_ex_reg_bit2_as_outer_bank_bit() {
        let mut mapper = make_mapper();

        // Map R1 = 0 → slots 2-3 use banks 0 and 1 in CHR mode 0
        mapper.write_prg(0x8000, 0x01); // select R1
        mapper.write_prg(0x8001, 0); // R1 = 0 → slots 2-3 use bank 0

        // ex_reg bit2 = 0 → outer = 0 → effective bank 0
        mapper.write_prg(0x4100, 0x00);
        assert_eq!(
            mapper.read_chr(0x0800),
            0x00,
            "slot 2 with outer=0 → bank 0"
        );

        // ex_reg bit2 = 1 → outer = 0x100 = 256 → 256 % 48 = 16
        mapper.write_prg(0x4100, 0x04); // bit2 set
        assert_eq!(
            mapper.read_chr(0x0800),
            16,
            "slot 2 with outer=1, R1=0 → effective bank 256 % 48 = 16"
        );
    }

    #[test]
    fn chr_slots_4_5_use_ex_reg_bit0_as_outer_bank_bit() {
        let mut mapper = make_mapper();

        // Map R2 = 0 → slot 4 uses bank 0
        mapper.write_prg(0x8000, 0x02); // select R2
        mapper.write_prg(0x8001, 0); // R2 = 0

        // ex_reg bit0 = 0 → outer = 0
        mapper.write_prg(0x4100, 0x00);
        assert_eq!(
            mapper.read_chr(0x1000),
            0x00,
            "slot 4 with outer=0 → bank 0"
        );

        // ex_reg bit0 = 1 → outer = 0x100 → 256 % 48 = 16
        mapper.write_prg(0x4100, 0x01); // bit0 set
        assert_eq!(
            mapper.read_chr(0x1000),
            16,
            "slot 4 with outer=1, R2=0 → effective bank 256 % 48 = 16"
        );
    }

    #[test]
    fn chr_slots_6_7_use_ex_reg_bit1_as_outer_bank_bit() {
        let mut mapper = make_mapper();

        // Map R4 = 0 → slot 6 uses bank 0
        mapper.write_prg(0x8000, 0x04); // select R4
        mapper.write_prg(0x8001, 0); // R4 = 0

        // ex_reg bit1 = 0 → outer = 0
        mapper.write_prg(0x4100, 0x00);
        assert_eq!(
            mapper.read_chr(0x1800),
            0x00,
            "slot 6 with outer=0 → bank 0"
        );

        // ex_reg bit1 = 1 → outer = 0x100 → 256 % 48 = 16
        mapper.write_prg(0x4100, 0x02); // bit1 set
        assert_eq!(
            mapper.read_chr(0x1800),
            16,
            "slot 6 with outer=1, R4=0 → effective bank 256 % 48 = 16"
        );
    }

    // -------------------------------------------------------------------------
    // MMC3 mirroring
    // -------------------------------------------------------------------------

    #[test]
    fn mmc3_mirroring_is_controllable_via_a000() {
        let mut mapper = make_mapper();

        // $A000 bit 0: 0=Vertical, 1=Horizontal
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------------
    // Save state
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_and_restore_roundtrip() {
        let mut mapper = make_mapper();

        // Set non-default state
        mapper.write_prg(0x4100, 0x08); // ex_reg with bit3
        mapper.reset(); // reset_switch = 0xFF
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 3); // R6 = 3

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x4100),
            0xFF,
            "reset_switch must be restored"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "MMC3 PRG bank must be restored"
        );
    }

    #[test]
    fn chr_ram_snapshot_and_restore_roundtrip() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x40); // CHR-RAM mode
        mapper.write_chr(0x0100, 0xDE);
        mapper.write_chr(0x1F00, 0xAD);

        let snap = mapper.chr_ram_snapshot();
        assert_eq!(snap.len(), 8 * 1024, "CHR-RAM snapshot must be 8 KiB");

        let mut restored = make_mapper();
        restored.restore_chr_ram(&snap);
        restored.write_prg(0x4100, 0x40); // enable CHR-RAM mode to read back

        assert_eq!(restored.read_chr(0x0100), 0xDE);
        assert_eq!(restored.read_chr(0x1F00), 0xAD);
    }

    #[test]
    fn initialize_ram_zero_clears_chr_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x40); // CHR-RAM mode
        mapper.write_chr(0x0000, 0xFF);

        mapper.initialize_ram(RamInitMode::Zero);

        assert_eq!(
            mapper.read_chr(0x0000),
            0x00,
            "initialize_ram(Zero) must clear CHR-RAM"
        );
    }
}
