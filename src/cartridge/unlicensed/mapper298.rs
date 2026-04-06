//! Mapper 298 – TF-1201
//!
//! Specifications:
//! - Primary source: NesDev wiki (unavailable; Cloudflare-protected)
//! - Fallback source: Mesen2 `Core/NES/Mappers/Ntdec/Tf1201.h`
//!   Source: <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Ntdec/Tf1201.h>
//!
//! Hardware summary:
//! - PCB: TF-1201 (NTDEC)
//! - PRG-ROM: Up to 2 MiB (4 × 8 KiB switchable windows, with optional swap of windows 0 and 2)
//! - CHR: Up to 2 MiB (8 × 1 KiB switchable windows, nibble-pair writes)
//! - Mirroring: Programmable (vertical / horizontal) via $9000 bit 0
//! - IRQ: VRC-style CPU-clock-based IRQ counter (8-bit counter with 341-step scaler)
//!
//! Register map:
//! ```text
//! $8000 – PRG bank A (8 KiB at slot 0 or slot 2 when swap active)
//! $9000 – Mirroring: bit 0 = 0 → Vertical, 1 → Horizontal
//! $9001 – PRG swap: bits 1:0 non-zero → swap slots 0/2
//! $A000 – PRG bank B (8 KiB at slot 1, always)
//! $B000–$E003 – CHR banks (two nibble writes per 1 KiB slot; see below)
//! $F000 – IRQ reload low nibble: bits 3:0
//! $F001 – IRQ control: bit 1 = enable (also loads counter and resets scaler)
//! $F002 – IRQ reload high nibble: bits 3:0 → reload bits 7:4
//! $F003 – IRQ acknowledge (clears pending)
//! ```
//!
//! PRG layout:
//! ```text
//!   Slot 0 ($8000–$9FFF): prg_regs[0] or fixed second-to-last when swap active
//!   Slot 1 ($A000–$BFFF): prg_regs[1] (always switchable)
//!   Slot 2 ($C000–$DFFF): fixed second-to-last or prg_regs[0] when swap active
//!   Slot 3 ($E000–$FFFF): always fixed last bank
//! ```
//!
//! CHR nibble addressing:
//! The hardware decodes CHR registers with an address transformation:
//! `addr = (addr & 0xF003) | ((addr & 0x0C) >> 2)`
//! Then if in $B000–$E003:
//! - slot  = `(((addr >> 11) - 6) | (addr & 0x01)) & 0x07`
//! - shift = `(addr & 0x02) << 1` (0 for low nibble, 4 for high nibble)
//! - `chr_regs[slot] = (chr_regs[slot] & (0xF0 >> shift)) | ((value & 0x0F) << shift)`
//!
//! IRQ behavior (VRC-style):
//! - On enable: scaler = 341, counter = reload value
//! - Per CPU clock: scaler -= 3; if scaler ≤ 0 { scaler += 341; counter += 1; if counter == 0 { fire } }
//! - $F001 write always clears pending IRQ regardless of enable/disable
//! - $F003 write clears pending IRQ (acknowledge)
//!
//! Known Limitations:
//! - PRG-RAM: Not used by known TF-1201 games; not implemented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 298 – TF-1201 (NTDEC)
pub struct Mapper298 {
    base: BaseMapper,

    /// PRG bank registers: [0] = $8000 register, [1] = $A000 register
    prg_regs: [u8; 2],
    /// When true, slot 0 is fixed to second-to-last bank and slot 2 uses prg_regs[0]
    swap_prg: bool,
    /// CHR bank registers (8 × 1 KiB), each assembled from two nibble writes
    chr_regs: [u8; 8],

    // IRQ state
    /// IRQ reload value (8-bit), loaded into counter when IRQ is enabled
    irq_reload: u8,
    /// IRQ counter (8-bit); fires when it overflows (wraps from 0xFF to 0x00)
    irq_counter: u8,
    /// VRC-style scaler: starts at 341 when enabled, decrements by 3 per CPU clock
    irq_scaler: i16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mapper298 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);

        let mut mapper = Self {
            base,
            prg_regs: [0; 2],
            swap_prg: false,
            chr_regs: [0; 8],
            irq_reload: 0,
            irq_counter: 0,
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
        for i in 0..8usize {
            self.base.select_chr_page(i, self.chr_regs[i] as i16);
        }
    }

    fn write_chr_register(&mut self, raw_addr: u16, value: u8) {
        let addr = (raw_addr & 0xF003) | ((raw_addr & 0x0C) >> 2);
        if (0xB000..=0xE003).contains(&addr) {
            let slot = (((addr >> 11).wrapping_sub(6)) | (addr & 0x01)) & 0x07;
            let shift = (addr & 0x02) << 1;
            let mask = 0xF0u8 >> shift;
            self.chr_regs[slot as usize] =
                (self.chr_regs[slot as usize] & mask) | ((value & 0x0F) << shift);
            self.update_chr();
        }
    }
}

