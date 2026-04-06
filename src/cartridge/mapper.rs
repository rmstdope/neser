use crate::cartridge::NametableLayout;
use crate::cartridge::hardware_type::HardwareType;
use crate::cartridge::ines::ParsedRom;
use crate::cartridge::rom_db::VsHardwareType;
use std::io;

// Nintendo mappers
use super::nintendo::axrom::AxROMMapper;
use super::nintendo::bnrom_nina::BnromNinaMapper;
use super::nintendo::cnrom::CNROMMapper;
use super::nintendo::cnrom_security::CnromSecurityMapper;
use super::nintendo::cprom::CpromMapper;
use super::nintendo::fds::FdsMapper;
use super::nintendo::gxrom::GxROMMapper;
use super::nintendo::mapper100::Mapper100;
use super::nintendo::mmc1::MMC1Mapper;
use super::nintendo::mmc2::MMC2Mapper;
use super::nintendo::mmc3::MMC3Mapper;
use super::nintendo::mmc4::MMC4Mapper;
use super::nintendo::mmc5::MMC5Mapper;
use super::nintendo::nes_event::NesEventMapper;
use super::nintendo::nrom::NROMMapper;
use super::nintendo::tqrom::TqromMapper;
use super::nintendo::txsrom::TxsromMapper;
use super::nintendo::un1rom::Un1romMapper;
use super::nintendo::uxrom::UxROMMapper;
use super::nintendo::uxrom_inverted::UxromInvertedMapper;

// Konami mappers
use super::konami::vrc1::Vrc1Mapper;
use super::konami::vrc2_vrc4::Vrc2Vrc4Mapper;
use super::konami::vrc3::Vrc3Mapper;
use super::konami::vrc6::VRC6Mapper;
use super::konami::vrc7::VRC7Mapper;

// Namco mappers
use super::namco::namco118::Namco118Mapper;
use super::namco::namco163::Namco163Mapper;
use super::namco::namcot_3425::Namcot3425Mapper;
use super::namco::namcot_3443::Namcot3443Mapper;
use super::namco::namcot_3446::Namcot3446Mapper;

// Bandai mappers
use super::bandai::bandai_fcg::BandaiFcgMapper;
use super::bandai::mapper70::Mapper70;
use super::bandai::mapper96::Mapper96;

// Sunsoft mappers
use super::sunsoft::sunsoft_2::Sunsoft2Mapper;
use super::sunsoft::sunsoft_3::Sunsoft3Mapper;
use super::sunsoft::sunsoft_4::Sunsoft4Mapper;
use super::sunsoft::sunsoft_early::SunsoftEarlyMapper;
use super::sunsoft::sunsoft_fme7::SunsoftFme7Mapper;

// Taito mappers
use super::taito::taito_tc0190::TaitoTc0190Mapper;
use super::taito::taito_tc0350::TaitoTc0350Mapper;
use super::taito::taito_x1005::TaitoX1005Mapper;
use super::taito::taito_x1017::TaitoX1017Mapper;

// Jaleco mappers
use super::jaleco::jaleco_jf10::JalecoJf10Mapper;
use super::jaleco::jaleco_jf11::JalecoJf11Mapper;
use super::jaleco::jaleco_jf13::JalecoJf13Mapper;
use super::jaleco::jaleco_jf17::JalecoJf17Mapper;
use super::jaleco::jaleco_jf19::JalecoJf19Mapper;
use super::jaleco::jaleco_ss88006::JalecoSs88006Mapper;
use super::jaleco::mapper87::Mapper87;

// Irem mappers
use super::irem::irem_g101::IremG101Mapper;
use super::irem::irem_h3001::IremH3001Mapper;
use super::irem::irem_lrog017::IremLrog017Mapper;
use super::irem::irem_tam_s1::IremTamS1Mapper;
use super::irem::nina_tengen::NinaTengenMapper;

// Camerica mappers
use super::camerica::camerica::CamericaMapper;

// Tengen mappers
use super::tengen::tengen_rambo1::TengenRambo1Mapper;

// Sachen mappers
use super::sachen::mapper36::Mapper36;
use super::sachen::mapper132::Mapper132;
use super::sachen::mapper133::Mapper133;
use super::sachen::mapper243::Mapper243;

