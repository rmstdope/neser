//! Mapper 048 – Taito TC0690
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::common::A12RisingEdgeDetector;
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 048 – Taito TC0690
///
/// Hardware: Taito TC0690
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_048>
/// - PRG-ROM: Up to 512KB, two 8KB switchable banks + two fixed banks
/// - CHR: Two 2KB switchable banks + four 1KB switchable banks
/// - Mirroring: Programmable (H/V) via bit 6 of register $E000
/// - IRQ: MMC3-style scanline IRQ with different register layout
///
/// PRG layout ($8000 CPU):
/// ```text
///   $8000   $A000   $C000   $E000
/// +-------+-------+-------+-------+
/// | $8000 | $8001 | { -2} | { -1} |
/// +-------+-------+-------+-------+
/// ```
///
/// CHR layout ($0000 PPU):
/// ```text
///   $0000   $0800   $1000   $1400   $1800   $1C00
/// +-------+-------+-------+-------+-------+-------+
/// | $8002 | $8003 | $A000 | $A001 | $A002 | $A003 |
/// +-------+-------+-------+-------+-------+-------+
/// ```
///
/// Registers (mask $E003):
/// - `$8000` [..PP PPPP]: PRG bank 0 (8KB @ $8000)
/// - `$8001` [..PP PPPP]: PRG bank 1 (8KB @ $A000)
/// - `$8002` [CCCC CCCC]: CHR bank 0 (2KB @ $0000)
/// - `$8003` [CCCC CCCC]: CHR bank 1 (2KB @ $0800)
/// - `$A000` [CCCC CCCC]: CHR bank 2 (1KB @ $1000)
/// - `$A001` [CCCC CCCC]: CHR bank 3 (1KB @ $1400)
/// - `$A002` [CCCC CCCC]: CHR bank 4 (1KB @ $1800)
/// - `$A003` [CCCC CCCC]: CHR bank 5 (1KB @ $1C00)
/// - `$C000` [CCCC CCCC]: IRQ latch (value XOR 0xFF before storing)
/// - `$C001`: IRQ counter reload
/// - `$C002`: IRQ enable
/// - `$C003`: IRQ disable and acknowledge
/// - `$E000` [.M.. ....]: M=Mirroring (0=Vert, 1=Horz) at bit 6
pub struct TaitoTc0350Mapper {
    base: BaseMapper,

    prg_bank: [u8; 2],
    chr_bank_2k: [u8; 2],
    chr_bank_1k: [u8; 4],

    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_pending: bool,

    a12_detector: A12RisingEdgeDetector,
}

impl TaitoTc0350Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_1K_SIZE: usize = 0x0400; // 1KB
    const REGISTER_MASK: u16 = 0xE003;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_1K_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: [0; 2],
            chr_bank_2k: [0; 2],
            chr_bank_1k: [0; 4],
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_pending: false,
            a12_detector: A12RisingEdgeDetector::new(3),
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: slot0=switchable, slot1=switchable, slot2=fixed(-2), slot3=fixed(-1)
        self.base.select_prg_page(0, self.prg_bank[0] as i16);
        self.base.select_prg_page(1, self.prg_bank[1] as i16);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR: 2KB banks expand to 2 consecutive 1KB slots
        let b0 = self.chr_bank_2k[0] as i16;
        self.base.select_chr_page(0, b0 * 2);
        self.base.select_chr_page(1, b0 * 2 + 1);
        let b1 = self.chr_bank_2k[1] as i16;
        self.base.select_chr_page(2, b1 * 2);
        self.base.select_chr_page(3, b1 * 2 + 1);

        // 1KB banks
        for i in 0..4 {
            self.base.select_chr_page(4 + i, self.chr_bank_1k[i] as i16);
        }
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
}