impl Mapper for Mapper298 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let _ = self.base.try_write_prg_ram(addr, value);

        let decoded = (addr & 0xF003) | ((addr & 0x0C) >> 2);

        // CHR registers: $B000–$E003
        if (0xB000..=0xE003).contains(&decoded) {
            self.write_chr_register(addr, value);
            return;
        }

        match decoded & 0xF003 {
            0x8000 => {
                self.prg_regs[0] = value;
                self.update_prg();
            }
            0x9000 => {
                self.base.set_mirroring(if value & 0x01 != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                });
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
                self.irq_reload = (self.irq_reload & 0xF0) | (value & 0x0F);
            }
            0xF001 => {
                self.irq_enabled = (value & 0x02) == 0x02;
                if self.irq_enabled {
                    self.irq_scaler = 341;
                    self.irq_counter = self.irq_reload;
                }
                self.irq_pending = false;
            }
            0xF002 => {
                self.irq_reload = (self.irq_reload & 0x0F) | (value << 4);
            }
            0xF003 => {
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
            self.irq_counter = self.irq_counter.wrapping_add(1);
            if self.irq_counter == 0 {
                self.irq_pending = true;
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0..1]: prg_regs
        // [2]:    swap_prg
        // [3..10]: chr_regs
        // [11]:   irq_reload
        // [12]:   irq_counter
        // [13..14]: irq_scaler (i16 little-endian)
        // [15]:   irq_flags (bit0=enabled, bit1=pending)
        // [16]:   mirroring (NametableLayout snapshot byte)
        let mut v = Vec::with_capacity(17);
        v.extend_from_slice(&self.prg_regs);
        v.push(self.swap_prg as u8);
        v.extend_from_slice(&self.chr_regs);
        v.push(self.irq_reload);
        v.push(self.irq_counter);
        let scaler_bytes = self.irq_scaler.to_le_bytes();
        v.push(scaler_bytes[0]);
        v.push(scaler_bytes[1]);
        v.push((self.irq_enabled as u8) | ((self.irq_pending as u8) << 1));
        v.push(self.base.mirroring().to_snapshot_byte());
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 16 {
            return;
        }
        self.prg_regs.copy_from_slice(&data[0..2]);
        self.swap_prg = data[2] != 0;
        self.chr_regs.copy_from_slice(&data[3..11]);
        self.irq_reload = data[11];
        self.irq_counter = data[12];
        self.irq_scaler = i16::from_le_bytes([data[13], data[14]]);
        self.irq_enabled = (data[15] & 0x01) != 0;
        self.irq_pending = (data[15] & 0x02) != 0;
        if data.len() >= 17 {
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[16]));
        }
        self.update_prg();
        self.update_chr();
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 2];
        self.swap_prg = false;
        self.chr_regs = [0; 8];
        self.irq_reload = 0;
        self.irq_counter = 0;
        self.irq_scaler = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
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

    const PRG_BANKS: usize = 8;
    const CHR_BANKS: usize = 64; // 64 × 1KB to accommodate bank indices up to 0xFF

    fn make_mapper() -> Mapper298 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1 * 1024, CHR_BANKS);
        Mapper298::new(
            MapperContext::new_for_test(298, prg, chr, NametableLayout::Vertical)
                .with_prg_ram_banks(0),
        )
    }

    // ── Factory ───────────────────────────────────────────────────────────────

    #[test]
    fn mapper_298_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                298,
                banked_data(8 * 1024, PRG_BANKS),
                banked_data(1 * 1024, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 298 must be registered in the factory"
        );
    }

    #[test]
    fn mapper_298_reports_irq_capability() {
        let mapper = make_mapper();
        assert!(
            mapper.capabilities().has_irq,
            "Mapper 298 must report has_irq=true"
        );
    }

    // ── PRG banking (default: no swap) ────────────────────────────────────────

    #[test]
    fn prg_slot3_is_always_last_bank() {
        let mapper = make_mapper();
        // Last bank = PRG_BANKS - 1 = 7; bank content = 7 (banked_data pattern)
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 slot 3 must always be last bank"
        );
    }

    #[test]
    fn prg_slot2_defaults_to_second_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 slot 2 must default to second-to-last bank"
        );
    }

    #[test]
    fn prg_slot0_defaults_to_bank0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 slot 0 must default to bank 0"
        );
    }

    #[test]
    fn prg_slot1_defaults_to_bank0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "$A000 slot 1 must default to bank 0"
        );
    }

    #[test]
    fn write_8000_selects_prg_slot0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 write must select PRG slot 0"
        );
        // slot 2 stays fixed at second-to-last
        assert_eq!(mapper.read_prg(0xC000), (PRG_BANKS - 2) as u8);
    }

    #[test]
    fn write_a000_selects_prg_slot1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 write must select PRG slot 1"
        );
    }

    // ── PRG swap mode ─────────────────────────────────────────────────────────

    #[test]
    fn write_9001_nonzero_enables_prg_swap() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3); // prg_regs[0] = 3
        mapper.write_prg(0x9001, 0x01); // enable swap
        // Slot 0 should now be fixed to second-to-last bank
        assert_eq!(
            mapper.read_prg(0x8000),
            (PRG_BANKS - 2) as u8,
            "Slot 0 must be second-to-last when swap active"
        );
        // Slot 2 should now be prg_regs[0] = 3
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "Slot 2 must use prg_regs[0] when swap active"
        );
    }

    #[test]
    fn write_9001_zero_disables_prg_swap() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0x9001, 0x01);
        mapper.write_prg(0x9001, 0x00); // disable swap
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Slot 0 must use prg_regs[0] after swap disabled"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "Slot 2 must be second-to-last after swap disabled"
        );
    }

    #[test]
    fn slot1_unchanged_by_swap() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 2);
        mapper.write_prg(0x9001, 0x03);
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "Slot 1 must not be affected by swap"
        );
    }

    #[test]
    fn slot3_unchanged_by_swap() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9001, 0x03);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Slot 3 must always be last bank regardless of swap"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_all_slots_default_to_bank0() {
        let mut mapper = make_mapper();
        for slot in 0..8usize {
            let addr = 0x0000u16 + (slot as u16 * 0x0400);
            assert_eq!(
                mapper.read_chr(addr),
                0,
                "CHR slot {} must default to bank 0",
                slot
            );
        }
    }

    #[test]
    fn chr_slot0_low_nibble_write_via_b000() {
        let mut mapper = make_mapper();
        // $B000 sets low nibble of chr_regs[0]
        mapper.write_prg(0xB000, 0x05);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "$B000 write must set CHR slot 0 to bank 5"
        );
    }

    #[test]
    fn chr_slot0_high_nibble_write_via_b002() {
        let mut mapper = make_mapper();
        // Write low nibble first (bank 2), then high nibble (bank 0x10)
        mapper.write_prg(0xB000, 0x02); // low nibble = 2
        mapper.write_prg(0xB002, 0x01); // high nibble = 1 → bank = 0x12
        assert_eq!(
            mapper.read_chr(0x0000),
            0x12,
            "$B000/$B002 pair must assemble CHR slot 0 from nibbles"
        );
    }

    #[test]
    fn chr_slot1_via_b001_b003() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB001, 0x07); // low nibble of slot 1
        mapper.write_prg(0xB003, 0x02); // high nibble of slot 1
        assert_eq!(mapper.read_chr(0x0400), 0x27, "CHR slot 1 via $B001/$B003");
    }

    #[test]
    fn chr_slot2_via_c000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x0A); // low nibble of slot 2
        assert_eq!(mapper.read_chr(0x0800), 0x0A, "CHR slot 2 via $C000");
    }

    #[test]
    fn chr_slot6_via_e000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x03); // low nibble of slot 6
        mapper.write_prg(0xE002, 0x01); // high nibble of slot 6 → bank 0x13
        assert_eq!(mapper.read_chr(0x1800), 0x13, "CHR slot 6 via $E000/$E002");
    }

    #[test]
    fn chr_slot7_via_e001() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE001, 0x09); // low nibble of slot 7
        assert_eq!(mapper.read_chr(0x1C00), 0x09, "CHR slot 7 via $E001");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_defaults_to_power_on_value() {
        let mapper = make_mapper();
        // Power-on mirroring comes from iNES header (Vertical in test)
        assert_eq!(mapper.base.mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn write_9000_bit0_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_9000_bit0_clear_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01);
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.base.mirroring(), NametableLayout::Vertical);
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn irq_not_pending_when_disabled() {
        let mut mapper = make_mapper();
        // Write $F001 without bit 1 set → disabled
        mapper.write_prg(0xF001, 0x00);
        for _ in 0..10000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    #[test]
    fn irq_fires_on_counter_overflow() {
        let mut mapper = make_mapper();
        // Set reload = 0xFF so counter starts at 0xFF; first overflow is 1 increment away.
        mapper.write_prg(0xF000, 0x0F); // low nibble = 0xF
        mapper.write_prg(0xF002, 0x0F); // high nibble = 0xF → reload = 0xFF
        mapper.write_prg(0xF001, 0x02); // enable IRQ; counter = 0xFF, scaler = 341

        // Scaler ticks: each CPU cycle decrements scaler by 3.
        // scaler=341; after 114 cycles: 341 - 3*113 = 341 - 339 = 2 > 0; 341 - 342 = -1 ≤ 0 at cycle 114.
        // Actually 341/3 = 113.67, so after 114 cycles scaler ≤ 0 → counter increments to 0x00 → IRQ fires.
        let mut fired = false;
        for _ in 0..500 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(fired, "IRQ must fire after counter overflow");
    }

    #[test]
    fn irq_fires_after_expected_cycles() {
        let mut mapper = make_mapper();
        // reload = 0xFF → counter starts at 0xFF; needs 1 counter increment to overflow
        mapper.write_prg(0xF000, 0x0F);
        mapper.write_prg(0xF002, 0x0F); // reload = 0xFF
        mapper.write_prg(0xF001, 0x02); // enable; counter = 0xFF, scaler = 341

        // scaler starts at 341; decrements by 3 per cycle.
        // Overflow at scaler ≤ 0: cycle 114 (341 - 3*114 = 341 - 342 = -1)
        for _ in 0..113 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before scaler overflow"
        );

        mapper.cpu_cycle(); // cycle 114 triggers scaler overflow → counter wraps to 0 → IRQ
        assert!(mapper.irq_pending(), "IRQ must fire on cycle 114");
    }

    #[test]
    fn irq_reload_low_nibble_via_f000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x07); // low nibble = 7
        mapper.write_prg(0xF002, 0x00); // high nibble = 0 → reload = 0x07
        mapper.write_prg(0xF001, 0x02); // enable; counter = 0x07

        // counter = 7; needs 249 more increments to overflow (0xFF - 0x07 + 1 = 249? No:
        // counter starts at 7, wraps at 256. So needs 256 - 7 = 249 increments.
        // Each increment needs 341/3 ≈ 113.67 cycles → ~28,282 cycles total)
        let mut fired = false;
        for _ in 0..40_000 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(fired, "IRQ must fire eventually with reload=7");
    }

    #[test]
    fn f001_write_clears_irq_pending_when_disabling() {
        let mut mapper = make_mapper();
        // Manually trigger IRQ by firing via overflow
        mapper.write_prg(0xF000, 0x0F);
        mapper.write_prg(0xF002, 0x0F); // reload = 0xFF
        mapper.write_prg(0xF001, 0x02); // enable
        for _ in 0..200 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        // Disable IRQ via $F001 (bit 1 = 0) → must clear pending
        mapper.write_prg(0xF001, 0x00);
        assert!(
            !mapper.irq_pending(),
            "$F001 write must always clear pending IRQ"
        );
    }

    #[test]
    fn f003_write_acknowledges_irq() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x0F);
        mapper.write_prg(0xF002, 0x0F);
        mapper.write_prg(0xF001, 0x02);
        for _ in 0..200 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xF003, 0x00);
        assert!(
            !mapper.irq_pending(),
            "$F003 must acknowledge (clear) pending IRQ"
        );
    }

    // ── Save state ────────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        mapper.write_prg(0x9001, 0x01);
        mapper.write_prg(0xB000, 0x07);
        mapper.write_prg(0xB002, 0x02); // chr_regs[0] = 0x27
        mapper.write_prg(0x9000, 0x01); // horizontal
        mapper.write_prg(0xF000, 0x05);
        mapper.write_prg(0xF002, 0x03); // reload = 0x35

        let snap = mapper.registers_snapshot();
        assert!(!snap.is_empty(), "snapshot must not be empty");

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // PRG banking
        assert_eq!(mapper2.read_prg(0x8000), (PRG_BANKS - 2) as u8); // swap active: slot0=second-to-last
        assert_eq!(mapper2.read_prg(0xC000), 3); // slot2=prg_regs[0]
        assert_eq!(mapper2.read_prg(0xA000), 5); // slot1
        assert_eq!(mapper2.read_prg(0xE000), (PRG_BANKS - 1) as u8); // slot3=last

        // CHR banking
        assert_eq!(mapper2.read_chr(0x0000), 0x27);

        // Mirroring
        assert_eq!(mapper2.base.mirroring(), NametableLayout::Horizontal);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0x9001, 0x01); // swap
        mapper.write_prg(0xF001, 0x02); // enable IRQ

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0, "slot 0 must reset to bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "slot 2 must reset to second-to-last"
        );
        assert!(!mapper.swap_prg, "swap_prg must reset to false");
        assert!(!mapper.irq_enabled, "IRQ must be disabled after reset");
        assert!(!mapper.irq_pending, "IRQ must not be pending after reset");
    }
}
