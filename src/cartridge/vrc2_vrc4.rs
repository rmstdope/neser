//! Mappers 21/22/23/25/27 - Konami VRC2/VRC4
//!
//! Known Limitations:
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.

use crate::cartridge::BaseMapper;
use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
use crate::cartridge::vrc_irq::VrcIrq;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

/// Mappers 21, 22, 23, 25, 27 - Konami VRC2/VRC4
///
/// Hardware: Konami's VRC2 and VRC4 chips with different pin configurations
///
/// Specifications:
/// - VRC2a (Mapper 22): <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC2a>
/// - VRC2b (Mapper 23): <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC2b>
/// - VRC4 variants: <https://www.nesdev.org/wiki/VRC2_and_VRC4#VRC4_Pinout>
/// - IRQ: <https://www.nesdev.org/wiki/VRC_IRQ> (VRC4 only)
/// - PRG-ROM: Up to 512KB (two 8KB banks switchable, one fixed)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR: Up to 256KB (eight 1KB switchable banks) or CHR-RAM
/// - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
///
/// Mapper variants (different address line connections):
/// - Mapper 21: VRC4a (Wai Wai World 2) / VRC4c (Ganbare Goemon Gaiden 2)
/// - Mapper 22: VRC2a (no IRQ support)
/// - Mapper 23: VRC2b / VRC4e (has IRQ, typically VRC4)
/// - Mapper 25: VRC4b (Gradius II, Teenage Mutant Ninja Turtles II) / VRC4d (Bio Miracle)
/// - Mapper 27: VRC4_27 / World Hero (Kotobuki System) — A0→chip A0, A1→chip A1, full VRC4
///
/// Notes:
/// - VRC2 variants (22, some 23) have no IRQ support
/// - VRC4 has CPU-cycle or scanline-driven IRQ counter
/// - Different mappers due to different A0/A1 pin connections
/// - Used in Gradius II, Contra, Castlevania III (Japan)
///
/// Implementation:
/// - Supports all address line variants via mapper number
/// - VRC IRQ system fully implemented for VRC4 variants
/// - No expansion audio (see VRC6 for audio)
///
/// Each variant carries its active pin mode, which determines how the CPU address
/// lines are mapped to the chip's register address inputs. This varies by submapper
/// (NES 2.0) or defaults to combined OR of all known wirings (iNES 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vrc2Vrc4Variant {
    Mapper21(Mapper21PinMode), // VRC4a, VRC4c
    Mapper22,                  // VRC2a (no IRQ)
    Mapper23(Mapper23PinMode), // VRC4f, VRC4e, VRC2b
    Mapper25(Mapper25PinMode), // VRC4b, VRC4d, VRC2c
    Mapper27,                  // VRC4_27 (World Hero): A0→chip A0, A1→chip A1, full VRC4
}

impl Vrc2Vrc4Variant {
    fn has_irq(&self) -> bool {
        match self {
            Vrc2Vrc4Variant::Mapper21(_) => true,
            Vrc2Vrc4Variant::Mapper22 => false,
            Vrc2Vrc4Variant::Mapper23(Mapper23PinMode::Vrc2bOnly) => false,
            Vrc2Vrc4Variant::Mapper23(_) => true,
            Vrc2Vrc4Variant::Mapper25(Mapper25PinMode::Vrc2cOnly) => false,
            Vrc2Vrc4Variant::Mapper25(_) => true,
            Vrc2Vrc4Variant::Mapper27 => true,
        }
    }

    /// Returns true if the variant supports all four mirroring modes (H, V, 1-lower, 1-upper).
    ///
    /// Mapper 23 iNES 1.0 (Combined), VRC4f (submapper 1), and VRC2b (submapper 3) use
    /// 1-bit mirroring for VRC2b compatibility. Wai Wai World (VRC2b) writes $FF to the
    /// mirroring register expecting Horizontal (bit 0 only); VRC4 2-bit masking would give
    /// SingleScreenUpper. Only the pure VRC4e submapper (NES 2.0 sub 2) supports all four modes.
    fn has_four_mode_mirroring(&self) -> bool {
        match self {
            Vrc2Vrc4Variant::Mapper21(_) => true,
            Vrc2Vrc4Variant::Mapper22 => false,
            Vrc2Vrc4Variant::Mapper23(Mapper23PinMode::Vrc4eOnly) => true,
            Vrc2Vrc4Variant::Mapper23(_) => false,
            Vrc2Vrc4Variant::Mapper25(Mapper25PinMode::Vrc2cOnly) => false,
            Vrc2Vrc4Variant::Mapper25(_) => true,
            Vrc2Vrc4Variant::Mapper27 => true,
        }
    }

    /// Returns the mask applied to the high nibble when writing a CHR bank register.
    /// VRC2 supports 4 high bits (0x0F); VRC4 supports 5 high bits (0x1F) for 512KB CHR.
    fn chr_high_nibble_mask(&self) -> u8 {
        match self {
            Vrc2Vrc4Variant::Mapper22 => 0x0F,
            Vrc2Vrc4Variant::Mapper23(Mapper23PinMode::Vrc2bOnly) => 0x0F,
            Vrc2Vrc4Variant::Mapper25(Mapper25PinMode::Vrc2cOnly) => 0x0F,
            _ => 0x1F,
        }
    }

    /// VRC2a (mapper 22) wires CHR data lines shifted right by 1; the low register
    /// bit is not connected to the CHR ROM address lines.
    fn shifts_chr_bank_right(&self) -> bool {
        *self == Vrc2Vrc4Variant::Mapper22
    }
}

/// Controls which address line mapping(s) are active for mapper 21.
///
/// iNES 1.0 uses Combined (both VRC4a and VRC4c active simultaneously).
/// NES 2.0 submapper 1 = VRC4a only, submapper 2 = VRC4c only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapper21PinMode {
    Combined,  // Both VRC4a (A1,A2) and VRC4c (A6,A7) active (iNES 1.0 default)
    Vrc4aOnly, // Submapper 1: A1→chip A0, A2→chip A1
    Vrc4cOnly, // Submapper 2: A6→chip A0, A7→chip A1
}

/// Controls which address line mapping(s) are active for mapper 23.
///
/// iNES 1.0 uses Combined (both VRC2b/VRC4f and VRC4e active simultaneously).
/// NES 2.0 submapper 1 = VRC4f only, submapper 2 = VRC4e only, submapper 3 = VRC2b only.
///
/// VRC4f and VRC2b share the same pin wiring (A0→chipA0, A1→chipA1) but differ in
/// chip capabilities: VRC4f has IRQ, 5-bit CHR, and $9002 PRG swap; VRC2b has none of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapper23PinMode {
    Combined,  // VRC2b/VRC4f (A0,A1) + VRC4e (A2,A3): A0|A2→chipA0, A1|A3→chipA1
    Vrc4fOnly, // Submapper 1: A0→chip A0, A1→chip A1 (VRC4 capabilities)
    Vrc2bOnly, // Submapper 3: A0→chip A0, A1→chip A1 (VRC2 capabilities — no IRQ, 4-bit CHR)
    Vrc4eOnly, // Submapper 2: A2→chip A0, A3→chip A1
}

/// Controls which address line mapping(s) are active for mapper 25.
///
/// iNES 1.0 uses Combined (both VRC2c/VRC4b and VRC4d active simultaneously).
/// NES 2.0 submapper 1 = VRC4b only, submapper 2 = VRC4d only, submapper 3 = VRC2c only.
///
/// VRC4b and VRC2c share the same pin wiring (A1→chipA0, A0→chipA1) but differ in
/// chip capabilities: VRC4b has IRQ, 5-bit CHR, and $9002 PRG swap; VRC2c has none of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapper25PinMode {
    Combined,  // VRC2c/VRC4b (A1,A0) + VRC4d (A3,A2): A1|A3→chipA0, A0|A2→chipA1
    Vrc4bOnly, // Submapper 1: A1→chip A0, A0→chip A1 (VRC4 capabilities)
    Vrc2cOnly, // Submapper 3: A1→chip A0, A0→chip A1 (VRC2 capabilities — no IRQ, 4-bit CHR)
    Vrc4dOnly, // Submapper 2: A3→chip A0, A2→chip A1
}

pub struct Vrc2Vrc4Mapper {
    /// Mapper variant including its active address-line pin mode.
    variant: Vrc2Vrc4Variant,

    base: BaseMapper,
    prg_ram: PrgRam,
    /// VRC4 WRAM enable ($9002 bit 0). When false, PRG RAM at $6000-$7FFF is inaccessible.
    prg_ram_enabled: bool,

