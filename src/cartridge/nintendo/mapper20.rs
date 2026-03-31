//! Mapper 020 - Famicom Disk System (FDS)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_020>
//! - FDS hardware: <https://www.nesdev.org/wiki/Family_Computer_Disk_System>
//!
//! Known Limitations:
//! - Disk I/O is not emulated (no .fds disk image loading).
//! - FDS audio channels (wave table, modulation) are stored but not mixed.
//! - BIOS ROM is taken from PRG-ROM; no user-supplied BIOS loading.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// FDS work-RAM region: $6000–$DFFF (32 KiB).
const WORK_RAM_SIZE: usize = 0x8000;
const WORK_RAM_BASE: u16 = 0x6000;
const WORK_RAM_END: u16 = 0xDFFF;

const BIOS_ROM_BASE: u16 = 0xE000;

const WAVE_RAM_BASE: u16 = 0x4040;
const WAVE_RAM_END: u16 = 0x407F;
const WAVE_RAM_MASK: u8 = 0x3F;
const WAVE_RAM_LEN: usize = 64;

const AUDIO_REG_BASE: u16 = 0x4080;
const AUDIO_REG_END: u16 = 0x408F;
const AUDIO_REGS_LEN: usize = 16;

const FDS_CONTROL_MIRROR_BIT: u8 = 0x08;
const DISK_STATUS_NOT_READY: u8 = 0x80;

const IRQ_ENABLE_BIT: u8 = 0x01;
const IRQ_REPEAT_BIT: u8 = 0x02;

/// Snapshot layout: 8-byte header, then wave RAM, then audio regs.
const SNAPSHOT_HEADER_LEN: usize = 8;
const SNAPSHOT_MIN_LEN: usize = SNAPSHOT_HEADER_LEN + WAVE_RAM_LEN + AUDIO_REGS_LEN;

/// Mapper 020 — Famicom Disk System
///
/// Memory map (CPU):
/// - `$4020–$402F`: FDS disk I/O registers (write) / status (read)
/// - `$4030–$403F`: FDS drive status registers (read)
/// - `$4040–$407F`: Wave RAM (6-bit, R/W)
/// - `$4080–$408F`: Audio control registers (write)
/// - `$6000–$DFFF`: 32 KiB work RAM (R/W)
/// - `$E000–$FFFF`: BIOS ROM (8 KiB, from PRG-ROM)
///
/// Mirroring: Fixed horizontal (hardwired on FDS hardware).
/// IRQ: 16-bit CPU-cycle count-down timer; fires on reach-zero; optional reload.
pub struct Mapper20 {
    base: BaseMapper,

    /// 32 KiB work RAM ($6000–$DFFF).
    work_ram: Box<[u8; WORK_RAM_SIZE]>,

    // --- IRQ timer ---
    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    irq_repeat: bool,

    // --- I/O enable register ($4023) ---
    io_enable: u8,

    // --- FDS disk control register ($4025, write-only) ---
    fds_control: u8,

    // --- Audio ---
    /// Wave RAM: 64 × 6-bit entries at $4040–$407F.
    wave_ram: [u8; 64],
    /// Audio control registers $4080–$408F (write, backing store for snapshot).
    audio_regs: [u8; 16],
}

impl Mapper20 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_expansion_audio: true,
            has_dynamic_mirroring: false,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.set_mirroring(NametableLayout::Horizontal);

        Self {
            base,
            work_ram: Box::new([0u8; WORK_RAM_SIZE]),
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_repeat: false,
            io_enable: 0,
            fds_control: 0,
            wave_ram: [0u8; 64],
            audio_regs: [0u8; 16],
        }
    }

    /// Read a byte from BIOS ROM at $E000–$FFFF.
    fn read_bios(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        if prg.is_empty() {
            return 0;
        }
        let offset = (addr as usize - BIOS_ROM_BASE as usize) % prg.len();
        prg[offset]
    }

    fn pack_irq_flags(&self) -> u8 {
        (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1) | ((self.irq_repeat as u8) << 2)
    }

    fn unpack_irq_flags(&mut self, flags: u8) {
        self.irq_enabled = (flags & 0x01) != 0;
        self.irq_pending = (flags & 0x02) != 0;
        self.irq_repeat = (flags & 0x04) != 0;
    }
}

