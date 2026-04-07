//! Mapper 165 - MMC3-based variant with MMC2-style CHR latches
//!
//! Specifications:
//! - Mesen reference: `MMC3_165` (Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_165.h`)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 165 is an MMC3 variant that replaces MMC3's standard 1KB/2KB CHR banking
//! with a two-page 4KB CHR scheme using MMC2-style pattern-table address latches.
//!
//! ## PRG Banking
//!
//! Full MMC3 PRG banking — $8000/$8001 writes with bank select index 6/7, plus
//! mirroring ($A000), PRG-RAM control ($A001), IRQ counter ($C000/$C001/$E000/$E001).
//!
//! ## CHR Banking
//!
//! CHR address space is split into two 4KB windows:
//! - Window 0: PPU $0000–$0FFF
//! - Window 1: PPU $1000–$1FFF
//!
//! Each window is controlled by an MMC2-style latch that switches between two
//! candidate MMC3 registers:
//!
//! | Window | Latch=false | Latch=true |
//! |--------|-------------|-----------|
//! | 0      | MMC3 R0     | MMC3 R1   |
//! | 1      | MMC3 R2     | MMC3 R4   |
//!
//! The latch for window W is toggled by PPU address patterns matching
//! `addr & 0x2FF8 == 0x0FD8` (set false) or `addr & 0x2FF8 == 0x0FE8` (set true),
//! where bit 12 of addr selects which window's latch is updated.
//!
//! ## CHR Memory Routing
//!
//! For the selected register value `reg`:
//! - `reg == 0`   → 4KB CHR-RAM (offset 0)
//! - `reg != 0`   → 4KB CHR-ROM at 4KB page `reg >> 2`
//!
//! ## IRQ
//!
//! Standard MMC3 scanline IRQ via A12 rising-edge detection.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 0x1000; // 4 KiB

pub struct Mapper165 {
    inner: MMC3Mapper,
    /// MMC2-style latches: latch[0] for PPU $0000-$0FFF, latch[1] for PPU $1000-$1FFF
    chr_latch: [bool; 2],
    /// 4 KiB CHR-RAM (used when the selected MMC3 register value is 0)
    chr_ram: Vec<u8>,
    /// Latch update is deferred one PPU address cycle (mirrors MMC2/Mesen behavior)
    pending_latch_update: Option<(usize, bool)>,
}

impl Mapper165 {
    const MAPPER_NUMBER: u8 = 165;
    const CHR_4K_PAGE_SIZE: usize = 0x1000;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_latch: [false; 2],
            chr_ram: vec![0u8; CHR_RAM_SIZE],
            pending_latch_update: None,
        }
    }

    /// Returns the MMC3 register index for the given 4KB window and current latch state.
    fn reg_index_for_window(&self, window: usize) -> usize {
        match window {
            0 => if self.chr_latch[0] { 1 } else { 0 },
            1 => if self.chr_latch[1] { 4 } else { 2 },
            _ => 0,
        }
    }

    /// Reads a byte from CHR for the given PPU address using latch state.
    fn chr_read_byte(&self, addr: u16) -> u8 {
        let window = (addr >> 12) as usize & 1;
        let offset = (addr as usize) & (Self::CHR_4K_PAGE_SIZE - 1);
        let reg_index = self.reg_index_for_window(window);
        let reg_val = self.inner.chr_bank_reg(reg_index) as usize;
        if reg_val == 0 {
            // Route to CHR-RAM
            self.chr_ram.get(offset).copied().unwrap_or(0)
        } else {
            // Route to CHR-ROM at 4KB page (reg >> 2)
            let page_4k = reg_val >> 2;
            let count_1k = self.inner.chr_bank_count_1k();
            if count_1k == 0 {
                return 0;
            }
            let count_4k = count_1k / 4;
            if count_4k == 0 {
                return 0;
            }
            let wrapped_page = page_4k % count_4k;
            let rom_offset = wrapped_page * Self::CHR_4K_PAGE_SIZE + offset;
            self.inner
                .read_chr_1k_at(rom_offset / 0x400, rom_offset & 0x3FF)
        }
    }

    /// Writes a byte to CHR-RAM (only when the current window maps to CHR-RAM).
    fn chr_write_byte(&mut self, addr: u16, value: u8) {
        let window = (addr >> 12) as usize & 1;
        let offset = (addr as usize) & (Self::CHR_4K_PAGE_SIZE - 1);
        let reg_index = self.reg_index_for_window(window);
        let reg_val = self.inner.chr_bank_reg(reg_index) as usize;
        if reg_val == 0
            && let Some(slot) = self.chr_ram.get_mut(offset)
        {
            *slot = value;
        }
    }
}