    /// 8KB PRG bank for $8000-$9FFF (or $C000-$DFFF when swap mode is active)
    prg_bank_0: u8,
    /// 8KB PRG bank for $A000-$BFFF (always)
    prg_bank_1: u8,
    /// VRC4 swap mode: when true, $8000-$9FFF is fixed to second-to-last bank
    /// and $C000-$DFFF is controlled by prg_bank_0 (register $9002 bit 1)
    prg_swap_mode: bool,
    /// Eight 1KB CHR bank selectors; each is a 9-bit value (VRC4 supports 512KB CHR).
    /// Written via split low-nibble / high-nibble registers.
    chr_banks_1k: [u16; 8],

    mirroring_reg: u8,

    // --- VRC IRQ (used by VRC4 variants only) ---
    irq: VrcIrq,
}

impl Vrc2Vrc4Mapper {
    const PRG_BANK_MASK: u8 = 0x1F;
    /// Mask to preserve the low nibble of a 9-bit CHR bank value.
    const CHR_LOW_NIBBLE_MASK: u16 = 0x000F;
    /// Mask to preserve the high 5 bits of a 9-bit CHR bank value.
    const CHR_HIGH_BITS_MASK: u16 = 0x01F0;

    /// Expected byte length of the registers snapshot produced by `registers_snapshot`.
    const SNAPSHOT_SIZE: usize = 27;

    // Flag bit positions in the snapshot flags byte.
    const FLAG_IRQ_ENABLED: u8 = 1 << 0;
    const FLAG_IRQ_MODE_CYCLE: u8 = 1 << 1;
    const FLAG_IRQ_ENABLE_AFTER_ACK: u8 = 1 << 2;
    const FLAG_IRQ_ASSERTED: u8 = 1 << 3;
    const FLAG_PRG_SWAP_MODE: u8 = 1 << 4;

