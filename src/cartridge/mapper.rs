use crate::cartridge::NametableLayout;
use crate::cartridge::ines::{ConsoleType, ParsedRom};
use std::io;

use super::axrom::AxROMMapper;
use super::bandai_fcg::BandaiFcgMapper;
use super::bnrom_nina::BnromNinaMapper;
use super::camerica::CamericaMapper;
use super::cnrom::CNROMMapper;
use super::colordreams::ColorDreamsMapper;
use super::cprom::CpromMapper;
use super::gxrom::GxROMMapper;
use super::irem_g101::IremG101Mapper;
use super::mapper12::Mapper12;
use super::mapper14::Mapper14;
use super::mapper18::Mapper18;
use super::mapper20::Mapper20;
use super::mapper28::Mapper28;
use super::mapper29::Mapper29;
use super::mapper30::Mapper30;
use super::mapper31::Mapper31;
use super::mapper35::Mapper35;
use super::mapper36::Mapper36;
use super::mapper37::Mapper37;
use super::mapper38::Mapper38;
use super::mapper39::Mapper39;
use super::mapper41::Mapper41;
use super::mapper42::Mapper42;
use super::mapper43::Mapper43;
use super::mapper44::Mapper44;
use super::mapper45::Mapper45;
use super::mapper46::Mapper46;
use super::mapper47::Mapper47;
use super::mapper48::Mapper48;
use super::mapper49::Mapper49;
use super::mapper50::Mapper50;
use super::mapper51::Mapper51;
use super::mapper52::Mapper52;
use super::mapper53::Mapper53;
use super::mapper54::Mapper54;
use super::mapper55::Mapper55;
use super::mapper56::Mapper56;
use super::mapper57::Mapper57;
use super::mapper58::Mapper58;
use super::mapper59::Mapper59;
use super::mapper60::Mapper60;
use super::mapper61::Mapper61;
use super::mapper62::Mapper62;
use super::mapper63::Mapper63;
use super::mapper64::Mapper64;
use super::mapper65::Mapper65;
use super::mapper67::Mapper67;
use super::mapper70::Mapper70;
use super::mapper72::Mapper72;
use super::mapper73::Mapper73;
use super::mapper74::Mapper74;
use super::mapper75::Mapper75;
use super::mapper76::Mapper76;
use super::mapper77::Mapper77;
use super::mapper79::Mapper79;
use super::mapper80::Mapper80;
use super::mapper81::Mapper81;
use super::mapper82::Mapper82;
use super::mapper83::Mapper83;
use super::mapper86::Mapper86;
use super::mapper87::Mapper87;
use super::mapper88::Mapper88;
use super::mapper90::Mapper90;
use super::mapper91::Mapper91;
use super::mapper93::Mapper93;
use super::mapper95::Mapper95;
use super::mapper96::Mapper96;
use super::mapper100::Mapper100;
use super::mapper101::Mapper101;
use super::mapper104::Mapper104;
use super::mapper105::Mapper105;
use super::mapper115::Mapper115;
use super::mapper117::Mapper117;
use super::mapper132::Mapper132;
use super::mapper133::Mapper133;
use super::mapper140::Mapper140;
use super::mapper185::Mapper185;
use super::mapper205::Mapper205;
use super::mapper241::Mapper241;
use super::mapper242::Mapper242;
use super::mapper243::Mapper243;
use super::mapper244::Mapper244;
use super::mapper245::Mapper245;
use super::mapper246::Mapper246;
use super::mapper251::Mapper251;
use super::mapper254::Mapper254;
use super::mapper255::Mapper255;
use super::mapper302::Mapper302;
use super::mapper307::Mapper307;
use super::mapper319::Mapper319;
use super::mapper320::Mapper320;
use super::mapper324::Mapper324;
use super::mapper326::Mapper326;
use super::mapper327::Mapper327;
use super::mapper328::Mapper328;
use super::mapper329::Mapper329;
use super::mapper330::Mapper330;
use super::mapper332::Mapper332;
use super::mapper335::Mapper335;
use super::mapper338::Mapper338;
use super::mapper339::Mapper339;
use super::mapper340::Mapper340;
use super::mapper341::Mapper341;
use super::mapper342::Mapper342;
use super::mapper344::Mapper344;
use super::mapper345::Mapper345;
use super::mapper346::Mapper346;
use super::mapper347::Mapper347;
use super::mapper348::Mapper348;
use super::mapper349::Mapper349;
use super::mapper350::Mapper350;
use super::mmc1::MMC1Mapper;
use super::mmc2::MMC2Mapper;
use super::mmc3::MMC3Mapper;
use super::mmc4::MMC4Mapper;
use super::mmc5::MMC5Mapper;
use super::multicart_15::Multicart15Mapper;
use super::namco118::Namco118Mapper;
use super::namco163::Namco163Mapper;
use super::nina_tengen::NinaTengenMapper;
use super::nrom::NROMMapper;
use super::ntdec_2722::Ntdec2722Mapper;
#[cfg(test)]
use super::rom_db;
use super::sunsoft_4::Sunsoft4Mapper;
use super::sunsoft_fme7::SunsoftFme7Mapper;
use super::super_magic_card::SuperMagicCardMapper;
use super::taito_tc0190::TaitoTc0190Mapper;
use super::uxrom::UxROMMapper;
use super::vrc2_vrc4::Vrc2Vrc4Mapper;
use super::vrc6::VRC6Mapper;
use super::vrc7::VRC7Mapper;

/// Metadata for constructing a mapper, containing cartridge header details and
/// derived values (e.g., CRC32) used by the factory.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MapperContext {
    /// iNES/NES 2.0 mapper number. Submapper is kept separately.
    pub mapper: u16,
    /// NES 2.0 submapper id (0 when not specified).
    pub submapper: u8,
    /// PPU nametable mirroring mode from the header.
    pub mirroring: NametableLayout,
    /// Console type from iNES/NES 2.0 header.
    pub console_type: ConsoleType,
    /// PRG ROM bytes.
    pub prg_rom: Vec<u8>,
    /// CHR ROM bytes (empty when CHR-RAM).
    pub chr_rom: Vec<u8>,
    /// PRG-RAM size in 8KB units (minimum 1).
    pub prg_ram_banks_8k: u8,
    /// Whether PRG-RAM size was explicitly specified by header metadata.
    pub prg_ram_size_specified: bool,
    /// Whether PRG-RAM is battery backed.
    pub battery_backed_prg_ram: bool,
    /// CRC32 of concatenated PRG/CHR; may be overridden for tests.
    pub crc32: u32,
}

