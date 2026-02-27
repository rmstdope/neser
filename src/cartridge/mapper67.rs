//! Mapper 067 – Sunsoft 3
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_067>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 067 – Sunsoft 3
///
/// Hardware: Sunsoft 3 ASIC
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_067>
/// - PRG-ROM: Up to 256 KiB (16 × 16 KiB banks)
/// - CHR: Up to 256 KiB (128 × 2 KiB banks)
/// - Mirroring: Programmable (H/V/1-screen-A/1-screen-B)
/// - IRQ: 16-bit CPU-cycle down-counter; wraps $0000→$FFFF and fires once.
///
/// PRG layout:
/// ```text
///   $8000           $C000           $FFFF
/// +---------------+---------------+
/// |  switchable   |   fixed last  |
/// |   16KB bank   |   16KB bank   |
/// +---------------+---------------+
/// ```
///
/// CHR layout:
/// ```text
///   $0000   $0800   $1000   $1800   $1FFF
/// +-------+-------+-------+-------+
/// | chr0  | chr1  | chr2  | chr3  |
/// +-------+-------+-------+-------+
/// ```
///
/// Registers (decoded as `addr & 0xF800`):
/// - `$8000`: IRQ acknowledge (any write)
/// - `$8800`: CHR bank 0 (2KB @ PPU $0000)
/// - `$9800`: CHR bank 1 (2KB @ PPU $0800)
/// - `$A800`: CHR bank 2 (2KB @ PPU $1000)
/// - `$B800`: CHR bank 3 (2KB @ PPU $1800)
/// - `$C800`: IRQ counter (write-twice: high byte first, then low byte)
/// - `$D800`: IRQ enable [.... E...] bit 4; also resets write-twice toggle
/// - `$E800`: Mirroring [.... ..MM] 0=Vert 1=Horz 2=1-screenA 3=1-screenB
/// - `$F800`: PRG bank [.... PPPP] (16KB bank at $8000)
pub struct Mapper67 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    prg_bank: u8,
    chr_banks: [u8; 4],
    irq_enabled: bool,
    irq_pending: bool,
    irq_counter: u16,
    irq_write_hi_next: bool,
}

impl Mapper67 {
    const MAPPER_NUMBER: u8 = 67;
    const PRG_BANK_SIZE: usize = 0x4000; // 16 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0800; // 2 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            mirroring: NametableLayout::Vertical,
            prg_bank: 0,
            chr_banks: [0; 4],
            irq_enabled: false,
            irq_pending: false,
            irq_counter: 0,
            irq_write_hi_next: true,
        }
    }

    fn num_prg_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn read_prg_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.num_prg_banks();
        if count == 0 {
            return 0;
        }
        let b = bank % count;
        self.prg_rom
            .get(b * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn num_chr_banks(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
    }
}