impl Mapper for Mapper165 {
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
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.chr_read_byte(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_write_byte(addr, value);
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        // Apply any pending latch update from the previous address cycle
        if let Some((latch_index, latch_value)) = self.pending_latch_update.take() {
            self.chr_latch[latch_index] = latch_value;
        }

        // Delegate to inner MMC3 for A12-based IRQ tracking
        self.inner.ppu_address_changed(addr);

        // MMC2-style latch: detect $xFD8-$xFDF and $xFE8-$xFEF patterns
        // addr & 0x2FF8 == 0x0FD8  →  latch[(addr>>12)&1] = false
        // addr & 0x2FF8 == 0x0FE8  →  latch[(addr>>12)&1] = true
        if let pattern @ (0x0FD8 | 0x0FE8) = addr & 0x2FF8 {
            let latch_index = ((addr >> 12) & 0x01) as usize;
            self.pending_latch_update = Some((latch_index, pattern == 0x0FE8));
        }
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
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

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.chr_latch = [false; 2];
        self.pending_latch_update = None;
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        let latch_byte = (self.chr_latch[0] as u8) | ((self.chr_latch[1] as u8) << 1);
        snap.push(latch_byte);
        snap.extend_from_slice(&self.chr_ram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // MMC3 snapshot is 16 bytes; mapper 165 appends 1 + CHR_RAM_SIZE bytes
        let mmc3_len = 16;
        let total_extra = 1 + CHR_RAM_SIZE;
        if data.len() >= mmc3_len + total_extra {
            let (mmc3_data, tail) = data.split_at(mmc3_len);
            self.inner.restore_registers(mmc3_data);
            let latch_byte = tail[0];
            self.chr_latch[0] = (latch_byte & 0x01) != 0;
            self.chr_latch[1] = (latch_byte & 0x02) != 0;
            let ram_data = &tail[1..1 + CHR_RAM_SIZE];
            self.chr_ram[..CHR_RAM_SIZE].copy_from_slice(ram_data);
        } else {
            // Legacy/truncated snapshot: restore only MMC3 state
            self.inner.restore_registers(data);
            self.chr_latch = [false; 2];
            self.chr_ram = vec![0u8; CHR_RAM_SIZE];
        }
        self.pending_latch_update = None;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
            ..Default::default()
        }
    }

    fn get_mirroring(&self) -> crate::cartridge::NametableLayout {
        self.inner.get_mirroring()
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // PRG: 8 × 8KB = 64KB
    const PRG_BANKS_8K: usize = 8;
    // CHR-ROM: 64 × 4KB = 256KB  (expressed as 256 × 1KB banks for the MMC3 layer)
    const CHR_1K_BANKS: usize = 256;

    /// Build marker-encoded CHR-ROM: each 4KB page N has first byte = N.
    fn chr_rom_4k_marked(num_4k_pages: usize) -> Vec<u8> {
        let mut data = vec![0u8; num_4k_pages * 4096];
        for page in 0..num_4k_pages {
            data[page * 4096] = page as u8;
        }
        data
    }

    fn make_mapper() -> Box<dyn Mapper> {
        make_mapper_with_chr(chr_rom_4k_marked(CHR_1K_BANKS / 4))
    }

    fn make_mapper_with_chr(chr: Vec<u8>) -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        create_mapper(MapperContext::new_for_test(
            165,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 165 should be implemented")
    }

    // ── Factory ───────────────────────────────────────────────────────────────

    #[test]
    fn mapper_165_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            165,
            banked_data(8 * 1024, PRG_BANKS_8K),
            chr_rom_4k_marked(CHR_1K_BANKS / 4),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 165 must be registered in the factory"
        );
    }

    // ── PRG banking (full MMC3 delegation) ────────────────────────────────────

