//! Mapper 298 – NTDEC TF-1201
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki not available (403/404).
//! - Fallback source: Mesen2 `Core/NES/Mappers/Ntdec/Tf1201.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Ntdec/Tf1201.h>
//!
//! ## Hardware overview
//!
//! The TF-1201 board is an NTDEC mapper used for games such as "Fudou Myouou Den"
//! (Kaiketsu Yanchamaru) pirate ports. It provides:
//!
//! - 4 × 8 KB switchable PRG-ROM slots with optional PRG swap mode
//! - 8 × 1 KB switchable CHR-ROM/RAM slots via a VRC-style nibble-write scheme
//! - Software-controlled H/V mirroring
//! - CPU-cycle-based scanline-approximate IRQ counter
//!
//! ## PRG banking (`$8000–$FFFF`)
//!
//! | Address | Mask   | Function                            |
//! |---------|--------|-------------------------------------|
//! | $8000   | $F003  | Set PRG register 0 (8 KB)           |
//! | $9000   | $F003  | Mirroring: bit 0 = 1→H, 0→V        |
//! | $9001   | $F003  | Swap PRG: non-zero swaps slots 0↔2  |
//! | $A000   | $F003  | Set PRG register 1 (8 KB)           |
//!
//! Window map (depending on `swap_prg`):
//!
//! | Slot | `swap_prg=false` | `swap_prg=true`      |
//! |------|-----------------|----------------------|
//! | $8000–$9FFF | `prg_regs[0]` | fixed second-last |
//! | $A000–$BFFF | `prg_regs[1]` | `prg_regs[1]`    |
//! | $C000–$DFFF | fixed second-last | `prg_regs[0]` |
//! | $E000–$FFFF | fixed last    | fixed last         |
//!
//! ## CHR banking (`$B000–$E003`)
//!
//! Each of the 8 × 1 KB CHR slots is controlled by two nibble writes. The raw
//! address is first remapped via:
//!
//! ```text
//! effective_addr = (addr & 0xF003) | ((addr & 0x0C) >> 2)
//! ```
//!
//! Then:
//! ```text
//! slot  = (((effective_addr >> 11) - 6) | (effective_addr & 0x01)) & 0x07
//! shift = (effective_addr & 0x02) << 1   // 0 for low nibble, 4 for high nibble
//! chr_regs[slot] = (chr_regs[slot] & (0xF0 >> shift))
//!                | ((value & 0x0F) << shift)
//! ```
//!
//! ## IRQ
//!
//! A CPU-clock-based scaler fires an IRQ approximately once per scanline. On each
//! CPU cycle when enabled: `scaler -= 3`. When `scaler ≤ 0`: `scaler += 341`,
//! then `counter++`. When `counter` wraps from 255 to 0, the IRQ is asserted.
//!
//! | Address | Mask   | Function                                  |
//! |---------|--------|-------------------------------------------|
//! | $F000   | $F003  | Load IRQ reload low nibble (bits 3:0)     |
//! | $F002   | $F003  | Load IRQ reload high nibble (bits 7:4)    |
//! | $F001   | $F003  | Enable IRQ; counter ← reload; clear IRQ  |
//! | $F003   | $F003  | Acknowledge (clear) IRQ                  |
//!
//! Power-on / reset: all counters and flags 0.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 298 – NTDEC TF-1201
pub struct Mapper298 {
    base: BaseMapper,
    /// 8 KB switchable PRG banks for slots 0 and 1.
    prg_regs: [u8; 2],
    /// When true, PRG slots 0 and 2 are swapped.
    swap_prg: bool,
    /// 8 × 1 KB CHR bank registers (one full byte per 1 KB CHR slot).
    chr_regs: [u8; 8],

    // ── IRQ state ─────────────────────────────────────────────────────────────
    /// 8-bit IRQ counter; fires on 255→0 wrap.
    irq_counter: u8,
    /// Reload value loaded into the counter when IRQ is (re-)enabled.
    irq_reload: u8,
    /// Prescaler: decremented by 3 per CPU cycle; reset to +341 when ≤ 0.
    irq_scaler: i16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mapper298 {
    const MAPPER_NUMBER: u16 = 298;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KB

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_regs: [0; 2],
            swap_prg: false,
            chr_regs: [0; 8],
            irq_counter: 0,
            irq_reload: 0,
            irq_scaler: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.update_prg();
        mapper.update_chr();
        mapper
    }

