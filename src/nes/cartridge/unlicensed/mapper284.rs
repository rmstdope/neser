//! Mapper 284 – UNL-DripGame (homebrew)
//!
//! ## Specifications
//!
//! - Primary source: Mesen2 `Core/NES/Mappers/Homebrew/UnlDripGame.h`
//! - NesDev page: <https://www.nesdev.org/wiki/NES_2.0_Mapper_284>
//!
//! ## Hardware overview
//!
//! Custom mapper for the homebrew game _Drip_ (UNIF board UNL-DripGame).
//!
//! - PRG-ROM: 2 × 16 KiB windows. `$C000-$FFFF` is fixed to the last bank;
//!   `$8000-$BFFF` is switchable via register `$800B`.
//! - CHR-ROM: 4 × 2 KiB banks via registers `$800C`–`$800F`.
//! - PRG-RAM: 8 KiB at `$6000-$7FFF`; write-enable controlled by `$800A` bit 3.
//! - IRQ: CPU-cycle countdown. `$8008` = low byte of latch; `$8009` bits 6:0 =
//!   high 7 bits of latch; bit 7 of `$8009` enables and loads the counter.
//!   Clears the pending flag on any write to `$8009`.
//! - Mirroring: software-controlled via `$800A` bits 1:0.
//! - Extended attributes: 2 × 1 KiB attribute RAM written by the CPU at
//!   `$C000-$FFFF` (mirrored). When enabled (bit 2 of `$800A`), nametable
//!   attribute fetches are replaced with per-tile palette data from this RAM.
//! - DIP switch: single switch; readable at `$4800` bit 7.
//! - Audio: **not implemented** (Known Limitation).
//!
//! ## Register map
//!
//! | Address | Mask   | Function                                             |
//! |---------|--------|------------------------------------------------------|
//! | `$4800` | –      | Read: bit 7 = DIP switch; bits 6:0 = `0x64`         |
//! | `$8008` | `$800F`| IRQ latch low byte                                   |
//! | `$8009` | `$800F`| IRQ latch high 7 bits + enable (bit 7), clears IRQ  |
//! | `$800A` | `$800F`| Mirroring (1:0), ext-attr enable (2), WRAM w/e (3)  |
//! | `$800B` | `$800F`| PRG bank select (bits 3:0) → `$8000-$BFFF`          |
//! | `$800C`–`$800F` | `$800F` | CHR bank 0–3 (bits 3:0)              |
//! | `$C000`–`$FFFF` | –   | Extended attribute RAM write (2 × 1 KiB)         |

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const PRG_BANK_SIZE: usize = 0x4000; // 16 KiB
const CHR_BANK_SIZE: usize = 0x0800; // 2 KiB
const EXT_ATTR_LEN: usize = 0x0400; // 1 KiB per nametable

/// Mapper 284 – UNL-DripGame homebrew mapper.
pub struct Mapper284 {
    base: BaseMapper,
    prg_bank: u8,
    chr_banks: [u8; 4],
    /// IRQ latch: low byte buffered on $8008 write, committed on $8009 write.
    irq_latch_lo: u8,
    irq: CpuCycleIrq,
    ext_attr_enabled: bool,
    wram_write_enabled: bool,
    /// Extended attribute RAM – 2 nametables × 1 KiB.
    ext_attr: Box<[[u8; EXT_ATTR_LEN]; 2]>,
    /// Tile offset saved on nametable tile fetch; used to look up palette on the
    /// following attribute fetch.
    last_tile_offset: u16,
    dip_switch: bool,
}

impl Mapper284 {
    const MAPPER_NUMBER: u16 = 284;

    fn capabilities_spec() -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 2,
            max_prg_ram_kb: 8,
            ..Default::default()
        }
    }

    pub fn new(ctx: MapperContext) -> Self {
        let dip_switch = ctx.submapper == 1;
        let mut base = BaseMapper::new(&ctx, Self::capabilities_spec());
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_banks: [0; 4],
            irq_latch_lo: 0,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownToZero),
            ext_attr_enabled: false,
            wram_write_enabled: false,
            ext_attr: Box::new([[0; EXT_ATTR_LEN]; 2]),
            last_tile_offset: 0,
            dip_switch,
        };
        mapper.apply_banking();
        mapper
    }

    fn apply_banking(&mut self) {
        let num_banks = (self.base.prg_rom().len() / PRG_BANK_SIZE) as i16;
        self.base.select_prg_page(0, (self.prg_bank & 0x0F) as i16);
        self.base.select_prg_page(1, num_banks - 1);
        for i in 0..4 {
            self.base
                .select_chr_page(i, (self.chr_banks[i] & 0x0F) as i16);
        }
    }

    fn update_mirroring(&mut self, raw: u8) {
        let layout = match raw & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreen,
            _ => NametableLayout::SingleScreenUpper,
        };
        self.base.set_mirroring(layout);
    }
}

