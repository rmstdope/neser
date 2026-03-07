//! Mapper 85 - Konami VRC7
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/VRC7>
//! - Audio: <https://www.nesdev.org/wiki/VRC7_audio>
//!
//! Known Limitations:
//! - FM audio (YM2413/OPLL) is stubbed with silence. Full YM2413 emulation
//!   is out of scope for this implementation.

use crate::cartridge::BaseMapper;
use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::vrc_irq::VrcIrq;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 85 - Konami VRC7 (FM audio stubbed with silence)
///
/// Hardware: Konami VRC7
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/VRC7>
/// - IRQ: <https://www.nesdev.org/wiki/VRC_IRQ>
/// - PRG-ROM: Up to 512KB (three 8KB switchable banks + one fixed last bank)
/// - PRG-RAM: 8KB at $6000-$7FFF (enabled by $E000 bit 7)
/// - CHR: Up to 256KB (eight 1KB switchable banks) or CHR-RAM
/// - Mirroring: Programmable (vertical, horizontal, one-screen A/B) via $E000 bits[1:0]
/// - Expansion audio: YM2413/OPLL FM (stubbed with silence)
///
/// Register map:
/// - $8000: PRG bank 0 (8KB at $8000) — bits[5:0]
/// - $8008/$8010: PRG bank 1 (8KB at $A000) — bits[5:0]
/// - $9000: PRG bank 2 (8KB at $C000) — bits[5:0]
/// - $9010: FM audio address register (stubbed)
/// - $9030: FM audio data register (stubbed)
/// - $A000/$A008: CHR bank 0/1 (1KB)
/// - $B000/$B008: CHR bank 2/3 (1KB)
/// - $C000/$C008: CHR bank 4/5 (1KB)
/// - $D000/$D008: CHR bank 6/7 (1KB)
/// - $E000: Control (bit 7=PRG-RAM enable, bit 6=FM mute, bits[1:0]=mirroring)
/// - $E008: IRQ latch
/// - $F000: IRQ control
/// - $F008: IRQ acknowledge
///
/// Notes:
/// - Address bit 4 is remapped to bit 3 (for PCB pin compatibility), except for $9010
/// - CPU-cycle driven IRQ (uses VrcIrq)
pub struct VRC7Mapper {
    base: BaseMapper,
    prg_ram: PrgRam,

    prg_banks_8k: [u8; 3],
    chr_banks_1k: [u8; 8],

    control: u8,

    irq: VrcIrq,
}

impl VRC7Mapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);

        let mut mapper = Self {
            base,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_banks_8k: [0; 3],
            chr_banks_1k: [0; 8],
            control: 0,
            irq: VrcIrq::new(341, 3),
        };
        mapper.base.set_mirroring(mirroring);
        mapper.update_banks();
        mapper
    }

    /// Normalize VRC7 register address.
    ///
    /// On VRC7 boards, address bit 4 is decoded as bit 3 (except for $9010).
    /// This function applies the remapping: if bit 4 is set and the address is
    /// not $9010, remap bit 4 → bit 3 and clear bit 4.
    fn normalize_addr(addr: u16) -> u16 {
        let mut a = addr;
        if (a & 0x0010) != 0 && (a & 0xF010) != 0x9010 {
            a |= 0x0008;
            a &= !0x0010;
        }
        a & 0xF038
    }

    fn update_mirroring(&mut self) {
        let new_mirroring = match self.control & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreen,
            3 => NametableLayout::SingleScreenUpper,
            _ => unreachable!(),
        };
        self.base.set_mirroring(new_mirroring);
    }

    fn update_banks(&mut self) {
        // PRG: 3 switchable 8KB pages, fixed last page at $E000
        for i in 0..3 {
            self.base
                .select_prg_page(i, (self.prg_banks_8k[i] & 0x3F) as i16);
        }
        self.base.select_prg_page(3, -1);

        // CHR: 8 × 1KB slots
        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_banks_1k[i] as i16);
        }
    }
}