// Unlicensed/other mappers
#[cfg(test)]
use super::rom_db;
use super::unlicensed::action53::Action53Mapper;
use super::unlicensed::colordreams::ColorDreamsMapper;
use super::unlicensed::jy_company::JyCompanyMapper;
use super::unlicensed::mapper12::Mapper12;
use super::unlicensed::mapper14::Mapper14;
use super::unlicensed::mapper29::Mapper29;
use super::unlicensed::mapper31::Mapper31;
use super::unlicensed::mapper35::Mapper35;
use super::unlicensed::mapper37::Mapper37;
use super::unlicensed::mapper38::Mapper38;
use super::unlicensed::mapper39::Mapper39;
use super::unlicensed::mapper41::Mapper41;
use super::unlicensed::mapper42::Mapper42;
use super::unlicensed::mapper43::Mapper43;
use super::unlicensed::mapper44::Mapper44;
use super::unlicensed::mapper45::Mapper45;
use super::unlicensed::mapper46::Mapper46;
use super::unlicensed::mapper47::Mapper47;
use super::unlicensed::mapper49::Mapper49;
use super::unlicensed::mapper50::Mapper50;
use super::unlicensed::mapper51::Mapper51;
use super::unlicensed::mapper52::Mapper52;
use super::unlicensed::mapper53::Mapper53;
use super::unlicensed::mapper54::Mapper54;
use super::unlicensed::mapper55::Mapper55;
use super::unlicensed::mapper56::Mapper56;
use super::unlicensed::mapper57::Mapper57;
use super::unlicensed::mapper58::Mapper58;
use super::unlicensed::mapper59::Mapper59;
use super::unlicensed::mapper60::Mapper60;
use super::unlicensed::mapper61::Mapper61;
use super::unlicensed::mapper62::Mapper62;
use super::unlicensed::mapper63::Mapper63;
use super::unlicensed::mapper74::Mapper74;
use super::unlicensed::mapper79::Mapper79;
use super::unlicensed::mapper81::Mapper81;
use super::unlicensed::mapper83::Mapper83;
use super::unlicensed::mapper91::Mapper91;
use super::unlicensed::mapper103::Mapper103;
use super::unlicensed::mapper104::Mapper104;
use super::unlicensed::mapper106::Mapper106;
use super::unlicensed::mapper107::Mapper107;
use super::unlicensed::mapper110::Mapper110;
use super::unlicensed::mapper111::GtromMapper;
use super::unlicensed::mapper112::Mapper112;
use super::unlicensed::mapper113::Mapper113;
use super::unlicensed::mapper114::Mapper114;
use super::unlicensed::mapper115::Mapper115;
use super::unlicensed::mapper116::Mapper116;
use super::unlicensed::mapper117::Mapper117;
use super::unlicensed::mapper120::Mapper120;
use super::unlicensed::mapper121::Mapper121;
use super::unlicensed::mapper122::Mapper122;
use super::unlicensed::mapper123::Mapper123;
use super::unlicensed::mapper205::Mapper205;
use super::unlicensed::mapper214::Mapper214;
use super::unlicensed::mapper215::Mapper215;
use super::unlicensed::mapper216::Mapper216;
use super::unlicensed::mapper217::Mapper217;
use super::unlicensed::mapper218::Mapper218;
use super::unlicensed::mapper219::Mapper219;
use super::unlicensed::mapper222::Mapper222;
use super::unlicensed::mapper227::Mapper227;
use super::unlicensed::mapper228::Mapper228;
use super::unlicensed::mapper229::Mapper229;
use super::unlicensed::mapper230::Mapper230;
use super::unlicensed::mapper231::Mapper231;
use super::unlicensed::mapper232::Mapper232;
use super::unlicensed::mapper233::Mapper233;
use super::unlicensed::mapper234::Mapper234;
use super::unlicensed::mapper236::Mapper236;
use super::unlicensed::mapper237::Mapper237;
use super::unlicensed::mapper238::Mapper238;
use super::unlicensed::mapper241::Mapper241;
use super::unlicensed::mapper242::Mapper242;
use super::unlicensed::mapper244::Mapper244;
use super::unlicensed::mapper245::Mapper245;
use super::unlicensed::mapper246::Mapper246;
use super::unlicensed::mapper249::Mapper249;
use super::unlicensed::mapper250::Mapper250;
use super::unlicensed::mapper251::Mapper251;
use super::unlicensed::mapper253::Mapper253;
use super::unlicensed::mapper254::Mapper254;
use super::unlicensed::mapper255::Mapper255;
use super::unlicensed::mapper257::Mapper257;
use super::unlicensed::mapper260::Mapper260;
use super::unlicensed::mapper262::Mapper262;
use super::unlicensed::mapper263::Mapper263;
use super::unlicensed::mapper264::Mapper264;
use super::unlicensed::mapper267::Mapper267;
use super::unlicensed::mapper268::Mapper268;
use super::unlicensed::mapper271::Mapper271;
use super::unlicensed::mapper274::Mapper274;
use super::unlicensed::mapper281::Mapper281;
use super::unlicensed::mapper285::Mapper285;
use super::unlicensed::mapper286::Mapper286;
use super::unlicensed::mapper287::Mapper287;
use super::unlicensed::mapper288::Mapper288;
use super::unlicensed::mapper291::Mapper291;
use super::unlicensed::mapper292::Mapper292;
use super::unlicensed::mapper293::Mapper293;
use super::unlicensed::mapper294::Mapper294;
use super::unlicensed::mapper295::Mapper295;
use super::unlicensed::mapper296::Mapper296;
use super::unlicensed::mapper298::Mapper298;
use super::unlicensed::mapper300::Mapper300;
use super::unlicensed::mapper302::Mapper302;
use super::unlicensed::mapper304::Mapper304;
use super::unlicensed::mapper305::Mapper305;
use super::unlicensed::mapper306::Mapper306;
use super::unlicensed::mapper307::Mapper307;
use super::unlicensed::mapper308::Mapper308;
use super::unlicensed::mapper310::Mapper310;
use super::unlicensed::mapper311::Mapper311;
use super::unlicensed::mapper313::Mapper313;
use super::unlicensed::mapper314::Mapper314;
use super::unlicensed::mapper315::Mapper315;
use super::unlicensed::mapper319::Mapper319;
use super::unlicensed::mapper320::Mapper320;
use super::unlicensed::mapper323::Mapper323;
use super::unlicensed::mapper324::Mapper324;
use super::unlicensed::mapper325::Mapper325;
use super::unlicensed::mapper326::Mapper326;
use super::unlicensed::mapper327::Mapper327;
use super::unlicensed::mapper328::Mapper328;
use super::unlicensed::mapper329::Mapper329;
use super::unlicensed::mapper330::Mapper330;
use super::unlicensed::mapper331::Mapper331;
use super::unlicensed::mapper332::Mapper332;
use super::unlicensed::mapper335::Mapper335;
use super::unlicensed::mapper337::Mapper337;
use super::unlicensed::mapper338::Mapper338;
use super::unlicensed::mapper339::Mapper339;
use super::unlicensed::mapper340::Mapper340;
use super::unlicensed::mapper341::Mapper341;
use super::unlicensed::mapper342::Mapper342;
use super::unlicensed::mapper344::Mapper344;
use super::unlicensed::mapper345::Mapper345;
use super::unlicensed::mapper346::Mapper346;
use super::unlicensed::mapper347::Mapper347;
use super::unlicensed::mapper348::Mapper348;
use super::unlicensed::mapper349::Mapper349;
use super::unlicensed::mapper350::Mapper350;
use super::unlicensed::multicart_15::Multicart15Mapper;
use super::unlicensed::ntdec_2722::Ntdec2722Mapper;
use super::unlicensed::super_magic_card::SuperMagicCardMapper;
use super::unlicensed::unrom512::Unrom512Mapper;

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
    /// Hardware type derived from console type and timing mode.
    pub hardware_type: HardwareType,
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
    /// CHR-RAM size in bytes from NES 2.0 header (None = use default 8 KiB).
    pub chr_ram_size_bytes: Option<usize>,
    /// CRC32 of concatenated PRG/CHR; may be overridden for tests.
    pub crc32: u32,
    /// VS System hardware type for game-specific protection quirks.
    pub vs_hardware_type: Option<VsHardwareType>,
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
            hardware_type: HardwareType::from_console_type_and_timing(
                info.console_type,
                info.timing_mode,
            ),
            prg_rom: parsed.prg_rom.clone(),
            chr_rom: parsed.chr_rom.clone(),
            prg_ram_banks_8k: Self::prg_ram_banks_8k_total(
                info.prg_ram_size_bytes,
                info.prg_nvram_size_bytes,
            ),
            prg_ram_size_specified: info.prg_ram_size_bytes.is_some()
                || info.prg_nvram_size_bytes.is_some(),
            battery_backed_prg_ram: info.battery_backed_prg_ram,
            chr_ram_size_bytes: match (info.chr_ram_size_bytes, info.chr_nvram_size_bytes) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            },
            crc32: parsed.crc32,
            vs_hardware_type: parsed.header.vs_hardware_type.map(VsHardwareType::from_raw),
        }
    }

    fn prg_ram_banks_8k_total(
        prg_ram_size_bytes: Option<usize>,
        prg_nvram_size_bytes: Option<usize>,
    ) -> u8 {
        // Both PRG-RAM and PRG-NVRAM sizes are 0/unspecified in ParsedRom metadata
        // → use default PRG-RAM bank count (1 × 8KB) for callers that ignore
        //    prg_ram_size_specified.
        if prg_ram_size_bytes.is_none() && prg_nvram_size_bytes.is_none() {
            return DEFAULT_PRG_RAM_BANKS_8K;
        }
        let bytes = prg_ram_size_bytes
            .unwrap_or(0)
            .max(prg_nvram_size_bytes.unwrap_or(0));
        bytes.div_ceil(PRG_RAM_BANK_SIZE).min(u8::MAX as usize) as u8
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
            hardware_type: HardwareType::NesNtsc,
            prg_rom,
            chr_rom,
            prg_ram_banks_8k: 1,
            prg_ram_size_specified: true,
            battery_backed_prg_ram: false,
            chr_ram_size_bytes: None,
            crc32,
            vs_hardware_type: None,
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
    18 => JalecoSs88006Mapper::new,
    19 => Namco163Mapper::new,
    20 => FdsMapper::new,
    21 => Vrc2Vrc4Mapper::new,
    22 => Vrc2Vrc4Mapper::new,
    23 => Vrc2Vrc4Mapper::new,
    24 => VRC6Mapper::new,
    25 => Vrc2Vrc4Mapper::new,
    26 => VRC6Mapper::new,
    27 => Vrc2Vrc4Mapper::new,
    28 => Action53Mapper::new,
    29 => Mapper29::new,
    30 => Unrom512Mapper::new,
    31 => Mapper31::new,
    300 => Mapper300::new,
    315 => Mapper315::new,
    319 => Mapper319::new,
    320 => Mapper320::new,
    323 => Mapper323::new,
    327 => Mapper327::new,
    328 => Mapper328::new,
    329 => Mapper329::new,
    330 => Mapper330::new,
    331 => Mapper331::new,
    332 => Mapper332::new,
    335 => Mapper335::new,
    337 => Mapper337::new,
    324 => Mapper324::new,
    325 => Mapper325::new,
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
    48 => TaitoTc0350Mapper::new,
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
    64 => TengenRambo1Mapper::new,
    65 => IremH3001Mapper::new,
    66 => GxROMMapper::new,
    67 => Sunsoft3Mapper::new,
    68 => Sunsoft4Mapper::new,
    69 => SunsoftFme7Mapper::new,
    70 => Mapper70::new,
    71 => CamericaMapper::new,
    72 => JalecoJf17Mapper::new,
    73 => Vrc3Mapper::new,
    74 => Mapper74::new,
    75 => Vrc1Mapper::new,
    76 => Namcot3446Mapper::new,
    77 => IremLrog017Mapper::new,
    78 => NinaTengenMapper::new,
    79 => Mapper79::new,
    80 => TaitoX1005Mapper::new,
    81 => Mapper81::new,
    82 => TaitoX1017Mapper::new,
    83 => Mapper83::new,
    84 => Ntdec2722Mapper::new,
    85 => VRC7Mapper::new,
    86 => JalecoJf13Mapper::new,
    87 => Mapper87::new,
    88 => Namcot3443Mapper::new,
    89 => SunsoftEarlyMapper::new,
    90 => JyCompanyMapper::new,
    91 => Mapper91::new,
    92 => JalecoJf19Mapper::new,
    93 => Sunsoft2Mapper::new,
    94 => Un1romMapper::new,
    95 => Namcot3425Mapper::new,
    96 => Mapper96::new,
    97 => IremTamS1Mapper::new,
    100 => Mapper100::new,
    101 => JalecoJf10Mapper::new,
    102 => NROMMapper::new,
    103 => Mapper103::new,
    104 => Mapper104::new,
    105 => NesEventMapper::new,
    106 => Mapper106::new,
    107 => Mapper107::new,
    110 => Mapper110::new,
    111 => GtromMapper::new,
    112 => Mapper112::new,
    113 => Mapper113::new,
    114 => Mapper114::new,
    115 => Mapper115::new,
    116 => Mapper116::new,
    117 => Mapper117::new,
    118 => TxsromMapper::new,
    119 => TqromMapper::new,
    120 => Mapper120::new,
    121 => Mapper121::new,
    122 => Mapper122::new,
    123 => Mapper123::new,
    129 => Mapper58::new,
    132 => Mapper132::new,
    133 => Mapper133::new,
    140 => JalecoJf11Mapper::new,
    155 => MMC1Mapper::new,
    180 => UxromInvertedMapper::new,
    185 => CnromSecurityMapper::new,
    205 => Mapper205::new,
    206 => Namco118Mapper::new,
    214 => Mapper214::new,
    215 => Mapper215::new,
    216 => Mapper216::new,
    217 => Mapper217::new,
    218 => Mapper218::new,
    219 => Mapper219::new,
    // 220: FCEUX debug mapper — not real hardware, never implement.
    222 => Mapper222::new,
    227 => Mapper227::new,
    228 => Mapper228::new,
    229 => Mapper229::new,
    230 => Mapper230::new,
    231 => Mapper231::new,
    232 => Mapper232::new,
    233 => Mapper233::new,
    234 => Mapper234::new,
    236 => Mapper236::new,
    237 => Mapper237::new,
    238 => Mapper238::new,
    241 => Mapper241::new,
    242 => Mapper242::new,
    243 => Mapper243::new,
    244 => Mapper244::new,
    245 => Mapper245::new,
    246 => Mapper246::new,
    249 => Mapper249::new,
    250 => Mapper250::new,
    251 => Mapper251::new,
    253 => Mapper253::new,
    254 => Mapper254::new,
    255 => Mapper255::new,
    257 => Mapper257::new,
    260 => Mapper260::new,
    262 => Mapper262::new,
    263 => Mapper263::new,
    264 => Mapper264::new,
    267 => Mapper267::new,
    268 => Mapper268::new,
    271 => Mapper271::new,
    274 => Mapper274::new,
    281 => Mapper281::new,
    285 => Mapper285::new,
    292 => Mapper292::new,
    293 => Mapper293::new,
    291 => Mapper291::new,
    294 => Mapper294::new,
    295 => Mapper295::new,
    296 => Mapper296::new,
    298 => Mapper298::new,
    288 => Mapper288::new,
    287 => Mapper287::new,
    286 => Mapper286::new,
    302 => Mapper302::new,
    304 => Mapper304::new,
    305 => Mapper305::new,
    306 => Mapper306::new,
    307 => Mapper307::new,
    308 => Mapper308::new,
    310 => Mapper310::new,
    311 => Mapper311::new,
    313 => Mapper313::new,
    314 => Mapper314::new,
}