const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
const DEFAULT_PRG_RAM_BANKS_8K: u8 = 1;

impl MapperContext {
    /// Create mapper context from a fully parsed ROM, extracting all relevant
    /// header fields (mapper, submapper, mirroring, PRG-RAM size, battery flag)
    /// and ROM data.
    pub fn from_parsed_rom(parsed: &ParsedRom) -> Self {
        let info = &parsed.header;
        Self {
            mapper: info.mapper,
            submapper: info.submapper,
            mirroring: info.mirroring,
            console_type: info.console_type,
            prg_rom: parsed.prg_rom.clone(),
            chr_rom: parsed.chr_rom.clone(),
            prg_ram_banks_8k: Self::prg_ram_banks_8k(info.prg_ram_size_bytes),
            prg_ram_size_specified: info.prg_ram_size_bytes.is_some(),
            battery_backed_prg_ram: info.battery_backed_prg_ram,
            crc32: parsed.crc32,
        }
    }

    fn prg_ram_banks_8k(prg_ram_size_bytes: Option<usize>) -> u8 {
        prg_ram_size_bytes
            .map(|size| {
                if size == 0 {
                    return 0;
                }

                size.div_ceil(PRG_RAM_BANK_SIZE).clamp(1, u8::MAX as usize) as u8
            })
            .unwrap_or(DEFAULT_PRG_RAM_BANKS_8K)
    }

    /// Create mapper metadata with default submapper 0, 1×8KB PRG-RAM (not battery-backed),
    /// and CRC32 computed from PRG+CHR data. Intended for unit tests.
    #[cfg(test)]
    pub fn new_for_test(
        mapper: u16,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Self {
        let crc32 = rom_db::calculate_rom_crc32(&prg_rom, &chr_rom);
        Self {
            mapper,
            submapper: 0,
            mirroring,
            console_type: ConsoleType::NesFamicom,
            prg_rom,
            chr_rom,
            prg_ram_banks_8k: 1,
            prg_ram_size_specified: true,
            battery_backed_prg_ram: false,
            crc32,
        }
    }

    /// Set NES 2.0 submapper id.
    #[cfg(test)]
    pub fn with_submapper(mut self, submapper: u8) -> Self {
        self.submapper = submapper;
        self
    }

    /// Override PRG-RAM size in 8KB units (clamped to at least one bank).
    #[cfg(test)]
    pub fn with_prg_ram_banks(mut self, prg_ram_banks_8k: u8) -> Self {
        self.prg_ram_banks_8k = prg_ram_banks_8k;
        self
    }

    /// Mark PRG-RAM size as unspecified in metadata.
    #[cfg(test)]
    pub fn with_unspecified_prg_ram_size(mut self) -> Self {
        self.prg_ram_size_specified = false;
        self
    }
}

/// Describes the hardware capabilities of a mapper.
///
/// Each mapper should return accurate capabilities from [`Mapper::capabilities()`]
/// to enable runtime feature detection, conditional test execution, and
/// documentation of the mapper feature matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct MapperCapabilities {
    /// Whether the mapper can generate IRQ interrupts.
    pub has_irq: bool,
    /// Whether the mapper supports CHR bank switching.
    pub has_chr_banking: bool,
    /// Whether the mapper can change nametable mirroring mode dynamically.
    pub has_dynamic_mirroring: bool,
    /// Whether the mapper provides expansion audio channels.
    pub has_expansion_audio: bool,
    /// Maximum PRG-RAM size in KB.
    pub max_prg_ram_kb: usize,
    /// Smallest PRG bank size granularity in KB.
    pub prg_bank_size_kb: usize,
    /// Smallest CHR bank size granularity in KB.
    pub chr_bank_size_kb: usize,
    /// Whether the mapper hardware executes the trainer via JSR $7003 before the
    /// game's reset vector (Mapper 6 / SMC-801 specific behaviour).
    pub trainer_jsr: bool,
    /// CPU address at which the 512-byte iNES trainer block is loaded.
    /// Default is $7000 (standard for Mapper 6). Mapper 17 submappers 1–3
    /// use $5D00, $5E00, and $5F00 (scratch RAM) respectively.
    pub trainer_load_address: u16,
}

impl Default for MapperCapabilities {
    fn default() -> Self {
        Self {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            trainer_load_address: 0x7000,
        }
    }
}

pub trait Mapper {
    /// Return a reference to the embedded [`BaseMapper`], if present.
    ///
    /// Return a reference to the embedded [`BaseMapper`].
    ///
    /// All mappers must embed a `BaseMapper` and implement this method.
    /// The default trait methods for mirroring, WRAM, CHR-RAM, capabilities, etc.
    /// then delegate to the `BaseMapper`, eliminating boilerplate.
    ///
    /// [`BaseMapper`]: crate::cartridge::base_mapper::BaseMapper
    fn base(&self) -> &super::base_mapper::BaseMapper;

    /// Return a mutable reference to the embedded [`BaseMapper`].
    ///
    /// See [`base`](Mapper::base) for details.
    fn base_mut(&mut self) -> &mut super::base_mapper::BaseMapper;