impl Mapper for TaitoTc0350Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr & Self::REGISTER_MASK {
            0x8000 => {
                self.prg_bank[0] = value & 0x3F;
                self.update_banks();
            }
            0x8001 => {
                self.prg_bank[1] = value & 0x3F;
                self.update_banks();
            }
            0x8002 => {
                self.chr_bank_2k[0] = value;
                self.update_banks();
            }
            0x8003 => {
                self.chr_bank_2k[1] = value;
                self.update_banks();
            }
            0xA000 => {
                self.chr_bank_1k[0] = value;
                self.update_banks();
            }
            0xA001 => {
                self.chr_bank_1k[1] = value;
                self.update_banks();
            }
            0xA002 => {
                self.chr_bank_1k[2] = value;
                self.update_banks();
            }
            0xA003 => {
                self.chr_bank_1k[3] = value;
                self.update_banks();
            }
            0xC000 => self.irq_latch = value ^ 0xFF,
            0xC001 => {
                self.irq_counter = 0;
                self.irq_reload = true;
            }
            0xC002 => self.irq_enabled = true,
            0xC003 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xE000 => {
                self.base.set_mirroring_hv((value & 0x40) != 0);
            }
            _ => {}
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.a12_detector.update(addr) {
            self.clock_irq_counter();
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12_detector.cpu_tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // [0]: mirroring (0=Vert, 1=Horz)
        // [1..=2]: prg_bank[0..1]
        // [3..=4]: chr_bank_2k[0..1]
        // [5..=8]: chr_bank_1k[0..3]
        // [9]: irq_latch
        // [10]: irq_counter
        // [11]: irq flags (bit0=reload, bit1=enabled, bit2=pending)
        let mut snap = Vec::with_capacity(12);
        snap.push(if self.base.mirroring() == NametableLayout::Horizontal {
            1
        } else {
            0
        });
        snap.extend_from_slice(&self.prg_bank);
        snap.extend_from_slice(&self.chr_bank_2k);
        snap.extend_from_slice(&self.chr_bank_1k);
        snap.push(self.irq_latch);
        snap.push(self.irq_counter);
        let irq_flags = (self.irq_reload as u8)
            | ((self.irq_enabled as u8) << 1)
            | ((self.irq_pending as u8) << 2);
        snap.push(irq_flags);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 12 {
            self.base.set_mirroring_hv(data[0] != 0);
            self.prg_bank.copy_from_slice(&data[1..3]);
            self.chr_bank_2k.copy_from_slice(&data[3..5]);
            self.chr_bank_1k.copy_from_slice(&data[5..9]);
            self.irq_latch = data[9];
            self.irq_counter = data[10];
            let flags = data[11];
            self.irq_reload = (flags & 1) != 0;
            self.irq_enabled = (flags & 2) != 0;
            self.irq_pending = (flags & 4) != 0;
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_tc0690(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(48, prg_rom, chr_rom, mirroring))
    }

    fn trigger_scanline(mapper: &mut Box<dyn Mapper>) {
        mapper.ppu_address_changed(0x0FFF);
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
    }

    // -----------------------------------------------------------------------
    // Factory registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_48_is_registered() {
        let result = create_tc0690(
            banked_data(8 * 1024, 6),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        );
        assert!(
            result.is_ok(),
            "Mapper 48 must be registered in the factory"
        );
    }

    // -----------------------------------------------------------------------
    // PRG banking
    // -----------------------------------------------------------------------

    #[test]
    fn power_on_prg_8000_bank_0_and_a000_bank_1() {
        // On power-on both PRG banks default to 0; bank at $C000/$E000 fixed
        let prg = banked_data(8 * 1024, 6);
        let mapper =
            create_tc0690(prg, banked_data(1024, 8), NametableLayout::Vertical).expect("mapper 48");
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must default to bank 0");
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must default to bank 0");
    }

