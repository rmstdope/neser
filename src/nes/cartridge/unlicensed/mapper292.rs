//! Mapper 292 – BMW8544 / Dragon Fighter (Thin Chen / JY)
//!
//! Specifications:
//! - Primary: FCEUX libretro-fceumm source (negativeExponent/libretro-fceumm_next)
//! - Secondary: Furbtendulator (FurblandChannel/Furbtendulator) for complete behavior
//!
//! Hardware summary:
//! - PCB: BMW8544 / UNIF UNL-DRAGONFIGHTER
//! - PRG-ROM: Up to 512 KiB (standard MMC3 R6/R7 banking, with $8000–$9FFF
//!   overridden by bits 4:0 of the last write to $6000–$7FFF)
//! - CHR-ROM: Fixed-layout custom banking (sprites at $0000–$0FFF, BG at $1000–$1FFF):
//!   - $0000–$07FF: 2 KiB bank = `((MMC3_R0 & 0xFE) >> 1) XOR sprite_xor`
//!   - $0800–$0FFF: 2 KiB bank = `((MMC3_R1 & 0xFE) >> 1) XOR sprite_or`
//!   - $1000–$1FFF: 4 KiB bank = `bg_chr & 0x3F`
//! - Mirroring: Programmable via MMC3 ($A000 write)
//! - IRQ: Standard MMC3 scanline counter (A12 rising-edge)
//! - PRG-RAM: None ($6000–$7FFF is the protection register region)
//!
//! Protection mechanism:
//! - Write to $6000–$7FFF: sets protection register; bits 4:0 = extra PRG bank for
//!   $8000–$9FFF; bit 5 = mode (0 = sprite, 1 = BG)
//! - Every CPU write to $6000–$FFFF updates the cpu_latch
//! - Read from $6000–$7FFF (returns open bus):
//!   - If mode = BG (bit 5 set): `bg_chr = cpu_latch & 0x3F`,
//!     `sprite_or = (cpu_latch & 0x40) << 1`
//!   - If mode = sprite (bit 5 clear): `sprite_xor = cpu_latch`
//!
//! Known Limitations:
//! - The cpu_latch only tracks writes in $4020–$FFFF (mapper device range).
//!   Hardware intercepts all 65536 CPU writes, but neser's mapper interface
//!   only delivers writes for addresses $4020 and above. Games that rely on
//!   writes to CPU RAM ($0000–$401F) before reading $6000–$7FFF may behave
//!   differently on hardware.

use std::cell::Cell;

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 292 – BMW8544 (Dragon Fighter protected MMC3 variant)
pub struct Mapper292 {
    inner: MMC3Mapper,

    /// Protection register: bit 5 = load_bg mode; bits 4:0 = extra PRG bank
    prot_reg: u8,
    /// CPU write latch: holds the most recent value written to $6000–$FFFF.
    cpu_latch: u8,

    // CHR protection state – mutable from reads via interior mutability
    /// XOR applied to the MMC3 R0-based 2KB bank index for PPU $0000–$07FF
    sprite_xor: Cell<u8>,
    /// XOR applied to the MMC3 R1-based 2KB bank index for PPU $0800–$0FFF
    /// (always 0 or 0x80; set from bit 6 of the latch)
    sprite_or: Cell<u8>,
    /// 4KB BG CHR bank for PPU $1000–$1FFF (bits 5:0)
    bg_chr: Cell<u8>,
}