#[allow(clippy::style)]
#[cfg(test)]
const SUPPORTED_MAPPERS: &[u16] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96,
    100, 101, 102, 103, 104, 106, 110, 114, 115, 117, 118, 120, 121, 122, 123, 129, 132, 133, 140,
    155, 180, 185, 205, 206, 214, 216, 217, 218, 219, 222, 227, 228, 229, 230, 231, 232, 233, 234,
    236, 237, 238, 241, 242, 243, 244, 245, 246, 249, 250, 251, 253, 254, 255, 257, 260, 262, 263,
    264, 267, 268, 271, 274, 281, 285, 286, 287, 288, 291, 292, 293, 294, 295, 296, 300, 302, 304,
    305, 306, 307, 308, 310, 311, 313, 314, 315, 319, 320, 323, 324, 326, 327, 328, 329, 330, 331,
    332, 335, 337, 338, 339, 340, 342, 343, 344, 345, 346, 347, 348, 349, 350,
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
    fn create_mapper_accepts_mapper_106() {
        let prg_rom = vec![0u8; 8 * 1024 * 48];
        let chr_rom = vec![0u8; 1024 * 64];
        let metadata =
            MapperContext::new_for_test(106, prg_rom, chr_rom, NametableLayout::Horizontal);

        let result = create_mapper(metadata);

        assert!(result.is_ok(), "Mapper 106 should be created");
    }

    #[test]
    fn mapper_106_switches_prg_chr_and_reports_irq_without_expansion_audio() {
        let prg_rom = (0u8..48)
            .flat_map(|bank| std::iter::repeat_n(bank, 8 * 1024))
            .collect();
        let chr_rom = (0u8..64)
            .flat_map(|bank| std::iter::repeat_n(bank, 1024))
            .collect();
        let mut mapper = create_mapper(MapperContext::new_for_test(
            106,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 106 should be created");

        mapper.write_prg(0x8008, 0x03);
        mapper.write_prg(0x8009, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 0x13);
        assert_eq!(mapper.read_prg(0xA000), 0x05);

        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x04);
        assert_eq!(mapper.read_chr(0x0000), 0x02);
        assert_eq!(mapper.read_chr(0x0400), 0x05);

        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(!caps.has_expansion_audio);
    }

    #[test]
    fn supported_mappers_includes_mapper_106() {
        assert!(supported_mappers().contains(&106));
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
    fn create_mapper_accepts_mapper_122() {
        let metadata = MapperContext::new_for_test(
            122,
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024 * 4],
            NametableLayout::Horizontal,
        );
        let result = create_mapper(metadata);
        assert!(result.is_ok(), "Mapper 122 should be created");
    }

    #[test]
    fn supported_mappers_includes_mapper_122() {
        assert!(supported_mappers().contains(&122));
    }

    #[test]
    fn create_mapper_accepts_mapper_218() {
        let metadata = MapperContext::new_for_test(
            218,
            vec![0u8; 32 * 1024],
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        );
        let result = create_mapper(metadata);
        assert!(result.is_ok(), "Mapper 218 should be created");
    }

    #[test]
    fn supported_mappers_includes_mapper_218() {
        assert!(supported_mappers().contains(&218));
    }

    #[test]
    fn mapper_102_behaves_as_nrom() {
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
        // mapper 16 now supports PRG-RAM when the header specifies it;
        // with the default test context (1 × 8KB), expect 8KB.
        assert_eq!(make_mapper(16).capabilities().max_prg_ram_kb, 8);
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

    // --- NES 2.0 NVRAM allocation tests ---

    #[test]
    fn from_parsed_rom_allocates_prg_ram_from_nvram_when_volatile_absent() {
        use crate::cartridge::ines::{ConsoleType, InesHeader, ParsedRom, TimingMode};

        // Given: NES 2.0 ROM where only NVRAM (battery-backed) is present — no volatile PRG-RAM.
        // This is the S8K holy-mapperel pattern: header byte 10 = 0x70 → volatile=0, NVRAM=8KB.
        let header = InesHeader {
            mapper: 69,
            submapper: 0,
            console_type: ConsoleType::NesFamicom,
            mirroring: NametableLayout::Horizontal,
            has_trainer: false,
            header_version: "2.0",
            battery_backed_prg_ram: true,
            prg_rom_size_bytes: 128 * 1024,
            chr_rom_size_bytes: 64 * 1024,
            prg_ram_size_bytes: None,         // no volatile RAM
            prg_nvram_size_bytes: Some(8192), // 8KB NVRAM (battery-backed)
            chr_ram_size_bytes: None,
            chr_nvram_size_bytes: None,
            timing_mode: TimingMode::Ntsc,
            vs_ppu_type: None,
            vs_hardware_type: None,
            misc_roms: 0,
            default_expansion_device: 0,
        };
        let parsed = ParsedRom {
            header,
            prg_rom: vec![0u8; 128 * 1024],
            chr_rom: vec![0u8; 64 * 1024],
            trainer: None,
            crc32: 0,
            payload_crc32: 0,
        };

        // When: creating MapperContext from the parsed NES 2.0 ROM
        let ctx = MapperContext::from_parsed_rom(&parsed);

        // Then: 1 bank of PRG-RAM should be allocated (from the NVRAM field)
        assert_eq!(
            ctx.prg_ram_banks_8k, 1,
            "NVRAM should count as PRG-RAM banks when volatile RAM is absent"
        );
        assert!(
            ctx.prg_ram_size_specified,
            "prg_ram_size_specified should be true when NVRAM is present"
        );
    }

    #[test]
    fn from_parsed_rom_uses_larger_of_volatile_and_nvram_when_both_present() {
        use crate::cartridge::ines::{ConsoleType, InesHeader, ParsedRom, TimingMode};

        // Given: a ROM with both volatile RAM (8KB) and NVRAM (16KB)
        let header = InesHeader {
            mapper: 69,
            submapper: 0,
            console_type: ConsoleType::NesFamicom,
            mirroring: NametableLayout::Horizontal,
            has_trainer: false,
            header_version: "2.0",
            battery_backed_prg_ram: true,
            prg_rom_size_bytes: 128 * 1024,
            chr_rom_size_bytes: 64 * 1024,
            prg_ram_size_bytes: Some(8192),    // 8KB volatile
            prg_nvram_size_bytes: Some(16384), // 16KB NVRAM
            chr_ram_size_bytes: None,
            chr_nvram_size_bytes: None,
            timing_mode: TimingMode::Ntsc,
            vs_ppu_type: None,
            vs_hardware_type: None,
            misc_roms: 0,
            default_expansion_device: 0,
        };
        let parsed = ParsedRom {
            header,
            prg_rom: vec![0u8; 128 * 1024],
            chr_rom: vec![0u8; 64 * 1024],
            trainer: None,
            crc32: 0,
            payload_crc32: 0,
        };

        // When: creating MapperContext from the parsed NES 2.0 ROM
        let ctx = MapperContext::from_parsed_rom(&parsed);

        // Then: should allocate enough banks for the larger (NVRAM = 16KB = 2 banks)
        assert_eq!(
            ctx.prg_ram_banks_8k, 2,
            "Should allocate banks for max(volatile, nvram)"
        );
        assert!(
            ctx.prg_ram_size_specified,
            "prg_ram_size_specified should be true when NVRAM is present"
        );
    }

    #[test]
    fn from_parsed_rom_propagates_vs_hardware_type() {
        use crate::cartridge::ines::{ConsoleType, InesHeader, ParsedRom, TimingMode};
        use crate::cartridge::rom_db::VsHardwareType;

        // Given: a VS System ROM with vs_hardware_type=1 (RbiBaseball) in the header
        let header = InesHeader {
            mapper: 99,
            submapper: 0,
            console_type: ConsoleType::VsSystem,
            mirroring: NametableLayout::Horizontal,
            has_trainer: false,
            header_version: "2.0",
            battery_backed_prg_ram: false,
            prg_rom_size_bytes: 32 * 1024,
            chr_rom_size_bytes: 8 * 1024,
            prg_ram_size_bytes: None,
            prg_nvram_size_bytes: None,
            chr_ram_size_bytes: None,
            chr_nvram_size_bytes: None,
            timing_mode: TimingMode::Ntsc,
            vs_ppu_type: Some(0),
            vs_hardware_type: Some(1),
            misc_roms: 0,
            default_expansion_device: 0,
        };
        let parsed = ParsedRom {
            header,
            prg_rom: vec![0u8; 32 * 1024],
            chr_rom: vec![0u8; 8 * 1024],
            trainer: None,
            crc32: 0,
            payload_crc32: 0,
        };

        // When
        let ctx = MapperContext::from_parsed_rom(&parsed);

        // Then: vs_hardware_type should be propagated as a typed enum
        assert_eq!(ctx.vs_hardware_type, Some(VsHardwareType::RbiBaseball));
    }

    #[test]
    fn from_parsed_rom_non_vs_rom_has_no_vs_hardware_type() {
        use crate::cartridge::ines::{ConsoleType, InesHeader, ParsedRom, TimingMode};

        // Given: a standard NES ROM (not VS System)
        let header = InesHeader {
            mapper: 0,
            submapper: 0,
            console_type: ConsoleType::NesFamicom,
            mirroring: NametableLayout::Horizontal,
            has_trainer: false,
            header_version: "1.0",
            battery_backed_prg_ram: false,
            prg_rom_size_bytes: 32 * 1024,
            chr_rom_size_bytes: 8 * 1024,
            prg_ram_size_bytes: None,
            prg_nvram_size_bytes: None,
            chr_ram_size_bytes: None,
            chr_nvram_size_bytes: None,
            timing_mode: TimingMode::Ntsc,
            vs_ppu_type: None,
            vs_hardware_type: None,
            misc_roms: 0,
            default_expansion_device: 0,
        };
        let parsed = ParsedRom {
            header,
            prg_rom: vec![0u8; 32 * 1024],
            chr_rom: vec![0u8; 8 * 1024],
            trainer: None,
            crc32: 0,
            payload_crc32: 0,
        };

        // When
        let ctx = MapperContext::from_parsed_rom(&parsed);

        // Then
        assert_eq!(ctx.vs_hardware_type, None);
    }
}