impl Mapper for Mapper67 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.read_prg_bank(self.prg_bank as usize, offset)
            }
            0xC000..=0xFFFF => {
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                let last = self.num_prg_banks().saturating_sub(1);
                self.read_prg_bank(last, offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xF800 {
            0x8000 => {
                // IRQ acknowledge
                self.irq_pending = false;
            }
            0x8800 => self.chr_banks[0] = value,
            0x9800 => self.chr_banks[1] = value,
            0xA800 => self.chr_banks[2] = value,
            0xB800 => self.chr_banks[3] = value,
            0xC800 => {
                if self.irq_write_hi_next {
                    self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                    self.irq_write_hi_next = false;
                } else {
                    self.irq_counter = (self.irq_counter & 0xFF00) | (value as u16);
                    self.irq_write_hi_next = true;
                }
            }
            0xD800 => {
                self.irq_enabled = (value & 0x10) != 0;
                self.irq_write_hi_next = true;
            }
            0xE800 => {
                self.mirroring = match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    _ => NametableLayout::SingleScreenUpper,
                };
            }
            0xF800 => {
                self.prg_bank = value & 0x0F;
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let slot = (addr as usize) / Self::CHR_BANK_SIZE;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        let count = self.num_chr_banks();
        if count == 0 {
            return self.chr_memory.read(addr);
        }
        let bank = (self.chr_banks[slot & 3] as usize) % count;
        self.chr_memory
            .read_at_index(bank * Self::CHR_BANK_SIZE + offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        if self.irq_counter == 0 {
            self.irq_pending = true;
            self.irq_enabled = false;
            return;
        }
        self.irq_counter -= 1;
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.mirroring {
            NametableLayout::Vertical => 0u8,
            NametableLayout::Horizontal => 1,
            NametableLayout::SingleScreenLower => 2,
            NametableLayout::SingleScreenUpper => 3,
            _ => 0,
        };
        let irq_flags = (self.irq_enabled as u8)
            | ((self.irq_pending as u8) << 1)
            | ((self.irq_write_hi_next as u8) << 2);
        let mut v = vec![
            self.prg_bank,
            mirror_byte,
            irq_flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
        ];
        v.extend_from_slice(&self.chr_banks);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 9 {
            return;
        }
        self.prg_bank = data[0];
        self.mirroring = match data[1] {
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreenLower,
            3 => NametableLayout::SingleScreenUpper,
            _ => NametableLayout::Vertical,
        };
        self.irq_enabled = (data[2] & 1) != 0;
        self.irq_pending = (data[2] & 2) != 0;
        self.irq_write_hi_next = (data[2] & 4) != 0;
        self.irq_counter = (data[3] as u16) | ((data[4] as u16) << 8);
        self.chr_banks.copy_from_slice(&data[5..9]);
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_banks = [0; 4];
        self.mirroring = NametableLayout::Vertical;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_counter = 0;
        self.irq_write_hi_next = true;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 2,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16; // non-power-of-two not needed for 16KB banks
    const CHR_BANKS: usize = 3; // non-power-of-two to avoid modulo false-passes

    fn make_mapper() -> Mapper67 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_BANKS);
        Mapper67::new(MapperContext::new_for_test(
            67,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_67_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            67,
            banked_data(16 * 1024, PRG_BANKS),
            banked_data(2 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 67 must be registered");
    }

    // --- Power-on PRG ---

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must start at PRG bank 0");
    }

    #[test]
    fn prg_c000_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 must be fixed to last PRG bank"
        );
    }

    #[test]
    fn prg_bank_switched_via_f800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF800, 5);
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 must reflect PRG bank 5");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 fixed bank must not change"
        );
    }

    // --- CHR banking ---

    #[test]
    fn chr_2k_bank0_at_0000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8800, 1); // CHR bank 0 → 2KB bank 1
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x07FF), 1);
    }

    #[test]
    fn chr_2k_bank1_at_0800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9800, 2); // CHR bank 1 → 2KB bank 2
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0FFF), 2);
    }

    #[test]
    fn chr_2k_bank2_at_1000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA800, 1); // CHR bank 2 → 2KB bank 1
        assert_eq!(mapper.read_chr(0x1000), 1);
        assert_eq!(mapper.read_chr(0x17FF), 1);
    }

    #[test]
    fn chr_2k_bank3_at_1800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB800, 2); // CHR bank 3 → 2KB bank 2
        assert_eq!(mapper.read_chr(0x1800), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical_via_e800_value_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE800, 1); // first set to something else
        mapper.write_prg(0xE800, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_via_e800_value_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE800, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_single_screen_lower_via_e800_value_2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE800, 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn mirroring_single_screen_upper_via_e800_value_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE800, 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn irq_fires_after_n_cycles_counting_down_from_set_value() {
        // Set counter to 0 via write-twice: hi=0x00, lo=0x00
        // With counter=0 and enabled: first cpu_cycle() triggers $0000→$FFFF wrap → IRQ fires
        let mut mapper = make_mapper();
        mapper.write_prg(0xC800, 0x00); // high byte
        mapper.write_prg(0xC800, 0x00); // low byte → counter = 0
        mapper.write_prg(0xD800, 0x10); // enable IRQ
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before first cycle"
        );
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must fire on first cycle when counter=0"
        );
    }

    #[test]
    fn irq_disables_itself_after_firing() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC800, 0x00);
        mapper.write_prg(0xC800, 0x00);
        mapper.write_prg(0xD800, 0x10);
        mapper.cpu_cycle(); // IRQ fires
        assert!(mapper.irq_pending());
        // Additional cycles must not interfere (IRQ disabled after firing)
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        // irq_enabled should be false; counter should stay at 0xFFFF
        assert!(!mapper.irq_enabled, "IRQ must disable itself after firing");
    }

    #[test]
    fn irq_acknowledge_via_8000_clears_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC800, 0x00);
        mapper.write_prg(0xC800, 0x00);
        mapper.write_prg(0xD800, 0x10);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
        mapper.write_prg(0x8000, 0x00); // acknowledge
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after write to $8000"
        );
    }

    #[test]
    fn irq_counter_loaded_high_then_low_via_c800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC800, 0x12); // high byte = 0x12
        mapper.write_prg(0xC800, 0x34); // low byte  = 0x34
        assert_eq!(
            mapper.irq_counter, 0x1234,
            "Counter must be 0x1234 after two writes"
        );
    }

    #[test]
    fn c800_write_toggle_resets_on_d800_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC800, 0xAB); // high byte
        // Write to $D800 resets the toggle
        mapper.write_prg(0xD800, 0x00);
        // Now next $C800 write should be the high byte again
        mapper.write_prg(0xC800, 0x56); // high byte (again)
        mapper.write_prg(0xC800, 0x78); // low byte
        assert_eq!(
            mapper.irq_counter, 0x5678,
            "Toggle must reset after $D800 write"
        );
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF800, 7); // PRG bank 7
        mapper.write_prg(0x8800, 1); // CHR bank 0 → 1
        mapper.write_prg(0x9800, 2); // CHR bank 1 → 2
        mapper.write_prg(0xA800, 0); // CHR bank 2 → 0
        mapper.write_prg(0xB800, 1); // CHR bank 3 → 1
        mapper.write_prg(0xE800, 1); // Horizontal mirroring
        mapper.write_prg(0xC800, 0xAB); // IRQ counter hi
        mapper.write_prg(0xC800, 0xCD); // IRQ counter lo
        mapper.write_prg(0xD800, 0x10); // enable IRQ

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
        assert_eq!(restored.irq_counter, mapper.irq_counter);
        assert_eq!(restored.irq_enabled, mapper.irq_enabled);
        assert_eq!(restored.chr_banks, mapper.chr_banks);
    }
}