    const FLAG_PRG_RAM_ENABLED: u8 = 1 << 5;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mapper_number = ctx.mapper as u8;
        let submapper = ctx.submapper;
        let mirroring = ctx.mirroring;
        let variant = match mapper_number {
            21 => Vrc2Vrc4Variant::Mapper21(match submapper {
                1 => Mapper21PinMode::Vrc4aOnly,
                2 => Mapper21PinMode::Vrc4cOnly,
                _ => Mapper21PinMode::Combined,
            }),
            22 => Vrc2Vrc4Variant::Mapper22,
            23 => Vrc2Vrc4Variant::Mapper23(match submapper {
                1 => Mapper23PinMode::Vrc4fOnly,
                2 => Mapper23PinMode::Vrc4eOnly,
                3 => Mapper23PinMode::Vrc2bOnly,
                _ => Mapper23PinMode::Combined,
            }),
            25 => Vrc2Vrc4Variant::Mapper25(match submapper {
                1 => Mapper25PinMode::Vrc4bOnly,
                2 => Mapper25PinMode::Vrc4dOnly,
                3 => Mapper25PinMode::Vrc2cOnly,
                _ => Mapper25PinMode::Combined,
            }),
            27 => Vrc2Vrc4Variant::Mapper27,
            _ => Vrc2Vrc4Variant::Mapper21(Mapper21PinMode::Combined),
        };
        let capabilities = MapperCapabilities {
            has_irq: variant.has_irq(),
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_chr_banking(0x0400);
        base.set_mirroring(mirroring);
        let mut mapper = Self {
            variant,
            base,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            prg_ram_enabled: false,
            prg_bank_0: 0,
            prg_bank_1: 0,
            prg_swap_mode: false,
            chr_banks_1k: [0u16; 8],
            mirroring_reg: 0,
            irq: VrcIrq::new(341, 3),
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let bank0 = (self.prg_bank_0 & Self::PRG_BANK_MASK) as i16;
        let bank1 = (self.prg_bank_1 & Self::PRG_BANK_MASK) as i16;
        if self.prg_swap_mode {
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(2, bank0);
        } else {
            self.base.select_prg_page(0, bank0);
            self.base.select_prg_page(2, -2);
        }
        self.base.select_prg_page(1, bank1);
        self.base.select_prg_page(3, -1);
        for i in 0..8 {
            let bank = self.chr_banks_1k[i];
            let effective = if self.variant.shifts_chr_bank_right() {
                (bank >> 1) as i16
            } else {
                bank as i16
            };
            self.base.select_chr_page(i, effective);
        }
    }

    /// Normalize register address based on the mapper variant.
    ///
    /// Each mapper variant has different CPU address line connections to the chip's
    /// register address inputs (chip A0 and chip A1 select the register within a page):
    /// - Mapper 21 VRC4a: CPU A1→chip A0, CPU A2→chip A1
    /// - Mapper 21 VRC4c: CPU A6→chip A0, CPU A7→chip A1
    /// - Mapper 22 VRC2a: CPU A1→chip A0, CPU A0→chip A1 (A0/A1 swapped)
    /// - Mapper 23 VRC4f/VRC2b: CPU A0→chip A0, CPU A1→chip A1
    /// - Mapper 23 VRC4e:       CPU A2→chip A0, CPU A3→chip A1
    /// - Mapper 25 VRC4b/VRC2c: CPU A1→chip A0, CPU A0→chip A1
    /// - Mapper 25 VRC4d:       CPU A3→chip A0, CPU A2→chip A1
    /// - Mapper 27 VRC4_27:     CPU A0→chip A0, CPU A1→chip A1 (direct, no swap)
    ///
    /// iNES 1.0 combined mode ORs all known wirings for a mapper together.
    fn normalize_reg_addr(&self, addr: u16) -> u16 {
        // Base address uses A12-A15 for register page selection.
        let base = addr & 0xF000;

        match self.variant {
            Vrc2Vrc4Variant::Mapper21(pin_mode) => {
                // VRC4a: CPU A1→chip A0, CPU A2→chip A1  ($x000, $x002, $x004, $x006)
                // VRC4c: CPU A6→chip A0, CPU A7→chip A1  ($x000, $x040, $x080, $x0C0)
                // pin_mode selects which wiring(s) are active (submapper or combined for iNES 1.0).
                let a0 = match pin_mode {
                    Mapper21PinMode::Vrc4aOnly => (addr >> 1) & 0x01,
                    Mapper21PinMode::Vrc4cOnly => (addr >> 6) & 0x01,
                    Mapper21PinMode::Combined => ((addr >> 1) | (addr >> 6)) & 0x01,
                };
                let a1 = match pin_mode {
                    Mapper21PinMode::Vrc4aOnly => (addr >> 2) & 0x01,
                    Mapper21PinMode::Vrc4cOnly => (addr >> 7) & 0x01,
                    Mapper21PinMode::Combined => ((addr >> 2) | (addr >> 7)) & 0x01,
                };
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper22 => {
                // VRC2a: CPU A1→chip A0, CPU A0→chip A1 (A0/A1 swapped vs. standard)
                let a0 = (addr >> 1) & 0x01;
                let a1 = addr & 0x01;
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper23(pin_mode) => {
                // VRC4f/VRC2b: A0→chip A0, A1→chip A1
                // VRC4e:       A2→chip A0, A3→chip A1
                // Combined (iNES 1.0): chip A0 = A0|A2, chip A1 = A1|A3
                let a0 = match pin_mode {
                    Mapper23PinMode::Vrc4fOnly | Mapper23PinMode::Vrc2bOnly => addr & 0x01,
                    Mapper23PinMode::Vrc4eOnly => (addr >> 2) & 0x01,
                    Mapper23PinMode::Combined => (addr & 0x01) | ((addr >> 2) & 0x01),
                };
                let a1 = match pin_mode {
                    Mapper23PinMode::Vrc4fOnly | Mapper23PinMode::Vrc2bOnly => (addr >> 1) & 0x01,
                    Mapper23PinMode::Vrc4eOnly => (addr >> 3) & 0x01,
                    Mapper23PinMode::Combined => ((addr >> 1) & 0x01) | ((addr >> 3) & 0x01),
                };
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper25(pin_mode) => {
                // VRC4b/VRC2c: A1→chip A0, A0→chip A1 (A0/A1 swapped vs. mapper 23)
                // VRC4d:       A3→chip A0, A2→chip A1
                // Combined (iNES 1.0): chip A0 = A1|A3, chip A1 = A0|A2
                let a0 = match pin_mode {
                    Mapper25PinMode::Vrc4bOnly | Mapper25PinMode::Vrc2cOnly => (addr >> 1) & 0x01,
                    Mapper25PinMode::Vrc4dOnly => (addr >> 3) & 0x01,
                    Mapper25PinMode::Combined => ((addr >> 1) & 0x01) | ((addr >> 3) & 0x01),
                };
                let a1 = match pin_mode {
                    Mapper25PinMode::Vrc4bOnly | Mapper25PinMode::Vrc2cOnly => addr & 0x01,
                    Mapper25PinMode::Vrc4dOnly => (addr >> 2) & 0x01,
                    Mapper25PinMode::Combined => (addr & 0x01) | ((addr >> 2) & 0x01),
                };
                base | (a1 << 1) | a0
            }
            Vrc2Vrc4Variant::Mapper27 => {
                // VRC4_27: A0→chip A0, A1→chip A1 (direct mapping, no swap)
                let a0 = addr & 0x01;
                let a1 = (addr >> 1) & 0x01;
                base | (a1 << 1) | a0
            }
        }
    }

    fn apply_mirroring_register(&mut self) {
        let layout = if self.variant.has_four_mode_mirroring() {
            match self.mirroring_reg & 0x03 {
                0x0 => NametableLayout::Vertical,
                0x1 => NametableLayout::Horizontal,
                0x2 => NametableLayout::SingleScreenLower,
                0x3 => NametableLayout::SingleScreenUpper,
                _ => self.base.mirroring(),
            }
        } else {
            // 1-bit mirroring: only H/V, bit 1 is ignored.
            match self.mirroring_reg & 0x01 {
                0 => NametableLayout::Vertical,
                _ => NametableLayout::Horizontal,
            }
        };
        self.base.set_mirroring(layout);
    }

    /// Update a single 1KB CHR bank slot with either the low or high nibble of
    /// the 9-bit bank number.  Low nibble sets bits [3:0]; high nibble sets bits [8:4].
    fn write_chr_bank_nibble(&mut self, bank_idx: usize, is_high_nibble: bool, value: u8) {
        let high_mask = self.variant.chr_high_nibble_mask();
        if is_high_nibble {
            self.chr_banks_1k[bank_idx] = (self.chr_banks_1k[bank_idx] & Self::CHR_LOW_NIBBLE_MASK)
                | (((value & high_mask) as u16) << 4);
        } else {
            self.chr_banks_1k[bank_idx] =
                (self.chr_banks_1k[bank_idx] & Self::CHR_HIGH_BITS_MASK) | (value & 0x0F) as u16;
        }
    }

    /// Handle a normalised CHR bank register write ($B000-$E003).
    ///
    /// Registers are arranged as four pages ($Bxxx–$Exxx), each covering two 1KB CHR banks.
    /// Within each page, positions 0/1 address the even bank (low/high nibble) and
    /// positions 2/3 address the odd bank (low/high nibble).
    fn write_chr_bank_register(&mut self, reg: u16, value: u8) {
        let page_base: usize = match reg & 0xF000 {
            0xB000 => 0,
            0xC000 => 2,
            0xD000 => 4,
            0xE000 => 6,
            _ => return,
        };
        let pos = (reg & 0x0003) as usize;
        let bank_idx = page_base + (pos >> 1);
        let is_high_nibble = (pos & 1) != 0;
        self.write_chr_bank_nibble(bank_idx, is_high_nibble, value);
    }

    /// Handle a normalised write to the $9000-$9003 register range.
    ///
    /// Position 2 ($9002) is the VRC4-only PRG Swap Mode / WRAM control register.
    /// All other positions update the mirroring control register.
    fn write_9000_register(&mut self, reg: u16, value: u8) {
        let pos = reg & 0x0003;
        if pos == 0x0002 && self.variant.has_irq() {
            // VRC4 only: bit 1 = PRG swap mode, bit 0 = WRAM enable
            self.prg_swap_mode = (value & 0x02) != 0;
            self.prg_ram_enabled = (value & 0x01) != 0;
        } else {
            self.mirroring_reg = value;
            self.apply_mirroring_register();
        }
    }

    /// Return the raw (non-wrapped) CHR bank register value for the 1KB slot
    /// that covers the given PPU address.
    ///
    /// Each 1KB slot covers 0x400 bytes in the pattern table range $0000-$1FFF.
    /// The raw value is the value stored in the CHR bank register _before_
    /// modulo-wrapping against CHR-ROM size, which allows callers to detect
    /// special-meaning bank numbers (e.g., CHR-RAM banks).
    ///
    /// `ppu_addr` is expected to be in the pattern table range ($0000-$1FFF).
    pub fn raw_chr_1k_bank(&self, ppu_addr: u16) -> u16 {
        debug_assert!(
            ppu_addr < 0x2000,
            "raw_chr_1k_bank called with non-pattern-table PPU address: {ppu_addr:04X}"
        );

        // Normalise to $0000-$1FFF in case callers accidentally pass a mirrored address.
        let ppu = (ppu_addr & 0x1FFF) as usize;
        let slot = ppu >> 10; // divide by 1024 (1KB slots)
        self.chr_banks_1k[slot & 7]
    }

    /// Handle a normalised IRQ register write ($F000-$F003).
    ///
    /// VRC2 variants silently ignore these writes (no IRQ hardware).
    fn write_irq_registers(&mut self, reg: u16, value: u8) {
        if !self.variant.has_irq() {
            return;
        }
        match reg {
            0xF000 => self.irq.write_latch_low_nibble(value),
            0xF001 => self.irq.write_latch_high_nibble(value),
            0xF002 => self.irq.write_control(value),
            0xF003 => self.irq.write_acknowledge(),
            _ => {}
        }
    }
}

impl Mapper for Vrc2Vrc4Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF (only when WRAM is enabled; VRC2 is always enabled)
        if (!self.variant.has_irq() || self.prg_ram_enabled)
            && let Some(value) = self.prg_ram.try_read(addr)
        {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (only when WRAM is enabled; VRC2 is always enabled)
        if (!self.variant.has_irq() || self.prg_ram_enabled) && self.prg_ram.try_write(addr, value)
        {
            return;
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            let reg = self.normalize_reg_addr(addr);
            match reg {
                0x8000..=0x8003 => self.prg_bank_0 = value & Self::PRG_BANK_MASK,
                0x9000..=0x9003 => self.write_9000_register(reg, value),
                0xA000..=0xA003 => self.prg_bank_1 = value & Self::PRG_BANK_MASK,
                0xB000..=0xB003 | 0xC000..=0xC003 | 0xD000..=0xD003 | 0xE000..=0xE003 => {
                    self.write_chr_bank_register(reg, value);
                }
                0xF000..=0xF003 => self.write_irq_registers(reg, value),
                _ => {}
            }
            self.update_banks();
        }
    }

    fn cpu_cycle(&mut self) {
        trace_mapper!(5; "[vrc2_vrc4] cpu_cycle (irq)");
        if self.variant.has_irq() {
            self.irq.tick();
        }
    }

    fn irq_pending(&self) -> bool {
        if self.variant.has_irq() {
            self.irq.pending()
        } else {
            false
        }
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

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0]:    prg_bank_0
        // [1]:    prg_bank_1
        // [2-17]: chr_banks_1k[0-7] as little-endian u16 pairs (9-bit values)
        // [18]:   mirroring_reg
        // [19]:   irq_latch
        // [20]:   irq_counter
        // [21]:   flags (see FLAG_* constants)
        // [22-25]: irq_prescaler (little-endian i32)
        // [26]:   mirroring
        let mut snapshot = Vec::with_capacity(Self::SNAPSHOT_SIZE);
        snapshot.push(self.prg_bank_0);
        snapshot.push(self.prg_bank_1);
        for bank in &self.chr_banks_1k {
            snapshot.extend_from_slice(&bank.to_le_bytes());
        }
        snapshot.push(self.mirroring_reg);
        snapshot.push(self.irq.latch());
        snapshot.push(self.irq.counter());
        let mut flags = 0u8;
        if self.irq.enabled() {
            flags |= Self::FLAG_IRQ_ENABLED;
        }
        if self.irq.mode_cycle() {
            flags |= Self::FLAG_IRQ_MODE_CYCLE;
        }
        if self.irq.enable_after_ack() {
            flags |= Self::FLAG_IRQ_ENABLE_AFTER_ACK;
        }
        if self.irq.pending() {
            flags |= Self::FLAG_IRQ_ASSERTED;
        }
        if self.prg_swap_mode {
            flags |= Self::FLAG_PRG_SWAP_MODE;
        }
        if self.prg_ram_enabled {
            flags |= Self::FLAG_PRG_RAM_ENABLED;
        }
        snapshot.push(flags);
        snapshot.extend_from_slice(&self.irq.prescaler().to_le_bytes());
        snapshot.push(self.base.mirroring().to_snapshot_byte());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= Self::SNAPSHOT_SIZE {
            self.prg_bank_0 = data[0];
            self.prg_bank_1 = data[1];
            for (i, bank) in self.chr_banks_1k.iter_mut().enumerate() {
                *bank = u16::from_le_bytes([data[2 + i * 2], data[2 + i * 2 + 1]]);
            }
            self.mirroring_reg = data[18];
            self.irq.write_latch(data[19]);
            self.irq.set_counter(data[20]);
            let flags = data[21];
            self.irq.set_enabled((flags & Self::FLAG_IRQ_ENABLED) != 0);
            self.irq
                .set_mode_cycle((flags & Self::FLAG_IRQ_MODE_CYCLE) != 0);
            self.irq
                .set_enable_after_ack((flags & Self::FLAG_IRQ_ENABLE_AFTER_ACK) != 0);
            self.irq
                .set_asserted((flags & Self::FLAG_IRQ_ASSERTED) != 0);
            self.prg_swap_mode = (flags & Self::FLAG_PRG_SWAP_MODE) != 0;
            self.prg_ram_enabled = (flags & Self::FLAG_PRG_RAM_ENABLED) != 0;
            self.irq
                .set_prescaler(i32::from_le_bytes([data[22], data[23], data[24], data[25]]));
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[26]));
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    fn create_vrc_mapper(
        mapper_number: u16,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            mapper_number,
            prg_rom,
            chr_rom,
            mirroring,
        ))
        .expect("VRC mapper should be implemented")
    }

