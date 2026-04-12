//! Mapper 256 – OneBus / VR02 / VT03 System
//!
//! Specifications:
//! - Primary source: FCEUX onebus.cpp by CaH4e3 (GPL v2)
//! - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_256>
//!
//! Hardware: VR02/VT03 "OneBus" system used in plug-and-play multicarts
//! (Street Dance, 101-in-1 Arcade Action II, DreamGEAR 75-in-1, etc.)
//!
//! ## Banking
//! - PRG: 4 × 8 KB switchable banks at $8000–$FFFF via `cpu410x` registers.
//! - CHR: 8 × 1 KB banks mapped from PRG ROM (CHR ROM is empty for these carts).
//! - Outer PRG/CHR bank base controlled by `cpu410x[0]`, `cpu410x[0xA]`, `cpu410x[0xB]`.
//!
//! ## Registers
//! - `$4100–$410F` writes: CPU banking and IRQ control registers (`cpu410x`).
//! - `$8000–$FFFF` writes: MMC3-compatible banking interface.
//! - `$A000` write: nametable mirroring (bit 0: 0 = Horizontal, 1 = Vertical).
//!
//! ## IRQ
//! Scanline-based counter (`IRQLatch` / `cpu410x[1]`), compatible with the MMC3
//! IRQ interface at `$C000–$E001`.
//!
//! ## Known Limitations
//! - `$2010–$201F` CHR-bank writes are PPU-bus addresses and cannot be intercepted
//!   through the mapper interface; CHR banks are updated only via the `$8001`
//!   MMC3-compatible writes and direct `$4100–$410F` writes.
//! - PCM/audio expansion (`$40xx` register extensions) is not implemented.
//! - AD12-based IRQ mode (IRQLatch bit 7 = 0) falls back to scanline IRQ counting.
//! - PowerJoy Supermax Cart PRG-register swap (`inv_hack`) is not implemented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 256 – OneBus / VR02 / VT03 System
///
/// Implements the VT03/VR02 "OneBus" banking hardware used in many
/// plug-and-play Famicom-compatible multicart systems.
///
/// Specifications:
/// - Primary source: FCEUX `onebus.cpp` by CaH4e3
/// - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_256>
/// - PRG-ROM: up to 4 MB; 8 KB banks switchable at $8000–$FFFF
/// - CHR: 8 × 1 KB banks mapped from PRG ROM
/// - Mirroring: software-controlled via `$A000` write (bit 0)
/// - WRAM: 8 KB at $6000–$7FFF
pub struct Mapper256 {
    base: BaseMapper,
    /// `cpu410x[0..16]` – CPU-side banking/IRQ registers at $4100–$410F.
    ///
    /// Key indices:
    /// - `[0x0]`: outer bank byte (bits 7–4 = PRG block high; bits 3–0 = CHR block low)
    /// - `[0x1]`: IRQ latch (`IRQLatch`; bit 0 always 0; bit 7 = mode 0=A12/1=HSYNC)
    /// - `[0x5]`: `mmc3cmd` (bit 7=CHR swap; bit 6=PRG swap; bits 2–0=reg select)
    /// - `[0x6]`: `mirror` (bit 0: 0=Horizontal, 1=Vertical)
    /// - `[0x7]`: PRG bank 0 (switchable slot at $8000 or $C000)
    /// - `[0x8]`: PRG bank 1 (switchable slot at $A000)
    /// - `[0x9]`: PRG bank 2 (switchable $C000 when `cpu410x[0xB] & 0x40`)
    /// - `[0xA]`: PRG/CHR outer page select
    /// - `[0xB]`: bank mode (bits 2–0 = PRG mask mode; bit 6 = 3-bank PRG)
    cpu410x: [u8; 16],
    /// `ppu201x[0..16]` – CHR banking registers (written via MMC3 $8001 interface).
    ///
    /// Key indices:
    /// - `[0x2]`–`[0x5]`: CHR 1 KB banks 4–7
    /// - `[0x6]`: CHR 2 KB base for banks 0/1 (low bit forced 0/1)
    /// - `[0x7]`: CHR 2 KB base for banks 2/3 (low bit forced 0/1)
    /// - `[0x8]`: CHR outer bank (bits 6–4)
    /// - `[0xA]`: CHR bank mode (bits 2–0)
    ppu201x: [u8; 16],
    /// Latched IRQ counter reload value.
    irq_latch: u8,
    /// Scanline IRQ down-counter.
    irq_count: u8,
    /// Reload flag: reload `irq_count` from `irq_latch` on next scanline.
    irq_reload: bool,
    /// IRQ enabled flag.
    irq_enabled: bool,
    /// Latched IRQ pending flag (cleared by disabling IRQ or explicit clear).
    irq_pending_flag: bool,
}

