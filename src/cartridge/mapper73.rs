//! Mapper 073 - Konami VRC3
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/VRC3>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 073 - Konami VRC3
///
/// Hardware: Konami VRC3 ASIC
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/VRC3>
/// - CPU $6000-$7FFF: optional 8KB PRG RAM
/// - CPU $8000-$BFFF: 16KB switchable PRG ROM bank
/// - CPU $C000-$FFFF: 16KB PRG ROM, fixed to last bank
/// - CHR: 8KB CHR-RAM (no CHR-ROM banking)
/// - Mirroring: Fixed from header (not programmable)
/// - IRQ: 16-bit CPU-cycle counter with 8-bit mode option
///
/// Register map:
/// - $8000-$8FFF: IRQ latch bits 3:0
/// - $9000-$9FFF: IRQ latch bits 7:4
/// - $A000-$AFFF: IRQ latch bits 11:8
/// - $B000-$BFFF: IRQ latch bits 15:12
/// - $C000-$CFFF: IRQ Control [.... .MEA]
/// - $D000-$DFFF: IRQ Acknowledge
/// - $F000-$FFFF: PRG bank select [.... .PPP]
pub struct Mapper73 {
    base: BaseMapper,
    prg_ram: Vec<u8>,
    prg_bank: u8,
    irq_latch: u16,
    irq_counter: u16,
    irq_mode_8bit: bool,
    pub(crate) irq_enable: bool,
    irq_enable_on_ack: bool,
    irq_pending: bool,
}

impl Mapper73 {
    const PRG_BANK_SIZE: usize = 0x4000; // 16 KiB
    const PRG_RAM_SIZE: usize = 0x2000; // 8 KiB

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let prg_ram_size = (ctx.prg_ram_banks_8k as usize).max(1) * Self::PRG_RAM_SIZE;
        let capabilities = MapperCapabilities {
            has_irq: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 16,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_ram: vec![0u8; prg_ram_size],
            prg_bank: 0,
            irq_latch: 0,
            irq_counter: 0,
            irq_mode_8bit: false,
            irq_enable: false,
            irq_enable_on_ack: false,
            irq_pending: false,
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, -1); // $C000 fixed last
    }

    fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
        self.irq_enable = self.irq_enable_on_ack;
    }

    fn tick_counter(&mut self) {
        if self.irq_mode_8bit {
            let low = (self.irq_counter & 0x00FF) as u8;
            let (new_low, overflow) = low.overflowing_add(1);
            self.irq_counter = (self.irq_counter & 0xFF00) | (new_low as u16);
            if overflow {
                self.irq_pending = true;
                self.irq_counter = (self.irq_counter & 0xFF00) | (self.irq_latch & 0x00FF);
            }
        } else {
            let (new_counter, overflow) = self.irq_counter.overflowing_add(1);
            self.irq_counter = new_counter;
            if overflow {
                self.irq_pending = true;
                self.irq_counter = self.irq_latch;
            }
        }
    }
}

