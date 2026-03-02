//! Mapper 064 - Tengen RAMBO-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_064>
//!
//! Known Limitations:
//! - IRQ delay (M2 cycles) is approximated; edge cases in Hard Drivin' prototype may differ.
//! - Skull & Crossbones "Continue" screen glitch is a known timing issue across emulators.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 064 - Tengen RAMBO-1
///
/// Hardware: Tengen RAMBO-1 ASIC (MMC3-compatible with extra CHR/PRG features and dual IRQ mode)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_064>
/// - PRG-ROM: Up to 256 KiB (32 × 8 KiB banks)
/// - CHR: Up to 256 KiB (256 × 1 KiB banks)
/// - Mirroring: Programmable (H/V)
/// - IRQ: 8-bit counter; scanline (PPU A12) mode or CPU-cycle (every 4 cycles) mode
///
/// Register map (even/odd pairs within each block):
/// - $8000 (even): Bank select (CPKX RRRR)
///   - C(7): CHR A12 inversion; P(6): PRG swap; K(5): full 1KB CHR mode
///   - RRRR: select target register (0-9 = R0-R9, 0xF = RF)
/// - $8001 (odd): Bank data (write to RRRR-selected register)
/// - $A000 (even): Mirroring (bit0: 0=V, 1=H)
/// - $A001 (odd): (unused on RAMBO-1)
/// - $C000 (even): IRQ latch (8-bit reload value)
/// - $C001 (odd): IRQ mode select / reload (bit0: 0=scanline, 1=cpu-cycle; also resets counter)
/// - $E000 (even): IRQ disable / acknowledge
/// - $E001 (odd): IRQ enable
///
/// PRG windows: R6@$8000 (or RF if P=1), R7@$A000, RF@$C000 (or R6 if P=1), fixed-last@$E000
/// CHR: see NesDev table; K=0 → R0/R1 are 2KB banks; K=1 → all 1KB
pub struct Mapper64 {
    base: BaseMapper,
    regs: [u8; 16],    // R0-R9 at indices 0-9, RF at index 15
    bank_select: u8,   // last $8000 write
    chr_a12_inv: bool, // C bit
    prg_swap: bool,    // P bit
    k_bit: bool,       // K bit (full 1KB CHR mode)
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    irq_pending: bool,
    irq_cpu_mode: bool,    // false=scanline (A12), true=cpu-cycle
    irq_reload_flag: bool, // set by $C001 write; cleared on next clock
    // CPU cycle mode prescaler (clock every 4 CPU cycles)
    cpu_prescaler: u8,
    // PPU A12 scanline filter
    prev_a12: bool,
    a12_low_cycles: u8,
}