impl Mapper292 {
    const MAPPER_NUMBER: u16 = 292;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        // Dragon Fighter has no PRG-RAM; $6000–$7FFF is the protection window.
        // Pass prg_ram_banks_8k = 0 so the inner MMC3 does not allocate PRG-RAM.
        let inner = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            0,
        );
        Self {
            inner,
            prot_reg: 0,
            cpu_latch: 0,
            sprite_xor: Cell::new(0),
            sprite_or: Cell::new(0),
            bg_chr: Cell::new(0),
        }
    }

    /// Compute the 1KB CHR bank index and intra-bank offset for a PPU address.
    ///
    /// CHR layout (always sprites-low, BG-high regardless of MMC3 CHR mode bit):
    /// - $0000–$07FF: 2 KiB sprite bank A (XOR with sprite_xor)
    /// - $0800–$0FFF: 2 KiB sprite bank B (XOR with sprite_or)
    /// - $1000–$1FFF: 4 KiB BG bank (direct from bg_chr)
    fn chr_1k_bank_and_offset(&self, ppu_addr: u16) -> (usize, usize) {
        let addr = (ppu_addr & 0x1FFF) as usize;

        if addr < 0x0800 {
            // $0000–$07FF: sprite bank A
            // MMC3 R0 selects even-aligned 2KB bank; convert to 2KB index, then XOR
            let r0 = (self.inner.chr_bank_reg(0) & 0xFE) as usize;
            let base_2k = r0 >> 1;
            let actual_2k = base_2k ^ (self.sprite_xor.get() as usize);
            let bank_1k = actual_2k * 2 + (addr >> 10);
            let offset = addr & 0x3FF;
            (bank_1k, offset)
        } else if addr < 0x1000 {
            // $0800–$0FFF: sprite bank B
            let r1 = (self.inner.chr_bank_reg(1) & 0xFE) as usize;
            let base_2k = r1 >> 1;
            let actual_2k = base_2k ^ (self.sprite_or.get() as usize);
            let local = addr - 0x0800;
            let bank_1k = actual_2k * 2 + (local >> 10);
            let offset = local & 0x3FF;
            (bank_1k, offset)
        } else {
            // $1000–$1FFF: 4KB BG bank
            let bg = (self.bg_chr.get() & 0x3F) as usize;
            let local = addr - 0x1000;
            let bank_1k = bg * 4 + (local >> 10);
            let offset = local & 0x3FF;
            (bank_1k, offset)
        }
    }

    /// Apply the protection read side-effect using the stored prot_reg / cpu_latch.
    fn apply_protection_latch(&self) {
        if self.prot_reg & 0x20 != 0 {
            // BG mode: load bg_chr and sprite_or from cpu_latch
            self.bg_chr.set(self.cpu_latch & 0x3F);
            self.sprite_or.set((self.cpu_latch & 0x40) << 1);
        } else {
            // Sprite mode: load sprite_xor from cpu_latch
            self.sprite_xor.set(self.cpu_latch);
        }
    }
}

