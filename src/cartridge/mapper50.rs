//! Mapper 050 - N-32 (Romeo / Super Mario Bros. 2 Japanese conversion)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_050>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 050 - N-32 (Romeo / SMB2 Japanese FDS conversion)
///
/// Hardware: 761214 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_050>
/// - PRG-ROM: 128 KiB (8 KiB banks)
/// - PRG-RAM: None
/// - CHR: 8 KiB fixed (ROM or RAM)
/// - Mirroring: Fixed from header
///
/// Registers (range $4020-$5FFF, mask $4120):
/// - $4020: [.... HLLM] PRG bank selector (scrambled)
/// - $4120: [.... ...E] IRQ enable
///
/// PRG window map (8 KiB each):
/// - $6000-$7FFF: Fixed bank $0F
/// - $8000-$9FFF: Fixed bank $08
/// - $A000-$BFFF: Fixed bank $09
/// - $C000-$DFFF: Switchable via $4020
/// - $E000-$FFFF: Fixed bank $0B
///
/// PRG register unscrambling ([HLLM] → bank):
/// - bit 3 (H) → bank bit 3
/// - bit 2 (L_hi) → bank bit 1
/// - bit 1 (L_lo) → bank bit 0
/// - bit 0 (M) → bank bit 2
///
/// IRQ: CPU-cycle counter; fires on the $0FFF→$1000 transition; auto-disables.
/// Disable ($4120 E=0): clear IRQ, reset counter. Enable ($4120 E=1): start counting.
///
/// Known games: Romeo (N-32), Super Mario Bros. 2 (JU) Alt Levels pirate
pub struct Mapper50 {
    base: BaseMapper,
    prg_reg: u8,
    irq_enabled: bool,
    irq_counter: u16,
    irq_pending: bool,
}

impl Mapper50 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB

    const BANK_AT_6000: usize = 0x0F;

    // IRQ fires on the $0FFF→$1000 transition (after 4096 cycles)
    const IRQ_FIRE_COUNT: u16 = 0x1000;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_reg: 0,
            irq_enabled: false,
            irq_counter: 0,
            irq_pending: false,
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // $8000=0x08, $A000=0x09, $C000=switchable, $E000=0x0B
        self.base.select_prg_page(0, 0x08);
        self.base.select_prg_page(1, 0x09);
        self.base
            .select_prg_page(2, Self::unscramble_prg(self.prg_reg) as i16);
        self.base.select_prg_page(3, 0x0B);
    }

    fn read_prg_6000(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        let bank_count = prg.len() / Self::PRG_BANK_SIZE;
        if bank_count == 0 {
            return 0;
        }
        let bank = Self::BANK_AT_6000 % bank_count;
        let offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);
        prg.get(bank * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    /// Unscramble the PRG register [HLLM] to get the actual 4-bit bank number.
    /// bit3=H → bank_bit3, bit2=L_hi → bank_bit1, bit1=L_lo → bank_bit0, bit0=M → bank_bit2
    fn unscramble_prg(reg: u8) -> usize {
        ((reg & 0x08) as usize)          // H  → bit 3
            | (((reg & 0x01) << 2) as usize) // M  → bit 2
            | (((reg & 0x06) >> 1) as usize) // LL → bits 1:0
    }
}

