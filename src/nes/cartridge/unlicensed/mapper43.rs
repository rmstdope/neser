//! Mapper 043 - TONY-I / YS-612 (Super Mario Bros. 2 FDS conversion)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_043>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

use crate::nes::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

/// Mapper 043 - TONY-I / YS-612 (SMB2 Japanese FDS conversion)
///
/// Hardware: TONY-I / YS-612 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_043>
/// - PRG-ROM: 80 KiB (10 × 8 KiB banks)
/// - PRG-RAM: None
/// - CHR: 8 KiB fixed (ROM)
/// - Mirroring: Fixed from header
///
/// PRG window map (8 KiB each):
/// - $5000-$5FFF: 2KB chip at offset 0x10000, masked to 2KB
/// - $6000-$7FFF: Fixed bank 2 (offset 0x4000)
/// - $8000-$9FFF: Fixed bank 1 (offset 0x2000)
/// - $A000-$BFFF: Fixed bank 0 (offset 0x0000)
/// - $C000-$DFFF: Switchable via register at ($addr & 0x71FF) == $4022
/// - $E000-$FFFF: Fixed bank 9 (offset 0x12000)
///
/// Register: ($addr & 0x71FF) == $4022 → bits[2:0] index into BANK_LOOKUP
/// BANK_LOOKUP: [4, 3, 4, 4, 4, 7, 5, 6]
///
/// IRQ: 12-bit CPU-cycle counter; fires on overflow ($FFF→$1000 = 4096 cycles).
/// Writing to ($addr & 0x71FF) == $4122 resets counter to 0 and clears pending IRQ.
pub struct Mapper43 {
    base: BaseMapper,
    switchable_bank: u8,
    irq: CpuCycleIrq,
}

impl Mapper43 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const BANK_LOOKUP: [u8; 8] = [4, 3, 4, 4, 4, 7, 5, 6];

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_prg_6000_banking();
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            switchable_bank: 0,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::UpReset { threshold: 0x1000 }),
        };
        mapper.irq.set_enabled(true); // Mapper 43 always counts

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // $6000-$7FFF: fixed bank 2
        self.base.select_prg_6000_page(2);

        // $8000=bank1, $A000=bank0, $C000=switchable, $E000=bank9
        self.base.select_prg_page(0, 1);
        self.base.select_prg_page(1, 0);
        let c000_bank = Self::BANK_LOOKUP[self.switchable_bank as usize & 7] as i16;
        self.base.select_prg_page(2, c000_bank);
        self.base.select_prg_page(3, 9);
    }

    fn read_prg_5000(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        // 2KB chip at 0x10000, masked to 2KB
        let chip_offset = addr as usize & 0x7FF;
        prg.get(0x10000 + chip_offset).copied().unwrap_or(0)
    }
}