impl Mapper for VRC7Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.prg_ram.try_read(addr)
            && (self.control & 0x80) != 0
        {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM write at $6000-$7FFF when enabled
        if (self.control & 0x80) != 0 && self.prg_ram.try_write(addr, value) {
            return;
        }

        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }

        let reg = Self::normalize_addr(addr);
        match reg {
            0x8000 => {
                self.prg_banks_8k[0] = value & 0x3F;
                self.update_banks();
            }
            0x8008 => {
                self.prg_banks_8k[1] = value & 0x3F;
                self.update_banks();
            }
            0x9000 => {
                self.prg_banks_8k[2] = value & 0x3F;
                self.update_banks();
            }
            // FM audio registers — stubbed with silence
            0x9010 | 0x9030 => {}

            0xA000 => {
                self.chr_banks_1k[0] = value;
                self.update_banks();
            }
            0xA008 => {
                self.chr_banks_1k[1] = value;
                self.update_banks();
            }
            0xB000 => {
                self.chr_banks_1k[2] = value;
                self.update_banks();
            }
            0xB008 => {
                self.chr_banks_1k[3] = value;
                self.update_banks();
            }
            0xC000 => {
                self.chr_banks_1k[4] = value;
                self.update_banks();
            }
            0xC008 => {
                self.chr_banks_1k[5] = value;
                self.update_banks();
            }
            0xD000 => {
                self.chr_banks_1k[6] = value;
                self.update_banks();
            }
            0xD008 => {
                self.chr_banks_1k[7] = value;
                self.update_banks();
            }

            0xE000 => {
                self.control = value;
                self.update_mirroring();
            }
            0xE008 => {
                self.irq.write_latch(value);
            }
            0xF000 => {
                self.irq.write_control(value);
            }
            0xF008 => {
                self.irq.write_acknowledge();
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.pending()
    }

    fn expansion_audio_sample(&self) -> f32 {
        // FM audio (YM2413/OPLL) is stubbed with silence.
        0.0
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.prg_ram.initialize(mode);
        self.base.initialize_ram(mode);
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        // Reads from $6000-$7FFF return open bus when PRG-RAM is disabled.
        // All other addresses (PRG-ROM at $8000-$FFFF) return ROM data.
        if (self.control & 0x80) == 0 && (0x6000..=0x7FFF).contains(&addr) {
            return open_bus;
        }
        self.read_prg(addr)
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // [0-2]: prg_banks_8k[0..2]
        // [3-10]: chr_banks_1k[0..7]
        // [11]: control
        // [12]: irq_latch
        // [13]: irq_counter
        // [14]: irq_flags (enabled, mode_cycle, enable_after_ack, asserted)
        // [15-18]: irq_prescaler (little-endian i32)
        // [19]: mirroring
        let mut snapshot = Vec::with_capacity(20);
        snapshot.extend_from_slice(&self.prg_banks_8k);
        snapshot.extend_from_slice(&self.chr_banks_1k);
        snapshot.push(self.control);
        snapshot.push(self.irq.latch());
        snapshot.push(self.irq.counter());
        let flags = (self.irq.enabled() as u8)
            | ((self.irq.mode_cycle() as u8) << 1)
            | ((self.irq.enable_after_ack() as u8) << 2)
            | ((self.irq.pending() as u8) << 3);
        snapshot.push(flags);
        snapshot.extend_from_slice(&self.irq.prescaler().to_le_bytes());
        snapshot.push(self.base.mirroring().to_snapshot_byte());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 20 {
            self.prg_banks_8k.copy_from_slice(&data[0..3]);
            self.chr_banks_1k.copy_from_slice(&data[3..11]);
            self.control = data[11];
            self.irq.write_latch(data[12]);
            self.irq.set_counter(data[13]);
            let flags = data[14];
            self.irq.set_enabled((flags & 1) != 0);
            self.irq.set_mode_cycle((flags & 2) != 0);
            self.irq.set_enable_after_ack((flags & 4) != 0);
            self.irq.set_asserted((flags & 8) != 0);
            self.irq
                .set_prescaler(i32::from_le_bytes([data[15], data[16], data[17], data[18]]));
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[19]));
            self.update_banks();
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_vrc7(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(85, prg_rom, chr_rom, mirroring))
            .expect("VRC7 (mapper 85) should be implemented")
    }

    /// PRG ROM with `num_banks` 8KB banks; bank N is filled with byte N.
    fn prg_rom(num_banks: usize) -> Vec<u8> {
        banked_data(8 * 1024, num_banks)
    }

    /// CHR ROM with `num_banks` 1KB banks; bank N is filled with byte N.
    fn chr_rom(num_banks: usize) -> Vec<u8> {
        banked_data(1024, num_banks)
    }

    // ── PRG banking ──────────────────────────────────────────────────────────

    #[test]
    fn prg_bank0_switched_via_8000() {
        // Given a mapper with 48 PRG banks (non-power-of-two to avoid modulo wrapping)
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);

        // When bank 7 is selected for window 0
        mapper.write_prg(0x8000, 7);

        // Then reads from $8000 return bank 7 content
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    #[test]
    fn prg_bank1_switched_via_8008() {
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0x8008, 5);
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    #[test]
    fn prg_bank1_switched_via_8010_bit4_remap() {
        // $8010 should be treated as $8008 (bit 4 → bit 3 remapping)
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0x8010, 5);
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    #[test]
    fn prg_bank2_switched_via_9000() {
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0x9000, 3);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    #[test]
    fn prg_bank3_is_fixed_to_last_bank() {
        // Given 48 banks (0..47); last bank = 47
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);

        // Writing any value to bank 0 slot does not change the fixed last bank
        mapper.write_prg(0x8000, 0);

        // $E000 always maps to the last PRG bank
        assert_eq!(mapper.read_prg(0xE000), 47);
    }

    #[test]
    fn prg_bank_mask_applied_correctly() {
        // Bits above bit 5 (0x3F) should be masked off
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(8), NametableLayout::Horizontal);
        // Write 0xFF → masked to 0x3F = 63, but we only have 48 banks, so mod 48 = 15
        mapper.write_prg(0x8000, 0xFF);
        // 0xFF & 0x3F = 63; 63 % 48 = 15
        assert_eq!(mapper.read_prg(0x8000), 63 % 48);
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn chr_bank0_switched_via_a000() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(48), NametableLayout::Horizontal);
        mapper.write_prg(0xA000, 5);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn chr_bank1_switched_via_a008() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(48), NametableLayout::Horizontal);
        mapper.write_prg(0xA008, 6);
        assert_eq!(mapper.read_chr(0x0400), 6);
    }

    #[test]
    fn chr_bank1_switched_via_a010_bit4_remap() {
        // $A010 should be treated as $A008
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(48), NametableLayout::Horizontal);
        mapper.write_prg(0xA010, 6);
        assert_eq!(mapper.read_chr(0x0400), 6);
    }

    #[test]
    fn chr_bank7_switched_via_d008() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(48), NametableLayout::Horizontal);
        mapper.write_prg(0xD008, 9);
        assert_eq!(mapper.read_chr(0x1C00), 9);
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_bits00_selects_vertical() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0xE000, 0x00); // bits[1:0] = 0 → Vertical
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bits01_selects_horizontal() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Vertical);
        mapper.write_prg(0xE000, 0x01); // bits[1:0] = 1 → Horizontal
        assert_eq!(mapper.base().mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_bits10_selects_single_screen_lower() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0xE000, 0x02);
        assert_eq!(mapper.base().mirroring(), NametableLayout::SingleScreen);
    }

    #[test]
    fn mirroring_bits11_selects_single_screen_upper() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0xE000, 0x03);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::SingleScreenUpper
        );
    }

    // ── PRG-RAM ──────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_disabled_by_default() {
        let mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        // PRG-RAM at $6000 should be open bus (0) when disabled
        assert_eq!(mapper.read_prg(0x6000), 0);
    }

    #[test]
    fn prg_ram_enabled_via_e000_bit7() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        // Enable PRG-RAM (bit 7 = 1)
        mapper.write_prg(0xE000, 0x80);
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn prg_ram_write_ignored_when_disabled() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        // PRG-RAM disabled — write should be ignored
        mapper.write_prg(0x6000, 0xCD);
        // Enable RAM and verify value was not written
        mapper.write_prg(0xE000, 0x80);
        assert_ne!(mapper.read_prg(0x6000), 0xCD);
    }

    // ── read_prg_open_bus ────────────────────────────────────────────────────

    #[test]
    fn read_prg_open_bus_returns_rom_data_for_prg_rom_addresses() {
        // PRG-ROM reads ($8000-$FFFF) should return ROM data, not open bus.
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0x8000, 3); // select bank 3 for window 0
        // read_prg and read_prg_open_bus must agree for ROM addresses
        let rom_value = mapper.read_prg(0x8000);
        assert_eq!(mapper.read_prg_open_bus(0x8000, 0xFF), rom_value);
        // open-bus sentinel (0xFF) must NOT bleed through
        assert_ne!(mapper.read_prg_open_bus(0x8000, 0xFF), 0xFF);
    }

    #[test]
    fn read_prg_open_bus_returns_open_bus_for_disabled_ram() {
        // When PRG-RAM is disabled, $6000 reads must return the open-bus value.
        let mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0xAB), 0xAB);
    }

    #[test]
    fn read_prg_open_bus_returns_ram_data_when_enabled() {
        // When PRG-RAM is enabled, $6000-$7FFF reads must return RAM contents.
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        mapper.write_prg(0xE000, 0x80); // enable PRG-RAM
        mapper.write_prg(0x6000, 0x55);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0xFF), 0x55);
    }

    // ── IRQ ──────────────────────────────────────────────────────────────────

    #[test]
    fn irq_fires_in_cycle_mode_after_latch_overflow() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);

        // Load latch = 0xFE, enable cycle mode (M=1, E=1)
        mapper.write_prg(0xE008, 0xFE);
        mapper.write_prg(0xF000, 0b0000_0110); // M=1, E=1, A=0

        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // counter: 0xFE → 0xFF
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // counter: 0xFF → overflow → IRQ
        assert!(mapper.irq_pending());
    }

    #[test]
    fn irq_acknowledge_clears_pending() {
        let mut mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);

        mapper.write_prg(0xE008, 0xFE);
        mapper.write_prg(0xF000, 0b0000_0110);
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        mapper.write_prg(0xF008, 0); // acknowledge
        assert!(!mapper.irq_pending());
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = create_vrc7(prg_rom(48), chr_rom(48), NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0x8008, 10);
        mapper.write_prg(0x9000, 15);
        mapper.write_prg(0xA000, 2);
        mapper.write_prg(0xD008, 9);
        mapper.write_prg(0xE000, 0x80); // bits[1:0] = 00 → Vertical mirroring + PRG-RAM enable

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_vrc7(prg_rom(48), chr_rom(48), NametableLayout::Horizontal);
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_prg(0xA000), 10);
        assert_eq!(restored.read_prg(0xC000), 15);
        assert_eq!(restored.read_chr(0x0000), 2);
        assert_eq!(restored.read_chr(0x1C00), 9);
        assert_eq!(restored.base().mirroring(), NametableLayout::Vertical);
    }

    // ── Expansion audio (stub) ────────────────────────────────────────────────

    #[test]
    fn expansion_audio_sample_is_zero_silence_stub() {
        let mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        assert_eq!(mapper.expansion_audio_sample(), 0.0);
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_reports_irq_and_expansion_audio() {
        let mapper = create_vrc7(prg_rom(8), chr_rom(8), NametableLayout::Horizontal);
        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(caps.has_expansion_audio);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
    }
}