impl Mapper for Mapper50 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_6000(addr),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Registers are only in $4020-$5FFF; ignore writes to PRG ROM/RAM space.
        // Without this guard, writes to $C000-$FFFF (e.g. from RMW instructions)
        // with bit14=1, bit5=1, bit8=0 would accidentally match $4020 via the mask.
        if addr > 0x5FFF {
            return;
        }
        match addr & 0x4120 {
            0x4020 => {
                // PRG bank register (scrambled [HLLM])
                self.prg_reg = value & 0x0F;
                self.update_banks();
            }
            0x4120 => {
                // IRQ control
                if (value & 0x01) != 0 {
                    self.irq_enabled = true;
                } else {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                    self.irq_counter = 0;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base.write_chr(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.base.mirroring()
    }

    fn mapper_number(&self) -> u8 {
        self.base.mapper_number()
    }

    fn wram_size(&self) -> usize {
        self.base.wram_size()
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        self.irq_counter = self.irq_counter.wrapping_add(1);
        if self.irq_counter == Self::IRQ_FIRE_COUNT {
            self.irq_pending = true;
            self.irq_enabled = false; // auto-disable on fire
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.base.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.base.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.base.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        vec![
            self.prg_reg,
            flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.prg_reg = data[0];
            self.irq_enabled = (data[1] & 1) != 0;
            self.irq_pending = (data[1] & 2) != 0;
            self.irq_counter = (data[2] as u16) | ((data[3] as u16) << 8);
            self.update_banks();
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.base.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper50;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 19 banks × 8 KiB = 152 KiB (non-power-of-two)
    const PRG_BANKS: usize = 19;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        create_mapper(MapperContext::new_for_test(
            50,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 50 should be implemented")
    }

    fn make_mapper_direct() -> Mapper50 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 1);
        Mapper50::new(MapperContext::new_for_test(
            50,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // --- Factory ---

    #[test]
    fn mapper_50_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            50,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(8 * 1024, 1),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 50 must be registered in the factory"
        );
    }

    // --- PRG fixed banks ---

    #[test]
    fn prg_6000_is_fixed_bank_0x0f() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0x0F % PRG_BANKS as u8,
            "$6000 window must read from fixed bank $0F"
        );
    }

    #[test]
    fn prg_8000_is_fixed_bank_0x08() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0x08,
            "$8000 window must read from fixed bank $08"
        );
    }

    #[test]
    fn prg_a000_is_fixed_bank_0x09() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0x09,
            "$A000 window must read from fixed bank $09"
        );
    }

    #[test]
    fn prg_e000_is_fixed_bank_0x0b() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            0x0B,
            "$E000 window must read from fixed bank $0B"
        );
    }

    // --- PRG switchable bank ($C000) ---

    #[test]
    fn prg_c000_defaults_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must default to bank 0");
    }

    #[test]
    fn prg_c000_selects_bank_via_4020_register() {
        let mut mapper = make_mapper();
        // Reg=2 (0010): H=0, L_hi=0, L_lo=1, M=0 → bank bit1=0, bit0=1 → bank=1
        mapper.write_prg(0x4020, 0x02);
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$4020=2 must select bank 1 at $C000"
        );
    }

    #[test]
    fn prg_register_unscrambling_bit0_m_maps_to_bit2() {
        let mut mapper = make_mapper();
        // reg=1 (0001): M=1 → bank bit2 set → bank=4
        mapper.write_prg(0x4020, 0x01);
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$4020=1 (M bit) must map to bank 4 (bit2)"
        );
    }

    #[test]
    fn prg_register_unscrambling_bit3_h_maps_to_bit3() {
        let mut mapper = make_mapper();
        // reg=8 (1000): H=1 → bank bit3 set → bank=8
        mapper.write_prg(0x4020, 0x08);
        assert_eq!(
            mapper.read_prg(0xC000),
            8,
            "$4020=8 (H bit) must map to bank 8 (bit3)"
        );
    }

    #[test]
    fn prg_register_only_uses_low_nibble() {
        let mut mapper = make_mapper();
        // High nibble must be ignored
        mapper.write_prg(0x4020, 0xF2); // low nibble = 2 → bank 1
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_initially() {
        let mapper = make_mapper_direct();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper_direct();
        for _ in 0..0x2000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_fires_after_4096_cycles_when_enabled() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn irq_does_not_fire_before_4096_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x0FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles"
        );
    }

    #[test]
    fn irq_auto_disables_when_it_fires() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        // After firing, counter should not advance further
        let count_before = mapper.irq_counter;
        mapper.cpu_cycle();
        assert_eq!(
            mapper.irq_counter,
            count_before.wrapping_add(0), // counter stopped
            "Counter must stop after IRQ fires"
        );
    }

    #[test]
    fn irq_acknowledged_and_reset_by_writing_4120_with_e_clear() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4120, 0x00); // disable + ack + reset
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_counter_resets_on_disable() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4120, 0x01); // enable
        for _ in 0..0x1000 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x4120, 0x00); // disable + reset
        mapper.write_prg(0x4120, 0x01); // re-enable
        for _ in 0..0x0FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire until 4096 cycles after re-enable"
        );
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4020, 0x04); // prg_reg=4
        mapper.write_prg(0x4120, 0x01); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored PRG bank must match"
        );
    }
}