impl Mapper for Mapper292 {
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

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // Protection region: no PRG-RAM; return 0 (open-bus side effect
                // is handled by read_prg_open_bus, not here)
                0
            }
            0x8000..=0x9FFF => {
                // $8000–$9FFF: overridden by bits 4:0 of prot_reg
                let bank = (self.prot_reg & 0x1F) as usize;
                let offset = (addr - 0x8000) as usize;
                self.inner.read_prg_at_bank(bank, offset)
            }
            _ => self.inner.read_prg(addr),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) {
            // Reading the protection register applies the cpu_latch to CHR state
            self.apply_protection_latch();
            open_bus
        } else {
            self.inner.read_prg_open_bus(addr, open_bus)
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Every write to $6000–$FFFF updates the cpu_latch
        self.cpu_latch = value;

        if (0x6000..=0x7FFF).contains(&addr) {
            // Protection register write: no PRG-RAM, just store control bits
            self.prot_reg = value;
        } else {
            // $8000–$FFFF: delegate to MMC3 for bank registers / IRQ / mirroring
            self.inner.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let (bank_1k, offset) = self.chr_1k_bank_and_offset(ppu_addr);
        self.inner.read_chr_1k_at(bank_1k, offset)
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let (bank_1k, offset) = self.chr_1k_bank_and_offset(ppu_addr);
        self.inner.write_chr_1k_at(bank_1k, offset, value);
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_wram_snapshot(&mut self, _data: &[u8]) {}

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.push(self.prot_reg);
        snap.push(self.cpu_latch);
        snap.push(self.sprite_xor.get());
        snap.push(self.sprite_or.get());
        snap.push(self.bg_chr.get());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const EXTRA: usize = 5;
        if data.len() >= EXTRA {
            let (mmc3_data, extra) = data.split_at(data.len() - EXTRA);
            self.inner.restore_registers(mmc3_data);
            self.prot_reg = extra[0];
            self.cpu_latch = extra[1];
            self.sprite_xor.set(extra[2]);
            self.sprite_or.set(extra[3]);
            self.bg_chr.set(extra[4]);
        } else {
            // Snapshot predates mapper292 extra bytes; restore inner MMC3 state only
            self.inner.restore_registers(data);
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.prot_reg = 0;
        self.cpu_latch = 0;
        self.sprite_xor.set(0);
        self.sprite_or.set(0);
        self.bg_chr.set(0);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // PRG: 12 × 8KB = 96KB  (non-power-of-2 avoids wrap false-passes)
    const PRG_BANKS_8K: usize = 12;
    // CHR: 96 × 1KB (non-power-of-2)
    const CHR_BANKS_1K: usize = 96;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        let chr = banked_data(1024, CHR_BANKS_1K);
        create_mapper(MapperContext::new_for_test(
            292,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 292 must be registered")
    }

    // ── Factory ──────────────────────────────────────────────────────────────

    #[test]
    fn mapper_292_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            292,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 292 must be registered in the factory"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    /// At power-on prot_reg=0 so bits 4:0 = 0 → $8000 maps to PRG bank 0.
    #[test]
    fn power_on_8000_maps_to_prg_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on (prot_reg=0)"
        );
    }

    /// Write V to $6000: bits 4:0 of V select the 8KB PRG bank at $8000–$9FFF.
    #[test]
    fn prot_reg_write_sets_8000_prg_bank() {
        let mut mapper = make_mapper();
        // Set extra PRG bank = 3 via $6000 write (bits 4:0 = 3)
        mapper.write_prg(0x6000, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "prot_reg bits 4:0 = 3 must select PRG bank 3 at $8000"
        );
    }

    /// $A000–$BFFF uses MMC3 R7 (unaffected by prot_reg).
    #[test]
    fn mmc3_r7_selects_a000_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 5); // R7 = 5
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 must follow MMC3 R7 bank selection"
        );
    }

    /// $E000–$FFFF is always fixed to the last PRG bank.
    #[test]
    fn e000_fixed_to_last_prg_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS_8K - 1) as u8,
            "$E000 must be fixed to the last PRG bank"
        );
    }

    // ── CHR banking – sprite bank A ($0000–$07FF) ────────────────────────────

    /// Power-on: sprite_xor=0, R0=0 → 2KB bank 0 at $0000.
    #[test]
    fn power_on_chr_0000_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "At power-on PPU $0000 must read CHR bank 0 (sprite_xor=0, R0=0)"
        );
    }

    /// MMC3 R0 selects the base 2KB sprite bank A (even-aligned, no XOR).
    #[test]
    fn mmc3_r0_selects_sprite_bank_a_base() {
        let mut mapper = make_mapper();
        // Set R0 = 4 (even-aligned): base 2KB = 4/2 = 2 → 1KB bank 4 at $0000
        mapper.write_prg(0x8000, 0b0000_0000); // select R0
        mapper.write_prg(0x8001, 4); // R0 = 4
        assert_eq!(
            mapper.read_chr(0x0000),
            4, // CHR bank 4 = byte value 4 from banked_data
            "R0=4 must map $0000 to CHR 1KB bank 4"
        );
        assert_eq!(
            mapper.read_chr(0x0400),
            5, // 1KB bank 5 (second half of 2KB bank 2)
            "R0=4 must map $0400 to CHR 1KB bank 5"
        );
    }

    /// sprite_xor XORs the 2KB bank index from R0 for the $0000–$07FF region.
    #[test]
    fn sprite_xor_modifies_0000_chr_bank() {
        let mut mapper = make_mapper();
        // R0 = 8 → base 2KB bank = 8/2 = 4
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 8);

        // Set sprite mode (prot_reg bit5=0), cpu_latch = 6
        // sprite_xor = 6 → actual 2KB bank = 4 XOR 6 = 2 → 1KB bank 4
        mapper.write_prg(0x6000, 0x00); // prot_reg: mode=sprite (bit5=0)
        mapper.write_prg(0x6001, 0x06); // cpu_latch = 6 (any write in $6000-$FFFF)
        mapper.read_prg_open_bus(0x6000, 0xFF); // trigger protection read → sprite_xor=6

        // 2KB bank = 4 XOR 6 = 2; 1KB bank at $0000 = 2*2 = 4
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "With R0=8, sprite_xor=6: 2KB bank=(8/2)^6=2, 1KB bank=4 at $0000"
        );
    }

    // ── CHR banking – sprite bank B ($0800–$0FFF) ────────────────────────────

    /// Power-on: sprite_or=0, R1=0 → bank 0 at $0800.
    #[test]
    fn power_on_chr_0800_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0800),
            0,
            "At power-on PPU $0800 must read CHR bank 0"
        );
    }

    /// MMC3 R1 selects the base 2KB sprite bank B.
    #[test]
    fn mmc3_r1_selects_sprite_bank_b_base() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0001); // R1
        mapper.write_prg(0x8001, 10); // R1 = 10 (even-aligned → 10/2=5)
        assert_eq!(
            mapper.read_chr(0x0800),
            10, // 1KB bank 10
            "R1=10 must map $0800 to CHR 1KB bank 10"
        );
    }

    /// sprite_or (bit 7) XORs the 2KB bank index for $0800–$0FFF.
    #[test]
    fn sprite_or_bit7_flips_0800_chr_bank() {
        let mut mapper = make_mapper();
        // R1 = 4 → base 2KB = 4/2 = 2
        mapper.write_prg(0x8000, 0b0000_0001); // R1
        mapper.write_prg(0x8001, 4);

        // Set BG mode (bit5=1), cpu_latch with bit6 set → sprite_or = 0x80
        // sprite_or = (0x40 << 1) = 0x80 → actual 2KB = 2 XOR 0x80 = 130
        // 130 % CHR_BANKS_1K/2 = 130 % 48 = 34 → 1KB bank = 68
        mapper.write_prg(0x6000, 0x20); // prot_reg: mode=BG (bit5=1)
        // Use $C000 (IRQ latch) to set cpu_latch without changing CHR registers
        mapper.write_prg(0xC000, 0x40); // cpu_latch = 0x40 (bit6=1), prot_reg unchanged
        mapper.read_prg_open_bus(0x6000, 0xFF); // trigger → sprite_or=0x80, bg_chr=0

        let expected_2k = 2_usize ^ 0x80_usize; // = 130
        let chr_banks_2k = CHR_BANKS_1K / 2;
        let wrapped_2k = expected_2k % chr_banks_2k;
        let expected_1k = (wrapped_2k * 2) as u8;
        assert_eq!(
            mapper.read_chr(0x0800),
            expected_1k,
            "sprite_or=0x80 must XOR 2KB bank index for $0800"
        );
    }

    // ── CHR banking – BG bank ($1000–$1FFF) ──────────────────────────────────

    /// Power-on: bg_chr=0 → 4KB BG bank 0 at $1000.
    #[test]
    fn power_on_chr_1000_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "At power-on PPU $1000 must read CHR bank 0 (bg_chr=0)"
        );
    }

    /// bg_chr selects the 4KB BG bank for $1000–$1FFF.
    #[test]
    fn bg_chr_selects_1000_bank() {
        let mut mapper = make_mapper();
        // Set BG mode, cpu_latch = 5 → bg_chr = 5 & 0x3F = 5
        // 4KB bank 5 → 1KB banks 20..23; $1000 → 1KB bank 20
        mapper.write_prg(0x6000, 0x20); // BG mode
        mapper.write_prg(0x8001, 0x05); // cpu_latch = 5, prot_reg unchanged
        mapper.read_prg_open_bus(0x6000, 0xFF); // trigger → bg_chr=5

        assert_eq!(
            mapper.read_chr(0x1000),
            20, // 4KB bank 5 → 1KB bank 20 (5*4=20)
            "bg_chr=5 must map $1000 to 1KB CHR bank 20"
        );
        assert_eq!(
            mapper.read_chr(0x1400),
            21, // 1KB bank 21
            "bg_chr=5 must map $1400 to 1KB CHR bank 21"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            22, // 1KB bank 22
            "bg_chr=5 must map $1800 to 1KB CHR bank 22"
        );
        assert_eq!(
            mapper.read_chr(0x1C00),
            23, // 1KB bank 23
            "bg_chr=5 must map $1C00 to 1KB CHR bank 23"
        );
    }

    /// bg_chr bits 5:0 only; bit 6 does not affect bg_chr.
    #[test]
    fn bg_chr_masked_to_6_bits() {
        let mut mapper = make_mapper();
        // cpu_latch = 0x47 = 0b0100_0111; bg_chr = 0x47 & 0x3F = 0x07
        mapper.write_prg(0x6000, 0x20); // BG mode
        mapper.write_prg(0x8001, 0x47); // cpu_latch = 0x47, prot_reg unchanged
        mapper.read_prg_open_bus(0x6000, 0xFF);

        // 4KB bank 7 → 1KB bank 28 at $1000
        assert_eq!(
            mapper.read_chr(0x1000),
            28,
            "bg_chr = 0x47 & 0x3F = 7 → 1KB bank 28 at $1000"
        );
    }

    // ── Protection mechanism: sprite vs BG mode ───────────────────────────────

    /// When prot_reg bit5=0 (sprite mode), reading $6000 sets sprite_xor.
    #[test]
    fn protection_read_sprite_mode_sets_sprite_xor() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00); // prot_reg: sprite mode (bit5=0)
        mapper.write_prg(0x6001, 0x07); // cpu_latch = 7
        mapper.read_prg_open_bus(0x6000, 0); // trigger

        // Verify sprite_xor was applied: R0=0 → base_2k=0; xor 7 → 2KB bank 7; 1KB bank 14
        assert_eq!(
            mapper.read_chr(0x0000),
            14,
            "sprite mode: sprite_xor=7 with R0=0 → 2KB bank 7 → 1KB bank 14"
        );
    }

    /// When prot_reg bit5=1 (BG mode), reading $6000 sets bg_chr.
    #[test]
    fn protection_read_bg_mode_sets_bg_chr() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20); // prot_reg: BG mode (bit5=1)
        mapper.write_prg(0x8001, 0x03); // cpu_latch = 3, prot_reg unchanged
        mapper.read_prg_open_bus(0x6000, 0); // trigger

        // 4KB bank 3 → 1KB bank 12 at $1000
        assert_eq!(
            mapper.read_chr(0x1000),
            12,
            "BG mode: bg_chr=3 → 4KB bank 3 → 1KB bank 12 at $1000"
        );
    }

    /// Protection read returns open bus value.
    #[test]
    fn protection_read_returns_open_bus() {
        let mapper = make_mapper();
        let open_bus = 0xAB;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "Reading $6000 must return the open bus value"
        );
    }

    /// $6000–$7FFF writes do not produce readable PRG-RAM.
    #[test]
    fn protection_region_has_no_prg_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(
            mapper.wram_size(),
            0,
            "Mapper 292 must have no PRG-RAM (wram_size = 0)"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn initial_mirroring_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must match header"
        );
    }

    #[test]
    fn mmc3_mirroring_register_controls_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 1); // horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xA000, 0); // vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn mmc3_irq_fires_after_a12_rising_edges() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(
            mapper.irq_pending(),
            "MMC3 IRQ must fire after 2 A12 rising edges"
        );
    }

    // ── Save state ────────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();

        // Set up some state
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        mapper.write_prg(0x6000, 0x20); // BG mode
        mapper.write_prg(0x6001, 0x05); // cpu_latch = 5
        mapper.read_prg_open_bus(0x6000, 0); // trigger → bg_chr=5

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0xA000), // R7=0 (default)
            mapper.read_prg(0xA000),
            "PRG bank state must survive snapshot round-trip"
        );
        assert_eq!(
            mapper2.read_chr(0x1000),
            mapper.read_chr(0x1000),
            "CHR bg_chr state must survive snapshot round-trip"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_protection_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20); // BG mode
        mapper.write_prg(0x6001, 0x05);
        mapper.read_prg_open_bus(0x6000, 0); // set bg_chr=5

        mapper.reset();

        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "After reset bg_chr must be 0 → CHR bank 0 at $1000"
        );
    }
}
