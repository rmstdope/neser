//! Mapper 065 - Irem H3001
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_065>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

use crate::nes::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

/// Mapper 065 - Irem H3001
///
/// Hardware: Irem H3001 ASIC
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_065>
/// - PRG-ROM: Up to 256 KiB (32 × 8 KiB banks)
/// - CHR: Up to 256 KiB (256 × 1 KiB banks)
/// - Mirroring: Programmable (H/V/1-screen-A)
/// - IRQ: 16-bit CPU-cycle counter, fires on reach-0, does not wrap.
///
/// Register map:
/// - $8000: PRG bank 0 (8KB at $8000 or $C000, depending on $9000 mode)
/// - $A000: PRG bank 1 (8KB at $A000-$BFFF)
/// - $B000-$B007: CHR banks 0-7 (8 × 1KB banks for PPU $0000-$1FFF)
/// - $9000 [X... ....]: PRG mode
///   - 0: reg0 → $8000, fixed 0x3E → $C000
///   - 1: reg0 → $C000, fixed 0x3E → $8000
/// - $9001 [MM.. ....]: Mirroring: 00=Vert, 10=Horz, 01/11=1-screen A
/// - $9003 [E... ....]: IRQ Enable; also acknowledges pending IRQ
/// - $9004 [.... ....]: Reload IRQ counter; also acknowledges pending IRQ
/// - $9005 [IIII IIII]: High 8 bits of IRQ reload value
/// - $9006 [IIII IIII]: Low 8 bits of IRQ reload value
///
/// $E000-$FFFF: always fixed to last bank.
///
/// Power-on state: prg[0]=0x00, prg[1]=0x01, IRQ disabled.
pub struct IremH3001Mapper {
    base: BaseMapper,
    prg_regs: [u8; 2], // reg0 ($8000), reg1 ($A000)
    chr_regs: [u8; 8], // $B000-$B007
    prg_mode: bool,    // $9000 bit7: false=mode0, true=mode1
    irq: CpuCycleIrq,
}

impl IremH3001Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
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
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(NametableLayout::Vertical);

        let mut mapper = Self {
            base,
            prg_regs: [0x00, 0x01], // power-on state
            chr_regs: [0; 8],
            prg_mode: false,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownToZero),
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: 4 slots of 8KB
        if self.prg_mode {
            // mode1: $8000 = fixed -2, $C000 = reg0
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(2, self.prg_regs[0] as i16);
        } else {
            // mode0: $8000 = reg0, $C000 = fixed -2
            self.base.select_prg_page(0, self.prg_regs[0] as i16);
            self.base.select_prg_page(2, -2);
        }
        self.base.select_prg_page(1, self.prg_regs[1] as i16);
        self.base.select_prg_page(3, -1); // $E000 always last

        // CHR: 8 slots of 1KB
        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_regs[i] as i16);
        }
    }
}