impl Mapper64 {
    const A12_LOW_FILTER: u8 = 3;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);
        base.set_mirroring(NametableLayout::Vertical);
        let mut mapper = Self {
            base,
            regs: [0; 16],
            bank_select: 0,
            chr_a12_inv: false,
            prg_swap: false,
            k_bit: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_cpu_mode: false,
            irq_reload_flag: false,
            cpu_prescaler: 0,
            prev_a12: false,
            a12_low_cycles: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: 4 × 8KB slots
        let r6 = self.regs[6] as i16;
        let r7 = self.regs[7] as i16;
        let rf = self.regs[15] as i16;
        if self.prg_swap {
            self.base.select_prg_page(0, rf);
            self.base.select_prg_page(2, r6);
        } else {
            self.base.select_prg_page(0, r6);
            self.base.select_prg_page(2, rf);
        }
        self.base.select_prg_page(1, r7);
        self.base.select_prg_page(3, -1); // fixed last
        // CHR: 8 × 1KB slots
        for slot in 0..8 {
            let bank = self.chr_1k_bank_for_slot(slot) as i16;
            self.base.select_chr_page(slot, bank);
        }
    }

    /// Returns the 1KB CHR bank for a given 1KB slot (0-7 for PPU $0000-$1FFF).
    fn chr_1k_bank_for_slot(&self, slot: usize) -> usize {
        let k = self.k_bit;
        let c = self.chr_a12_inv;
        // effective_slot flips the upper/lower halves when C=1
        let eff = if c { slot ^ 4 } else { slot };
        match eff {
            0 => {
                if k {
                    self.regs[0] as usize
                } else {
                    (self.regs[0] as usize) & !1
                }
            }
            1 => {
                if k {
                    self.regs[8] as usize
                } else {
                    ((self.regs[0] as usize) & !1) | 1
                }
            }
            2 => {
                if k {
                    self.regs[1] as usize
                } else {
                    (self.regs[1] as usize) & !1
                }
            }
            3 => {
                if k {
                    self.regs[9] as usize
                } else {
                    ((self.regs[1] as usize) & !1) | 1
                }
            }
            4 => self.regs[2] as usize,
            5 => self.regs[3] as usize,
            6 => self.regs[4] as usize,
            7 => self.regs[5] as usize,
            _ => 0,
        }
    }

    fn clock_irq_counter(&mut self) {
        let latch = self.irq_latch;
        if self.irq_reload_flag {
            // Reload: if latch != 0, OR with 1 (rounds up to next odd, fixing timing)
            self.irq_counter = if latch != 0 { latch | 1 } else { 0 };
            self.irq_reload_flag = false;
        } else if self.irq_counter == 0 {
            self.irq_counter = latch;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
}

impl Mapper for Mapper64 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let even = (addr & 1) == 0;
        match addr & 0xE000 {
            0x8000 => {
                if even {
                    // Bank select
                    self.bank_select = value & 0x0F;
                    self.chr_a12_inv = (value & 0x80) != 0;
                    self.prg_swap = (value & 0x40) != 0;
                    self.k_bit = (value & 0x20) != 0;
                } else {
                    // Bank data
                    let r = self.bank_select as usize;
                    if r < 10 || r == 15 {
                        self.regs[r] = value;
                    }
                }
                self.update_banks();
            }
            0xA000 => {
                if even {
                    self.base.set_mirroring(if (value & 0x01) != 0 {
                        NametableLayout::Horizontal
                    } else {
                        NametableLayout::Vertical
                    });
                }
                // Odd ($A001): unused
            }
            0xC000 => {
                if even {
                    self.irq_latch = value;
                } else {
                    // Mode select + reload
                    self.irq_cpu_mode = (value & 0x01) != 0;
                    self.irq_reload_flag = true;
                    self.irq_counter = 0; // cleared; reloads on next clock
                    self.cpu_prescaler = 0;
                }
            }
            0xE000 => {
                if even {
                    // Disable + ack
                    self.irq_enabled = false;
                    self.irq_pending = false;
                } else {
                    // Enable
                    self.irq_enabled = true;
                }
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        let a12 = (addr & 0x1000) != 0;
        if a12 {
            // Rising edge detection with low-pass filter
            if !self.prev_a12 && self.a12_low_cycles >= Self::A12_LOW_FILTER {
                // Clock IRQ counter in scanline mode
                if !self.irq_cpu_mode {
                    self.clock_irq_counter();
                }
            }
            self.a12_low_cycles = 0;
        } else {
            self.a12_low_cycles = self.a12_low_cycles.saturating_add(1);
        }
        self.prev_a12 = a12;
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_cpu_mode {
            return;
        }
        self.cpu_prescaler += 1;
        if self.cpu_prescaler >= 4 {
            self.cpu_prescaler = 0;
            self.clock_irq_counter();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.base.mirroring() {
            NametableLayout::Horizontal => 1u8,
            _ => 0u8,
        };
        let flags = (self.chr_a12_inv as u8)
            | ((self.prg_swap as u8) << 1)
            | ((self.k_bit as u8) << 2)
            | ((self.irq_enabled as u8) << 3)
            | ((self.irq_pending as u8) << 4)
            | ((self.irq_cpu_mode as u8) << 5)
            | ((self.irq_reload_flag as u8) << 6);
        let mut v = vec![
            self.bank_select,
            mirror_byte,
            flags,
            self.irq_latch,
            self.irq_counter,
            self.cpu_prescaler,
        ];
        v.extend_from_slice(&self.regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 22 {
            return;
        }
        self.bank_select = data[0];
        self.base.set_mirroring(if data[1] != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        });
        let flags = data[2];
        self.chr_a12_inv = (flags & 1) != 0;
        self.prg_swap = (flags & 2) != 0;
        self.k_bit = (flags & 4) != 0;
        self.irq_enabled = (flags & 8) != 0;
        self.irq_pending = (flags & 16) != 0;
        self.irq_cpu_mode = (flags & 32) != 0;
        self.irq_reload_flag = (flags & 64) != 0;
        self.irq_latch = data[3];
        self.irq_counter = data[4];
        self.cpu_prescaler = data[5];
        self.regs.copy_from_slice(&data[6..22]);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.regs = [0; 16];
        self.bank_select = 0;
        self.chr_a12_inv = false;
        self.prg_swap = false;
        self.k_bit = false;
        self.base.set_mirroring(NametableLayout::Vertical);
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_cpu_mode = false;
        self.irq_reload_flag = false;
        self.cpu_prescaler = 0;
        self.prev_a12 = false;
        self.a12_low_cycles = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 32;
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Mapper64 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        Mapper64::new(MapperContext::new_for_test(
            64,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    fn set_prg_bank(mapper: &mut Mapper64, reg: u8, bank: u8) {
        mapper.write_prg(0x8000, reg); // select register
        mapper.write_prg(0x8001, bank); // set bank value
    }

    fn set_chr_bank(mapper: &mut Mapper64, reg: u8, bank: u8) {
        mapper.write_prg(0x8000, reg);
        mapper.write_prg(0x8001, bank);
    }

    #[test]
    fn mapper_64_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            64,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 64 must be registered");
    }

    // --- PRG banking ---

    #[test]
    fn prg_r6_selects_bank_at_8000() {
        let mut mapper = make_mapper();
        set_prg_bank(&mut mapper, 6, 5);
        assert_eq!(mapper.read_prg(0x8000), 5, "R6 must control $8000 bank");
    }

    #[test]
    fn prg_r7_selects_bank_at_a000() {
        let mut mapper = make_mapper();
        set_prg_bank(&mut mapper, 7, 9);
        assert_eq!(mapper.read_prg(0xA000), 9, "R7 must control $A000 bank");
    }

    #[test]
    fn prg_rf_selects_bank_at_c000() {
        let mut mapper = make_mapper();
        set_prg_bank(&mut mapper, 0x0F, 3);
        assert_eq!(mapper.read_prg(0xC000), 3, "RF must control $C000 bank");
    }

    #[test]
    fn prg_e000_is_fixed_last() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must be last bank"
        );
    }

    #[test]
    fn prg_swap_p_bit_swaps_r6_and_rf() {
        let mut mapper = make_mapper();
        set_prg_bank(&mut mapper, 6, 4);
        set_prg_bank(&mut mapper, 0x0F, 7);
        mapper.write_prg(0x8000, 0x40); // set P=1
        // P=1: RF@$8000, R6@$C000
        assert_eq!(mapper.read_prg(0x8000), 7, "P=1: RF at $8000");
        assert_eq!(mapper.read_prg(0xC000), 4, "P=1: R6 at $C000");
    }

    // --- CHR banking ---

    #[test]
    fn chr_r0_2k_bank_fills_slots_0_and_1() {
        let mut mapper = make_mapper();
        // K=0 (default), C=0: R0 is 2KB at $0000-$07FF
        set_chr_bank(&mut mapper, 0, 10); // R0 = 10 (even)
        // slot 0 ($0000): bank = R0 & ~1 = 10
        // slot 1 ($0400): bank = (R0 & ~1) | 1 = 11
        assert_eq!(mapper.read_chr(0x0000), 10, "2KB R0: slot 0 = bank 10");
        assert_eq!(mapper.read_chr(0x0400), 11, "2KB R0: slot 1 = bank 11");
    }

    #[test]
    fn chr_k1_mode_uses_r8_at_slot1() {
        let mut mapper = make_mapper();
        // K=1: set via bank_select bit 5
        mapper.write_prg(0x8000, 0x20); // K=1 bit
        // Now set R0 = 3 and R8 = 5
        set_chr_bank(&mut mapper, 0, 3); // R0
        mapper.write_prg(0x8000, 0x20 | 8); // K=1, select R8
        mapper.write_prg(0x8001, 5); // R8 = 5
        // slot 0 ($0000): K=1 → bank = R0 = 3
        // slot 1 ($0400): K=1 → bank = R8 = 5
        assert_eq!(mapper.read_chr(0x0000), 3, "K=1: slot 0 = R0 = 3");
        assert_eq!(mapper.read_chr(0x0400), 5, "K=1: slot 1 = R8 = 5");
    }

    #[test]
    fn chr_c_bit_inverts_halves() {
        let mut mapper = make_mapper();
        // Set R2 = 20 (CHR slot 4 in normal mode → goes to slot 0 when C=1)
        set_chr_bank(&mut mapper, 2, 20);
        // Enable C=1 via $8000
        mapper.write_prg(0x8000, 0x80); // C=1
        // C=1: lower half ($0000-$0FFF) gets R2-R5 (slots 4-7 normally)
        // slot 0 ($0000) should be R2 = 20
        assert_eq!(mapper.read_chr(0x0000), 20, "C=1: $0000 should be R2");
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0); // 0 = vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 1); // 1 = horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ (CPU cycle mode) ---

    #[test]
    fn irq_cpu_mode_fires_every_4_cycles() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 4); // latch = 4
        mapper.write_prg(0xC001, 1); // cpu mode + reload (sets reload_flag, counter=0)
        mapper.write_prg(0xE001, 0); // enable IRQ
        // Behavior: at first clock (cycle 4), reload_flag=true → counter = 4|1 = 5, then
        // 5 more clocks (cycles 8..28) decrement to 0 → IRQ fires at cycle 24
        let mut irq_at = 0u32;
        for i in 1..=50u32 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                irq_at = i;
                break;
            }
        }
        assert!(irq_at > 0, "IRQ must fire in CPU cycle mode");
        // 6 clocks × 4 cycles = 24 CPU cycles
        assert_eq!(
            irq_at, 24,
            "IRQ must fire after 6 clocks × 4 cycles = 24 cpu cycles"
        );
    }

    #[test]
    fn irq_disable_and_ack() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 1); // cpu mode
        mapper.write_prg(0xE001, 0); // enable
        for _ in 0..20 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xE000, 0); // disable + ack
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after $E000 write"
        );
        assert!(
            !mapper.irq_enabled,
            "IRQ must be disabled after $E000 write"
        );
    }

    // --- Scanline (PPU A12) mode ---

    #[test]
    fn irq_scanline_mode_clocks_on_a12_rise() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 3); // latch = 3
        mapper.write_prg(0xC001, 0); // scanline mode + reload
        mapper.write_prg(0xE001, 0); // enable
        // counter = 3 | 1 = 3 (odd already)
        // Need 3 scanline clocks (A12 low→high transitions with filter)
        let mut fired = false;
        for _ in 0..20 {
            // Simulate A12 going low (several cycles) then high
            for _ in 0..10 {
                mapper.ppu_address_changed(0x0000); // A12=0
            }
            mapper.ppu_address_changed(0x1000); // A12=1 → clock
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(
            fired,
            "IRQ must fire in scanline mode after enough A12 rising edges"
        );
    }
}