    fn update_prg(&mut self) {
        if self.swap_prg {
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(2, self.prg_regs[0] as i16);
        } else {
            self.base.select_prg_page(0, self.prg_regs[0] as i16);
            self.base.select_prg_page(2, -2);
        }
        self.base.select_prg_page(1, self.prg_regs[1] as i16);
        self.base.select_prg_page(3, -1);
    }

    fn update_chr(&mut self) {
        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_regs[i] as i16);
        }
    }

    /// Apply the TF-1201 address remapping before decoding the CHR slot/shift.
    fn write_chr_nibble(&mut self, addr: u16, value: u8) {
        let effective = (addr & 0xF003) | ((addr & 0x0C) >> 2);
        let slot = ((((effective >> 11).wrapping_sub(6)) | (effective & 0x01)) & 0x07) as usize;
        let shift = (effective & 0x02) << 1; // 0 or 4
        let mask: u8 = 0xF0 >> shift; // keep bits we're not writing
        self.chr_regs[slot] = (self.chr_regs[slot] & mask) | ((value & 0x0F) << shift);
        self.update_chr();
    }
}

impl Mapper for Mapper298 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        // CHR registers: $B000–$EFFF (raw), decoded after address remapping
        if (0xB000..=0xEFFF).contains(&addr) {
            self.write_chr_nibble(addr, value);
            return;
        }
        match addr & 0xF003 {
            0x8000 => {
                self.prg_regs[0] = value;
                self.update_prg();
            }
            0x9000 => {
                self.base.set_mirroring_hv(value & 0x01 != 0);
            }
            0x9001 => {
                self.swap_prg = (value & 0x03) != 0;
                self.update_prg();
            }
            0xA000 => {
                self.prg_regs[1] = value;
                self.update_prg();
            }
            0xF000 => {
                // IRQ reload low nibble
                self.irq_reload = (self.irq_reload & 0xF0) | (value & 0x0F);
            }
            0xF002 => {
                // IRQ reload high nibble
                self.irq_reload = (self.irq_reload & 0x0F) | (value << 4);
            }
            0xF001 => {
                // Enable IRQ; load counter from reload; clear pending
                self.irq_enabled = (value & 0x02) != 0;
                if self.irq_enabled {
                    self.irq_scaler = 341;
                    self.irq_counter = self.irq_reload;
                }
                self.irq_pending = false;
            }
            0xF003 => {
                // Acknowledge IRQ
                self.irq_pending = false;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        self.irq_scaler -= 3;
        if self.irq_scaler <= 0 {
            self.irq_scaler += 341;
            let (new_counter, overflow) = self.irq_counter.overflowing_add(1);
            self.irq_counter = new_counter;
            if overflow {
                self.irq_pending = true;
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 2];
        self.swap_prg = false;
        self.chr_regs = [0; 8];
        self.irq_counter = 0;
        self.irq_reload = 0;
        self.irq_scaler = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.update_prg();
        self.update_chr();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.base.banking_snapshot();
        snap.push(self.prg_regs[0]);
        snap.push(self.prg_regs[1]);
        snap.push(self.swap_prg as u8);
        snap.extend_from_slice(&self.chr_regs);
        snap.push(self.irq_counter);
        snap.push(self.irq_reload);
        snap.push((self.irq_scaler & 0xFF) as u8);
        snap.push(((self.irq_scaler >> 8) & 0xFF) as u8);
        snap.push(self.irq_enabled as u8);
        snap.push(self.irq_pending as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_len = self.base.banking_snapshot().len();
        // Extra bytes after banking snapshot: 2 prg_regs + 1 swap + 8 chr_regs
        //   + 1 irq_counter + 1 irq_reload + 2 irq_scaler + 1 irq_enabled + 1 irq_pending = 17
        const EXTRA: usize = 17;

        if data.len() < banking_len + EXTRA {
            let safe = data.len().min(banking_len);
            self.base.restore_banking(&data[..safe]);
            return;
        }

        self.base.restore_banking(&data[..banking_len]);
        let e = &data[banking_len..];
        self.prg_regs[0] = e[0];
        self.prg_regs[1] = e[1];
        self.swap_prg = e[2] != 0;
        self.chr_regs.copy_from_slice(&e[3..11]);
        self.irq_counter = e[11];
        self.irq_reload = e[12];
        self.irq_scaler = (e[13] as i16) | ((e[14] as i16) << 8);
        self.irq_enabled = e[15] != 0;
        self.irq_pending = e[16] != 0;
        self.update_prg();
        self.update_chr();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANK_SIZE: usize = 8 * 1024;
    const CHR_BANK_SIZE: usize = 1024;

    fn make_mapper(prg_banks: usize, chr_banks: usize) -> Mapper298 {
        Mapper298::new(
            MapperContext::new_for_test(
                Mapper298::MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, prg_banks),
                banked_data(CHR_BANK_SIZE, chr_banks),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_298_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                Mapper298::MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 8),
                banked_data(CHR_BANK_SIZE, 8),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 298 must be registered in factory");
    }

    // ── Power-on PRG banking ─────────────────────────────────────────────────

    #[test]
    fn power_on_slot0_is_bank0() {
        let mapper = make_mapper(8, 8);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "slot $8000 = bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_slot1_is_bank0() {
        let mapper = make_mapper(8, 8);
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "slot $A000 = bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_slot2_fixed_second_last() {
        let mapper = make_mapper(8, 8);
        // Fixed to second-to-last bank (bank 6 of 8)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "slot $C000 fixed to second-last bank"
        );
    }

    #[test]
    fn power_on_slot3_fixed_last() {
        let mapper = make_mapper(8, 8);
        // Fixed to last bank (bank 7 of 8)
        assert_eq!(mapper.read_prg(0xE000), 7, "slot $E000 fixed to last bank");
    }

    // ── PRG register writes ───────────────────────────────────────────────────

    #[test]
    fn write_8000_sets_prg_slot0() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 = PRG bank 3");
    }

    #[test]
    fn write_a000_sets_prg_slot1() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0xA000, 5);
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 = PRG bank 5");
    }

    #[test]
    fn prg_slot3_stays_fixed_last_after_register_writes() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0xA000, 3);
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 stays last bank");
    }

    // ── PRG swap mode ────────────────────────────────────────────────────────

    #[test]
    fn swap_prg_nonzero_swaps_slots_0_and_2() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0x8000, 2); // prg_regs[0] = 2
        mapper.write_prg(0x9001, 0x01); // enable swap
        // Slot $8000 now fixed second-last, slot $C000 = prg_regs[0] = 2
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "slot $8000 = second-last (6) in swap mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "slot $C000 = prg_regs[0] in swap mode"
        );
    }

    #[test]
    fn swap_prg_zero_restores_normal_layout() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x9001, 0x01); // enable
        mapper.write_prg(0x9001, 0x00); // disable
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "slot $8000 = prg_regs[0] after swap off"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "slot $C000 = second-last after swap off"
        );
    }

    #[test]
    fn swap_prg_slot1_unchanged_by_swap() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0xA000, 4);
        mapper.write_prg(0x9001, 0x01);
        assert_eq!(mapper.read_prg(0xA000), 4, "slot $A000 unchanged by swap");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    /// Write a full 8-bit CHR bank register for slot `slot` (0–7).
    /// Follows the TF-1201 two-write scheme: low nibble at $B000+2*slot,
    /// high nibble at $B002+2*slot (relative base depends on slot group).
    fn write_chr_bank(mapper: &mut Mapper298, slot: usize, bank: u8) {
        // Compute the base address for this slot group
        // Slots 0–1: $B000-$B003, slots 2–3: $C000-$C003, etc.
        let group_base: u16 = 0xB000 + ((slot as u16 / 2) * 0x1000);
        let pair_offset: u16 = slot as u16 & 0x01; // 0 or 1
        // Low nibble: addr = group_base + pair_offset  (shift=0 since A1=0)
        mapper.write_prg(group_base + pair_offset, bank & 0x0F);
        // High nibble: addr = group_base + pair_offset + 2  (shift=4 since A1=1)
        mapper.write_prg(group_base + pair_offset + 2, (bank >> 4) & 0x0F);
    }

    #[test]
    fn chr_slot0_write_and_read() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        write_chr_bank(&mut mapper, 0, 5);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR slot 0 = bank 5");
    }

    #[test]
    fn chr_slot1_write_and_read() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        write_chr_bank(&mut mapper, 1, 3);
        assert_eq!(mapper.read_chr(0x0400), 3, "CHR slot 1 = bank 3");
    }

    #[test]
    fn chr_slot7_write_and_read() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        write_chr_bank(&mut mapper, 7, 9);
        assert_eq!(
            mapper.read_chr(0x1C00),
            9 % chr_banks as u8,
            "CHR slot 7 = bank 9"
        );
    }

    #[test]
    fn chr_all_slots_independent() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        for slot in 0..8 {
            write_chr_bank(&mut mapper, slot, slot as u8 + 1);
        }
        for slot in 0..8 {
            let ppu_addr = (slot as u16) * 0x0400;
            assert_eq!(
                mapper.read_chr(ppu_addr),
                (slot as u8 + 1) % chr_banks as u8,
                "CHR slot {slot} should be bank {}",
                slot + 1
            );
        }
    }

    #[test]
    fn chr_low_nibble_write_preserves_high() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        // Set full byte first
        write_chr_bank(&mut mapper, 0, 0xAB);
        // Overwrite only low nibble with 0x05
        mapper.write_prg(0xB000, 0x05);
        assert_eq!(
            mapper.chr_regs[0], 0xA5,
            "Low nibble overwrite preserves high nibble"
        );
    }

    #[test]
    fn chr_high_nibble_write_preserves_low() {
        let chr_banks = 16_usize;
        let mut mapper = make_mapper(4, chr_banks);
        write_chr_bank(&mut mapper, 0, 0xAB);
        // Overwrite only high nibble with 0x05
        mapper.write_prg(0xB002, 0x05);
        assert_eq!(
            mapper.chr_regs[0], 0x5B,
            "High nibble overwrite preserves low nibble"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_bit0_one_sets_horizontal() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$9000 bit 0 = 1 → horizontal"
        );
    }

    #[test]
    fn mirroring_bit0_zero_sets_vertical() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0x9000, 0x01); // set horizontal first
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$9000 bit 0 = 0 → vertical"
        );
    }

    // ── IRQ – reload register ─────────────────────────────────────────────────

    #[test]
    fn f000_sets_irq_reload_low_nibble() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF000, 0x0A);
        assert_eq!(
            mapper.irq_reload & 0x0F,
            0x0A,
            "$F000 sets IRQ reload low nibble"
        );
    }

    #[test]
    fn f002_sets_irq_reload_high_nibble() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF002, 0x05);
        assert_eq!(
            mapper.irq_reload & 0xF0,
            0x50,
            "$F002 sets IRQ reload high nibble"
        );
    }

    #[test]
    fn f000_f002_combine_full_reload_byte() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF000, 0x0B); // low nibble = B
        mapper.write_prg(0xF002, 0x0A); // high nibble = A
        assert_eq!(mapper.irq_reload, 0xAB, "Full reload byte = 0xAB");
    }

    // ── IRQ – enable / acknowledge ────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper(4, 8);
        assert!(!mapper.irq_pending(), "IRQ not pending at power-on");
    }

    #[test]
    fn f001_with_bit1_enables_irq_and_loads_counter() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF000, 0x0A); // reload low = A
        mapper.write_prg(0xF002, 0x0B); // reload high = B → reload = 0xBA
        mapper.write_prg(0xF001, 0x02); // enable IRQ (bit 1)
        assert!(
            mapper.irq_enabled,
            "IRQ should be enabled after $F001 write with bit 1"
        );
        assert_eq!(mapper.irq_counter, 0xBA, "Counter loaded from reload");
        assert!(
            !mapper.irq_pending(),
            "IRQ not immediately pending on enable"
        );
    }

    #[test]
    fn f001_without_bit1_disables_irq() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF001, 0x02); // enable
        mapper.write_prg(0xF001, 0x00); // disable
        assert!(
            !mapper.irq_enabled,
            "IRQ should be disabled after $F001 write without bit 1"
        );
    }

    #[test]
    fn f003_acknowledges_irq() {
        let mut mapper = make_mapper(4, 8);
        // Manually set pending
        mapper.irq_pending = true;
        mapper.write_prg(0xF003, 0x00);
        assert!(!mapper.irq_pending(), "IRQ cleared after $F003 write");
    }

    #[test]
    fn f001_clears_pending_irq() {
        let mut mapper = make_mapper(4, 8);
        mapper.irq_pending = true;
        mapper.write_prg(0xF001, 0x02);
        assert!(!mapper.irq_pending(), "$F001 must clear pending IRQ");
    }

    // ── IRQ – CPU-clock counter ───────────────────────────────────────────────

    #[test]
    fn cpu_cycle_does_nothing_when_irq_disabled() {
        let mut mapper = make_mapper(4, 8);
        // With IRQ disabled, counter and scaler stay at 0
        for _ in 0..500 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "No IRQ when disabled");
        assert_eq!(mapper.irq_scaler, 0, "Scaler stays 0 when disabled");
    }

    #[test]
    fn irq_fires_after_counter_overflows_from_255() {
        let mut mapper = make_mapper(4, 8);
        // Set reload so counter starts at 255; one more tick overflows to 0 → IRQ
        mapper.write_prg(0xF000, 0x0F); // low = F
        mapper.write_prg(0xF002, 0x0F); // high = F → reload = 0xFF
        mapper.write_prg(0xF001, 0x02); // enable; counter = 0xFF
        assert_eq!(mapper.irq_counter, 0xFF);

        // Need to advance the scaler by one counter tick:
        // initial scaler = 341; decrements by 3 per cycle; after 114 cycles ≈ 342 dec → triggers
        // after 114 cycles: scaler = 341 - 342 = -1 ≤ 0 → scaler += 341 = 340, counter++
        // counter was 0xFF, now 0x00 → overflow → IRQ
        for _ in 0..115 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ fires when counter overflows from 0xFF to 0"
        );
    }

    #[test]
    fn irq_does_not_fire_before_counter_overflow() {
        let mut mapper = make_mapper(4, 8);
        // reload = 0xFE; counter starts at 0xFE; needs 2 ticks to fire
        mapper.write_prg(0xF000, 0x0E);
        mapper.write_prg(0xF002, 0x0F); // reload = 0xFE
        mapper.write_prg(0xF001, 0x02);
        // Tick once (one counter increment: 0xFE → 0xFF, no overflow yet)
        for _ in 0..115 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire on first counter tick from 0xFE"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper(4, 8);
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "must have IRQ");
        assert!(caps.has_chr_banking, "must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "must have dynamic mirroring");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_prg_layout() {
        let mut mapper = make_mapper(8, 8);
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        mapper.write_prg(0x9001, 0x01);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "slot $8000 = bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "slot $A000 = bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "slot $C000 = second-last after reset"
        );
        assert_eq!(mapper.read_prg(0xE000), 7, "slot $E000 = last after reset");
        assert!(!mapper.swap_prg, "swap_prg cleared after reset");
    }

    #[test]
    fn reset_clears_irq_state() {
        let mut mapper = make_mapper(4, 8);
        mapper.write_prg(0xF001, 0x02);
        mapper.irq_pending = true;
        mapper.reset();
        assert!(!mapper.irq_enabled, "irq_enabled cleared after reset");
        assert!(!mapper.irq_pending(), "irq_pending cleared after reset");
        assert_eq!(mapper.irq_counter, 0, "irq_counter cleared after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper(8, 16);
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        mapper.write_prg(0x9001, 0x01);
        write_chr_bank(&mut mapper, 0, 0xAB);
        write_chr_bank(&mut mapper, 7, 0x05);
        mapper.write_prg(0xF000, 0x09);
        mapper.write_prg(0xF002, 0x06); // reload = 0x69
        mapper.write_prg(0xF001, 0x02);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper(8, 16);
        restored.restore_registers(&snap);

        assert_eq!(
            restored.prg_regs, mapper.prg_regs,
            "prg_regs survive round-trip"
        );
        assert_eq!(
            restored.swap_prg, mapper.swap_prg,
            "swap_prg survives round-trip"
        );
        assert_eq!(
            restored.chr_regs, mapper.chr_regs,
            "chr_regs survive round-trip"
        );
        assert_eq!(
            restored.irq_reload, mapper.irq_reload,
            "irq_reload survives round-trip"
        );
        assert_eq!(
            restored.irq_enabled, mapper.irq_enabled,
            "irq_enabled survives round-trip"
        );
        // PRG reads should match
        assert_eq!(
            restored.read_prg(0x8000),
            6,
            "PRG slot 0 correct in restored"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            3,
            "PRG slot 2 (swap) correct in restored"
        );
    }

    #[test]
    fn restore_with_truncated_snapshot_does_not_panic() {
        let mut mapper = make_mapper(4, 8);
        mapper.restore_registers(&[0u8; 4]);
        // Should not panic; state may be partial
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_writable_when_no_chr_rom() {
        let mut mapper = Mapper298::new(
            MapperContext::new_for_test(
                Mapper298::MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, 4),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM"
        );
    }
}