    fn create_vrc_mapper_with_submapper(
        mapper_number: u16,
        submapper: u8,
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new_for_test(mapper_number, prg_rom, chr_rom, mirroring)
                .with_submapper(submapper),
        )
        .expect("VRC mapper with submapper should be implemented")
    }

    #[test]
    fn test_vrc4_mapper_21_prg_banking() {
        // VRC4 PRG banking:
        // $8000-$9FFF: 8KB switchable (PRG Select 0, register $800x)
        // $A000-$BFFF: 8KB switchable (PRG Select 1, register $A00x)
        // $C000-$DFFF: fixed to second-to-last 8KB bank
        // $E000-$FFFF: fixed to last 8KB bank
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Select 8KB bank #1 at $8000-$9FFF
        mapper.write_prg(0x8000, 0x01);
        // Select 8KB bank #5 at $A000-$BFFF
        mapper.write_prg(0xA000, 0x05);

        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 should be bank 1");
        assert_eq!(mapper.read_prg(0x9FFF), 1, "$9FFF should still be bank 1");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should be bank 5");
        assert_eq!(mapper.read_prg(0xBFFF), 5, "$BFFF should still be bank 5");
        // $C000-$DFFF fixed to second-to-last (bank 6 for 8 banks)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-to-last bank 6"
        );
        // $E000-$FFFF fixed to last (bank 7)
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be last bank 7");
    }

    #[test]
    fn test_vrc2_mapper_22_no_irq() {
        // Mapper 22 is VRC2a which has no IRQ support
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Try to enable IRQ (should be ignored for VRC2)
        mapper.write_prg(0xF000, 0xFF);
        mapper.write_prg(0xF001, 0b0000_0110); // Enable in cycle mode

        // Run many cycles
        for _ in 0..1000 {
            mapper.cpu_cycle();
        }

        // IRQ should never trigger on VRC2
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_23_irq_cycle_mode() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Mapper 23 correct normalisation: chip A0 = CPU A0|A2, chip A1 = CPU A1|A3
        // (VRC2b/VRC4f: A0→chipA0, A1→chipA1; VRC4e: A2→chipA0, A3→chipA1)
        // VRC2b/VRC4f register addresses: $F000 (pos 0), $F001 (pos 1), $F002 (pos 2), $F003 (pos 3)
        mapper.write_prg(0xF000, 0x0E); // chip pos 0: latch bits [3:0] = 0xE
        mapper.write_prg(0xF001, 0x0F); // chip pos 1 (VRC2b A0=1): latch bits [7:4] = 0xF → latch = 0xFE
        // IRQ Control at chip pos 2 (VRC2b A1=1 → $F002):
        mapper.write_prg(0xF002, 0b0000_0110); // M=1, E=1, A=0

        // After enable, counter reloaded to 0xFE
        // Cycle 1: 0xFE -> 0xFF (no IRQ)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        // Cycle 2: counter == 0xFF -> trip IRQ
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        // IRQ Acknowledge at chip pos 3 (VRC2b A0=1, A1=1 → $F003)
        mapper.write_prg(0xF003, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_vrc4_mapper_25_chr_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 32);

        let mut mapper = create_vrc_mapper(25, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set CHR bank 0 (PPU $0000-$03FF) to bank 7 via low nibble write at $B000
        mapper.write_prg(0xB000, 7);
        assert_eq!(mapper.read_chr(0x0000), 7);

        // Set CHR bank 4 (PPU $1000-$13FF) to bank 15 via low nibble write at $D000
        // ($D000 → page_base=4, pos=0 → bank_idx=4, low nibble)
        mapper.write_prg(0xD000, 15);
        assert_eq!(mapper.read_chr(0x1000), 15);
    }

    #[test]
    fn test_vrc2_vrc4_mirroring_control() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Test vertical mirroring
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Test horizontal mirroring
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Test single screen lower bank mirroring (value 2)
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Test single screen upper bank mirroring (value 3)
        mapper.write_prg(0x9000, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    /// VRC4 spec: $8000-$9FFF and $A000-$BFFF are TWO INDEPENDENT 8KB switchable
    /// banks. Register $800x selects the 8KB bank at $8000-$9FFF (not 16KB).
    /// Register $A00x selects the 8KB bank at $A000-$BFFF.
    /// $C000-$DFFF is fixed to the second-to-last 8KB bank.
    /// $E000-$FFFF is fixed to the last 8KB bank.
    #[test]
    fn test_vrc4_prg_8000_9fff_and_a000_bfff_are_independent_8kb_banks() {
        let prg_rom = banked_data(8 * 1024, 8); // 8 × 8KB banks filled with index byte
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // PRG Select 0: bank 3 at $8000-$9FFF
        mapper.write_prg(0x8000, 3);
        // PRG Select 1: bank 5 at $A000-$BFFF
        mapper.write_prg(0xA000, 5);

        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 should read from 8KB bank 3"
        );
        assert_eq!(
            mapper.read_prg(0x9FFF),
            3,
            "$9FFF should still be in 8KB bank 3"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 should read from 8KB bank 5"
        );
        assert_eq!(
            mapper.read_prg(0xBFFF),
            5,
            "$BFFF should still be in 8KB bank 5"
        );
        // $C000-$DFFF fixed to second-to-last bank (bank 6 for 8 banks)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be fixed to second-to-last bank"
        );
        // $E000-$FFFF fixed to last bank (bank 7)
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should be fixed to last bank"
        );
    }

    /// VRC4 CHR banks are 9-bit values written via split low/high nibble registers.
    /// Low nibble register (even position after normalisation, e.g. $B000):
    ///   sets bits [3:0] of the CHR bank number.
    /// High nibble register (odd position after normalisation, e.g. $B001/physical $B002 on VRC4a):
    ///   sets bits [8:4] of the CHR bank number.
    /// For mapper 21 (VRC4a): A1→chip A0, A2→chip A1, so:
    ///   physical $B000 → chip pos 0 = low nibble of CHR bank 0
    ///   physical $B002 → A1=1 → chip A0=1 → chip pos 1 = high nibble of CHR bank 0
    #[test]
    fn test_vrc4_chr_split_nibble_combines_to_9bit_bank_number() {
        // Use 32 CHR banks of 1KB so we can address banks up to 31
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write CHR bank 0: target bank = 0x1F (0001_1111)
        // Low nibble  = 0xF (bits [3:0])  → physical $B000 (chip pos 0)
        // High nibble = 0x01 (bits [4])   → physical $B002 (chip pos 1, VRC4a A1=1)
        mapper.write_prg(0xB000, 0x0F); // low nibble = 0xF
        mapper.write_prg(0xB002, 0x01); // high nibble = 0x01 → bank bit 4 → bank 0x1F

        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "CHR $0000 should read from bank 31 (= low 0xF | high 0x10)"
        );
    }

    /// VRC4 IRQ latch is 8 bits split across two normalised register positions:
    ///   Normalised $F000 (pos 0): writes low 4 bits of latch
    ///   Normalised $F001 (pos 1): writes high 4 bits of latch
    ///   Normalised $F002 (pos 2): IRQ Control (E/M/A bits)
    ///   Normalised $F003 (pos 3): IRQ Acknowledge
    ///
    /// For mapper 21 VRC4a (A1→chip A0, A2→chip A1):
    ///   physical $F000 → chip pos 0 = IRQ latch low
    ///   physical $F002 → chip pos 1 = IRQ latch high
    ///   physical $F004 → chip pos 2 = IRQ Control
    ///   physical $F006 → chip pos 3 = IRQ Acknowledge
    #[test]
    fn test_vrc4_mapper21_irq_latch_split_low_high_nibbles() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set latch to 0xFE via split writes (VRC4a physical addresses):
        // low  nibble 0xE → $F000 (chip pos 0)
        // high nibble 0xF → $F002 (chip pos 1 for VRC4a, A1=1 → chip A0=1)
        mapper.write_prg(0xF000, 0x0E); // latch bits [3:0] = 0xE
        mapper.write_prg(0xF002, 0x0F); // latch bits [7:4] = 0xF → combined latch = 0xFE

        // IRQ Control at physical $F004 (chip pos 2, VRC4a A2=1 → chip A1=1): M=1, E=1
        mapper.write_prg(0xF004, 0b0000_0110);

        // After reload, counter = 0xFE. In cycle mode:
        // cycle 1: 0xFE → 0xFF (no IRQ yet)
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ should not fire after 1 cycle");
        // cycle 2: 0xFF overflows → reload and fire IRQ
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ should fire after 2 cycles (latch=0xFE, cycle mode)"
        );

        // Acknowledge at physical $F006 (chip pos 3, VRC4a A1=1,A2=1 → chip A0=1,A1=1)
        mapper.write_prg(0xF006, 0);
        assert!(!mapper.irq_pending(), "IRQ should clear after acknowledge");
    }

    /// Mapper 21 implements BOTH VRC4a (A1→chip A0, A2→chip A1) and
    /// VRC4c (A6→chip A0, A7→chip A1) address mappings simultaneously.
    /// This test verifies VRC4c-style addresses (bit 6 / bit 7) work.
    ///
    /// VRC4c IRQ addresses: $F000 (latch low), $F040 (latch high),
    ///                      $F080 (control),   $F0C0 (ack)
    #[test]
    fn test_mapper21_vrc4c_register_addressing_via_a6_a7() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write latch 0xFE via VRC4c addresses:
        mapper.write_prg(0xF000, 0x0E); // latch low nibble  = 0xE  (chip pos 0)
        mapper.write_prg(0xF040, 0x0F); // latch high nibble = 0xF  (VRC4c: A6=1 → chip A0=1 → pos 1)

        // IRQ Control via VRC4c: A7=1 → chip A1=1 → chip pos 2 → $F080
        mapper.write_prg(0xF080, 0b0000_0110); // M=1, E=1

        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ via VRC4c addresses should fire");

        // Acknowledge via VRC4c: A6=1, A7=1 → chip A0=1, A1=1 → chip pos 3 → $F0C0
        mapper.write_prg(0xF0C0, 0);
        assert!(
            !mapper.irq_pending(),
            "IRQ should clear via VRC4c ack address"
        );
    }

    /// VRC4 $9002 is PRG Swap Mode + WRAM control, NOT mirroring.
    /// When Swap Mode bit (bit 1) is set:
    ///   $8000-$9FFF becomes fixed to the second-to-last bank (not PRG Select 0)
    ///   $C000-$DFFF becomes the switchable bank controlled by PRG Select 0
    #[test]
    fn test_vrc4_9002_swap_mode_swaps_prg_bank_windows() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Select PRG Select 0 bank = 2
        mapper.write_prg(0x8000, 2);

        // Before swap mode: bank 2 should be at $8000-$9FFF
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "before swap: $8000 should be bank 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "before swap: $C000 should be second-to-last"
        );

        // Enable swap mode: $9002 bit 1 = M
        // For mapper 21 (VRC4a): $9002 normalized → base=$9000, A1=1→chip A0=1 → norm pos 1
        // But $9002 is chip position 2 on the $9xxx range.
        // For VRC4a: A2=1 → chip A1=1 → norm $9002 (pos 2)
        mapper.write_prg(0x9004, 0b0000_0010); // VRC4a: A2=1 → norm $9002, M=1

        // After swap mode: $8000-$9FFF should be fixed to second-to-last (bank 6)
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "after swap: $8000 should be second-to-last"
        );
        // $C000-$DFFF should now be controlled by PRG Select 0 (bank 2)
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "after swap: $C000 should be bank 2"
        );
    }

    #[test]
    fn test_vrc2_vrc4_registers_snapshot_restores_state() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_vrc_mapper(
            21,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        );

        // PRG Select 0: bank 3 at $8000-$9FFF
        mapper.write_prg(0x8000, 0x03);
        // PRG Select 1: bank 5 at $A000-$BFFF
        mapper.write_prg(0xA000, 0x05);
        // CHR bank 0 low nibble = 2 → bank 2
        mapper.write_prg(0xB000, 0x02);
        // CHR bank 4 low nibble = 4 → bank 4 (physical $D000 for VRC4a: A1=A2=0 → pos 0)
        mapper.write_prg(0xD000, 0x04);
        // Mirroring horizontal
        mapper.write_prg(0x9000, 0x01);

        // IRQ latch = 0xFE via split nibble writes (VRC4a physical addresses):
        //   low  nibble 0xE → $F000 (chip pos 0)
        //   high nibble 0xF → $F002 (VRC4a: A1=1 → chip A0=1 → pos 1)
        mapper.write_prg(0xF000, 0x0E);
        mapper.write_prg(0xF002, 0x0F);
        // IRQ Control at $F004 (VRC4a: A2=1 → chip A1=1 → pos 2): M=1, E=1
        mapper.write_prg(0xF004, 0b0000_0110);
        mapper.cpu_cycle(); // counter: 0xFE → 0xFF (no IRQ yet)

        let regs = mapper.registers_snapshot();

        let mut restored = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Vertical);
        restored.restore_registers(&regs);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "PRG bank 0 should be restored"
        );
        assert_eq!(
            restored.read_prg(0xA000),
            5,
            "PRG bank 1 should be restored"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            2,
            "CHR bank 0 should be restored"
        );
        assert_eq!(
            restored.read_chr(0x1000),
            4,
            "CHR bank 4 should be restored"
        );
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.irq_pending(), mapper.irq_pending());
    }

    // =========================================================================
    // Tests for remaining spec gaps (RED phase - should fail before fix)
    // =========================================================================

    /// VRC4 mirroring values 2 and 3 select one-screen lower and upper bank
    /// respectively, not both mapped to the same single screen.
    #[test]
    fn test_vrc4_single_screen_lower_and_upper_bank_mirroring() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Mirroring value 2 = one-screen, lower bank
        mapper.write_prg(0x9000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "value 2 should select single-screen lower bank"
        );

        // Mirroring value 3 = one-screen, upper bank
        mapper.write_prg(0x9000, 0x03);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "value 3 should select single-screen upper bank"
        );
    }

    /// VRC4 $9002 bit 0 is the WRAM enable bit.
    /// When clear (default after reset), PRG RAM at $6000-$7FFF is inaccessible
    /// (reads return open bus / 0, writes are ignored).
    /// When set, PRG RAM is accessible normally.
    #[test]
    fn test_vrc4_9002_wram_enable_gates_prg_ram_access() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(21, prg_rom, chr_rom, NametableLayout::Horizontal);

        // After reset, WRAM is disabled — write should be ignored
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "PRG RAM read should return 0 when WRAM disabled"
        );

        // Enable WRAM via $9002 bit 0 (VRC4a: $9004 → norm $9002)
        mapper.write_prg(0x9004, 0b0000_0001); // W=1

        // Now writes and reads should work
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG RAM read should return written value when WRAM enabled"
        );
    }

    /// NES 2.0 submapper 1 selects VRC4a-only addressing (A1→chip A0, A2→chip A1).
    /// A VRC4c-style address like $F040 (A6=1) must NOT be treated as chip pos 1
    /// when submapper 1 is active.
    #[test]
    fn test_mapper21_submapper1_uses_only_vrc4a_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        // Submapper 1 = VRC4a only
        let mut mapper =
            create_vrc_mapper_with_submapper(21, 1, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Set latch low nibble via $F000 (chip pos 0 — same in all variants)
        mapper.write_prg(0xF000, 0x0E); // latch bits [3:0] = 0xE

        // Attempt latch HIGH via VRC4c-style $F040 (A6=1 → chip A0=1 on VRC4c, but NOT on VRC4a).
        // For submapper 1 (VRC4a only), this should be a no-op — latch high stays 0x0.
        mapper.write_prg(0xF040, 0x0F); // should be ignored under submapper 1

        // Enable IRQ cycle mode via VRC4a $F004 (A2=1 → chip A1=1 → pos 2)
        mapper.write_prg(0xF004, 0b0000_0110); // M=1, E=1

        // If $F040 was correctly ignored, latch = 0x0E.
        // Counter starts at 0x0E, needs 0xF2 cycles to reach 0xFF — IRQ will NOT fire in 2 cycles.
        // If $F040 was wrongly accepted (combined addressing), latch = 0xFE → IRQ fires after 2 cycles.
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(
            !mapper.irq_pending(),
            "submapper 1 (VRC4a only) must ignore VRC4c $F040 address for latch high"
        );
    }

    // =========================================================================
    // Mapper 22 (VRC2a) specific spec tests — issue #645
    // =========================================================================

    /// NESdev spec: "On VRC2a (mapper 22), the low bit is ignored (right shift value by 1)."
    /// When a CHR bank register is written, the effective bank number used for
    /// PPU reads must be (stored_value >> 1).
    #[test]
    fn test_vrc2a_mapper22_chr_bank_right_shifted_by_1() {
        // 16 banks × 1KB, each bank filled with its index byte
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 16);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC2a address normalisation: CPU A1 → chip A0, CPU A0 → chip A1 (swapped).
        // To write CHR bank 0 low nibble (chip pos 0 = $B000):
        //   chip A0=0, chip A1=0 → CPU: A1 = chip A0 = 0, CPU A0 = chip A1 = 0 → $B000
        // Write 0x02 (binary 0010). Without the shift the game would select bank 2;
        // with the spec-mandated >> 1 shift it must select bank 1.
        mapper.write_prg(0xB000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "VRC2a CHR bank must be right-shifted by 1: writing 0x02 must select bank 1, not bank 2"
        );

        // Write 0x04 → effective bank 2
        mapper.write_prg(0xB000, 0x04);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "VRC2a CHR bank must be right-shifted by 1: writing 0x04 must select bank 2"
        );

        // Odd values: low bit is discarded — 0x03 >> 1 = 1 → bank 1
        mapper.write_prg(0xB000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "VRC2a CHR bank: bit 0 is discarded, 0x03 >> 1 must select bank 1"
        );
    }

    /// NESdev spec: "VRC2 supports only vertical or horizontal mirroring. Bit 1 is ignored."
    /// Writing values 2 or 3 to the mirroring register on mapper 22 must not
    /// produce single-screen mirroring — only bit 0 is honoured.
    #[test]
    fn test_vrc2a_mapper22_mirroring_ignores_bit1() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC2a $9000: chip pos 0 (CPU A1=0, A0=0) → $9000
        // value 0b10 (2): bit 1 set, bit 0 clear → must yield Vertical (same as value 0)
        mapper.write_prg(0x9000, 0b10);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "VRC2a: mirroring value 2 (bit1=1,bit0=0) must behave as Vertical (bit 1 ignored)"
        );

        // value 0b11 (3): bit 1 set, bit 0 set → must yield Horizontal (same as value 1)
        mapper.write_prg(0x9000, 0b11);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "VRC2a: mirroring value 3 (bit1=1,bit0=1) must behave as Horizontal (bit 1 ignored)"
        );
    }

    // =========================================================================
    // Mapper 23 address normalisation spec tests (issue #654)
    // =========================================================================

    /// NESdev spec: mapper 23 combines VRC2b (A0→chipA0, A1→chipA1) and
    /// VRC4e (A2→chipA0, A3→chipA1). Combined: chipA0 = CPU_A0|A2, chipA1 = CPU_A1|A3.
    ///
    /// VRC4e: $B004 (A2=1) → chip pos 1 = high nibble of CHR bank 0.
    /// Bug: current wrong mapping A2→chipA1 makes $B004 hit pos 2 (low nibble CHR bank 1).
    #[test]
    fn test_mapper23_vrc4e_chr_high_nibble_via_b004() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F); // low nibble CHR bank 0 = 0xF
        // VRC4e: $B004 (A2=1) → chipA0=1, chipA1=0 → chip pos 1 = high nibble CHR bank 0
        mapper.write_prg(0xB004, 0x01); // high nibble = 1 → bank = 0x10 | 0x0F = 0x1F = 31
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "VRC4e $B004 must be chip pos 1 (high nibble CHR bank 0), yielding bank 31"
        );
    }

    /// VRC2b: $B002 (A1=1) → chip pos 2 = low nibble of CHR bank 1.
    /// Bug: current wrong mapping A1→chipA0 makes $B002 hit pos 1 (high nibble CHR bank 0)
    /// — colliding with $B001.
    #[test]
    fn test_mapper23_vrc2b_chr_low_nibble_bank1_via_b002() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC2b: $B002 (A1=1) → chipA0=0, chipA1=1 → chip pos 2 = low nibble CHR bank 1
        mapper.write_prg(0xB002, 0x07);
        assert_eq!(
            mapper.read_chr(0x0400),
            7,
            "VRC2b $B002 must be chip pos 2 (low nibble CHR bank 1)"
        );
    }

    /// VRC4e addresses for IRQ: $F004 (latch high, chip pos 1), $F008 (control, chip pos 2),
    /// $F00C (ack, chip pos 3).
    #[test]
    fn test_mapper23_vrc4e_irq_register_addresses() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xF000, 0x0E); // pos 0: latch low = 0xE (A0=0, A2=0)
        // VRC4e: $F004 (A2=1) → chipA0=1, chipA1=0 → chip pos 1 = latch high
        mapper.write_prg(0xF004, 0x0F); // latch high = 0xF → latch = 0xFE
        // VRC4e: $F008 (A3=1) → chipA0=0, chipA1=1 → chip pos 2 = IRQ control
        mapper.write_prg(0xF008, 0b0000_0110); // M=1 (cycle mode), E=1 (enable)

        mapper.cpu_cycle(); // 0xFE → 0xFF
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle(); // 0xFF → overflow → IRQ
        assert!(
            mapper.irq_pending(),
            "VRC4e IRQ via $F008 control must fire after overflow"
        );

        // VRC4e: $F00C (A2=1, A3=1) → chipA0=1, chipA1=1 → chip pos 3 = acknowledge
        mapper.write_prg(0xF00C, 0);
        assert!(!mapper.irq_pending(), "VRC4e $F00C must acknowledge IRQ");
    }

    /// NES 2.0 submapper 2 = VRC4e only (A2→chipA0, A3→chipA1).
    /// $B004 (A2=1) → chip pos 1 = high nibble CHR bank 0.
    #[test]
    fn test_mapper23_submapper2_vrc4e_chr_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 2, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xB004, 0x01); // VRC4e: A2=1 → chip pos 1 (high nibble CHR bank 0)
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "submapper 2 (VRC4e): $B004 must set high nibble of CHR bank 0"
        );
    }

    /// NES 2.0 submapper 1 = VRC4f only (A0→chipA0, A1→chipA1).
    /// $B001 (A0=1) → chip pos 1 = high nibble CHR bank 0.
    #[test]
    fn test_mapper23_submapper1_vrc4f_chr_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 1, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xB001, 0x01); // VRC4f: A0=1 → chip pos 1 (high nibble CHR bank 0)
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "submapper 1 (VRC4f): $B001 must set high nibble of CHR bank 0"
        );
    }

    // =========================================================================
    // Mapper 25 address normalisation spec tests (issue #654)
    // =========================================================================

    /// NESdev spec: mapper 25 combines VRC2c/VRC4b (A1→chipA0, A0→chipA1) and
    /// VRC4d (A3→chipA0, A2→chipA1). Combined: chipA0 = CPU_A1|A3, chipA1 = CPU_A0|A2.
    ///
    /// VRC4d: $B008 (A3=1) → chip pos 1 = high nibble of CHR bank 0.
    /// Bug: current impl uses only A3→chipA1, making $B008 hit pos 2 (low nibble CHR bank 1).
    #[test]
    fn test_mapper25_vrc4d_chr_high_nibble_via_b008() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(25, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F); // low nibble CHR bank 0 = 0xF
        // VRC4d: $B008 (A3=1) → chipA0=1, chipA1=0 → chip pos 1 = high nibble CHR bank 0
        mapper.write_prg(0xB008, 0x01); // high nibble = 1 → bank = 0x1F = 31
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "VRC4d $B008 must be chip pos 1 (high nibble CHR bank 0), yielding bank 31"
        );
    }

    /// VRC4b: $B001 (A0=1) → chip pos 2 = low nibble of CHR bank 1.
    /// Bug: current impl maps A0 to neither chipA0 nor chipA1, so $B001 hits pos 0 (collision).
    #[test]
    fn test_mapper25_vrc4b_chr_low_nibble_bank1_via_b001() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper = create_vrc_mapper(25, prg_rom, chr_rom, NametableLayout::Horizontal);

        // VRC4b: $B001 (A0=1) → chipA0=0, chipA1=1 → chip pos 2 = low nibble CHR bank 1
        mapper.write_prg(0xB001, 0x07);
        assert_eq!(
            mapper.read_chr(0x0400),
            7,
            "VRC4b $B001 must be chip pos 2 (low nibble CHR bank 1)"
        );
    }

    /// NES 2.0 submapper 2 = VRC4d only (A3→chipA0, A2→chipA1).
    /// $B008 (A3=1) → chip pos 1 = high nibble CHR bank 0.
    #[test]
    fn test_mapper25_submapper2_vrc4d_chr_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 2, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xB008, 0x01); // VRC4d: A3=1 → chip pos 1
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "submapper 2 (VRC4d): $B008 must set high nibble of CHR bank 0"
        );
    }

    /// NES 2.0 submapper 1 = VRC4b only (A1→chipA0, A0→chipA1).
    /// $B002 (A1=1) → chip pos 1 = high nibble CHR bank 0.
    #[test]
    fn test_mapper25_submapper1_vrc4b_chr_addressing() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 32);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 1, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xB002, 0x01); // VRC4b: A1=1 → chip pos 1 (high nibble CHR bank 0)
        assert_eq!(
            mapper.read_chr(0x0000),
            31,
            "submapper 1 (VRC4b): $B002 must set high nibble of CHR bank 0"
        );
    }

    /// NESdev spec: "VRC2 only has 4 high bits of CHR select. $B001 bit 4 is ignored."
    /// Writing a high nibble value with bit 4 set on VRC2a must NOT advance the bank
    /// beyond what 4 bits allow — bit 4 must be silently discarded.
    ///
    /// Test uses 48 CHR banks so that 256 % 48 = 16 ≠ 0, meaning a raw 9-bit stored
    /// value of 0x100 does NOT accidentally wrap back to bank 0.
    #[test]
    fn test_vrc2a_mapper22_chr_high_nibble_4_bits_only() {
        // 48 × 1KB banks (256 % 48 = 16, so a 9-bit stored value 0x100 would map to
        // bank 16 without the fix, proving the mask is checked).
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);
        let mut mapper = create_vrc_mapper(22, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Write high nibble = 0x10 (only bit 4 set) to CHR bank 0.
        // VRC2a chip pos 1 (chip A0=1): CPU A1 = chip A0 = 1, CPU A0 = chip A1 = 0 → $B002.
        // With 5-bit mask: stored = 0x100 = 256 → >> 1 = 128 → 128 % 48 = 32 → bank 32 (≠ 0).
        // With 4-bit mask: bit 4 discarded → stored = 0x000 → >> 1 = 0 → bank 0 ✓.
        mapper.write_prg(0xB002, 0x10); // high nibble, bit 4 only
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "VRC2a: CHR high nibble bit 4 must be ignored — bank should be 0, not 32 or 128"
        );
    }

    // =========================================================================
    // Mapper 23 VRC2b mirroring compatibility (Wai Wai World)
    // =========================================================================

    /// NESdev: "VRC2-using games are usually well-behaved and only write 0 or 1 to this
    /// register, but Wai Wai World in one instance writes $FF instead."
    ///
    /// Wai Wai World is VRC2b (iNES mapper 23, no submapper). VRC2 only uses bit 0
    /// for mirroring (0 = Vertical, 1 = Horizontal). Writing $FF must give Horizontal,
    /// not SingleScreenUpper ($FF & 0x03 = 3), which VRC4 2-bit masking would produce.
    #[test]
    fn test_mapper23_combined_mirroring_ff_gives_horizontal() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Vertical);

        mapper.write_prg(0x9000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mapper 23 iNES 1.0: mirroring $FF must use VRC2 1-bit mask → Horizontal, not SingleScreenUpper"
        );
    }

    /// Mapper 23 iNES 1.0 combined: value 0x02 (bit 1 set, bit 0 clear) must give
    /// Vertical (1-bit VRC2 mask: 0x02 & 0x01 = 0), not SingleScreenLower (VRC4 2-bit).
    #[test]
    fn test_mapper23_combined_mirroring_ignores_bit1() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper = create_vrc_mapper(23, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0x9000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mapper 23 iNES 1.0: mirroring $02 must use VRC2 1-bit mask → Vertical, not SingleScreenLower"
        );
    }

    /// Mapper 23 submapper 2 (VRC4e only) must still support full 4-mode mirroring.
    /// Writing 0x02 to $9000 must give SingleScreenLower (VRC4 behavior).
    #[test]
    fn test_mapper23_submapper2_vrc4e_mirroring_supports_single_screen() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 2, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0x9000, 0x02);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "mapper 23 submapper 2 (VRC4e): mirroring $02 must give SingleScreenLower"
        );
    }

    // =========================================================================
    // Mapper 23 submapper 3 (VRC2b) spec correctness — 4 discrepancies
    // =========================================================================

    /// NESdev: VRC2b has no IRQ device.
    /// Setting up the IRQ counter and enabling it must never trigger an IRQ.
    ///
    /// VRC2b (mapper 23 sub 3) pin wiring: A0→chipA0, A1→chipA1.
    /// IRQ latch low  = chip pos 0 → CPU $F000.
    /// IRQ latch high = chip pos 1 (A0=1, A1=0) → CPU $F001.
    /// IRQ control    = chip pos 2 (A0=0, A1=1) → CPU $F002.
    #[test]
    fn test_mapper23_submapper3_vrc2b_no_irq() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Load latch = $FF so counter = $FF → fires on first cycle tick in cycle mode.
        mapper.write_prg(0xF000, 0xFF); // latch low nibble
        mapper.write_prg(0xF001, 0xFF); // latch high nibble (chip pos 1 → CPU $F001)
        // Enable IRQ in cycle mode (E=bit 1, M=bit 2 → value 0x06; also set A=bit 0 → 0x07).
        mapper.write_prg(0xF002, 0x07); // chip pos 2 (A0=0,A1=1) → CPU $F002

        // Tick enough cycles that a VRC4 chip would definitely assert IRQ.
        for _ in 0..260 {
            mapper.cpu_cycle();
        }

        assert!(
            !mapper.irq_pending(),
            "VRC2b (mapper 23 sub 3) has no IRQ hardware — irq_pending must stay false"
        );
    }

    /// NESdev: "VRC2 only has 4 high bits of CHR select. $B001 bit 4 is ignored."
    /// Writing high nibble 0x10 (only bit 4 set) to VRC2b must discard bit 4 → bank 0.
    ///
    /// 48 CHR banks: 256 % 48 = 16 ≠ 0, so a spurious 9-bit stored value would
    /// map to bank 16, not bank 0 — proving the mask is enforced.
    ///
    /// VRC2b pin wiring: A0→chipA0, A1→chipA1.
    /// CHR bank 0 high nibble = chip pos 1 (chipA0=1, chipA1=0) → CPU $B001.
    #[test]
    fn test_mapper23_submapper3_vrc2b_chr_ignores_bit4() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        // High nibble, bit 4 only. With 5-bit mask: bank = 256 % 48 = 16.
        // With 4-bit mask: bit 4 discarded → bank 0.
        mapper.write_prg(0xB001, 0x10); // chip pos 1 (A0=1, A1=0) → CPU $B001
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "VRC2b: CHR high nibble bit 4 must be ignored — bank should be 0, not 16"
        );
    }

    /// NESdev: "$9002 is VRC4-only; all four $9xxx positions are mirroring on VRC2."
    /// Writing 0x01 to the chip-position-2 register on VRC2b must update mirroring
    /// (bit 0 = 1 → Horizontal), not set PRG swap mode or enable WRAM.
    ///
    /// VRC2b: A0→chipA0, A1→chipA1.
    /// Chip pos 2 (chipA0=0, chipA1=1) → CPU $9002.
    #[test]
    fn test_mapper23_submapper3_vrc2b_9002_is_mirroring_not_swap() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 3, prg_rom, chr_rom, NametableLayout::Vertical);

        // Write 0x01 to chip pos 2 (CPU $9002). On VRC2b this is a mirroring write:
        // 1-bit mask → bit 0 = 1 → Horizontal. On VRC4 it would be PRG swap/WRAM control.
        mapper.write_prg(0x9002, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "VRC2b: chip pos 2 ($9002) must update mirroring, not PRG swap mode"
        );
    }

    /// NESdev: VRC2 has a 1-bit latch at $6000–$6FFF that is always accessible
    /// (no WRAM enable gate). The PRG RAM latch must be readable/writable without
    /// first enabling WRAM via a $9002 write.
    #[test]
    fn test_mapper23_submapper3_vrc2b_prg_ram_always_accessible() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(23, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        // No $9002 WRAM-enable write — VRC2b must still allow $6000 access.
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "VRC2b: $6000 latch must be accessible without WRAM-enable write"
        );
    }

    // =========================================================================
    // Mapper 25 submapper 3 (VRC2c) spec correctness — same 4 discrepancies
    // =========================================================================

    /// NESdev: VRC2c has no IRQ device.
    ///
    /// VRC2c (mapper 25 sub 3) pin wiring: A1→chipA0, A0→chipA1.
    /// IRQ latch low  = chip pos 0 → CPU $F000.
    /// IRQ latch high = chip pos 1 (chipA0=1, chipA1=0): CPU A0=chipA1=0, A1=chipA0=1 → $F002.
    /// IRQ control    = chip pos 2 (chipA0=0, chipA1=1): CPU A0=chipA1=1, A1=chipA0=0 → $F001.
    #[test]
    fn test_mapper25_submapper3_vrc2c_no_irq() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xF000, 0xFF); // latch low nibble (pos 0 → same address)
        mapper.write_prg(0xF002, 0xFF); // latch high nibble (chip pos 1 → CPU $F002)
        mapper.write_prg(0xF001, 0x07); // IRQ control: enable+cycle (chip pos 2 → CPU $F001)

        for _ in 0..260 {
            mapper.cpu_cycle();
        }

        assert!(
            !mapper.irq_pending(),
            "VRC2c (mapper 25 sub 3) has no IRQ hardware — irq_pending must stay false"
        );
    }

    /// NESdev: VRC2c CHR high nibble is 4 bits only; bit 4 must be ignored.
    ///
    /// VRC2c pin wiring: A1→chipA0, A0→chipA1.
    /// CHR bank 0 high nibble = chip pos 1 (chipA0=1, chipA1=0):
    /// CPU A0=chipA1=0, A1=chipA0=1 → $B002.
    #[test]
    fn test_mapper25_submapper3_vrc2c_chr_ignores_bit4() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0xB002, 0x10); // chip pos 1 (A1=1, A0=0) → CPU $B002
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "VRC2c: CHR high nibble bit 4 must be ignored — bank should be 0, not 16"
        );
    }

    /// NESdev: "$9002 is VRC4-only; all four $9xxx positions are mirroring on VRC2."
    /// VRC2c: chip pos 2 (chipA0=0, chipA1=1): CPU A0=chipA1=1, A1=chipA0=0 → $9001.
    /// Writing 0x01 to CPU $9001 must give Horizontal mirroring, not PRG swap/WRAM.
    #[test]
    fn test_mapper25_submapper3_vrc2c_9001_is_mirroring_not_swap() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 3, prg_rom, chr_rom, NametableLayout::Vertical);

        mapper.write_prg(0x9001, 0x01); // chip pos 2 (swapped wiring) → CPU $9001
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "VRC2c: chip pos 2 ($9001 CPU) must update mirroring, not PRG swap mode"
        );
    }

    /// NESdev: VRC2c has an always-accessible $6000 latch (no WRAM enable gate).
    #[test]
    fn test_mapper25_submapper3_vrc2c_prg_ram_always_accessible() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 2);
        let mut mapper =
            create_vrc_mapper_with_submapper(25, 3, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0x6000, 0xCD);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xCD,
            "VRC2c: $6000 latch must be accessible without WRAM-enable write"
        );
    }

    // =========================================================================
    // Mapper 27 (World Hero / VRC4_27) spec tests
    //
    // Mesen2 source (VRC2_4.h, VRCVariant::VRC4_27):
    //   A0 = addr & 0x01   (CPU bit 0 → chip A0)
    //   A1 = (addr >> 1) & 0x01   (CPU bit 1 → chip A1)
    // Full VRC4 capabilities: IRQ, 5-bit CHR, 4-way mirroring, PRG swap mode.
    // No heuristics — single well-defined addressing variant.
    // =========================================================================

    #[test]
    fn test_mapper27_is_registered() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper(MapperContext::new_for_test(
            27,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(mapper.is_ok(), "mapper 27 must be registered");
    }

    /// Mapper 27 PRG banking: two 8KB switchable windows at $8000/$A000,
    /// fixed second-to-last at $C000, fixed last at $E000.
    /// Address normalization: A0=bit0, A1=bit1 → $8000/$8001/$8002/$8003 all target
    /// the PRG bank 0 register range.
    #[test]
    fn test_mapper27_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);

        // PRG Select 0: bank 2 via $8000 (chip pos 0)
        mapper.write_prg(0x8000, 2);
        // PRG Select 1: bank 4 via $A000 (chip pos 0)
        mapper.write_prg(0xA000, 4);

        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 should be bank 2");
        assert_eq!(mapper.read_prg(0x9FFF), 2, "$9FFF should still be bank 2");
        assert_eq!(mapper.read_prg(0xA000), 4, "$A000 should be bank 4");
        assert_eq!(mapper.read_prg(0xBFFF), 4, "$BFFF should still be bank 4");
        // Second-to-last = bank 6 for 8-bank ROM
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be second-to-last (bank 6)"
        );
        // Last = bank 7
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should be last bank (bank 7)"
        );
    }

    /// Mapper 27 CHR banking: 5-bit addressing via split nibble writes, proving bit 4
    /// (bank >= 256) is honoured — a 4-bit mask would silently discard the high bit.
    /// Physical $B000 (A0=0, A1=0) → chip pos 0 = CHR bank 0 low nibble
    /// Physical $B001 (A0=1, A1=0) → chip pos 1 = CHR bank 0 high nibble
    #[test]
    fn test_mapper27_chr_banking_5bit_nibble_split() {
        let prg_rom = banked_data(8 * 1024, 2);
        // 512 × 1KB banks: banks 0-255 filled with 0, banks 256-511 filled with 1.
        // banked_data_with_upper_marker encodes (bank >> 8) per bank, so reading
        // bank 256+ returns 1 — impossible to achieve by accident under a 4-bit mask.
        let chr_rom = banked_data_with_upper_marker(1024, 512);
        let mut mapper = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Select CHR bank 0 = 256 (0x100): low nibble 0x00 via $B000, high nibble 0x10 via $B001.
        // 0x10 & 0x1F (VRC4 5-bit mask) = 0x10 → bank = (0x10 << 4) | 0x00 = 0x100 = 256.
        // Under a 4-bit mask (0x0F) the high nibble would be masked to 0 and bank would be 0.
        mapper.write_prg(0xB000, 0x00); // chip pos 0 (A0=0, A1=0): low nibble = 0
        mapper.write_prg(0xB001, 0x10); // chip pos 1 (A0=1, A1=0): high nibble bit 4 set → bank 256
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank 0 must map to bank 256 (upper-marker byte = 1) after 5-bit nibble write"
        );
    }

    /// Mapper 27 mirroring: full 4-way support (Vertical, Horizontal, Lower, Upper).
    /// Written via $9000 (chip pos 0): 0=V, 1=H, 2=lower, 3=upper.
    #[test]
    fn test_mapper27_four_mode_mirroring() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0x9000, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0x9000, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    /// Mapper 27 IRQ: VRC4 IRQ with latch-based counter.
    /// Physical $F000 (pos 0) = latch low nibble, $F001 (pos 1) = latch high nibble,
    /// $F002 (pos 2) = IRQ control, $F003 (pos 3) = acknowledge.
    #[test]
    fn test_mapper27_irq_fires_and_acknowledges() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);

        // Latch = 0xFE: low nibble 0xE via $F000, high nibble 0xF via $F001
        mapper.write_prg(0xF000, 0x0E); // chip pos 0 (A0=0, A1=0)
        mapper.write_prg(0xF001, 0x0F); // chip pos 1 (A0=1, A1=0)
        // IRQ control via $F002 (chip pos 2, A0=0, A1=1): M=1 (cycle mode), E=1
        mapper.write_prg(0xF002, 0b0000_0110);

        // counter = 0xFE → cycle 1: 0xFF (no IRQ), cycle 2: overflow → IRQ
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ must not fire after first cycle");
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must fire after counter overflow");

        // Acknowledge via $F003 (chip pos 3, A0=1, A1=1)
        mapper.write_prg(0xF003, 0x00);
        assert!(!mapper.irq_pending(), "IRQ must clear after acknowledge");
    }

    /// Mapper 27 PRG swap mode: $9002 bit 1 swaps $8000/$C000 windows.
    /// Physical $9002 (A0=0, A1=1) → chip pos 2 = PRG swap + WRAM enable.
    #[test]
    fn test_mapper27_prg_swap_mode() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 2); // PRG Select 0 = bank 2

        // Before swap: bank 2 at $8000
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "before swap: $8000 should be bank 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "before swap: $C000 should be second-to-last"
        );

        // Enable swap mode: $9002 (A0=0, A1=1) bit 1 = M
        mapper.write_prg(0x9002, 0b0000_0010);

        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "after swap: $8000 should be second-to-last"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "after swap: $C000 should be bank 2"
        );
    }

    /// Mapper 27 save-state: snapshot/restore roundtrip preserves all register state.
    #[test]
    fn test_mapper27_snapshot_restore_roundtrip() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 32);

        let mut mapper = create_vrc_mapper(
            27,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        );

        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xB001, 0x01); // CHR bank 0 = 0x1F
        mapper.write_prg(0x9000, 0x02); // mirroring = SingleScreenLower
        mapper.write_prg(0xF000, 0x0E);
        mapper.write_prg(0xF001, 0x0F);
        mapper.write_prg(0xF002, 0b0000_0110);
        mapper.cpu_cycle();

        let snapshot = mapper.registers_snapshot();
        let mut restored = create_vrc_mapper(27, prg_rom, chr_rom, NametableLayout::Horizontal);
        restored.restore_registers(&snapshot);

        assert_eq!(
            snapshot,
            restored.registers_snapshot(),
            "snapshot/restore roundtrip must be idempotent"
        );
    }
}