impl Mapper for Mapper20 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x4030 => DISK_STATUS_NOT_READY,
            0x4031 => 0, // read data register (no disk)
            0x4032 => 0, // drive status
            0x4033 => 0, // external connector
            WAVE_RAM_BASE..=WAVE_RAM_END => {
                let idx = (addr - WAVE_RAM_BASE) as usize;
                self.wave_ram[idx] & WAVE_RAM_MASK
            }
            WORK_RAM_BASE..=WORK_RAM_END => {
                let offset = (addr - WORK_RAM_BASE) as usize;
                self.work_ram[offset]
            }
            BIOS_ROM_BASE..=0xFFFF => self.read_bios(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x4020 => {
                self.irq_latch = (self.irq_latch & 0xFF00) | (value as u16);
            }
            0x4021 => {
                self.irq_latch = (self.irq_latch & 0x00FF) | ((value as u16) << 8);
            }
            0x4022 => {
                // Any write acknowledges a pending IRQ.
                self.irq_pending = false;
                self.irq_enabled = (value & IRQ_ENABLE_BIT) != 0;
                self.irq_repeat = (value & IRQ_REPEAT_BIT) != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                } else {
                    self.irq_counter = 0;
                }
            }
            0x4023 => {
                self.io_enable = value;
            }
            0x4025 => {
                self.fds_control = value;
                // Bit 3: 0 = vertical mirroring, 1 = horizontal mirroring.
                self.base
                    .set_mirroring_hv((value & FDS_CONTROL_MIRROR_BIT) != 0);
            }
            WAVE_RAM_BASE..=WAVE_RAM_END => {
                let idx = (addr - WAVE_RAM_BASE) as usize;
                self.wave_ram[idx] = value & WAVE_RAM_MASK;
            }
            AUDIO_REG_BASE..=AUDIO_REG_END => {
                let idx = (addr - AUDIO_REG_BASE) as usize;
                self.audio_regs[idx] = value;
            }
            WORK_RAM_BASE..=WORK_RAM_END => {
                let offset = (addr - WORK_RAM_BASE) as usize;
                self.work_ram[offset] = value;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled || self.irq_counter == 0 {
            return;
        }
        self.irq_counter = self.irq_counter.wrapping_sub(1);
        if self.irq_counter == 0 {
            self.irq_pending = true;
            if self.irq_repeat {
                self.irq_counter = self.irq_latch;
            } else {
                self.irq_enabled = false;
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.base.mirroring()
    }

    fn prg_ram_snapshot(&self) -> Vec<u8> {
        self.work_ram.to_vec()
    }

    fn restore_prg_ram(&mut self, data: &[u8]) {
        let len = data.len().min(WORK_RAM_SIZE);
        self.work_ram[..len].copy_from_slice(&data[..len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = matches!(self.base.mirroring(), NametableLayout::Horizontal) as u8;
        let mut v = vec![
            (self.irq_latch & 0xFF) as u8,
            (self.irq_latch >> 8) as u8,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
            self.pack_irq_flags(),
            self.io_enable,
            self.fds_control,
            mirror_byte,
        ];
        v.extend_from_slice(&self.wave_ram);
        v.extend_from_slice(&self.audio_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < SNAPSHOT_MIN_LEN {
            return;
        }
        self.irq_latch = (data[0] as u16) | ((data[1] as u16) << 8);
        self.irq_counter = (data[2] as u16) | ((data[3] as u16) << 8);
        self.unpack_irq_flags(data[4]);
        self.io_enable = data[5];
        self.fds_control = data[6];
        self.base.set_mirroring_hv(data[7] != 0);

        let wave_end = SNAPSHOT_HEADER_LEN + WAVE_RAM_LEN;
        let audio_end = wave_end + AUDIO_REGS_LEN;
        self.wave_ram
            .copy_from_slice(&data[SNAPSHOT_HEADER_LEN..wave_end]);
        self.audio_regs.copy_from_slice(&data[wave_end..audio_end]);
    }

    fn reset(&mut self) {
        self.work_ram.fill(0);
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_repeat = false;
        self.io_enable = 0;
        self.fds_control = 0;
        self.wave_ram.fill(0);
        self.audio_regs.fill(0);
        self.base.set_mirroring(NametableLayout::Horizontal);
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper20;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};

    fn make_mapper() -> Mapper20 {
        Mapper20::new(MapperContext::new_for_test(
            20,
            vec![0u8; 8 * 1024], // 8 KiB BIOS placeholder
            vec![],              // CHR-RAM
            NametableLayout::Horizontal,
        ))
    }

    // --- Factory ---

    #[test]
    fn mapper_20_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            20,
            vec![0u8; 8 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "mapper 20 must be registered in the factory"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_defaults_to_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "FDS mirroring must default to Horizontal"
        );
    }

    #[test]
    fn mirroring_controlled_by_fds_control_bit3() {
        let mut mapper = make_mapper();
        // bit 3 = 0 → Vertical
        mapper.write_prg(0x4025, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        // bit 3 = 1 → Horizontal
        mapper.write_prg(0x4025, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- Work RAM ---

    #[test]
    fn work_ram_is_readable_and_writable_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "Work RAM at $6000 must be writable and readable"
        );
    }

    #[test]
    fn work_ram_covers_full_6000_dfff_range() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x8000, 0x22);
        mapper.write_prg(0xDFFF, 0x33);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        assert_eq!(mapper.read_prg(0x8000), 0x22);
        assert_eq!(mapper.read_prg(0xDFFF), 0x33);
    }

    // --- BIOS ROM ---

    #[test]
    fn bios_rom_readable_at_e000() {
        let mut bios = vec![0u8; 8 * 1024];
        bios[0] = 0x42;
        bios[8 * 1024 - 1] = 0x99;
        let mapper = Mapper20::new(MapperContext::new_for_test(
            20,
            bios,
            vec![],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper.read_prg(0xE000), 0x42, "BIOS first byte at $E000");
        assert_eq!(mapper.read_prg(0xFFFF), 0x99, "BIOS last byte at $FFFF");
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_does_not_fire_when_disabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x01);
        mapper.write_prg(0x4021, 0x00);
        // Do NOT enable via $4022
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    #[test]
    fn irq_fires_after_counter_expires() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x01); // latch low = 1
        mapper.write_prg(0x4021, 0x00); // latch high = 0 → latch = 1
        mapper.write_prg(0x4022, 0x01); // enable, no repeat
        // Counter loaded to 1; one cycle decrements to 0 → IRQ fires
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must fire when 16-bit counter reaches zero"
        );
    }

    #[test]
    fn irq_does_not_fire_before_counter_expires() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x03); // latch = 3
        mapper.write_prg(0x4021, 0x00);
        mapper.write_prg(0x4022, 0x01); // enable
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ must not fire on first cycle");
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ must not fire on second cycle");
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must fire on third cycle (counter 3→0)"
        );
    }

    #[test]
    fn irq_acknowledged_by_writing_4022_with_bit0_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x01);
        mapper.write_prg(0x4021, 0x00);
        mapper.write_prg(0x4022, 0x01); // enable
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must be pending before ack");
        mapper.write_prg(0x4022, 0x00); // disable + acknowledge
        assert!(
            !mapper.irq_pending(),
            "IRQ must clear after $4022 write with bit0=0"
        );
    }

    #[test]
    fn irq_repeats_when_repeat_flag_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x02); // latch = 2
        mapper.write_prg(0x4021, 0x00);
        mapper.write_prg(0x4022, 0x03); // enable + repeat
        // Cycle 1: counter 2 → 1
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        // Cycle 2: counter 1 → 0 → fires, reloads to 2
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must fire after latch cycles");
        // Acknowledge and check the counter reloaded
        mapper.write_prg(0x4022, 0x03); // re-enable (also reloads)
        assert!(
            !mapper.irq_pending(),
            "Re-enabling must clear IRQ and reload counter"
        );
        // Should fire again after 2 more cycles
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must repeat every latch cycles");
    }

    // --- Audio ---

    #[test]
    fn audio_wave_ram_is_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4040, 0xFF); // only 6 bits stored
        assert_eq!(
            mapper.read_prg(0x4040),
            0x3F,
            "Wave RAM must store 6-bit values"
        );
        mapper.write_prg(0x407F, 0x1A);
        assert_eq!(mapper.read_prg(0x407F), 0x1A & 0x3F);
    }

    #[test]
    fn audio_control_registers_writable() {
        let mut mapper = make_mapper();
        // Should not panic — just a write, no assertion needed beyond that
        mapper.write_prg(0x4080, 0x80);
        mapper.write_prg(0x408A, 0xFF);
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4020, 0x05);
        mapper.write_prg(0x4021, 0x00);
        mapper.write_prg(0x4022, 0x01); // enable IRQ, latch=5
        mapper.write_prg(0x4040, 0x3F); // wave RAM
        mapper.write_prg(0x4080, 0x7F); // audio reg

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.registers_snapshot(),
            snap,
            "registers_snapshot/restore_registers roundtrip must preserve all register state"
        );
    }

    #[test]
    fn work_ram_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0xDFFF, 0xCD);

        let snap = mapper.prg_ram_snapshot();
        assert_eq!(snap.len(), 0x8000, "work RAM snapshot must be 32 KiB");

        let mut restored = make_mapper();
        restored.restore_prg_ram(&snap);

        assert_eq!(
            restored.read_prg(0x6000),
            0xAB,
            "Restored work RAM must match at $6000"
        );
        assert_eq!(
            restored.read_prg(0xDFFF),
            0xCD,
            "Restored work RAM must match at $DFFF"
        );
    }
}