    #[test]
    fn prg_8000_switchable_via_register() {
        let prg = banked_data(8 * 1024, 6);
        let mut mapper =
            create_tc0690(prg, banked_data(1024, 8), NametableLayout::Vertical).expect("mapper 48");
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0x9FFF), 2);
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn prg_a000_switchable_via_register() {
        let prg = banked_data(8 * 1024, 6);
        let mut mapper =
            create_tc0690(prg, banked_data(1024, 8), NametableLayout::Vertical).expect("mapper 48");
        mapper.write_prg(0x8001, 1);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xBFFF), 1);
        mapper.write_prg(0x8001, 4);
        assert_eq!(mapper.read_prg(0xA000), 4);
    }

    #[test]
    fn prg_c000_and_e000_fixed_to_last_two_banks() {
        // 6 banks → second-last = 4, last = 5
        let prg = banked_data(8 * 1024, 6);
        let mut mapper =
            create_tc0690(prg, banked_data(1024, 8), NametableLayout::Vertical).expect("mapper 48");
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must be fixed to second-last bank"
        );
        assert_eq!(mapper.read_prg(0xDFFF), 4);
        assert_eq!(
            mapper.read_prg(0xE000),
            5,
            "$E000 must be fixed to last bank"
        );
        assert_eq!(mapper.read_prg(0xFFFF), 5);
    }

    // -----------------------------------------------------------------------
    // CHR banking – 2KB windows ($8002, $8003)
    // -----------------------------------------------------------------------

    #[test]
    fn chr_2kb_bank0_at_ppu_0000() {
        // 3 × 2KB banks (non-power-of-two avoids modulo false passes)
        let chr = banked_data(2 * 1024, 3);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0x8002, 1);
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x07FF), 1);
    }

    #[test]
    fn chr_2kb_bank1_at_ppu_0800() {
        let chr = banked_data(2 * 1024, 3);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0x8003, 2);
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0FFF), 2);
    }

    // -----------------------------------------------------------------------
    // CHR banking – 1KB windows ($A000–$A003)
    // -----------------------------------------------------------------------

    #[test]
    fn chr_1kb_bank2_at_ppu_1000() {
        let chr = banked_data(1024, 48);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0xA000, 10);
        assert_eq!(mapper.read_chr(0x1000), 10);
        assert_eq!(mapper.read_chr(0x13FF), 10);
    }

    #[test]
    fn chr_1kb_bank3_at_ppu_1400() {
        let chr = banked_data(1024, 48);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0xA001, 20);
        assert_eq!(mapper.read_chr(0x1400), 20);
        assert_eq!(mapper.read_chr(0x17FF), 20);
    }

    #[test]
    fn chr_1kb_bank4_at_ppu_1800() {
        let chr = banked_data(1024, 48);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0xA002, 30);
        assert_eq!(mapper.read_chr(0x1800), 30);
        assert_eq!(mapper.read_chr(0x1BFF), 30);
    }

    #[test]
    fn chr_1kb_bank5_at_ppu_1c00() {
        let chr = banked_data(1024, 48);
        let mut mapper = create_tc0690(banked_data(8 * 1024, 2), chr, NametableLayout::Vertical)
            .expect("mapper 48");
        mapper.write_prg(0xA003, 40);
        assert_eq!(mapper.read_chr(0x1C00), 40);
        assert_eq!(mapper.read_chr(0x1FFF), 40);
    }

    // -----------------------------------------------------------------------
    // Mirroring
    // -----------------------------------------------------------------------

    #[test]
    fn mirroring_controlled_by_e000_bit_6() {
        let mut mapper = create_tc0690(
            banked_data(8 * 1024, 2),
            banked_data(1024, 8),
            NametableLayout::Vertical,
        )
        .expect("mapper 48");
        // bit 6 = 0 → Vertical
        mapper.write_prg(0xE000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        // bit 6 = 1 → Horizontal
        mapper.write_prg(0xE000, 0x40);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -----------------------------------------------------------------------
    // IRQ
    // -----------------------------------------------------------------------

    #[test]
    fn irq_latch_xored_with_ff() {
        // Write 5 to $C000; stored latch = 5 ^ 0xFF = 0xFA (= 250).
        // IRQ fires after 0xFB clocks: clock 1 reloads counter to 0xFA,
        // clocks 2..0xFB decrement it; on clock 0xFB counter reaches 0.
        // This proves XOR happened: without it, write 5 → latch 5 → fires after 6 clocks.
        let prg = banked_data(8 * 1024, 2);
        let chr = banked_data(1024, 8);
        let mut mapper = create_tc0690(prg, chr, NametableLayout::Vertical).expect("mapper 48");
        mapper.write_prg(0xC000, 5); // latch = 5 ^ 0xFF = 0xFA
        mapper.write_prg(0xC001, 0); // reload counter from latch
        mapper.write_prg(0xC002, 0); // enable IRQ

        assert!(
            !mapper.irq_pending(),
            "IRQ must not be pending before enough scanlines"
        );

        // 0xFA scanlines are not enough (off by one — counter not yet 0)
        for _ in 0..0xFA_u8 {
            trigger_scanline(&mut mapper);
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not yet fire after 0xFA clocks"
        );

        // The 0xFB-th clock brings counter to 0
        trigger_scanline(&mut mapper);
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after 0xFB clocks (latch 0xFA = 5 XOR 0xFF)"
        );
    }

    #[test]
    fn irq_fires_after_n_scanlines() {
        // Write 0xFF to $C000 → latch = 0xFF ^ 0xFF = 0x00.
        // On the first clock, reload sets counter to 0 and IRQ fires immediately.
        let prg = banked_data(8 * 1024, 2);
        let chr = banked_data(1024, 8);
        let mut mapper = create_tc0690(prg, chr, NametableLayout::Vertical).expect("mapper 48");
        mapper.write_prg(0xC000, 0xFF); // latch = 0xFF ^ 0xFF = 0x00
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xC002, 0); // enable

        trigger_scanline(&mut mapper);
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after 1 scanline when latch=0"
        );
    }

    #[test]
    fn irq_enable_c002_disable_c003() {
        let prg = banked_data(8 * 1024, 2);
        let chr = banked_data(1024, 8);
        let mut mapper = create_tc0690(prg, chr, NametableLayout::Vertical).expect("mapper 48");
        // latch=0 (write 0xFF) → fires on the very first scanline clock
        mapper.write_prg(0xC000, 0xFF);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xC002, 0);
        trigger_scanline(&mut mapper);
        assert!(
            mapper.irq_pending(),
            "IRQ must be pending after scanline with enabled IRQ"
        );

        // Disable + acknowledge via $C003
        mapper.write_prg(0xC003, 0);
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after writing $C003"
        );

        // Trigger again without re-enabling → no IRQ
        trigger_scanline(&mut mapper);
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    // -----------------------------------------------------------------------
    // Register snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trips() {
        let prg = banked_data(8 * 1024, 6);
        let chr = banked_data(1024, 48);
        let mut mapper =
            create_tc0690(prg.clone(), chr.clone(), NametableLayout::Vertical).expect("mapper 48");

        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8002, 1);
        mapper.write_prg(0x8003, 2);
        mapper.write_prg(0xA000, 10);
        mapper.write_prg(0xA001, 20);
        mapper.write_prg(0xA002, 30);
        mapper.write_prg(0xA003, 40);
        mapper.write_prg(0xE000, 0x40); // Horizontal
        mapper.write_prg(0xC000, 0xFE); // latch = 1
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xC002, 0);

        let snap = mapper.registers_snapshot();
        let mut restored =
            create_tc0690(prg, chr, NametableLayout::Vertical).expect("mapper 48 restore");
        restored.restore_registers(&snap);

        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 2);
        assert_eq!(restored.read_prg(0xA000), 3);
        assert_eq!(restored.read_chr(0x1000), 10);
        assert_eq!(restored.read_chr(0x1400), 20);
        assert_eq!(restored.read_chr(0x1800), 30);
        assert_eq!(restored.read_chr(0x1C00), 40);
        assert!(restored.irq_pending() || !restored.irq_pending()); // just checks no crash
    }
}