impl Mapper for Mapper43 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x5FFF => self.read_prg_5000(addr),
            _ => {
                if let Some(value) = self.base.try_read_prg_6000(addr) {
                    return value;
                }
                match addr {
                    0x8000..=0xFFFF => self.base.read_prg_rom(addr),
                    _ => 0,
                }
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr & 0x71FF == 0x4022 {
            self.switchable_bank = value & 0x07;
            self.update_banks();
        } else if addr & 0x71FF == 0x4122 {
            self.irq.set_counter(0);
            self.irq.acknowledge();
        }
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.switchable_bank,
            (self.irq.counter() & 0xFF) as u8,
            (self.irq.counter() >> 8) as u8,
            self.irq.is_pending() as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.switchable_bank = data[0];
            self.irq
                .set_counter((data[1] as u16) | ((data[2] as u16) << 8));
            self.irq.set_pending(data[3] != 0);
            self.irq.set_enabled(true);
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper43;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    const PRG_SIZE: usize = 80 * 1024; // 80 KiB
    const CHR_SIZE: usize = 8 * 1024; // 8 KiB

    /// Build a PRG ROM where every byte at offset `i` equals `(i / 0x2000) as u8`
    /// (i.e. bank-number fill), except the 2KB chip region at 0x10000 which is
    /// filled with the sentinel value 0xAB.
    fn make_prg() -> Vec<u8> {
        let mut prg = vec![0u8; PRG_SIZE];
        for (i, byte) in prg.iter_mut().enumerate().take(PRG_SIZE) {
            *byte = (i / 0x2000) as u8;
        }
        // 2KB chip at 0x10000..=0x107FF: fill with sentinel so reads are distinct
        for b in prg[0x10000..0x10800].iter_mut() {
            *b = 0xAB;
        }
        prg
    }

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            43,
            make_prg(),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 43 should be registered")
    }

    fn make_mapper_direct() -> Mapper43 {
        Mapper43::new(MapperContext::new_for_test(
            43,
            make_prg(),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ))
    }

    // ── Factory ─────────────────────────────────────────────────────────────

    #[test]
    fn mapper_43_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            43,
            make_prg(),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 43 must be registered in the factory"
        );
    }

    // ── Fixed PRG windows ────────────────────────────────────────────────────

    #[test]
    fn prg_5000_reads_2kb_chip_with_masking() {
        let mapper = make_mapper();
        // $5000 → offset 0x10000 + (0x5000 & 0x7FF) = 0x10000 → sentinel 0xAB
        assert_eq!(
            mapper.read_prg(0x5000),
            0xAB,
            "$5000 must read from the 2KB chip sentinel region"
        );
        // $5800 → (0x5800 & 0x7FF) = 0 → also sentinel
        assert_eq!(mapper.read_prg(0x5800), 0xAB);
        // Masking: $57FF → (0x57FF & 0x7FF) = 0x7FF → still within 2KB chip → 0xAB
        assert_eq!(mapper.read_prg(0x57FF), 0xAB);
    }

    #[test]
    fn prg_6000_is_fixed_bank_2() {
        let mapper = make_mapper();
        // Bank 2 at offset 0x4000; fill byte = 2
        assert_eq!(
            mapper.read_prg(0x6000),
            2,
            "$6000 must read from fixed bank 2"
        );
        assert_eq!(mapper.read_prg(0x7FFF), 2);
    }

    #[test]
    fn prg_8000_is_fixed_bank_1() {
        let mapper = make_mapper();
        // Bank 1 at offset 0x2000; fill byte = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must read from fixed bank 1"
        );
        assert_eq!(mapper.read_prg(0x9FFF), 1);
    }

    #[test]
    fn prg_a000_is_fixed_bank_0() {
        let mapper = make_mapper();
        // Bank 0 at offset 0x0000; fill byte = 0
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "$A000 must read from fixed bank 0"
        );
        assert_eq!(mapper.read_prg(0xBFFF), 0);
    }

    #[test]
    fn prg_e000_is_fixed_bank_9() {
        let mapper = make_mapper();
        // Bank 9 at offset 0x12000; fill byte = 9
        assert_eq!(
            mapper.read_prg(0xE000),
            9,
            "$E000 must read from fixed bank 9"
        );
        assert_eq!(mapper.read_prg(0xFFFF), 9);
    }

    // ── Switchable $C000-$DFFF ───────────────────────────────────────────────

    #[test]
    fn prg_c000_defaults_to_bank_4() {
        let mapper = make_mapper();
        // Default switchable_bank = 0; BANK_LOOKUP[0] = 4; fill byte = 4
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must default to BANK_LOOKUP[0] = bank 4"
        );
    }

    #[test]
    fn prg_c000_switches_via_register_at_4022() {
        let mut mapper = make_mapper();
        // Write index 1 → BANK_LOOKUP[1] = 3 → fill byte = 3
        mapper.write_prg(0x4022, 1);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "After writing 1 to $4022, $C000 should read bank 3"
        );
        // Mirrored address: $4222 also hits the register (addr & 0x71FF == 0x4022)
        mapper.write_prg(0x4222, 0); // back to BANK_LOOKUP[0] = 4
        assert_eq!(mapper.read_prg(0xC000), 4);
    }

    #[test]
    fn c000_bank_lookup_table_all_7_entries() {
        let mut mapper = make_mapper();
        let expected: [u8; 8] = [4, 3, 4, 4, 4, 7, 5, 6];
        for (idx, &bank) in expected.iter().enumerate() {
            mapper.write_prg(0x4022, idx as u8);
            assert_eq!(
                mapper.read_prg(0xC000),
                bank,
                "BANK_LOOKUP[{idx}] should select bank {bank}, got {}",
                mapper.read_prg(0xC000)
            );
        }
    }

    // ── IRQ ──────────────────────────────────────────────────────────────────

    #[test]
    fn irq_fires_after_4096_cycles() {
        let mut mapper = make_mapper_direct();
        assert!(!mapper.irq_pending(), "IRQ must not be pending initially");
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles"
        );
        mapper.cpu_cycle(); // 4096th cycle → overflow
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after exactly 4096 cycles"
        );
    }

    #[test]
    fn irq_acknowledge_resets_counter() {
        let mut mapper = make_mapper_direct();
        // Run 4096 cycles to fire IRQ
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending");
        // Writing to $4122 resets counter and clears IRQ
        mapper.write_prg(0x4122, 0x00);
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after writing $4122"
        );
        // Counter is reset; running 4095 more cycles must not fire again
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before counter overflows again"
        );
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must fire again after another 4096 cycles"
        );
    }

    // ── CHR ──────────────────────────────────────────────────────────────────

    #[test]
    fn chr_rom_readable() {
        let mut mapper = {
            let mut chr = vec![0u8; CHR_SIZE];
            chr[0x0000] = 0x42;
            chr[0x1FFF] = 0x99;
            Box::new(Mapper43::new(MapperContext::new_for_test(
                43,
                make_prg(),
                chr,
                NametableLayout::Vertical,
            )))
        };
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x99);
    }

    // ── Snapshot round-trip ──────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper_direct();
        // Change some state
        mapper.write_prg(0x4022, 5); // switchable_bank = 5
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();
        // Build a fresh mapper and restore
        let mut mapper2 = make_mapper_direct();
        mapper2.restore_registers(&snap);
        assert_eq!(
            mapper2.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Switchable bank must be preserved across snapshot"
        );
        // IRQ pending state
        assert_eq!(
            mapper2.irq_pending(),
            mapper.irq_pending(),
            "IRQ pending must be preserved"
        );
    }
}