impl Mapper for Mapper284 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn capabilities(&self) -> MapperCapabilities {
        Self::capabilities_spec()
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x4800 => (if self.dip_switch { 0x80 } else { 0x00 }) | 0x64,
            0x5000 | 0x5800 => 0, // audio stub
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                if self.wram_write_enabled {
                    self.base.try_write_prg_ram(addr, value);
                }
            }
            0x8000..=0xBFFF => match addr & 0x800F {
                0x8000..=0x8007 => {} // audio (stub)
                0x8008 => self.irq_latch_lo = value,
                0x8009 => {
                    let latch = (((value & 0x7F) as u16) << 8) | self.irq_latch_lo as u16;
                    self.irq.set_counter(latch);
                    self.irq.set_enabled((value & 0x80) != 0);
                    self.irq.acknowledge();
                }
                0x800A => {
                    self.update_mirroring(value);
                    self.ext_attr_enabled = (value & 0x04) != 0;
                    self.wram_write_enabled = (value & 0x08) != 0;
                }
                0x800B => {
                    self.prg_bank = value & 0x0F;
                    self.apply_banking();
                }
                n @ 0x800C..=0x800F => {
                    self.chr_banks[(n & 0x03) as usize] = value & 0x0F;
                    self.apply_banking();
                }
                _ => {}
            },
            0xC000..=0xFFFF => {
                // Extended attribute RAM: nametable 0 at $C000-$C3FF,
                // nametable 1 at $C400-$C7FF, mirrored every 0x800.
                let offset = (addr - 0xC000) as usize;
                let nt = (offset >> 10) & 1;
                let idx = offset & (EXT_ATTR_LEN - 1);
                self.ext_attr[nt][idx] = value;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        if !self.ext_attr_enabled {
            return None;
        }
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }
        let offset = addr & 0x03FF;
        if offset < 0x03C0 {
            // Tile fetch – save offset for the next attribute lookup.
            self.last_tile_offset = offset;
            None
        } else {
            // Attribute fetch – return palette from ext_attr RAM.
            let nt = if (addr & 0x0400) != 0 { 1usize } else { 0 };
            let pal = self.ext_attr[nt][self.last_tile_offset as usize & (EXT_ATTR_LEN - 1)] & 0x03;
            Some((pal << 6) | (pal << 4) | (pal << 2) | pal)
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags: u8 = (self.irq.enabled() as u8)
            | ((self.irq.is_pending() as u8) << 1)
            | ((self.ext_attr_enabled as u8) << 2)
            | ((self.wram_write_enabled as u8) << 3);
        let mut v = vec![
            self.prg_bank,
            self.chr_banks[0],
            self.chr_banks[1],
            self.chr_banks[2],
            self.chr_banks[3],
            self.irq_latch_lo,
            (self.irq.counter() >> 8) as u8,
            self.irq.counter() as u8,
            flags,
            (self.last_tile_offset >> 8) as u8,
            self.last_tile_offset as u8,
        ];
        for nt in 0..2 {
            v.extend_from_slice(&self.ext_attr[nt]);
        }
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const HEADER: usize = 11;
        if data.len() < HEADER {
            return;
        }
        self.prg_bank = data[0];
        self.chr_banks.copy_from_slice(&data[1..5]);
        self.irq_latch_lo = data[5];
        let irq_counter = ((data[6] as u16) << 8) | data[7] as u16;
        let flags = data[8];
        self.irq.set_counter(irq_counter);
        self.irq.set_enabled((flags & 0x01) != 0);
        self.irq.set_pending((flags & 0x02) != 0);
        self.ext_attr_enabled = (flags & 0x04) != 0;
        self.wram_write_enabled = (flags & 0x08) != 0;
        self.last_tile_offset = ((data[9] as u16) << 8) | data[10] as u16;
        if data.len() >= HEADER + EXT_ATTR_LEN * 2 {
            self.ext_attr[0].copy_from_slice(&data[HEADER..HEADER + EXT_ATTR_LEN]);
            self.ext_attr[1]
                .copy_from_slice(&data[HEADER + EXT_ATTR_LEN..HEADER + EXT_ATTR_LEN * 2]);
        }
        self.apply_banking();
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_banks = [0; 4];
        self.irq_latch_lo = 0;
        self.irq.set_counter(0);
        self.irq.set_enabled(false);
        self.irq.acknowledge();
        self.ext_attr_enabled = false;
        self.wram_write_enabled = false;
        self.last_tile_offset = 0;
        for nt in self.ext_attr.iter_mut() {
            nt.fill(0);
        }
        self.base.set_mirroring(NametableLayout::Vertical);
        self.apply_banking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_16K_BANKS: usize = 9; // non-power-of-two
    const CHR_2K_BANKS: usize = 11; // non-power-of-two

    fn make_mapper() -> Mapper284 {
        let prg = banked_data(PRG_BANK_SIZE, PRG_16K_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_2K_BANKS);
        Mapper284::new(MapperContext::new_for_test(
            Mapper284::MAPPER_NUMBER,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_284_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            Mapper284::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_16K_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_2K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 284 must be registered in the factory"
        );
    }

    // ── PRG banking ─────────────────────────────────────────────────

    #[test]
    fn c000_window_fixed_to_last_bank_at_power_on() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_16K_BANKS - 1) as u8,
            "$C000 window must be fixed to last PRG bank at power-on"
        );
    }

    #[test]
    fn prg_800b_selects_8000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$800B must select 16 KiB PRG bank at $8000"
        );
    }

    #[test]
    fn prg_800b_does_not_change_c000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_16K_BANKS - 1) as u8,
            "$C000 must remain fixed after $800B write"
        );
    }

    #[test]
    fn prg_bank_8_of_9_is_accessible() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 8);
        assert_eq!(mapper.read_prg(0x8000), 8, "bank 8 of 9 must be accessible");
    }

    // ── CHR banking ─────────────────────────────────────────────────

    #[test]
    fn chr_800c_selects_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800C, 3);
        assert_eq!(mapper.read_chr(0x0000), 3, "$800C must select CHR bank 0");
    }

    #[test]
    fn chr_800d_selects_bank_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800D, 5);
        assert_eq!(
            mapper.read_chr(0x0800),
            5,
            "$800D must select CHR bank 1 (offset 2 KiB)"
        );
    }

    #[test]
    fn chr_800e_selects_bank_2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800E, 7);
        assert_eq!(
            mapper.read_chr(0x1000),
            7,
            "$800E must select CHR bank 2 (offset 4 KiB)"
        );
    }

    #[test]
    fn chr_800f_selects_bank_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 9);
        assert_eq!(
            mapper.read_chr(0x1800),
            9 % CHR_2K_BANKS as u8,
            "$800F must select CHR bank 3 (offset 6 KiB)"
        );
    }

    // ── Mirroring ───────────────────────────────────────────────────

    #[test]
    fn mirroring_800a_00_is_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800A, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_800a_01_is_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800A, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_800a_02_is_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800A, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreen);
    }

    #[test]
    fn mirroring_800a_03_is_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800A, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    // ── IRQ ─────────────────────────────────────────────────────────

    #[test]
    fn irq_fires_after_counter_reaches_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 3);
        mapper.write_prg(0x8009, 0x80); // enable, counter = 3
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before counter expires"
        );
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ must not fire after 2 cycles");
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must fire after 3 cycles");
    }

    #[test]
    fn irq_latch_lo_is_buffered_until_8009_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0x05);
        assert_eq!(
            mapper.irq.counter(),
            0,
            "Counter must stay 0 until $8009 written"
        );
        mapper.write_prg(0x8009, 0x80); // enable, counter = 5
        assert_eq!(
            mapper.irq.counter(),
            5,
            "Counter must be 5 after $8009 write"
        );
    }

    #[test]
    fn irq_clears_pending_on_8009_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 1);
        mapper.write_prg(0x8009, 0x80);
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must be pending after counter expires"
        );
        mapper.write_prg(0x8008, 5);
        mapper.write_prg(0x8009, 0x80);
        assert!(
            !mapper.irq_pending(),
            "Writing $8009 must clear pending IRQ"
        );
    }

    // ── DIP switch ──────────────────────────────────────────────────

    #[test]
    fn dip_switch_off_reads_0x64() {
        let mapper = make_mapper(); // submapper 0 → dip = false
        assert_eq!(
            mapper.read_prg_open_bus(0x4800, 0),
            0x64,
            "DIP switch off: bit 7 clear, bits 6:0 = 0x64"
        );
    }

    #[test]
    fn dip_switch_on_reads_0xe4() {
        let ctx = MapperContext::new_for_test(
            Mapper284::MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_16K_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_2K_BANKS),
            NametableLayout::Vertical,
        )
        .with_submapper(1);
        let mapper = Mapper284::new(ctx);
        assert_eq!(
            mapper.read_prg_open_bus(0x4800, 0),
            0xE4,
            "DIP switch on: bit 7 set → 0xE4"
        );
    }

    // ── Extended attributes ─────────────────────────────────────────

    #[test]
    fn ext_attr_disabled_read_nametable_returns_none() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_nametable(0x2000),
            None,
            "read_nametable must return None when ext attr disabled"
        );
    }

    #[test]
    fn ext_attr_write_and_attribute_fetch_returns_palette() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800A, 0x04); // enable ext_attr
        // CPU writes palette 2 to ext_attr[0][0].
        mapper.write_prg(0xC000, 0x02);
        // PPU tile fetch at nametable 0, tile 0.
        let _ = mapper.read_nametable(0x2000);
        // PPU attribute fetch at nametable 0, attribute area.
        let result = mapper.read_nametable(0x23C0);
        assert!(
            result.is_some(),
            "Attribute fetch must return Some when ext attr enabled"
        );
        // Palette 2 = 0b10; all 4 slots packed → 0b10_10_10_10 = 0xAA.
        assert_eq!(
            result.unwrap(),
            0xAA,
            "Packed palette must fill all nibbles"
        );
    }

    // ── Reset ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 5);
        mapper.write_prg(0x800A, 0x01);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_16K_BANKS - 1) as u8,
            "After reset $C000 must be fixed to last bank"
        );
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert!(!mapper.irq_pending());
    }
}