    /// Return a reference to an embedded [`MMC3Mapper`] when this mapper wraps MMC3.
    ///
    /// Default returns `None` for non-MMC3 mappers.
    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        None
    }

    /// Return a mutable reference to an embedded [`MMC3Mapper`] when this mapper wraps MMC3.
    ///
    /// Default returns `None` for non-MMC3 mappers.
    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        None
    }

    /// Read a byte from PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, battery-backed on some cartridges)
    /// - $8000-$FFFF: PRG-ROM (with bank switching on advanced mappers)
    ///   Returns the byte at the given address after bank translation.
    ///
    /// Default checks for $6000-$7FFF PRG-ROM banking first, then tries
    /// PRG-RAM via `BaseMapper::try_read_prg_ram`, then falls back to
    /// PRG-ROM via `BaseMapper::read_prg_rom` (which auto-detects banked
    /// vs fixed access).
    /// Mappers with custom PRG address decoding must override this.
    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.base().try_read_prg_6000(addr) {
            return value;
        }
        if let Some(value) = self.base().try_read_prg_ram(addr) {
            return value;
        }
        match addr {
            0x8000..=0xFFFF => self.base().read_prg_rom(addr),
            _ => 0,
        }
    }

    /// Read a byte from PRG address space (CPU $6000-$FFFF), with open-bus context.
    ///
    /// Mappers that return open bus for disabled regions can override this method
    /// to return `open_bus` when appropriate.
    ///
    /// Default delegates to `BaseMapper` if available, otherwise falls back to
    /// `read_prg` with open-bus below $6000.
    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.base()
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    /// Write a byte to PRG address space (CPU $6000-$FFFF)
    /// - $6000-$7FFF: PRG-RAM (8KB, writable)
    /// - $8000-$FFFF: Mapper control registers (PRG-ROM is read-only)
    fn write_prg(&mut self, addr: u16, value: u8);

    /// Read a byte from CHR address space (PPU $0000-$1FFF)
    /// Returns the byte at the given address after bank translation.
    ///
    /// Default delegates to `BaseMapper::read_chr`, which auto-detects
    /// banked vs unbanked access based on whether CHR banking is configured.
    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base().read_chr(addr)
    }

    /// Write a byte to CHR address space (PPU $0000-$1FFF)
    /// Only works for CHR-RAM, CHR-ROM is read-only.
    ///
    /// Default delegates to `BaseMapper::write_chr`, which auto-detects
    /// banked vs unbanked access based on whether CHR banking is configured.
    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base_mut().write_chr(addr, value);
    }

    /// Notify mapper of PPU address bus changes
    /// Used for detecting A12 rising edges (for MMC3 IRQ)
    /// Default delegates to MMC3 when available, otherwise is a no-op.
    fn ppu_address_changed(&mut self, addr: u16) {
        if let Some(mmc3) = self.mmc3_delegate_mut() {
            mmc3.ppu_address_changed(addr);
        }
    }

    /// Set the current PPU CHR fetch kind.
    ///
    /// Some mappers (e.g., MMC5) need to distinguish between background and sprite CHR fetches.
    /// Default implementation is a no-op.
    fn ppu_set_chr_fetch_is_sprite(&mut self, _is_sprite: bool) {}

    /// Notify mapper that PPUDATA ($2007) is about to read from CHR.
    ///
    /// Some mappers (e.g., MMC5) need to distinguish rendering fetches from PPUDATA reads.
    /// Default implementation is a no-op.
    fn ppu_set_chr_fetch_is_ppudata(&mut self) {}

    /// Notify mapper of a write to PPUCTRL ($2000).
    ///
    /// The MMC5 monitors this to detect 8x16 sprite mode (bit 5).
    /// Default implementation is a no-op.
    fn ppu_write_ctrl(&mut self, _value: u8) {}

    /// Notify mapper of a write to PPUMASK ($2001).
    ///
    /// The MMC5 monitors this to detect rendering enable (bits 3-4).
    /// Default implementation is a no-op.
    fn ppu_write_mask(&mut self, _value: u8) {}

    /// Notify mapper of a CPU write to $4014 (OAMDMA).
    ///
    /// Some mappers (e.g., MMC5) reset scanline counters on OAMDMA writes.
    /// Default implementation is a no-op.
    fn on_oam_dma(&mut self) {}

    /// Notify mapper of a CPU read from an interrupt vector ($FFFA-$FFFF).
    ///
    /// The `_addr` argument indicates which vector was read:
    ///   - $FFFA/$FFFB: NMI
    ///   - $FFFC/$FFFD: Reset
    ///   - $FFFE/$FFFF: IRQ/BRK
    ///
    /// Some mappers (e.g., MMC5) reset scanline/in-frame tracking on interrupt
    /// vector reads. Default implementation is a no-op.
    fn on_irq_vector_read(&mut self, _addr: u16) {}

    /// Notify mapper about the current scanline (during rendering) for PPU-driven IRQ systems.
    /// Default implementation is a no-op.
    fn ppu_scanline(&mut self, _scanline: u16, _rendering_enabled: bool) {}

    /// Notify mapper that a frame has ended.
    /// Default implementation is a no-op.
    fn ppu_end_frame(&mut self) {}

    /// Optional mapper override for PPU nametable reads ($2000-$3EFF).
    ///
    /// If the mapper wishes to supply the byte (e.g., MMC5 ExRAM/fill), return `Some(value)`.
    /// Return `None` to fall back to the PPU's internal nametable VRAM.
    fn read_nametable(&mut self, _addr: u16) -> Option<u8> {
        None
    }

    /// Optional mapper override for PPU nametable writes ($2000-$3EFF).
    ///
    /// Return `true` if the mapper handled the write, `false` to fall back to internal VRAM.
    fn write_nametable(&mut self, _addr: u16, _value: u8) -> bool {
        false
    }

    /// Notify mapper that a CPU cycle has elapsed.
    ///
    /// Some mappers implement CPU-cycle-driven IRQ systems (e.g., Konami VRC IRQ).
    /// Default delegates to MMC3 when available, otherwise is a no-op.
    fn cpu_cycle(&mut self) {
        if let Some(mmc3) = self.mmc3_delegate_mut() {
            mmc3.cpu_cycle();
        }
    }

    /// Reset the mapper to its power-on state.
    ///
    /// This is called when the NES is reset. Mappers should reset their internal
    /// state (bank registers, IRQ counters, etc.) but typically preserve PRG-RAM contents.
    /// Default implementation is a no-op for simple mappers.
    fn reset(&mut self) {}

    /// Re-initialize cartridge RAM (PRG-RAM and CHR-RAM) based on the given mode.
    ///
    /// This is called when a cartridge is inserted or on hard reset to initialize
    /// RAM contents. Soft resets should NOT call this method (RAM should be preserved).
    ///
    /// Default delegates to MMC3 when available, otherwise delegates to `BaseMapper`.
    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        if let Some(mmc3) = self.mmc3_delegate_mut() {
            mmc3.initialize_ram(mode);
        } else {
            self.base_mut().initialize_ram(mode);
        }
    }

    /// Whether the mapper is currently asserting IRQ.
    ///
    /// This is used to model mapper-generated IRQs (e.g., MMC3 scanline IRQ).
    /// Default delegates to MMC3 when available, otherwise returns false.
    fn irq_pending(&self) -> bool {
        if let Some(mmc3) = self.mmc3_delegate() {
            mmc3.irq_pending()
        } else {
            false
        }
    }

    /// Current expansion-audio output sample contributed by the mapper.
    ///
    /// Some mappers provide additional sound channels (e.g., Konami VRC6).
    /// The returned value should be a linear sample intended to be added to the
    /// base APU mix (typically in a small range like 0.0..~0.5).
    ///
    /// Default implementation returns 0.0 for mappers without expansion audio.
    fn expansion_audio_sample(&self) -> f32 {
        0.0
    }

    /// Get the current nametable mirroring mode.
    /// Some mappers can change mirroring dynamically.
    ///
    /// Default delegates to `BaseMapper` if available.
    fn get_mirroring(&self) -> NametableLayout {
        self.base().mirroring()
    }

    /// Get the size of cartridge WRAM (PRG-RAM) in bytes.
    ///
    /// Returns the total size of battery-backed PRG-RAM that should be persisted to disk.
    ///
    /// Default delegates to `BaseMapper` if available, otherwise returns
    /// 8KB (0x2000 bytes).
    #[allow(dead_code)]
    fn wram_size(&self) -> usize {
        self.base().wram_size()
    }

    /// Create a snapshot of all cartridge WRAM (PRG-RAM) for persistence.
    ///
    /// This reads the raw WRAM independent of:
    /// - Enable/disable state (e.g., MMC3 PRG-RAM enable)
    /// - Write-protect state
    /// - Current bank mapping (for mappers with banked WRAM like MMC5)
    ///
    /// Returns a Vec containing the complete WRAM contents.
    /// Default delegates to `BaseMapper` if available, otherwise reads via
    /// `read_prg` at $6000-$7FFF (8KB window).
    /// **Mappers with >8KB WRAM MUST override this method to capture all banks.**
    fn wram_snapshot(&self) -> Vec<u8> {
        self.base().wram_snapshot()
    }

    /// Load a WRAM snapshot from persistence.
    ///
    /// This writes the raw WRAM independent of:
    /// - Enable/disable state (e.g., MMC3 PRG-RAM enable)
    /// - Write-protect state
    /// - Current bank mapping (for mappers with banked WRAM like MMC5)
    ///
    /// The data slice should contain the complete WRAM contents to restore.
    /// Default delegates to `BaseMapper` if available, otherwise writes via
    /// `write_prg` at $6000-$7FFF (8KB window).
    /// **Mappers with >8KB WRAM MUST override this method to restore all banks.**
    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.base_mut().load_wram_snapshot(data);
    }

    /// Get the mapper number (iNES mapper ID).
    ///
    /// Returns the iNES mapper number for this mapper (e.g., 0 for NROM, 1 for MMC1).
    ///
    /// Default delegates to `BaseMapper` if available, otherwise returns 0.
    fn mapper_number(&self) -> u16 {
        self.base().mapper_number()
    }

    /// Create a snapshot of PRG-RAM for save-state.
    ///
    /// Default implementation uses wram_snapshot.
    fn prg_ram_snapshot(&self) -> Vec<u8> {
        self.wram_snapshot()
    }

    /// Create a snapshot of CHR-RAM for save-state.
    ///
    /// Default delegates to MMC3 when available, otherwise delegates to `BaseMapper`.
    fn chr_ram_snapshot(&self) -> Vec<u8> {
        if let Some(mmc3) = self.mmc3_delegate() {
            mmc3.chr_ram_snapshot()
        } else {
            self.base().chr_ram_snapshot()
        }
    }

    /// Create a snapshot of mapper-specific registers for save-state.
    ///
    /// Default implementation returns empty (for simple mappers with no internal state).
    fn registers_snapshot(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Restore PRG-RAM from a save-state.
    ///
    /// Default implementation uses load_wram_snapshot.
    fn restore_prg_ram(&mut self, data: &[u8]) {
        self.load_wram_snapshot(data);
    }

    /// Restore CHR-RAM from a save-state.
    ///
    /// Default delegates to MMC3 when available, otherwise delegates to `BaseMapper`.
    fn restore_chr_ram(&mut self, data: &[u8]) {
        if let Some(mmc3) = self.mmc3_delegate_mut() {
            mmc3.restore_chr_ram(data);
        } else {
            self.base_mut().restore_chr_ram(data);
        }
    }

    /// Restore mapper-specific registers from a save-state.
    ///
    /// Default implementation is a no-op.
    fn restore_registers(&mut self, _data: &[u8]) {}

    /// Return the hardware capabilities of this mapper.
    ///
    /// Each mapper should override this to accurately describe its features
    /// (IRQ, CHR banking, dynamic mirroring, expansion audio, etc.).
    ///
    /// Default delegates to `BaseMapper` if available, otherwise returns
    /// conservative defaults suitable for the simplest mappers.
    #[allow(dead_code)]
    fn capabilities(&self) -> MapperCapabilities {
        self.base().capabilities()
    }
}