impl Mapper for IremH3001Mapper {
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
        match addr {
            0x8000 => {
                self.prg_regs[0] = value;
                self.update_banks();
            }
            0xA000 => {
                self.prg_regs[1] = value;
                self.update_banks();
            }
            0xB000..=0xB007 => {
                let reg = (addr & 0x0007) as usize;
                self.chr_regs[reg] = value;
                self.update_banks();
            }
            0x9000 => {
                self.prg_mode = (value & 0x80) != 0;
                self.update_banks();
            }
            0x9001 => {
                self.base.set_mirroring(match (value >> 6) & 0x03 {
                    0b00 => NametableLayout::Vertical,
                    0b10 => NametableLayout::Horizontal,
                    _ => NametableLayout::SingleScreenLower, // 1-screen A
                });
            }
            0x9003 => {
                self.irq.acknowledge();
                self.irq.set_enabled((value & 0x80) != 0);
            }
            0x9004 => {
                self.irq.acknowledge();
                self.irq.reload_counter();
            }
            0x9005 => {
                self.irq
                    .set_reload((self.irq.reload() & 0x00FF) | ((value as u16) << 8));
            }
            0x9006 => {
                self.irq
                    .set_reload((self.irq.reload() & 0xFF00) | (value as u16));
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.base.mirroring() {
            NametableLayout::Vertical => 0u8,
            NametableLayout::Horizontal => 1,
            _ => 2,
        };
        let irq_flags = (self.irq.enabled() as u8) | ((self.irq.is_pending() as u8) << 1);
        let mut v = vec![
            self.prg_regs[0],
            self.prg_regs[1],
            self.prg_mode as u8,
            mirror_byte,
            irq_flags,
            (self.irq.counter() & 0xFF) as u8,
            (self.irq.counter() >> 8) as u8,
            (self.irq.reload() & 0xFF) as u8,
            (self.irq.reload() >> 8) as u8,
        ];
        v.extend_from_slice(&self.chr_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 17 {
            return;
        }
        self.prg_regs[0] = data[0];
        self.prg_regs[1] = data[1];
        self.prg_mode = data[2] != 0;
        self.base.set_mirroring(match data[3] {
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            _ => NametableLayout::Vertical,
        });
        self.irq.set_enabled((data[4] & 1) != 0);
        self.irq.set_pending((data[4] & 2) != 0);
        self.irq
            .set_counter((data[5] as u16) | ((data[6] as u16) << 8));
        self.irq
            .set_reload((data[7] as u16) | ((data[8] as u16) << 8));
        self.chr_regs.copy_from_slice(&data[9..17]);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.prg_regs = [0x00, 0x01];
        self.chr_regs = [0; 8];
        self.prg_mode = false;
        self.base.set_mirroring(NametableLayout::Vertical);
        self.irq.set_enabled(false);
        self.irq.set_pending(false);
        self.irq.set_counter(0);
        self.irq.set_reload(0);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 32;
    const CHR_BANKS: usize = 64;

    fn make_mapper() -> IremH3001Mapper {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        IremH3001Mapper::new(MapperContext::new_for_test(
            65,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_65_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            65,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 65 must be registered");
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg0_is_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG reg0 must start at 0");
    }

    #[test]
    fn power_on_prg1_is_1() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xA000), 1, "PRG reg1 must start at 1");
    }

    #[test]
    fn power_on_c000_is_fixed_second_to_last() {
        let mapper = make_mapper();
        // mode0: $C000 = bank 30 (second to last in 32-bank ROM)
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 must be fixed to second-to-last bank"
        );
    }

    #[test]
    fn power_on_e000_is_fixed_last() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must always be last bank"
        );
    }

    // --- PRG mode ---

    #[test]
    fn prg_mode1_swaps_8000_and_c000_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5); // reg0 = 5
        mapper.write_prg(0x9000, 0x80); // mode1
        assert_eq!(
            mapper.read_prg(0x8000),
            (PRG_BANKS - 2) as u8,
            "mode1: $8000 = second-to-last"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "mode1: $C000 = reg0");
    }

    // --- CHR banking ---

    #[test]
    fn chr_bank_registers() {
        let mut mapper = make_mapper();
        for slot in 0..8u16 {
            mapper.write_prg(0xB000 + slot, (slot * 5) as u8 & 0x3F);
        }
        for slot in 0..8u16 {
            let expected = ((slot * 5) as usize * 1024) % (CHR_BANKS * 1024) / 1024;
            assert_eq!(
                mapper.read_chr(slot * 1024),
                expected as u8,
                "CHR slot {slot} wrong bank"
            );
        }
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9001, 0x80);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_by_default() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_fires_after_reload_value_cycles() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00); // high = 0
        mapper.write_prg(0x9006, 5); // low = 5 → reload = 5
        mapper.write_prg(0x9004, 0); // load counter
        mapper.write_prg(0x9003, 0x80); // enable IRQ
        for _ in 0..4 {
            assert!(!mapper.irq_pending());
            mapper.cpu_cycle();
        }
        mapper.cpu_cycle(); // 5th cycle → counter reaches 0 → IRQ
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after reload value cycles"
        );
    }

    #[test]
    fn irq_acknowledge_via_9003() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00);
        mapper.write_prg(0x9006, 1);
        mapper.write_prg(0x9004, 0);
        mapper.write_prg(0x9003, 0x80);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
        mapper.write_prg(0x9003, 0x00); // acknowledge (and disable)
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after write to $9003"
        );
    }

    #[test]
    fn irq_counter_stops_at_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9005, 0x00);
        mapper.write_prg(0x9006, 2);
        mapper.write_prg(0x9004, 0);
        mapper.write_prg(0x9003, 0x80);
        for _ in 0..10 {
            mapper.cpu_cycle();
        }
        // IRQ fires once; counter stays at 0 (no wrap)
        assert!(mapper.irq_pending(), "IRQ must remain pending");
        assert_eq!(mapper.irq.counter(), 0, "Counter must stop at 0");
    }

    // --- Snapshot ---

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 7);
        mapper.write_prg(0xA000, 4);
        mapper.write_prg(0x9001, 0x80); // horizontal
        mapper.write_prg(0x9005, 0x01);
        mapper.write_prg(0x9006, 0x23);
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_prg(0xA000), mapper.read_prg(0xA000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
        assert_eq!(r.irq.reload(), mapper.irq.reload());
    }
}