impl Mapper for Mapper73 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let idx = (addr as usize) - 0x6000;
                self.prg_ram.get(idx).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let idx = (addr as usize) - 0x6000;
                if let Some(b) = self.prg_ram.get_mut(idx) {
                    *b = value;
                }
            }
            0x8000..=0x8FFF => {
                self.irq_latch = (self.irq_latch & 0xFFF0) | ((value as u16) & 0x000F);
            }
            0x9000..=0x9FFF => {
                self.irq_latch = (self.irq_latch & 0xFF0F) | (((value as u16) & 0x000F) << 4);
            }
            0xA000..=0xAFFF => {
                self.irq_latch = (self.irq_latch & 0xF0FF) | (((value as u16) & 0x000F) << 8);
            }
            0xB000..=0xBFFF => {
                self.irq_latch = (self.irq_latch & 0x0FFF) | (((value as u16) & 0x000F) << 12);
            }
            0xC000..=0xCFFF => {
                // IRQ Control — also acknowledges pending IRQ
                self.irq_enable_on_ack = (value & 0x01) != 0;
                self.irq_mode_8bit = (value & 0x04) != 0;
                self.irq_pending = false;
                let enable = (value & 0x02) != 0;
                self.irq_enable = enable;
                if enable {
                    self.irq_counter = self.irq_latch;
                }
            }
            0xD000..=0xDFFF => {
                // IRQ Acknowledge
                self.acknowledge_irq();
            }
            0xF000..=0xFFFF => {
                self.prg_bank = value & 0x07;
                self.update_banks();
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
        Self::PRG_RAM_SIZE
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enable {
            return;
        }
        self.tick_counter();
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
        crate::console::initialize_ram(&mut self.prg_ram, mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.base.capabilities()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0]     prg_bank
        // [1-2]   irq_latch (little-endian)
        // [3-4]   irq_counter (little-endian)
        // [5]     flags: irq_mode_8bit | irq_enable<<1 | irq_enable_on_ack<<2 | irq_pending<<3
        let flags = (self.irq_mode_8bit as u8)
            | ((self.irq_enable as u8) << 1)
            | ((self.irq_enable_on_ack as u8) << 2)
            | ((self.irq_pending as u8) << 3);
        vec![
            self.prg_bank,
            (self.irq_latch & 0xFF) as u8,
            (self.irq_latch >> 8) as u8,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
            flags,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.prg_bank = data[0];
            self.irq_latch = (data[1] as u16) | ((data[2] as u16) << 8);
            self.irq_counter = (data[3] as u16) | ((data[4] as u16) << 8);
            self.irq_mode_8bit = (data[5] & 0x01) != 0;
            self.irq_enable = (data[5] & 0x02) != 0;
            self.irq_enable_on_ack = (data[5] & 0x04) != 0;
            self.irq_pending = (data[5] & 0x08) != 0;
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use a non-power-of-two bank count to prevent modulo-wrapping false-passes
    const PRG_BANKS: usize = 3; // 3 × 16KB = 48KB

    fn make_mapper() -> Mapper73 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        Mapper73::new(MapperContext::new_for_test(
            73,
            prg,
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_73_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            73,
            banked_data(16 * 1024, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 73 must be registered");
    }

    // -----------------------------------------------------------------------
    // Power-on PRG state
    // -----------------------------------------------------------------------

    #[test]
    fn power_on_prg_bank_at_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must start at bank 0");
    }

    #[test]
    fn prg_c000_is_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 must always map to last bank"
        );
    }

    // -----------------------------------------------------------------------
    // PRG bank switching
    // -----------------------------------------------------------------------

    #[test]
    fn prg_bank_select_via_f000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must switch to bank 1 after writing 1 to $F000"
        );
        // Fixed window must remain last bank
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 must still be last bank after bank switch"
        );
    }

    // -----------------------------------------------------------------------
    // PRG RAM
    // -----------------------------------------------------------------------

    #[test]
    fn prg_ram_readable_and_writable_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCD);
    }

    // -----------------------------------------------------------------------
    // CHR RAM
    // -----------------------------------------------------------------------

    #[test]
    fn chr_ram_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0x42);
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        mapper.write_chr(0x1FFF, 0x77);
        assert_eq!(mapper.read_chr(0x1FFF), 0x77);
    }

    // -----------------------------------------------------------------------
    // IRQ — basic state
    // -----------------------------------------------------------------------

    #[test]
    fn irq_not_pending_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    // -----------------------------------------------------------------------
    // IRQ — 16-bit mode
    // -----------------------------------------------------------------------

    #[test]
    fn irq_fires_after_n_cycles_in_16bit_mode() {
        let mut mapper = make_mapper();
        // Set latch = 0xFFFD → overflow in 3 cycles (FFFD → FFFE → FFFF → 0000 triggers)
        mapper.write_prg(0x8000, 0x0D); // bits 3:0 = D
        mapper.write_prg(0x9000, 0x0F); // bits 7:4 = F  → lower byte = 0xFD
        mapper.write_prg(0xA000, 0x0F); // bits 11:8 = F
        mapper.write_prg(0xB000, 0x0F); // bits 15:12 = F → latch = 0xFFFD
        // Enable IRQ with E=1 (reloads counter from latch = 0xFFFD)
        mapper.write_prg(0xC000, 0x02); // E=1, A=0, M=0
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // counter: 0xFFFE
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // counter: 0xFFFF
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // overflows → IRQ fires
        assert!(mapper.irq_pending(), "IRQ must fire after 3 cycles");
    }

    // -----------------------------------------------------------------------
    // IRQ — 8-bit mode
    // -----------------------------------------------------------------------

    #[test]
    fn irq_fires_faster_in_8bit_mode() {
        let mut mapper = make_mapper();
        // Latch lower byte = 0xFD, upper byte = 0x00
        mapper.write_prg(0x8000, 0x0D); // bits 3:0 = D
        mapper.write_prg(0x9000, 0x0F); // bits 7:4 = F → lower byte of latch = 0xFD
        mapper.write_prg(0xA000, 0x00);
        mapper.write_prg(0xB000, 0x00);
        // Enable with M=1 (8-bit) and E=1
        mapper.write_prg(0xC000, 0x06); // E=1, M=1
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // 0xFE
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // 0xFF
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // overflow at 0x00 → IRQ
        assert!(
            mapper.irq_pending(),
            "IRQ must fire in 8-bit mode after 3 cycles"
        );
    }

    // -----------------------------------------------------------------------
    // IRQ — acknowledge ($D000)
    // -----------------------------------------------------------------------

    #[test]
    fn irq_acknowledge_via_d000() {
        let mut mapper = make_mapper();
        // Latch = 0xFFFE → fires in 2 cycles
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0x9000, 0x0F);
        mapper.write_prg(0xA000, 0x0F);
        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xC000, 0x02); // E=1
        mapper.cpu_cycle();
        mapper.cpu_cycle(); // IRQ fires
        assert!(mapper.irq_pending());
        mapper.write_prg(0xD000, 0); // acknowledge
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after $D000 write"
        );
    }

    // -----------------------------------------------------------------------
    // IRQ — A bit (enable-on-ack) via $D000
    // -----------------------------------------------------------------------

    #[test]
    fn irq_re_enable_via_d000_using_a_bit() {
        let mut mapper = make_mapper();
        // Set latch = 0xFFFE, A=1, E=1
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0x9000, 0x0F);
        mapper.write_prg(0xA000, 0x0F);
        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xC000, 0x03); // E=1, A=1
        mapper.cpu_cycle();
        mapper.cpu_cycle(); // IRQ fires
        assert!(mapper.irq_pending());
        // After ack via $D000, A bit moves into E
        mapper.write_prg(0xD000, 0);
        assert!(!mapper.irq_pending());
        assert!(mapper.irq_enable, "E must be set from A after $D000 ack");
    }

    // -----------------------------------------------------------------------
    // IRQ — reload on $C000 with E set
    // -----------------------------------------------------------------------

    #[test]
    fn irq_reloads_on_c000_with_e_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05); // latch low nibble = 5
        mapper.write_prg(0x9000, 0x00);
        mapper.write_prg(0xA000, 0x00);
        mapper.write_prg(0xB000, 0x00); // latch = 0x0005
        mapper.write_prg(0xC000, 0x02); // E=1 → reload counter = 0x0005
        assert_eq!(
            mapper.irq_counter, 0x0005,
            "counter must be loaded from latch"
        );
    }

    // -----------------------------------------------------------------------
    // IRQ — counter stops when disabled
    // -----------------------------------------------------------------------

    #[test]
    fn irq_counter_not_incremented_when_disabled() {
        let mut mapper = make_mapper();
        // Do NOT enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        assert_eq!(
            mapper.irq_counter, 0,
            "Counter must not increment when disabled"
        );
        assert!(!mapper.irq_pending());
    }

    // -----------------------------------------------------------------------
    // Snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trips() {
        let mut original = make_mapper();
        original.write_prg(0xF000, 2); // prg_bank = 2
        original.write_prg(0x8000, 0x0A); // latch bits 3:0
        original.write_prg(0x9000, 0x0B); // latch bits 7:4
        original.write_prg(0xA000, 0x0C); // latch bits 11:8
        original.write_prg(0xB000, 0x0D); // latch bits 15:12 → latch = 0xDCBA
        original.write_prg(0xC000, 0x07); // E=1, A=1, M=1 → counter reloaded
        // Tick once so counter differs from latch
        original.cpu_cycle();

        let snap = original.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_bank, original.prg_bank);
        assert_eq!(restored.irq_latch, original.irq_latch);
        assert_eq!(restored.irq_counter, original.irq_counter);
        assert_eq!(restored.irq_mode_8bit, original.irq_mode_8bit);
        assert_eq!(restored.irq_enable, original.irq_enable);
        assert_eq!(restored.irq_enable_on_ack, original.irq_enable_on_ack);
        assert_eq!(restored.irq_pending, original.irq_pending);
    }
}