macro_rules! mapper_registry {
    ($($id:expr => $ctor:path),+ $(,)?) => {
        fn create_registry_mapper(
            metadata: MapperContext,
        ) -> Option<Box<dyn Mapper>> {
            match metadata.mapper {
                $(
                    $id => {
                        Some(Box::new($ctor(metadata)))
                    }
                )+
                _ => None,
            }
        }
    };
}

mapper_registry! {
    0 => NROMMapper::new,
    1 => MMC1Mapper::new,
    2 => UxROMMapper::new,
    3 => CNROMMapper::new,
    4 => MMC3Mapper::new,
    5 => MMC5Mapper::new,
    6 => SuperMagicCardMapper::new,
    7 => AxROMMapper::new,
    8 => SuperMagicCardMapper::new,
    9 => MMC2Mapper::new,
    10 => MMC4Mapper::new,
    11 => ColorDreamsMapper::new,
    12 => Mapper12::new,
    13 => CpromMapper::new,
    14 => Mapper14::new,
    15 => Multicart15Mapper::new,
    16 => BandaiFcgMapper::new,
    17 => SuperMagicCardMapper::new,
    18 => Mapper18::new,
    19 => Namco163Mapper::new,
    20 => Mapper20::new,
    21 => Vrc2Vrc4Mapper::new,
    22 => Vrc2Vrc4Mapper::new,
    23 => Vrc2Vrc4Mapper::new,
    24 => VRC6Mapper::new,
    25 => Vrc2Vrc4Mapper::new,
    26 => VRC6Mapper::new,
    27 => Vrc2Vrc4Mapper::new,
    28 => Mapper28::new,
    29 => Mapper29::new,
    30 => Mapper30::new,
    31 => Mapper31::new,
    319 => Mapper319::new,
    320 => Mapper320::new,
    327 => Mapper327::new,
    328 => Mapper328::new,
    329 => Mapper329::new,
    330 => Mapper330::new,
    332 => Mapper332::new,
    335 => Mapper335::new,
    324 => Mapper324::new,
    326 => Mapper326::new,
    32 => IremG101Mapper::new,
    33 => TaitoTc0190Mapper::new,
    34 => BnromNinaMapper::new,
    35 => Mapper35::new,
    339 => Mapper339::new,
    338 => Mapper338::new,
    340 => Mapper340::new,
    341 => Mapper341::new,
    344 => Mapper344::new,
    345 => Mapper345::new,
    346 => Mapper346::new,
    347 => Mapper347::new,
    348 => Mapper348::new,
    349 => Mapper349::new,
    350 => Mapper350::new,
    36 => Mapper36::new,
    37 => Mapper37::new,
    38 => Mapper38::new,
    39 => Mapper39::new,
    40 => Ntdec2722Mapper::new,
    41 => Mapper41::new,
    42 => Mapper42::new,
    43 => Mapper43::new,
    44 => Mapper44::new,
    45 => Mapper45::new,
    46 => Mapper46::new,
    47 => Mapper47::new,
    48 => Mapper48::new,
    49 => Mapper49::new,
    50 => Mapper50::new,
    51 => Mapper51::new,
    52 => Mapper52::new,
    53 => Mapper53::new,
    54 => Mapper54::new,
    55 => Mapper55::new,
    56 => Mapper56::new,
    57 => Mapper57::new,
    58 => Mapper58::new,
    59 => Mapper59::new,
    60 => Mapper60::new,
    342 => Mapper342::new,
    343 => Mapper60::new,
    61 => Mapper61::new,
    62 => Mapper62::new,
    63 => Mapper63::new,
    64 => Mapper64::new,
    65 => Mapper65::new,
    66 => GxROMMapper::new,
    67 => Mapper67::new,
    68 => Sunsoft4Mapper::new,
    69 => SunsoftFme7Mapper::new,
    70 => Mapper70::new,
    71 => CamericaMapper::new,
    72 => Mapper72::new,
    73 => Mapper73::new,
    74 => Mapper74::new,
    75 => Mapper75::new,
    76 => Mapper76::new,
    77 => Mapper77::new,
    78 => NinaTengenMapper::new,
    79 => Mapper79::new,
    80 => Mapper80::new,
    81 => Mapper81::new,
    82 => Mapper82::new,
    83 => Mapper83::new,
    84 => Ntdec2722Mapper::new,
    85 => VRC7Mapper::new,
    86 => Mapper86::new,
    87 => Mapper87::new,
    88 => Mapper88::new,
    90 => Mapper90::new,
    91 => Mapper91::new,
    93 => Mapper93::new,
    95 => Mapper95::new,
    96 => Mapper96::new,
    100 => Mapper100::new,
    101 => Mapper101::new,
    102 => NROMMapper::new,
    104 => Mapper104::new,
    105 => Mapper105::new,
    115 => Mapper115::new,
    117 => Mapper117::new,
    129 => Mapper58::new,
    132 => Mapper132::new,
    133 => Mapper133::new,
    140 => Mapper140::new,
    155 => MMC1Mapper::new,
    185 => Mapper185::new,
    205 => Mapper205::new,
    206 => Namco118Mapper::new,
    241 => Mapper241::new,
    242 => Mapper242::new,
    243 => Mapper243::new,
    244 => Mapper244::new,
    245 => Mapper245::new,
    246 => Mapper246::new,
    251 => Mapper251::new,
    254 => Mapper254::new,
    255 => Mapper255::new,
    302 => Mapper302::new,
    307 => Mapper307::new,
}
#[cfg(test)]
const SUPPORTED_MAPPERS: &[u16] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 90, 91, 93, 95, 96, 100, 101, 102, 104,
    115, 117, 129, 132, 133, 140, 155, 185, 205, 206, 241, 242, 243, 244, 245, 246, 251, 254, 255,
    302, 307, 319, 320, 324, 326, 327, 328, 329, 330, 332, 335, 338, 339, 340, 342, 343, 344, 345,
    346, 347, 348, 349, 350,
];

