//! Mapper 307 – Kaiser KS7037
//!
//! Specifications:
//! - Primary: NesDev wiki (no dedicated page found; behavior from board name)
//! - Fallback: Mesen2 `Kaiser7037.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 307 – Kaiser KS7037
///
/// Hardware: Kaiser KS7037
///
/// Specifications:
/// - Primary: NesDev wiki; Fallback: Mesen2 `Kaiser7037.h`
/// - PRG-ROM: 128 KiB (32 × 4 KiB pages)
/// - PRG-RAM (WRAM): 8 KiB (2 × 4 KiB pages)
/// - CHR: 8 KiB RAM (single bank, unbanked)
/// - Mirroring: per-nametable programmable (via regs[2–5])
/// - IRQ: None
/// - Bus conflicts: None
///
/// Memory map:
/// ```text
///  CPU $6000-$6FFF  WRAM page 0 (4 KiB)
///  CPU $7000-$7FFF  PRG-ROM page 15 (fixed, 4 KiB)
///  CPU $8000-$9FFF  PRG-ROM 8 KiB switchable (regs[6] × 2, regs[6] × 2 + 1)
///  CPU $A000-$AFFF  PRG-ROM page −4 (4th from last, fixed)
///  CPU $B000-$BFFF  WRAM page 1 (4 KiB)
///  CPU $C000-$DFFF  PRG-ROM 8 KiB switchable (regs[7] × 2, regs[7] × 2 + 1)
///  CPU $E000-$FFFF  PRG-ROM 8 KiB fixed (last two pages)
/// ```
///
/// Register map (write):
/// - `$8000` (addr & 0xE001 == 0x8000): select register index (bits 2–0)
/// - `$8001` (addr & 0xE001 == 0x8001): write to selected register; update banking
///   - reg 2: bit 0 → nametable 0 VRAM page (0 or 1)
///   - reg 3: bit 0 → nametable 2 VRAM page (0 or 1)
///   - reg 4: bit 0 → nametable 1 VRAM page (0 or 1)
///   - reg 5: bit 0 → nametable 3 VRAM page (0 or 1)
///   - reg 6: PRG bank select for $8000–$9FFF (maps banks reg*2, reg*2+1)
///   - reg 7: PRG bank select for $C000–$DFFF (maps banks reg*2, reg*2+1)
/// - No registers at $A000–$BFFF (those addresses are PRG-ROM / WRAM)
///
/// Power-on state: all registers zero.
pub struct Mapper307 {
    base: BaseMapper,
    current_register: u8,
    regs: [u8; 8],
    /// 2 KiB CIRAM for per-nametable routing (2 × 1 KiB pages).
    ciram: [u8; CIRAM_SIZE],
    /// 8 KiB WRAM: page 0 at $6000–$6FFF, page 1 at $B000–$BFFF.
    wram: [u8; 0x2000],
}

const PRG_PAGE_SIZE: usize = 0x1000; // 4 KiB
const CIRAM_SIZE: usize = 0x800; // 2 KiB (2 × 1 KiB nametable pages)

impl Mapper307 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 4,
            max_prg_ram_kb: 8,
            ..Default::default()
        };
        let base = BaseMapper::new(&ctx, capabilities);
        Self {
            base,
            current_register: 0,
            regs: [0; 8],
            ciram: [0; CIRAM_SIZE],
            wram: [0; 0x2000],
        }
    }

    fn prg_bank_count(&self) -> usize {
        (self.base.prg_rom().len() / PRG_PAGE_SIZE).max(1)
    }

    fn resolve_prg_bank(&self, bank: i32) -> usize {
        let count = self.prg_bank_count() as i32;
        let b = ((bank % count) + count) % count;
        b as usize
    }

    fn read_prg_rom_page(&self, bank: usize, offset: usize) -> u8 {
        let prg = self.base.prg_rom();
        prg.get(bank * PRG_PAGE_SIZE + offset).copied().unwrap_or(0)
    }

    fn nt_page_for_quadrant(&self, quadrant: usize) -> usize {
        let bit = match quadrant {
            0 => self.regs[2] & 1,
            1 => self.regs[4] & 1,
            2 => self.regs[3] & 1,
            3 => self.regs[5] & 1,
            _ => 0,
        };
        bit as usize
    }
}