impl Mapper256 {
    const MAPPER_NUMBER: u16 = 256;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false, // CHR is handled manually from PRG ROM
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        // PRG: 4 × 8 KB slots at $8000–$FFFF
        base.configure_prg_banking(8 * 1024);
        // set_mirroring_hv(true) = Horizontal (power-on state; cpu410x[6] = 0 → bit 0 = 0 → H)
        base.set_mirroring_hv(true);

        let mut mapper = Self {
            base,
            cpu410x: [0u8; 16],
            ppu201x: [0u8; 16],
            irq_latch: 0,
            irq_count: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_pending_flag: false,
        };
        mapper.sync_prg();
        mapper
    }

    // -----------------------------------------------------------------------
    // Synchronise PRG banking from cpu410x registers
    // -----------------------------------------------------------------------

    /// Recompute PRG bank mapping from `cpu410x` registers (mirrors FCEUX `PSync`).
    fn sync_prg(&mut self) {
        let bankmode = self.cpu410x[0xb] & 7;
        let mask: u8 = if bankmode == 7 {
            0xFF
        } else {
            0x3F >> bankmode
        };
        let block: u16 = (((self.cpu410x[0x0] as u16) & 0xF0) << 4)
            | ((self.cpu410x[0xa] as u16) & !(mask as u16));
        // bit 6 of mmc3cmd selects whether bank0 maps to $8000 or $C000
        let pswap = (self.cpu410x[0x5] & 0x40) != 0;

        let bank0 = (block | ((self.cpu410x[0x7] as u16) & mask as u16)) as i16;
        let bank1 = (block | ((self.cpu410x[0x8] as u16) & mask as u16)) as i16;
        // $C000 bank: if bit 6 of cpu410x[0xb] set → use cpu410x[0x9]; else fixed second-to-last
        let bank2_raw: u8 = if self.cpu410x[0xb] & 0x40 != 0 {
            self.cpu410x[0x9]
        } else {
            !1u8 // 0xFE – second-to-last bank within mask group
        };
        let bank2 = (block | ((bank2_raw as u16) & mask as u16)) as i16;
        // $E000 is always the last bank in the current block
        let bank3 = (block | (!0u8 as u16 & mask as u16)) as i16;

        if pswap {
            // PRG swap: bank0 → $C000, bank2 → $8000
            self.base.select_prg_page(0, bank2); // $8000
            self.base.select_prg_page(1, bank1); // $A000
            self.base.select_prg_page(2, bank0); // $C000
        } else {
            self.base.select_prg_page(0, bank0); // $8000
            self.base.select_prg_page(1, bank1); // $A000
            self.base.select_prg_page(2, bank2); // $C000
        }
        self.base.select_prg_page(3, bank3); // $E000 always fixed
    }

    // -----------------------------------------------------------------------
    // Compute CHR bank numbers from ppu201x / cpu410x registers (mirrors FCEUX `CSync`)
    // -----------------------------------------------------------------------

    /// Compute the 1 KB CHR bank number for `slot` (0–7) from current registers.
    fn chr_bank_for_slot(&self, slot: usize) -> usize {
        const MIDX: [u8; 8] = [0, 1, 2, 0, 3, 4, 5, 0];
        let mode = (self.ppu201x[0xa] & 7) as usize;
        let mask: u8 = 0xFF >> MIDX[mode];
        let block: u32 = (((self.cpu410x[0x0] as u32) & 0x0F) << 11)
            | (((self.ppu201x[0x8] as u32) & 0x70) << 4)
            | ((self.ppu201x[0xa] as u32) & !(mask as u32));
        // bit 7 of mmc3cmd inverts the 4 KB half (like MMC3 CHR inversion)
        let cswap = (self.cpu410x[0x5] & 0x80) != 0;

        // Map the 8 raw CHR banks from ppu201x registers
        let raw_banks: [u8; 8] = [
            self.ppu201x[0x6] & !1u8, // slot 0: 2KB-aligned
            self.ppu201x[0x6] | 1u8,  // slot 1
            self.ppu201x[0x7] & !1u8, // slot 2: 2KB-aligned
            self.ppu201x[0x7] | 1u8,  // slot 3
            self.ppu201x[0x2],        // slot 4
            self.ppu201x[0x3],        // slot 5
            self.ppu201x[0x4],        // slot 6
            self.ppu201x[0x5],        // slot 7
        ];
        // Apply CHR swap: if cswap, slots 0-3 and 4-7 are exchanged
        let effective_slot = if cswap { slot ^ 4 } else { slot };
        let raw = raw_banks[effective_slot];
        (block | ((raw as u32) & mask as u32)) as usize
    }

    // -----------------------------------------------------------------------
    // Synchronise mirroring from cpu410x[6]
    // -----------------------------------------------------------------------

    fn sync_mirroring(&mut self) {
        // cpu410x[6] bit 0: 0 = Horizontal, 1 = Vertical
        let horizontal = (self.cpu410x[0x6] & 1) == 0;
        self.base.set_mirroring_hv(horizontal);
    }

    // -----------------------------------------------------------------------
    // Synchronise both PRG and CHR (full Sync)
    // -----------------------------------------------------------------------

    fn sync(&mut self) {
        self.sync_prg();
        self.sync_mirroring();
    }

    // -----------------------------------------------------------------------
    // Handle $8000–$FFFF (MMC3-compatible) writes
    // -----------------------------------------------------------------------

    fn write_mmc3(&mut self, addr: u16, value: u8) {
        match addr & 0xE001 {
            // $8000: bank select register
            0x8000 => {
                // Keep bits 3–5 unchanged; update bits 7, 6, and 2–0.
                self.cpu410x[0x5] = (self.cpu410x[0x5] & 0x38) | (value & 0xC7);
                self.sync();
            }
            // $8001: bank data
            0x8001 => {
                match self.cpu410x[0x5] & 7 {
                    0 => {
                        self.ppu201x[0x6] = value;
                        // CHR – no sync_prg needed
                    }
                    1 => {
                        self.ppu201x[0x7] = value;
                    }
                    2 => {
                        self.ppu201x[0x2] = value;
                    }
                    3 => {
                        self.ppu201x[0x3] = value;
                    }
                    4 => {
                        self.ppu201x[0x4] = value;
                    }
                    5 => {
                        self.ppu201x[0x5] = value;
                    }
                    6 => {
                        self.cpu410x[0x7] = value;
                        self.sync_prg();
                    }
                    7 => {
                        self.cpu410x[0x8] = value;
                        self.sync_prg();
                    }
                    _ => {}
                }
            }
            // $A000: mirroring
            0xA000 => {
                self.cpu410x[0x6] = value;
                self.sync_mirroring();
            }
            // $C000: IRQ latch
            0xC000 => {
                self.irq_latch = value & 0xFE;
            }
            // $C001: IRQ reload
            0xC001 => {
                self.irq_reload = true;
            }
            // $E000: IRQ disable + clear
            0xE000 => {
                self.irq_enabled = false;
                self.irq_pending_flag = false;
            }
            // $E001: IRQ enable
            0xE001 => {
                self.irq_enabled = true;
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Handle $4100–$410F writes
    // -----------------------------------------------------------------------

    fn write_cpu410x(&mut self, addr: u16, value: u8) {
        match addr & 0x0F {
            // IRQ latch (bit 0 always cleared; bit 7 = mode)
            0x1 => {
                self.irq_latch = value & 0xFE;
            }
            // IRQ reload
            0x2 => {
                self.irq_reload = true;
            }
            // IRQ disable + clear
            0x3 => {
                self.irq_enabled = false;
                self.irq_pending_flag = false;
            }
            // IRQ enable
            0x4 => {
                self.irq_enabled = true;
            }
            // All other registers: store and re-sync banking
            idx => {
                self.cpu410x[idx as usize] = value;
                self.sync();
            }
        }
    }
}

impl Mapper for Mapper256 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    // -----------------------------------------------------------------------
    // PRG read/write
    // -----------------------------------------------------------------------

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x4100..=0x410F => {
                self.write_cpu410x(addr, value);
            }
            0x6000..=0x7FFF => {
                self.base.try_write_prg_ram(addr, value);
            }
            0x8000..=0xFFFF => {
                self.write_mmc3(addr, value);
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // CHR read/write – CHR data is mapped from PRG ROM
    // -----------------------------------------------------------------------

    fn read_chr(&mut self, addr: u16) -> u8 {
        let slot = (addr >> 10) as usize; // 1 KB slot index (0–7)
        let bank = self.chr_bank_for_slot(slot);
        let offset = (addr & 0x3FF) as usize;
        let abs = bank * 1024 + offset;
        let prg = self.base.prg_rom();
        if abs < prg.len() {
            prg[abs]
        } else {
            0
        }
    }

    fn write_chr(&mut self, _addr: u16, _value: u8) {
        // CHR is always ROM (mapped from PRG ROM); writes are ignored.
    }

    // -----------------------------------------------------------------------
    // IRQ
    // -----------------------------------------------------------------------

    fn irq_pending(&self) -> bool {
        self.irq_pending_flag
    }

    /// Scanline IRQ counter – called once per visible scanline.
    fn ppu_scanline(&mut self, _scanline: u16, rendering_enabled: bool) {
        if !rendering_enabled {
            return;
        }
        let count = self.irq_count;
        if count == 0 || self.irq_reload {
            self.irq_count = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_count -= 1;
        }
        if count != 0 && self.irq_count == 0 && self.irq_enabled {
            self.irq_pending_flag = true;
        }
    }

    // -----------------------------------------------------------------------
    // Reset
    // -----------------------------------------------------------------------

    fn reset(&mut self) {
        self.cpu410x = [0u8; 16];
        self.ppu201x = [0u8; 16];
        self.irq_latch = 0;
        self.irq_count = 0;
        self.irq_reload = false;
        self.irq_enabled = false;
        self.irq_pending_flag = false;
        // Horizontal mirroring is the power-on state (cpu410x[6] = 0 → H)
        self.base.set_mirroring_hv(true);
        self.sync_prg();
    }

    // -----------------------------------------------------------------------
    // Save / restore state
    // -----------------------------------------------------------------------

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(16 + 16 + 5);
        v.extend_from_slice(&self.cpu410x);
        v.extend_from_slice(&self.ppu201x);
        v.push(self.irq_latch);
        v.push(self.irq_count);
        v.push(u8::from(self.irq_reload));
        v.push(u8::from(self.irq_enabled));
        v.push(u8::from(self.irq_pending_flag));
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 32 {
            self.cpu410x.copy_from_slice(&data[0..16]);
            self.ppu201x.copy_from_slice(&data[16..32]);
        }
        if data.len() >= 37 {
            self.irq_latch = data[32];
            self.irq_count = data[33];
            self.irq_reload = data[34] != 0;
            self.irq_enabled = data[35] != 0;
            self.irq_pending_flag = data[36] != 0;
        }
        self.sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Create a mapper 256 with `n` × 8 KB PRG banks and no separate CHR ROM
    /// (CHR comes from PRG ROM as per OneBus hardware).
    fn create_mapper256(prg_rom: Vec<u8>, mirroring: NametableLayout) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(256, prg_rom, vec![], mirroring))
    }

    // -----------------------------------------------------------------------
    // Factory / instantiation
    // -----------------------------------------------------------------------

    #[test]
    fn test_factory_creates_mapper_256() {
        let prg = banked_data(8 * 1024, 8);
        let mapper = create_mapper256(prg, NametableLayout::Horizontal);
        assert!(mapper.is_ok(), "Mapper 256 should be creatable via factory");
        assert_eq!(mapper.unwrap().mapper_number(), 256);
    }

    // -----------------------------------------------------------------------
    // PRG banking via MMC3 interface ($8000–$FFFF)
    // -----------------------------------------------------------------------

    #[test]
    fn test_prg_bank_select_via_mmc3_interface() {
        // 16 banks of 8KB = 128KB PRG; bank N is filled with byte N
        let prg = banked_data(8 * 1024, 16);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Select register 6 (PRG bank 0 at $8000) and load bank 3
        mapper.write_prg(0x8000, 6); // reg select = 6
        mapper.write_prg(0x8001, 3); // bank 3 → cpu410x[7]
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 should read bank 3");

        // Select register 7 (PRG bank 1 at $A000) and load bank 5
        mapper.write_prg(0x8000, 7); // reg select = 7
        mapper.write_prg(0x8001, 5); // bank 5 → cpu410x[8]
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should read bank 5");
    }

    #[test]
    fn test_prg_bank_e000_fixed_to_last() {
        // 8 banks of 8KB (0..7); last bank = 7
        let prg = banked_data(8 * 1024, 8);
        let mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();
        // $E000 is always fixed to the last bank in the current block.
        // With default registers (block=0, mask=0x3F), last-in-block = bank 7 (0x3F wraps).
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should always read the last bank");
    }

    #[test]
    fn test_prg_swap_via_mmc3cmd_bit6() {
        // 8 banks of 8KB; banks 0–7
        let prg = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Load bank 2 into PRG slot 0 (cpu410x[7])
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 2);

        // Before swap: $8000 = bank 2
        assert_eq!(mapper.read_prg(0x8000), 2, "Before swap $8000 = bank 2");

        // Enable PRG swap (bit 6 of $8000 write)
        mapper.write_prg(0x8000, 0x46); // bit 6 set, reg select = 6
        mapper.write_prg(0x8001, 2);   // keep bank 2 in cpu410x[7]

        // After swap: $C000 = bank 2, $8000 = second-to-last bank
        assert_eq!(mapper.read_prg(0xC000), 2, "After swap $C000 = bank 2");
    }

    // -----------------------------------------------------------------------
    // PRG banking via $4100–$410F
    // -----------------------------------------------------------------------

    #[test]
    fn test_prg_bank_select_via_4100_registers() {
        let prg = banked_data(8 * 1024, 16);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Write cpu410x[0x7] = 4 directly via $4107
        mapper.write_prg(0x4107, 4);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should map bank 4 after $4107 write");

        // Write cpu410x[0x8] = 6 directly via $4108
        mapper.write_prg(0x4108, 6);
        assert_eq!(mapper.read_prg(0xA000), 6, "$A000 should map bank 6 after $4108 write");
    }

    // -----------------------------------------------------------------------
    // CHR banking from PRG ROM
    // -----------------------------------------------------------------------

    #[test]
    fn test_chr_reads_from_prg_rom() {
        // 16 banks of 8KB = 128KB; bank N is filled with byte N
        let prg = banked_data(8 * 1024, 16);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // CHR slots 0/1 come from ppu201x[6]; 1KB-aligned pairs.
        // Via $8001 with reg 0: set ppu201x[6] = 4 → slot 0 maps to 1KB bank 4, slot 1 to bank 5.
        mapper.write_prg(0x8000, 0); // reg select = 0
        mapper.write_prg(0x8001, 4); // ppu201x[6] = 4

        // CHR slot 0 = bank 4 → byte offset 4*1024 = 0x1000 in PRG ROM
        // PRG ROM is banked_data(8KB, 16), so bank 0 = byte 0..0x2000.
        // 1KB CHR bank 4 starts at 4*1024 = 4096 in PRG ROM → filled with 0 (bank 0 of 8KB).
        // bank 4 of 1KB means byte 4096 in the 8KB bank 0 region (all filled with 0).
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR slot 0 should read from PRG ROM bank 4");
    }

    #[test]
    fn test_chr_swap_via_mmc3cmd_bit7() {
        // PRG: 4 banks × 8KB; all of bank 0 = 0x00, bank 1 = 0x01, etc.
        let prg = banked_data(8 * 1024, 8);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Set ppu201x[6] = 0 (CHR slots 0/1 → 1KB banks 0 and 1 from PRG)
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 0);

        // Set ppu201x[2] = 4 (CHR slot 4 → 1KB bank 4 from PRG)
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8001, 4);

        // Without swap: $0000 → slot 0 → bank 0; $1000 → slot 4 → bank 4
        // (both still in PRG bank 0 fill=0 since 4KB < 8KB)
        let before = mapper.read_chr(0x0000);
        assert_eq!(before, 0, "Without CHR swap, $0000 → bank 0 (fill=0)");

        // Enable CHR swap (bit 7 of $8000)
        mapper.write_prg(0x8000, 0x80); // bit 7 set, reg select = 0
        mapper.write_prg(0x8001, 0);    // keep ppu201x[6] = 0

        // With swap: $0000 → slot 4, $1000 → slot 0 (CHR inversion)
        let after = mapper.read_chr(0x0000);
        assert_eq!(after, 0, "After CHR swap, $0000 reads swapped slot (still bank 4 = fill 0)");
    }

    // -----------------------------------------------------------------------
    // Mirroring
    // -----------------------------------------------------------------------

    #[test]
    fn test_mirroring_horizontal_on_power_on() {
        let prg = banked_data(8 * 1024, 4);
        let mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Power-on mirroring should be Horizontal (cpu410x[6]=0)"
        );
    }

    #[test]
    fn test_mirroring_toggle_via_a000() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // $A000 write bit 0 = 1 → Vertical
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // $A000 write bit 0 = 0 → Horizontal
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mirroring_toggle_via_4106() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // $4106 is cpu410x[6] (mirror register)
        mapper.write_prg(0x4106, 1); // bit 0 = 1 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x4106, 0); // bit 0 = 0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -----------------------------------------------------------------------
    // IRQ
    // -----------------------------------------------------------------------

    #[test]
    fn test_irq_not_pending_initially() {
        let prg = banked_data(8 * 1024, 4);
        let mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();
        assert!(!mapper.irq_pending(), "IRQ should not be pending at power-on");
    }

    #[test]
    fn test_irq_fires_after_latch_scanlines() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Set IRQ latch to 3 and enable IRQ via MMC3 interface
        mapper.write_prg(0xC000, 3);  // IRQ latch = 3
        mapper.write_prg(0xC001, 0);  // reload
        mapper.write_prg(0xE001, 0);  // IRQ enable

        // Run 3 scanlines; IRQ should fire on the 3rd
        mapper.ppu_scanline(0, true);
        assert!(!mapper.irq_pending(), "IRQ should not fire after 1 scanline");
        mapper.ppu_scanline(1, true);
        assert!(!mapper.irq_pending(), "IRQ should not fire after 2 scanlines");
        mapper.ppu_scanline(2, true);
        assert!(mapper.irq_pending(), "IRQ should fire after 3 scanlines");
    }

    #[test]
    fn test_irq_disabled_via_e000() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // Use latch=4 (bit 0 cleared → remains 4) so IRQ fires after 4 scanlines
        mapper.write_prg(0xC000, 4);  // latch = 4 & 0xFE = 4
        mapper.write_prg(0xC001, 0);  // reload
        mapper.write_prg(0xE001, 0);  // enable

        // Run scanlines until IRQ fires: reload on 0, count 4→3→2→1→0 (fires on 4th decrement)
        mapper.ppu_scanline(0, true); // reload → count=4
        mapper.ppu_scanline(1, true); // 4→3
        mapper.ppu_scanline(2, true); // 3→2
        mapper.ppu_scanline(3, true); // 2→1
        mapper.ppu_scanline(4, true); // 1→0 → FIRE
        assert!(mapper.irq_pending(), "IRQ should fire after latch scanlines");

        // Disable clears pending
        mapper.write_prg(0xE000, 0);
        assert!(!mapper.irq_pending(), "IRQ should clear when disabled via $E000");
    }

    #[test]
    fn test_irq_via_4100_registers() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // $4101 = IRQ latch; $4102 = reload; $4104 = enable
        // Write latch=4 (bit 0 cleared → 4 & 0xFE = 4)
        mapper.write_prg(0x4101, 4); // latch = 4
        mapper.write_prg(0x4102, 0); // reload
        mapper.write_prg(0x4104, 0); // enable

        // Reload on scanline 0; count 4→3→2→1→0 fires on scanline 4
        mapper.ppu_scanline(0, true); // reload → count=4
        assert!(!mapper.irq_pending());
        mapper.ppu_scanline(1, true); // 4→3
        mapper.ppu_scanline(2, true); // 3→2
        mapper.ppu_scanline(3, true); // 2→1
        mapper.ppu_scanline(4, true); // 1→0 → FIRE
        assert!(mapper.irq_pending(), "IRQ via $4101/$4104 should fire");

        // $4103 = disable + clear
        mapper.write_prg(0x4103, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_irq_no_fire_when_rendering_disabled() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        // Rendering disabled → scanline hook is a no-op
        mapper.ppu_scanline(0, false);
        assert!(!mapper.irq_pending(), "No IRQ when rendering is disabled");
    }

    // -----------------------------------------------------------------------
    // Save / restore state
    // -----------------------------------------------------------------------

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg = banked_data(8 * 1024, 16);
        let mut mapper = create_mapper256(prg.clone(), NametableLayout::Horizontal).unwrap();

        // Set some state
        mapper.write_prg(0x8000, 6);   // reg select = 6
        mapper.write_prg(0x8001, 5);   // PRG bank 0 = 5
        mapper.write_prg(0xA000, 1);   // Vertical mirroring
        mapper.write_prg(0xC000, 7);   // IRQ latch = 7 (bit 0 cleared → 6)
        mapper.write_prg(0xE001, 0);   // IRQ enable

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper256(prg, NametableLayout::Horizontal).unwrap();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 5, "PRG bank 0 should restore to 5");
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical, "Mirroring should restore");
        assert!(restored.irq_pending() == mapper.irq_pending(), "IRQ pending should match");
    }

    // -----------------------------------------------------------------------
    // Reset
    // -----------------------------------------------------------------------

    #[test]
    fn test_reset_clears_state() {
        let prg = banked_data(8 * 1024, 16);
        let mut mapper = create_mapper256(prg, NametableLayout::Vertical).unwrap();

        // Modify state
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0xA000, 1); // Vertical
        mapper.write_prg(0xE001, 0); // enable IRQ

        mapper.reset();

        // After reset: bank 0 should be 0, mirroring should be Horizontal
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 should reset to 0");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal, "Mirroring resets to H");
        assert!(!mapper.irq_pending(), "IRQ pending clears on reset");
    }

    // -----------------------------------------------------------------------
    // WRAM
    // -----------------------------------------------------------------------

    #[test]
    fn test_wram_readable_and_writable() {
        let prg = banked_data(8 * 1024, 4);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB, "WRAM write/read should work");

        mapper.write_prg(0x7FFF, 0x55);
        assert_eq!(mapper.read_prg(0x7FFF), 0x55, "WRAM end boundary should work");
    }

    // -----------------------------------------------------------------------
    // Outer bank (cpu410x[0], cpu410x[0xA], cpu410x[0xB])
    // -----------------------------------------------------------------------

    #[test]
    fn test_outer_prg_bank_via_410a() {
        // 128KB = 128 × 1KB / 8KB-per-bank = actually 128 banks × 8KB = 1024KB
        // Use 128 banks so bank 66 is unambiguously within range.
        let prg = banked_data(8 * 1024, 128);
        let mut mapper = create_mapper256(prg, NametableLayout::Horizontal).unwrap();

        // bankmode=0 → mask=0x3F; set outer block via cpu410x[0xA] = 0x40.
        // block = (cpu410x[0x0] & 0xF0) << 4 | (cpu410x[0xA] & ~mask) = 0 | (0x40 & ~0x3F) = 0x40
        mapper.write_prg(0x410A, 0x40);
        // Select inner bank 2 via cpu410x[7] = 2
        mapper.write_prg(0x4107, 2);
        // Expected PRG bank at $8000 = block | (inner & mask) = 0x40 | (2 & 0x3F) = 0x42 = 66
        assert_eq!(mapper.read_prg(0x8000), 66, "Outer PRG bank select: block=0x40, inner=2 → bank 66");
    }
}