/// List of supported iNES mapper IDs handled by the factory.
#[cfg(test)]
pub fn supported_mappers() -> &'static [u16] {
    SUPPORTED_MAPPERS
}

/// Create a mapper instance based on mapper metadata.
pub fn create_mapper(metadata: MapperContext) -> io::Result<Box<dyn Mapper>> {
    let mapper_number = metadata.mapper;
    create_registry_mapper(metadata).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Mapper {} not implemented", mapper_number),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;

    #[test]
    fn test_supported_mappers_contains_common_ids() {
        let supported = supported_mappers();
        assert!(supported.contains(&0));
        assert!(supported.contains(&1));
        assert!(supported.contains(&2));
        assert!(supported.contains(&3));
        assert!(supported.contains(&4));
        assert!(supported.contains(&5));
        assert!(supported.contains(&7));
        assert!(supported.contains(&8));
        assert!(supported.contains(&95));
    }

    #[test]
    fn create_mapper_accepts_mapper_95() {
        let prg_rom = vec![0u8; 8 * 1024 * 8];
        let chr_rom = vec![0u8; 1024 * 16];
        let metadata = MapperContext::new_for_test(95, prg_rom, chr_rom, NametableLayout::Vertical);

        let mapper = create_mapper(metadata).expect("Mapper 95 should be implemented");
        assert!(!mapper.capabilities().has_irq);
        assert!(mapper.capabilities().has_chr_banking);
    }

    #[test]
    fn create_mapper_uses_metadata_for_mmc5_prg_ram_size() {
        let prg_rom = vec![0u8; 8 * 1024 * 4];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata = MapperContext {
            prg_ram_banks_8k: 2,
            ..MapperContext::new_for_test(5, prg_rom, chr_rom, NametableLayout::Horizontal)
        };

        let mapper = create_mapper(metadata).expect("MMC5 mapper should be created");
        assert_eq!(mapper.wram_size(), 16 * 1024);
    }

    #[test]
    fn create_mapper_accepts_mapper_8_as_mapper_6_alias() {
        // Given mapper 8 metadata (iNES synonym for mapper 6 submapper 4)
        // and PRG-ROM where each 16 KiB bank returns its own index byte.
        let prg_rom = (0u8..16)
            .flat_map(|bank| std::iter::repeat_n(bank, 16 * 1024))
            .collect();
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(8, prg_rom, chr_rom, NametableLayout::Horizontal);

        // When creating a mapper instance
        let mut mapper =
            create_mapper(metadata).expect("Mapper 8 should be created as mapper 6 alias");

        // and selecting 32 KiB PRG bank 1 using bits 5-4 (mode 4 behavior)
        mapper.write_prg(0x8000, 0x10);

        // Then mapper 8 follows mapper 6 submapper 4 PRG behavior:
        // lower half -> 16 KiB bank 2, upper half -> 16 KiB bank 3.
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);

        // Then mapper 6-specific capabilities are available
        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(caps.has_chr_banking);
        assert!(caps.trainer_jsr);
    }

    #[test]
    fn mapper_8_reads_chr_rom_banks_in_mode_4() {
        // Given mapper 8 metadata with 4 x 8 KiB CHR-ROM banks.
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = (0u8..4)
            .flat_map(|bank| std::iter::repeat_n(0x10 + bank, 8 * 1024))
            .collect();
        let metadata =
            MapperContext::new_for_test(8, prg_rom, chr_rom, NametableLayout::Horizontal);

        // When selecting CHR bank 1 through mode 4 latch bits 1-0
        let mut mapper =
            create_mapper(metadata).expect("Mapper 8 should be created as mapper 6 alias");
        mapper.write_prg(0x8000, 0x01);

        // Then reads come from CHR-ROM bank 1.
        assert_eq!(mapper.read_chr(0x0000), 0x11);
    }

    #[test]
    fn create_mapper_accepts_mapper_129_as_mapper_58_alias() {
        let prg_rom = (0u8..8)
            .flat_map(|bank| std::iter::repeat_n(bank, 16 * 1024))
            .collect();
        let chr_rom = (0u8..8)
            .flat_map(|bank| std::iter::repeat_n(0x10 + bank, 8 * 1024))
            .collect();
        let metadata =
            MapperContext::new_for_test(129, prg_rom, chr_rom, NametableLayout::Vertical);

        let mut mapper = create_mapper(metadata)
            .expect("Mapper 129 should be created as mapper 58-compatible alias");

        mapper.write_prg(0x8080, 0);

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    fn write_mmc1_serial_register(mapper: &mut dyn Mapper, register_addr: u16, register_value: u8) {
        for shift in 0..5 {
            mapper.write_prg(register_addr, (register_value >> shift) & 1);
            mapper.cpu_cycle();
            mapper.cpu_cycle();
        }
    }

    #[test]
    fn create_mapper_accepts_mapper_155_as_mmc1a_alias() {
        let prg_rom = vec![0u8; 256 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(155, prg_rom, chr_rom, NametableLayout::Horizontal);

        let mut mapper = create_mapper(metadata)
            .expect("Mapper 155 should be created as MMC1A-compatible alias");

        mapper.write_prg(0x6000, 0x12);
        assert_eq!(mapper.read_prg(0x6000), 0x12);

        write_mmc1_serial_register(mapper.as_mut(), 0xE000, 0b1_0000);

        mapper.write_prg(0x6000, 0x34);
        assert_eq!(mapper.read_prg(0x6000), 0x34);
    }

    #[test]
    fn create_mapper_accepts_mapper_100_as_mmc3_compatible() {
        let prg_rom = vec![0u8; 8 * 1024 * 48];
        let chr_rom = vec![0u8; 1024 * 96];
        let metadata =
            MapperContext::new_for_test(100, prg_rom, chr_rom, NametableLayout::Horizontal);

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 100 should be created");
    }

    #[test]
    fn mapper_100_matches_mmc3_bank_mirroring_and_irq_capabilities() {
        let prg_rom = (0u8..48)
            .flat_map(|bank| std::iter::repeat_n(bank, 8 * 1024))
            .collect();
        let chr_rom = (0u8..96)
            .flat_map(|bank| std::iter::repeat_n(bank, 1024))
            .collect();
        let mut mapper = create_mapper(MapperContext::new_for_test(
            100,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 100 should be created");

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);

        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(!caps.has_expansion_audio);
    }

    #[test]
    fn supported_mappers_includes_mapper_100() {
        assert!(supported_mappers().contains(&100));
    }

    #[test]
    fn create_mapper_accepts_mapper_102() {
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(102, prg_rom, chr_rom, NametableLayout::Horizontal);

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 102 should be created");
    }

    #[test]
    fn supported_mappers_includes_mapper_102() {
        assert!(supported_mappers().contains(&102));
    }

    #[test]
    fn mapper_102_uses_nrom_fixed_mapping_behavior() {
        let mut prg_rom = vec![0; 16 * 1024];
        prg_rom[0x0000] = 0x12;
        prg_rom[0x3FFF] = 0x34;
        let mut mapper = create_mapper(MapperContext::new_for_test(
            102,
            prg_rom,
            vec![0x5A; 8 * 1024],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 102 should be created");

        mapper.write_prg(0x8000, 0xFF);

        assert_eq!(mapper.read_prg(0x8000), 0x12);
        assert_eq!(mapper.read_prg(0xBFFF), 0x34);
        assert_eq!(mapper.read_prg(0xC000), 0x12);
        assert_eq!(mapper.read_prg(0xFFFF), 0x34);
        assert_eq!(mapper.read_chr(0x0000), 0x5A);
    }

    #[test]
    fn create_mapper_accepts_mapper_105_nes_event() {
        let prg_rom = vec![0u8; 16 * 1024 * 16];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(105, prg_rom, chr_rom, NametableLayout::Horizontal);

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 105 should be created");
    }

    #[test]
    fn create_mapper_accepts_mapper_344_gn26() {
        let metadata = MapperContext::new_for_test(
            344,
            vec![0u8; 256 * 1024],
            vec![0u8; 128 * 1024],
            NametableLayout::Vertical,
        );

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 344 (GN-26) should be created");
    }

    #[test]
    fn create_mapper_accepts_mapper_340_k3036() {
        let metadata = MapperContext::new_for_test(
            340,
            vec![0u8; 16 * 1024 * 32],
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        );

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 340 (BMC-K-3036) should be created");
    }

    #[test]
    fn create_mapper_accepts_mapper_339_k3006() {
        let metadata = MapperContext::new_for_test(
            339,
            vec![0u8; 16 * 1024 * 32],
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        );

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 339 (BMC-K-3006) should be created");
    }

    #[test]
    fn supported_mappers_includes_mapper_339() {
        let supported = supported_mappers();
        assert!(supported.contains(&339));
    }

    #[test]
    fn mapper_340_supports_unrom_and_nrom128_modes_and_address_mirroring_latch() {
        let prg_rom = (0u8..32)
            .flat_map(|bank| std::iter::repeat_n(bank, 16 * 1024))
            .collect();
        let metadata = MapperContext::new_for_test(340, prg_rom, vec![], NametableLayout::Vertical);
        let mut mapper = create_mapper(metadata).expect("Mapper 340 should be created");

        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8025, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x8040, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn create_mapper_accepts_mapper_341_tj03() {
        let metadata = MapperContext::new_for_test(
            341,
            vec![0u8; 16 * 1024 * 8],
            vec![0u8; 8 * 1024 * 8],
            NametableLayout::Vertical,
        );

        let result = create_mapper(metadata);
        assert!(result.is_ok(), "Mapper 341 (BMC-TJ-03) should be created");
    }

    #[test]
    fn mapper_341_reports_no_irq_and_no_expansion_audio() {
        let metadata = MapperContext::new_for_test(
            341,
            vec![0u8; 16 * 1024 * 8],
            vec![0u8; 8 * 1024 * 8],
            NametableLayout::Vertical,
        );
        let mapper = create_mapper(metadata).expect("Mapper 341 should be created");
        let caps = mapper.capabilities();

        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
    }

    #[test]
    fn create_mapper_accepts_mapper_342_coolgirl() {
        let metadata = MapperContext::new_for_test(
            342,
            vec![0u8; 256 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        );

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 342 (COOLGIRL) should be created");
    }

    #[test]
    fn test_ppu_address_changed_default_is_noop() {
        // Test that simple mappers can use the default no-op implementation
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(0, prg_rom, chr_rom, NametableLayout::Horizontal);

        let mut mapper = create_mapper(metadata).expect("NROM mapper should be created");

        // Call ppu_address_changed - should be a no-op and not panic
        mapper.ppu_address_changed(0x0000);
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_address_changed(0x1FFF);
    }

    // --- MapperCapabilities tests ---

    fn make_mapper(id: u16) -> Box<dyn Mapper> {
        let prg_size = 32 * 1024; // Use 32KB PRG-ROM for these tests (MMC5 and others)
        let prg_rom = vec![0u8; prg_size];
        let chr_rom = vec![0u8; 8 * 1024];
        let metadata =
            MapperContext::new_for_test(id, prg_rom, chr_rom, NametableLayout::Horizontal);
        create_mapper(metadata).unwrap_or_else(|_| panic!("Mapper {} should be created", id))
    }

    // Capability: has_irq

    #[test]
    fn nrom_reports_no_irq() {
        assert!(!make_mapper(0).capabilities().has_irq);
    }

    #[test]
    fn mmc3_reports_irq_capability() {
        assert!(make_mapper(4).capabilities().has_irq);
    }

    #[test]
    fn mmc5_reports_irq_capability() {
        assert!(make_mapper(5).capabilities().has_irq);
    }

    #[test]
    fn sunsoft_fme7_reports_irq_capability() {
        assert!(make_mapper(69).capabilities().has_irq);
    }

    // Capability: has_chr_banking

    #[test]
    fn nrom_reports_no_chr_banking() {
        assert!(!make_mapper(0).capabilities().has_chr_banking);
    }

    #[test]
    fn mmc1_reports_chr_banking() {
        assert!(make_mapper(1).capabilities().has_chr_banking);
    }

    #[test]
    fn mmc3_reports_chr_banking() {
        assert!(make_mapper(4).capabilities().has_chr_banking);
    }

    // Capability: has_dynamic_mirroring

    #[test]
    fn nrom_reports_no_dynamic_mirroring() {
        assert!(!make_mapper(0).capabilities().has_dynamic_mirroring);
    }

    #[test]
    fn mmc1_reports_dynamic_mirroring() {
        assert!(make_mapper(1).capabilities().has_dynamic_mirroring);
    }

    #[test]
    fn axrom_reports_dynamic_mirroring() {
        assert!(make_mapper(7).capabilities().has_dynamic_mirroring);
    }

    // Capability: has_expansion_audio

    #[test]
    fn nrom_reports_no_expansion_audio() {
        assert!(!make_mapper(0).capabilities().has_expansion_audio);
    }

    #[test]
    fn mmc5_reports_expansion_audio() {
        assert!(make_mapper(5).capabilities().has_expansion_audio);
    }

    #[test]
    fn vrc6_reports_expansion_audio() {
        assert!(make_mapper(24).capabilities().has_expansion_audio);
    }

    #[test]
    fn namco163_reports_expansion_audio() {
        assert!(make_mapper(19).capabilities().has_expansion_audio);
    }

    // Capability: max_prg_ram_kb

    #[test]
    fn bandai_fcg_reports_zero_prg_ram() {
        assert_eq!(make_mapper(16).capabilities().max_prg_ram_kb, 0);
    }

    #[test]
    fn mmc1_reports_8kb_prg_ram() {
        assert_eq!(make_mapper(1).capabilities().max_prg_ram_kb, 8);
    }

    #[test]
    fn mmc5_reports_64kb_prg_ram() {
        assert_eq!(make_mapper(5).capabilities().max_prg_ram_kb, 64);
    }

    // Capability: prg_bank_size_kb

    #[test]
    fn nrom_reports_32kb_prg_bank_size() {
        assert_eq!(make_mapper(0).capabilities().prg_bank_size_kb, 32);
    }

    #[test]
    fn mmc3_reports_8kb_prg_bank_size() {
        assert_eq!(make_mapper(4).capabilities().prg_bank_size_kb, 8);
    }

    #[test]
    fn uxrom_reports_16kb_prg_bank_size() {
        assert_eq!(make_mapper(2).capabilities().prg_bank_size_kb, 16);
    }

    // Capability: chr_bank_size_kb

    #[test]
    fn nrom_reports_8kb_chr_bank_size() {
        assert_eq!(make_mapper(0).capabilities().chr_bank_size_kb, 8);
    }

    #[test]
    fn mmc3_reports_1kb_chr_bank_size() {
        assert_eq!(make_mapper(4).capabilities().chr_bank_size_kb, 1);
    }

    #[test]
    fn mmc1_reports_4kb_chr_bank_size() {
        assert_eq!(make_mapper(1).capabilities().chr_bank_size_kb, 4);
    }

    // Cross-cutting: all mappers return valid capabilities

    #[test]
    fn all_supported_mappers_return_capabilities() {
        for &id in supported_mappers() {
            let mapper = make_mapper(id);
            let caps = mapper.capabilities();
            // Verify the capabilities struct is populated (no panics)
            let _ = format!("{:?}", caps);
        }
    }

    // Conditional test execution: skip tests for mappers without IRQ

    #[test]
    fn irq_capable_mappers_report_irq_pending_false_initially() {
        for &id in supported_mappers() {
            let mapper = make_mapper(id);
            if mapper.capabilities().has_irq {
                assert!(
                    !mapper.irq_pending(),
                    "Mapper {} should not have IRQ pending initially",
                    id
                );
            }
        }
    }

    #[test]
    fn expansion_audio_mappers_return_silent_initially() {
        for &id in supported_mappers() {
            let mapper = make_mapper(id);
            if mapper.capabilities().has_expansion_audio {
                assert_eq!(
                    mapper.expansion_audio_sample(),
                    0.0,
                    "Mapper {} with expansion audio should be silent initially",
                    id
                );
            }
        }
    }

    #[test]
    fn prg_ram_capable_mappers_report_nonzero_wram_size() {
        for &id in supported_mappers() {
            let mapper = make_mapper(id);
            let caps = mapper.capabilities();
            if caps.max_prg_ram_kb > 0 {
                assert!(
                    mapper.wram_size() > 0,
                    "Mapper {} reports {}KB PRG-RAM capability but wram_size() is 0",
                    id,
                    caps.max_prg_ram_kb
                );
            }
        }
    }

    // --- Acceptance tests for composable mapper traits ---

    fn assert_core_contract<T: Mapper>(mapper: &mut T) {
        let _ = mapper.read_prg(0x8000);
        mapper.write_prg(0x8000, 0x12);
        let _ = mapper.read_chr(0x0000);
        mapper.write_chr(0x0000, 0x34);
        let _ = mapper.get_mirroring();
    }

    fn assert_irq_contract<T: Mapper>(mapper: &mut T) {
        mapper.cpu_cycle();
        let _ = mapper.irq_pending();
    }

    fn assert_ppu_extension_contract<T: Mapper>(mapper: &mut T) {
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_scanline(42, true);
    }

    fn assert_audio_contract<T: Mapper>(mapper: &mut T) {
        let _ = mapper.expansion_audio_sample();
    }

    fn assert_state_contract<T: Mapper>(mapper: &mut T) {
        let wram = mapper.wram_snapshot();
        mapper.load_wram_snapshot(&wram);
        let prg = mapper.prg_ram_snapshot();
        mapper.restore_prg_ram(&prg);
        let chr = mapper.chr_ram_snapshot();
        mapper.restore_chr_ram(&chr);
        let registers = mapper.registers_snapshot();
        mapper.restore_registers(&registers);
    }

    fn assert_composable_contract<T: Mapper>(mapper: &mut T) {
        let _ = mapper.wram_size();
        let _ = mapper.prg_ram_snapshot();
    }

    #[test]
    fn nrom_satisfies_core_and_state_traits() {
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_core_contract(&mut mapper);
        assert_state_contract(&mut mapper);
        assert_composable_contract(&mut mapper);
    }

    #[test]
    fn mmc3_satisfies_core_irq_ppu_and_state_traits() {
        let mut mapper = MMC3Mapper::new(MapperContext::new_for_test(
            4,
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_core_contract(&mut mapper);
        assert_irq_contract(&mut mapper);
        assert_ppu_extension_contract(&mut mapper);
        assert_state_contract(&mut mapper);
    }

    #[test]
    fn vrc6_satisfies_core_irq_audio_and_state_traits() {
        let mut mapper = VRC6Mapper::new(MapperContext::new_for_test(
            24,
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_core_contract(&mut mapper);
        assert_irq_contract(&mut mapper);
        assert_audio_contract(&mut mapper);
        assert_state_contract(&mut mapper);
    }
}