impl Mapper for Mapper307 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 4,
            max_prg_ram_kb: 8,
            ..Default::default()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        let count = self.prg_bank_count() as i32;
        match addr {
            0x6000..=0x6FFF => {
                let offset = (addr - 0x6000) as usize;
                self.wram.get(offset).copied().unwrap_or(0)
            }
            0x7000..=0x7FFF => {
                let bank = self.resolve_prg_bank(15);
                let offset = (addr - 0x7000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0x8000..=0x8FFF => {
                let bank = self.resolve_prg_bank((self.regs[6] as i32) * 2);
                let offset = (addr - 0x8000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0x9000..=0x9FFF => {
                let bank = self.resolve_prg_bank((self.regs[6] as i32) * 2 + 1);
                let offset = (addr - 0x9000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0xA000..=0xAFFF => {
                let bank = self.resolve_prg_bank(count - 4);
                let offset = (addr - 0xA000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0xB000..=0xBFFF => {
                let offset = (addr - 0xB000) as usize + 0x1000;
                self.wram.get(offset).copied().unwrap_or(0)
            }
            0xC000..=0xCFFF => {
                let bank = self.resolve_prg_bank((self.regs[7] as i32) * 2);
                let offset = (addr - 0xC000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0xD000..=0xDFFF => {
                let bank = self.resolve_prg_bank((self.regs[7] as i32) * 2 + 1);
                let offset = (addr - 0xD000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0xE000..=0xEFFF => {
                let bank = self.resolve_prg_bank(count - 2);
                let offset = (addr - 0xE000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            0xF000..=0xFFFF => {
                let bank = self.resolve_prg_bank(count - 1);
                let offset = (addr - 0xF000) as usize;
                self.read_prg_rom_page(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn wram_size(&self) -> usize {
        self.wram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.wram.to_vec()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(self.wram.len());
        self.wram[..len].copy_from_slice(&data[..len]);
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x6FFF => {
                let offset = (addr - 0x6000) as usize;
                if offset < 0x1000 {
                    self.wram[offset] = value;
                }
            }
            0xB000..=0xBFFF => {
                let offset = (addr - 0xB000) as usize + 0x1000;
                if offset < 0x2000 {
                    self.wram[offset] = value;
                }
            }
            0x8000..=0x9FFF | 0xC000..=0xFFFF => match addr & 0xE001 {
                0x8000 => self.current_register = value & 0x07,
                0x8001 => {
                    self.regs[self.current_register as usize] = value;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }
        let quadrant = ((addr - 0x2000) >> 10) as usize;
        let page = self.nt_page_for_quadrant(quadrant);
        let offset = (addr as usize & 0x3FF) + page * 0x400;
        Some(self.ciram[offset])
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }
        let quadrant = ((addr - 0x2000) >> 10) as usize;
        let page = self.nt_page_for_quadrant(quadrant);
        let offset = (addr as usize & 0x3FF) + page * 0x400;
        self.ciram[offset] = value;
        true
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // 8 bytes of regs + 1 byte of current_register + CIRAM_SIZE bytes of CIRAM
        let mut snap = Vec::with_capacity(9 + CIRAM_SIZE);
        snap.extend_from_slice(&self.regs);
        snap.push(self.current_register);
        snap.extend_from_slice(&self.ciram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // Expect 8 bytes regs, 1 byte current_register, and CIRAM_SIZE bytes CIRAM.
        if data.len() < 9 + CIRAM_SIZE {
            return;
        }
        self.regs.copy_from_slice(&data[0..8]);
        self.current_register = data[8];
        self.ciram.copy_from_slice(&data[9..9 + CIRAM_SIZE]);
    }

    fn reset(&mut self) {
        self.current_register = 0;
        self.regs = [0; 8];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::MapperContext;

    /// Create 128 KiB PRG-ROM with each 4 KiB bank filled with the bank number.
    fn make_prg_rom(num_banks: usize) -> Vec<u8> {
        let mut prg = vec![0u8; num_banks * PRG_PAGE_SIZE];
        for bank in 0..num_banks {
            let start = bank * PRG_PAGE_SIZE;
            for b in prg[start..start + PRG_PAGE_SIZE].iter_mut() {
                *b = bank as u8;
            }
        }
        prg
    }

    fn create_mapper(num_banks: usize) -> Mapper307 {
        // Use a non-power-of-two-friendly bank count to prevent modulo false-passes.
        let prg = make_prg_rom(num_banks);
        let chr = vec![0u8; 0x2000]; // 8 KiB CHR-RAM
        let ctx = MapperContext::new_for_test(307, prg, chr, NametableLayout::Horizontal);
        Mapper307::new(ctx)
    }

    // ── PRG banking tests ────────────────────────────────────────────────────

    #[test]
    fn test_prg_7000_fixed_to_bank_15() {
        let mapper = create_mapper(32);
        // $7000-$7FFF should always read bank 15 content (= 15u8 in our ROM).
        assert_eq!(mapper.read_prg(0x7000), 15);
        assert_eq!(mapper.read_prg(0x7FFF), 15);
    }

    #[test]
    fn test_prg_a000_fixed_to_4th_from_last() {
        // With 32 banks (indices 0-31), bank -4 = bank 28.
        let mapper = create_mapper(32);
        assert_eq!(mapper.read_prg(0xA000), 28);
    }

    #[test]
    fn test_prg_e000_fixed_to_second_last_bank() {
        // Bank -2 of 32 = bank 30.
        let mapper = create_mapper(32);
        assert_eq!(mapper.read_prg(0xE000), 30);
    }

    #[test]
    fn test_prg_f000_fixed_to_last_bank() {
        // Bank -1 of 32 = bank 31.
        let mapper = create_mapper(32);
        assert_eq!(mapper.read_prg(0xF000), 31);
    }

    #[test]
    fn test_prg_8000_bank_switching_via_reg6() {
        let mut mapper = create_mapper(32);
        // Select reg 6, set bank 2 → $8000-$8FFF = bank 4, $9000-$9FFF = bank 5.
        mapper.write_prg(0x8000, 6); // select register index 6
        mapper.write_prg(0x8001, 2); // reg[6] = 2
        assert_eq!(mapper.read_prg(0x8000), 4); // bank 2*2 = 4
        assert_eq!(mapper.read_prg(0x9000), 5); // bank 2*2+1 = 5
    }

    #[test]
    fn test_prg_c000_bank_switching_via_reg7() {
        let mut mapper = create_mapper(32);
        // Select reg 7, set bank 3 → $C000-$CFFF = bank 6, $D000-$DFFF = bank 7.
        mapper.write_prg(0x8000, 7); // select register index 7
        mapper.write_prg(0x8001, 3); // reg[7] = 3
        assert_eq!(mapper.read_prg(0xC000), 6); // bank 3*2 = 6
        assert_eq!(mapper.read_prg(0xD000), 7); // bank 3*2+1 = 7
    }

    #[test]
    fn test_prg_8000_default_is_bank_0_and_1() {
        let mapper = create_mapper(32);
        // regs[6] = 0 by default → banks 0 and 1.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0x9000), 1);
    }

    #[test]
    fn test_register_select_uses_low_3_bits() {
        let mut mapper = create_mapper(32);
        // Writing 0xFF to $8000 should select register index 7 (0xFF & 7).
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0x8001, 4); // reg[7] = 4
        assert_eq!(mapper.read_prg(0xC000), 8); // 4*2 = 8
    }

    #[test]
    fn test_writes_to_a000_bfff_do_not_change_registers() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x8000, 6); // select reg 6
        mapper.write_prg(0x8001, 2); // reg[6] = 2
        let before = mapper.read_prg(0x8000);
        // Writes to $A000-$BFFF should not affect registers.
        mapper.write_prg(0xA000, 7); // would be reg-select if it were a register
        mapper.write_prg(0xA001, 0); // would be reg-data if it were a register
        assert_eq!(
            mapper.read_prg(0x8000),
            before,
            "$A000-$BFFF must not affect registers"
        );
    }

    // ── WRAM tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_wram_page0_readable_at_6000() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn test_wram_page1_readable_at_b000() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0xB000, 0xCD);
        assert_eq!(mapper.read_prg(0xB000), 0xCD);
    }

    #[test]
    fn test_wram_page0_and_page1_are_independent() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0xB000, 0x22);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0xB000), 0x22);
    }

    // ── Nametable routing tests ───────────────────────────────────────────────

    #[test]
    fn test_nametable_default_all_map_to_page_0() {
        let mut mapper = create_mapper(32);
        // With regs all zero, all NTs map to CIRAM page 0.
        // Write distinct values at different offsets within each quadrant.
        mapper.write_nametable(0x2001, 0xAA); // NT0 offset 1 → page 0 offset 1
        mapper.write_nametable(0x2401, 0xBB); // NT1 offset 1 → page 0 offset 1 (overwrites 0xAA)
        // All quadrants share page 0, so reading NT0 offset 1 returns 0xBB (last write wins).
        assert_eq!(
            mapper.read_nametable(0x2001),
            Some(0xBB),
            "NT0 shares page 0"
        );
        assert_eq!(
            mapper.read_nametable(0x2401),
            Some(0xBB),
            "NT1 shares page 0"
        );
        assert_eq!(
            mapper.read_nametable(0x2801),
            Some(0xBB),
            "NT2 shares page 0"
        );
        assert_eq!(
            mapper.read_nametable(0x2C01),
            Some(0xBB),
            "NT3 shares page 0"
        );
    }

    #[test]
    fn test_nametable_reg4_routes_nt1_to_page1() {
        let mut mapper = create_mapper(32);
        // Set reg 4 bit 0 = 1 → NT1 ($2400-$27FF) routes to CIRAM page 1.
        mapper.write_prg(0x8000, 4); // select reg 4
        mapper.write_prg(0x8001, 1); // reg[4] = 1 → NT1 → page 1
        // Write different values to NT0 (page 0) and NT1 (page 1) at offset 0.
        mapper.write_nametable(0x2000, 0x11); // NT0 offset 0 → page 0
        mapper.write_nametable(0x2400, 0x22); // NT1 offset 0 → page 1
        assert_eq!(
            mapper.read_nametable(0x2000),
            Some(0x11),
            "NT0 should read page 0"
        );
        assert_eq!(
            mapper.read_nametable(0x2400),
            Some(0x22),
            "NT1 should read page 1"
        );
    }

    #[test]
    fn test_nametable_horizontal_mirroring_pattern() {
        let mut mapper = create_mapper(32);
        // Horizontal mirroring: NT0=page0, NT1=page0, NT2=page1, NT3=page1.
        // regs[2]=0(NT0→page0), regs[4]=0(NT1→page0), regs[3]=1(NT2→page1), regs[5]=1(NT3→page1).
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0x8001, 1); // reg[3]=1 → NT2→page1
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0x8001, 1); // reg[5]=1 → NT3→page1
        // Write distinct values to each page.
        mapper.write_nametable(0x2000, 0xAA); // NT0 offset 0 → page 0
        mapper.write_nametable(0x2800, 0xBB); // NT2 offset 0 → page 1
        // NT0 and NT1 both map to page 0.
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));
        assert_eq!(mapper.read_nametable(0x2400), Some(0xAA)); // same page 0
        // NT2 and NT3 both map to page 1.
        assert_eq!(mapper.read_nametable(0x2800), Some(0xBB));
        assert_eq!(mapper.read_nametable(0x2C00), Some(0xBB)); // same page 1
    }

    #[test]
    fn test_nametable_vertical_mirroring_pattern() {
        let mut mapper = create_mapper(32);
        // Vertical mirroring: NT0=page0, NT1=page1, NT2=page0, NT3=page1.
        // regs[4]=1(NT1→page1), regs[5]=1(NT3→page1); regs[2]=0, regs[3]=0.
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 1); // reg[4]=1 → NT1→page1
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0x8001, 1); // reg[5]=1 → NT3→page1
        mapper.write_nametable(0x2000, 0xAA); // NT0 offset 0 → page 0
        mapper.write_nametable(0x2400, 0xBB); // NT1 offset 0 → page 1
        // NT0 and NT2 share page 0.
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAA));
        assert_eq!(mapper.read_nametable(0x2800), Some(0xAA));
        // NT1 and NT3 share page 1.
        assert_eq!(mapper.read_nametable(0x2400), Some(0xBB));
        assert_eq!(mapper.read_nametable(0x2C00), Some(0xBB));
    }

    // ── Snapshot/restore tests ───────────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_round_trips() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x8000, 6); // select reg 6
        mapper.write_prg(0x8001, 5); // reg[6] = 5
        mapper.write_prg(0x8000, 7); // select reg 7
        mapper.write_prg(0x8001, 3); // reg[7] = 3
        // Write a value to a nametable to exercise CIRAM round-trip.
        mapper.write_nametable(0x2001, 0x42);
        let snap = mapper.registers_snapshot();
        let mut mapper2 = create_mapper(32);
        mapper2.restore_registers(&snap);
        // After restore, banking should match.
        assert_eq!(mapper2.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(mapper2.read_prg(0xC000), mapper.read_prg(0xC000));
        // CIRAM should also be restored.
        assert_eq!(mapper2.read_nametable(0x2001), Some(0x42));
    }

    #[test]
    fn test_wram_snapshot_round_trips() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x6000, 0xAB); // WRAM page 0 at $6000-$6FFF
        mapper.write_prg(0xB000, 0xCD); // WRAM page 1 at $B000-$BFFF
        let snap = mapper.wram_snapshot();
        let mut mapper2 = create_mapper(32);
        mapper2.load_wram_snapshot(&snap);
        assert_eq!(mapper2.read_prg(0x6000), 0xAB);
        assert_eq!(mapper2.read_prg(0xB000), 0xCD);
    }

    // ── Reset tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_registers() {
        let mut mapper = create_mapper(32);
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 5);
        mapper.reset();
        // After reset, reg[6]=0 → $8000-$9FFF = banks 0,1.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0x9000), 1);
    }
}