    #[test]
    fn mmc3_prg_banking_works() {
        let mut mapper = make_mapper();
        // Select R6=3 → $8000 reads bank 3
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "R6=3 must map $8000 to PRG bank 3"
        );
    }

    #[test]
    fn fixed_last_prg_bank_is_always_present() {
        let mapper = make_mapper();
        // Fixed-last bank ($E000) = bank 7 (last of 8)
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "Fixed-last PRG bank must be bank 7"
        );
    }

    // ── CHR-RAM routing (reg == 0) ────────────────────────────────────────────

    #[test]
    fn window0_maps_to_chr_ram_when_reg0_is_zero() {
        let mut mapper = make_mapper();
        // Default: R0=0 and latch[0]=false → window 0 uses R0=0 → CHR-RAM
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "Window 0 must read/write CHR-RAM when R0=0"
        );
    }

    #[test]
    fn window1_maps_to_chr_ram_when_reg2_is_zero() {
        let mut mapper = make_mapper();
        // Default: R2=0 and latch[1]=false → window 1 uses R2=0 → CHR-RAM
        mapper.write_chr(0x1000, 0xCD);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xCD,
            "Window 1 must read/write CHR-RAM when R2=0"
        );
    }

    #[test]
    fn chr_ram_is_4kb_and_window0_and_window1_share_the_same_4kb() {
        let mut mapper = make_mapper();
        // Both windows use CHR-RAM (reg=0). They share the same 4KB buffer.
        // Write to offset 0 of window 0, read back from same 4KB via window 1 offset 0.
        mapper.write_chr(0x0000, 0x55);
        // Window 1, offset 0 maps to CHR-RAM offset 0 (same buffer)
        assert_eq!(
            mapper.read_chr(0x1000),
            0x55,
            "Both windows share the same 4KB CHR-RAM"
        );
    }

    // ── CHR-ROM routing (reg != 0) ────────────────────────────────────────────

    #[test]
    fn window0_maps_to_chr_rom_when_r0_nonzero() {
        let mut mapper = make_mapper();
        // Set R0=4 → 4KB CHR-ROM page = 4>>2 = 1 (marker byte = 1)
        mapper.write_prg(0x8000, 0x00); // select R0
        mapper.write_prg(0x8001, 4);
        // latch[0]=false → use R0=4
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "R0=4 → 4KB page 1 → CHR-ROM marker byte must be 1"
        );
    }

    #[test]
    fn window1_maps_to_chr_rom_when_r2_nonzero() {
        let mut mapper = make_mapper();
        // Set R2=8 → 4KB CHR-ROM page = 8>>2 = 2 (marker byte = 2)
        mapper.write_prg(0x8000, 0x02); // select R2
        mapper.write_prg(0x8001, 8);
        // latch[1]=false → use R2=8
        assert_eq!(
            mapper.read_chr(0x1000),
            2,
            "R2=8 → 4KB page 2 → CHR-ROM marker byte must be 2"
        );
    }

    // ── MMC2-style latch behavior ─────────────────────────────────────────────

    /// Simulate a PPU pattern-table address change to trigger latch.
    /// In the mapper, the latch update from address N takes effect on the NEXT
    /// ppu_address_changed call (deferred update, matching Mesen behavior).
    fn trigger_latch(mapper: &mut Box<dyn Mapper>, addr: u16) {
        mapper.ppu_address_changed(addr);
        // A second call flushes the pending update
        mapper.ppu_address_changed(0x0000);
    }

    #[test]
    fn latch0_becomes_false_on_fd8_pattern() {
        let mut mapper = make_mapper();
        // First set latch[0] = true via $0FE8 pattern
        trigger_latch(&mut mapper, 0x0FE8);
        // Set R0=4 (ROM page 1) and R1=0 (CHR-RAM)
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 4); // R0=4
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8001, 0); // R1=0 (CHR-RAM)
        // After FE8: latch[0]=true → uses R1=0 → CHR-RAM
        mapper.write_chr(0x0000, 0x77);

        // Now trigger $0FD8 → latch[0] = false → uses R0=4 → CHR-ROM page 1
        trigger_latch(&mut mapper, 0x0FD8);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "After FD8 latch: window0 must use R0=4 → CHR-ROM page 1 (marker=1)"
        );
    }

    #[test]
    fn latch0_becomes_true_on_fe8_pattern() {
        let mut mapper = make_mapper();
        // Default latch[0]=false → uses R0
        // Set R0=4 (ROM page 1) and R1=8 (ROM page 2)
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 4); // R0=4 → page 1
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8001, 8); // R1=8 → page 2

        // Without latch update: window0 uses R0=4 → page 1 (marker=1)
        assert_eq!(mapper.read_chr(0x0000), 1, "Before FE8: uses R0=4 → page 1");

        // Trigger $0FE8 → latch[0] = true → window0 switches to R1=8 → page 2
        trigger_latch(&mut mapper, 0x0FE8);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "After FE8 latch: window0 must use R1=8 → CHR-ROM page 2 (marker=2)"
        );
    }

    #[test]
    fn latch1_becomes_true_on_1fe8_pattern() {
        let mut mapper = make_mapper();
        // Set R2=4 (ROM page 1) and R4=8 (ROM page 2)
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 4); // R2=4 → page 1
        mapper.write_prg(0x8000, 0x04);
        mapper.write_prg(0x8001, 8); // R4=8 → page 2

        // Default: latch[1]=false → uses R2=4 → page 1
        assert_eq!(
            mapper.read_chr(0x1000),
            1,
            "Before 1FE8: uses R2=4 → page 1"
        );

        // Trigger $1FE8 → latch[1] = true → window1 switches to R4=8 → page 2
        trigger_latch(&mut mapper, 0x1FE8);
        assert_eq!(
            mapper.read_chr(0x1000),
            2,
            "After 1FE8 latch: window1 must use R4=8 → CHR-ROM page 2 (marker=2)"
        );
    }

    #[test]
    fn latch1_becomes_false_on_1fd8_pattern() {
        let mut mapper = make_mapper();
        // First set latch[1] = true
        trigger_latch(&mut mapper, 0x1FE8);
        // Set R2=4 (page 1) and R4=8 (page 2)
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 4); // R2=4 → page 1
        mapper.write_prg(0x8000, 0x04);
        mapper.write_prg(0x8001, 8); // R4=8 → page 2

        // With latch[1]=true → uses R4=8 → page 2
        assert_eq!(mapper.read_chr(0x1000), 2, "After 1FE8: uses R4=8 → page 2");

        // Trigger $1FD8 → latch[1] = false → window1 switches back to R2=4 → page 1
        trigger_latch(&mut mapper, 0x1FD8);
        assert_eq!(
            mapper.read_chr(0x1000),
            1,
            "After 1FD8 latch: window1 must use R2=4 → CHR-ROM page 1 (marker=1)"
        );
    }

    #[test]
    fn latches_are_independent_per_window() {
        let mut mapper = make_mapper();
        // Set all four candidate registers to distinct non-zero ROM pages
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 4); // R0=4 → page 1
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8001, 8); // R1=8 → page 2
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 12); // R2=12 → page 3
        mapper.write_prg(0x8000, 0x04);
        mapper.write_prg(0x8001, 16); // R4=16 → page 4

        // Toggle only latch[0] via $0FE8
        trigger_latch(&mut mapper, 0x0FE8);

        // Window 0: latch[0]=true → R1=8 → page 2 (marker=2)
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Window0 with latch0=true must use R1=8 → page 2"
        );
        // Window 1: latch[1]=false → R2=12 → page 3 (marker=3)
        assert_eq!(
            mapper.read_chr(0x1000),
            3,
            "Window1 latch must remain false, using R2=12 → page 3"
        );
    }

    // ── CHR-ROM wrapping ──────────────────────────────────────────────────────

    #[test]
    fn chr_rom_page_wraps_modulo_rom_size() {
        // Create a mapper with 2 × 4KB CHR-ROM pages (very small)
        let chr = chr_rom_4k_marked(2);
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        let mut mapper = create_mapper(MapperContext::new_for_test(
            165,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .unwrap();

        // Set R2 = 12 → 4KB page = 12>>2 = 3; with 2 pages available → page 3%2 = 1 (marker=1)
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 12); // page=3, wraps to 1
        assert_eq!(
            mapper.read_chr(0x1000),
            1,
            "CHR-ROM 4KB page must wrap modulo total page count"
        );
    }

    // ── Mirroring delegation ──────────────────────────────────────────────────

    #[test]
    fn mmc3_mirroring_delegation_works() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 0x01); // → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── IRQ delegation ────────────────────────────────────────────────────────

    #[test]
    fn mmc3_irq_works_through_delegation() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 165");
    }

    // ── Save state round-trip ─────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips_latch_and_chr_ram() {
        let mut mapper = make_mapper();

        // Set a non-default latch state
        trigger_latch(&mut mapper, 0x0FE8); // latch[0] = true
        trigger_latch(&mut mapper, 0x1FE8); // latch[1] = true

        // Write to CHR-RAM via window 1 while latch[1]=true → uses R4
        // First ensure R4=0 so window1 still maps to CHR-RAM
        mapper.write_prg(0x8000, 0x04);
        mapper.write_prg(0x8001, 0); // R4=0 → CHR-RAM
        mapper.write_chr(0x1042, 0xBE);

        // Also set R1=0 so window0 maps to CHR-RAM
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8001, 0); // R1=0 → CHR-RAM
        mapper.write_chr(0x0010, 0xEF);

        // Set some PRG bank state
        mapper.write_prg(0x8000, 0x06); // R6
        mapper.write_prg(0x8001, 5);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // PRG bank restored
        assert_eq!(
            mapper2.read_prg(0x8000),
            5,
            "PRG bank must survive round-trip"
        );

        // Latches must be restored: latch[0]=true → uses R1=0 → CHR-RAM
        assert_eq!(
            mapper2.read_chr(0x0010),
            0xEF,
            "CHR-RAM data at $0010 must survive round-trip with latch0=true"
        );
        // latch[1]=true → uses R4=0 → CHR-RAM
        assert_eq!(
            mapper2.read_chr(0x1042),
            0xBE,
            "CHR-RAM data at $1042 must survive round-trip with latch1=true"
        );
    }
}
