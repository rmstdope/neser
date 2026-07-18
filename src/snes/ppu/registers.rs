//! PPU register read/write dispatch and VRAM/CGRAM/OAM access.
//!
//! The PPU register file is addressed by its 16-bit offset (`$2100-$213F`), plus the CPU I/O
//! ports the PPU owns (`$4200` NMITIMEN, `$4201` WRIO, `$4210` RDNMI, `$4211` TIMEUP,
//! `$4212` HVBJOY). The bus passes the bare offset to [`Ppu::write_register`] /
//! [`Ppu::read_register`].

use super::{
    CGRAM_SIZE, CPU_VERSION, HBLANK_START_DOT, PPU1_VERSION, PPU2_VERSION, Ppu, VISIBLE_DOT_START,
    VRAM_SIZE, VramAddressTranslation,
};
use crate::platform::debugging::ppu_trace_level;
use crate::trace_ppu;

impl Ppu {
    /// Write a PPU register by its 16-bit address offset.
    pub fn write_register(&mut self, addr: u16, value: u8) {
        let trace = ppu_trace_level() >= 3;
        if trace {
            trace_ppu!(3; "write {:04X}={:02X} y={} x={} inidisp={:02X} mode={} tm={:02X} ts={:02X} nmi={} vblank={} irq={} frame={}",
                addr,
                value,
                self.position.scanline,
                self.position.dot,
                self.inidisp,
                self.bg_mode,
                self.tm,
                self.ts,
                self.nmi_enable as u8,
                self.vblank_active as u8,
                self.irq_line as u8,
                (self.pending_completed_frames > 0) as u8,
            );
        }
        match addr {
            // INIDISP: forced blank (bit 7) + master brightness (bits 0-3).
            0x2100 => self.inidisp = value,
            // OBSEL: OBJ size pair (bits 7-5), name gap (bits 4-3), OBJ tile name base (bits 2-0).
            0x2101 => {
                self.obsel = value;
                self.obj_eval_dirty = true;
            }
            // BGMODE: BG screen mode (bits 0-2), BG3 high-priority (bit 3), per-BG tile size.
            0x2105 => {
                self.bg_mode = value & 0x07;
                self.bg3_priority = value & 0x08 != 0;
                for bg in 0..4 {
                    self.bg_tile_size_16[bg] = value & (0x10 << bg) != 0;
                }
            }
            // MOSAIC: mosaic size (bits 7-4) and per-BG enable (bits 3-0).
            0x2106 => self.mosaic = value,
            // BGnSC: tilemap base (bits 2-7, 1K-word steps) + size (bits 0-1).
            0x2107..=0x210A => {
                let bg = (addr - 0x2107) as usize;
                self.bg_tilemap_base[bg] = ((value as u16) >> 2) << 10;
                self.bg_screen_size[bg] = value & 0x03;
            }
            // BG12NBA: BG1 (bits 0-3) + BG2 (bits 4-7) char base (4K-word steps).
            0x210B => {
                self.bg_char_base[0] = ((value & 0x0F) as u16) << 12;
                self.bg_char_base[1] = ((value >> 4) as u16) << 12;
            }
            // BG34NBA: BG3 (bits 0-3) + BG4 (bits 4-7) char base (4K-word steps).
            0x210C => {
                self.bg_char_base[2] = ((value & 0x0F) as u16) << 12;
                self.bg_char_base[3] = ((value >> 4) as u16) << 12;
            }
            // BG1HOFS / M7HOFS ($210D) and BG1VOFS / M7VOFS ($210E): each write updates BOTH the
            // BG1 scroll (via BG_old) and the Mode 7 scroll (via the shared M7_old latch).
            0x210D => {
                self.write_bg_hofs(0, value);
                self.m7hofs = self.write_m7_twice(value);
            }
            0x210E => {
                self.write_bg_vofs(0, value);
                self.m7vofs = self.write_m7_twice(value);
            }
            // BGnHOFS / BGnVOFS: write-twice scroll via the shared BG_old latch.
            0x210F | 0x2111 | 0x2113 => {
                let bg = ((addr - 0x210D) / 2) as usize;
                self.write_bg_hofs(bg, value);
            }
            0x2110 | 0x2112 | 0x2114 => {
                let bg = ((addr - 0x210E) / 2) as usize;
                self.write_bg_vofs(bg, value);
            }
            // M7SEL: Mode 7 screen-over (bits 6-7) + screen V/H flip (bits 0-1).
            0x211A => self.m7sel = value,
            // M7A-M7D: Mode 7 matrix parameters, write-twice via the shared M7_old latch.
            0x211B => self.m7a = self.write_m7_twice(value),
            0x211C => self.m7b = self.write_m7_twice(value),
            0x211D => self.m7c = self.write_m7_twice(value),
            0x211E => self.m7d = self.write_m7_twice(value),
            // M7X / M7Y: Mode 7 center coordinates, write-twice via the shared M7_old latch.
            0x211F => self.m7x = self.write_m7_twice(value),
            0x2120 => self.m7y = self.write_m7_twice(value),
            // TM: main-screen layer enable.
            0x212C => self.tm = value,
            // TS: sub-screen layer enable.
            0x212D => self.ts = value,
            // TMW/TSW: window area layer disables.
            0x212E => self.tmw = value,
            0x212F => self.tsw = value,
            // CGWSEL: Color Math Control A. Bit 0 = direct-color mode; bits 1 = sub-screen
            // BG/OBJ enable; bits 5-4 = color math enable region; bits 7-6 = force-main-black.
            0x2130 => self.cgwsel = value,
            // CGADSUB: color math control B.
            0x2131 => self.cgadsub = value,
            // COLDATA: sub-screen backdrop selector; each channel bit updates that channel to the
            // current 5-bit intensity, preserving the others.
            0x2132 => {
                let intensity = (value & 0x1F) as u16;
                if value & 0x20 != 0 {
                    self.coldata = (self.coldata & !0x001F) | intensity;
                }
                if value & 0x40 != 0 {
                    self.coldata = (self.coldata & !0x03E0) | (intensity << 5);
                }
                if value & 0x80 != 0 {
                    self.coldata = (self.coldata & !0x7C00) | (intensity << 10);
                }
            }
            // Window control registers.
            0x2123 => self.w12sel = value,
            0x2124 => self.w34sel = value,
            0x2125 => self.wobjsel = value,
            0x2126..=0x2129 => self.wh[(addr - 0x2126) as usize] = value,
            0x212A => self.wbglog = value,
            0x212B => self.wobjlog = value,
            // SETINI: Display Control 2. Bits used in this core: bit 6 (EXTBG), bit 3
            // (pseudo-hires), bit 1 (OBJ interlace), bit 0 (interlace).
            0x2133 => self.setini = value,
            // NMITIMEN: VBlank NMI enable (bit 7). Re-evaluate the NMI line so that enabling NMI
            // while the VBlank flag is already set raises an edge.
            0x4200 => {
                let irq_mode = (value >> 4) & 0x03;
                if irq_mode == 0 {
                    self.timeup_flag = false;
                    self.irq_line = false;
                }
                self.irq_mode = irq_mode;
                self.nmi_enable = value & 0x80 != 0;
                self.update_nmi_line();
            }
            // WRIO: bit 7 gates H/V counter latching; a 1->0 transition latches.
            0x4201 => {
                let was_set = self.wrio & 0x80 != 0;
                self.wrio = value;
                if was_set && value & 0x80 == 0 {
                    self.latch_counters();
                }
            }
            // HTIMEL/HTIMEH: H timer compare target.
            0x4207 => self.htime = (self.htime & 0x0100) | value as u16,
            0x4208 => self.htime = (self.htime & 0x00FF) | (((value as u16) & 0x01) << 8),
            // VTIMEL/VTIMEH: V timer compare target.
            0x4209 => self.vtime = (self.vtime & 0x0100) | value as u16,
            0x420A => self.vtime = (self.vtime & 0x00FF) | (((value as u16) & 0x01) << 8),
            // VMAIN: VRAM address increment mode/step and address translation.
            0x2115 => {
                self.vram_increment_after_high = value & 0x80 != 0;
                self.vram_address_translation =
                    VramAddressTranslation::from_u8((value >> 2) & 0x03);
                self.vram_increment_step = match value & 0x03 {
                    0 => 1,
                    1 => 32,
                    2 | 3 => 128,
                    _ => unreachable!(),
                };
            }
            // VMADDL / VMADDH: VRAM word address; writing high byte prefetches.
            0x2116 => self.vram_address = (self.vram_address & 0xFF00) | value as u16,
            0x2117 => {
                self.vram_address = (self.vram_address & 0x00FF) | ((value as u16) << 8);
                self.vram_prefetch = self.read_vram_word(self.translated_vram_address());
            }
            // VMDATAL / VMDATAH: VRAM data write (low/high byte of the addressed word).
            // Outside VBlank/forced blank the data is silently dropped (fullsnes: "All video
            // memory can be accessed only during V-Blank, or Forced Blank"), but the VRAM
            // address still increments (Mesen2 `SnesPpu.cpp` $2118/$2119).
            0x2118 => {
                if self.vram_cpu_access_allowed() {
                    let index = self.vram_index();
                    self.vram[index] = value;
                }
                if !self.vram_increment_after_high {
                    self.increment_vram_address();
                }
            }
            0x2119 => {
                if self.vram_cpu_access_allowed() {
                    let index = self.vram_index() | 1;
                    self.vram[index] = value;
                }
                if self.vram_increment_after_high {
                    self.increment_vram_address();
                }
            }
            // CGADD: CGRAM word address (color index * 2).
            0x2121 => self.cgram_address = (value as u16) << 1,
            // CGDATA: CGRAM data write. Even byte latches the low byte; the odd byte commits the
            // 15-bit word (high byte keeps only bits 0-6). The address increments after each write.
            // Outside its access window (VBlank, forced blank, scanline 0, or HBlank) the
            // commit is redirected to the palette entry the renderer is currently fetching
            // ([`Ppu::cgram_render_index`]), corrupting the on-screen palette instead of
            // landing at the CPU address (Mesen2 `SnesPpu.cpp` $2122, `InternalCgramAddress`).
            // The latch and address increment happen either way.
            0x2122 => {
                let index = self.cgram_index();
                if index & 1 == 0 {
                    self.cgram_latch = value;
                } else {
                    let commit = if self.cgram_cpu_access_allowed() {
                        index
                    } else {
                        (((self.cgram_render_index.get() as usize) << 1) | 1) & (CGRAM_SIZE - 1)
                    };
                    self.cgram[commit - 1] = self.cgram_latch;
                    self.cgram[commit] = value & 0x7F;
                }
                self.increment_cgram_address();
            }
            // OAMADDL / OAMADDH: OAM word address + high-table select + priority rotation. Each
            // write updates the 9-bit reload value (bits 7-1 select the first OBJ for priority
            // rotation) and copies the whole reload to the address register with bit 0 cleared.
            0x2102 => {
                self.oam_addr_reload = (self.oam_addr_reload & 0x0100) | value as u16;
                self.oam_address = (self.oam_addr_reload << 1) & 0x03FE;
                self.obj_eval_dirty = true;
            }
            0x2103 => {
                self.oam_addr_reload =
                    (self.oam_addr_reload & 0x00FF) | (((value & 0x01) as u16) << 8);
                self.oam_priority_rotation = value & 0x80 != 0;
                self.oam_address = (self.oam_addr_reload << 1) & 0x03FE;
                self.obj_eval_dirty = true;
            }
            // OAMDATA: OAM data write. In the low table ($000-$1FF) an even byte latches and the
            // odd byte commits the word; the high table ($200-$21F) writes each byte directly.
            // The address increments after each write.
            // During active rendering the sprite unit's internal read cursor overrides the
            // CPU-facing address and the write is redirected into the high table at
            // 0x200 | ((addr & 0x1F0) >> 4) (Mesen2 `SnesPpu.cpp` $2104, needed for
            // Uniracers). Mesen2 derives the cursor from its per-dot sprite evaluation; we
            // approximate with the CPU-facing address, which reproduces the essential
            // behavior: the low table is protected and the high table gets corrupted.
            0x2104 => {
                let addr = (self.oam_address as usize) & 0x03FF;
                if self.oam_rendering_active() {
                    let redirected = 0x200 | ((addr & 0x1F0) >> 4);
                    self.oam_latch = value;
                    self.oam[redirected] = value;
                } else if addr < 0x200 {
                    if addr & 1 == 0 {
                        self.oam_latch = value;
                    } else {
                        self.oam[addr - 1] = self.oam_latch;
                        self.oam[addr] = value;
                    }
                } else {
                    let index = self.oam_index();
                    self.oam[index] = value;
                }
                self.increment_oam_address();
                self.obj_eval_dirty = true;
            }
            _ => {}
        }
        if trace {
            trace_ppu!(3; "after {:04X}={:02X} y={} x={} inidisp={:02X} mode={} tm={:02X} ts={:02X} nmi={} vblank={} irq={} frame={}",
                addr,
                value,
                self.position.scanline,
                self.position.dot,
                self.inidisp,
                self.bg_mode,
                self.tm,
                self.ts,
                self.nmi_enable as u8,
                self.vblank_active as u8,
                self.irq_line as u8,
                (self.pending_completed_frames > 0) as u8,
            );
        }
    }

    /// Read a PPU register by its 16-bit address offset.
    pub fn read_register(&mut self, addr: u16) -> u8 {
        let trace = ppu_trace_level() >= 3;
        let value = match addr {
            // MPYL/MPYM/MPYH: PPU1 signed multiply result, M7A (16-bit) * M7B (8-bit, most-recent
            // byte) = 24-bit signed product. We always expose the product (drawing-period
            // conflicts during Mode 7 are not modeled).
            0x2134 => (self.mode7_multiply() & 0xFF) as u8,
            0x2135 => ((self.mode7_multiply() >> 8) & 0xFF) as u8,
            0x2136 => ((self.mode7_multiply() >> 16) & 0xFF) as u8,
            // RDVRAML: low byte of the prefetch register; reloads/increments per VMAIN mode.
            // fullsnes "Increment/Prefetch in detail": return the OLD prefetch value, reload
            // the prefetch register from the OLD (pre-increment) address, then increment --
            // so the first word after setting $2116/17 is received twice (Mesen2
            // `SnesPpu.cpp` $2139: `UpdateVramReadBuffer` before the increment).
            0x2139 => {
                let value = (self.vram_prefetch & 0x00FF) as u8;
                if !self.vram_increment_after_high {
                    self.vram_prefetch = self.read_vram_word(self.translated_vram_address());
                    self.increment_vram_address();
                }
                value
            }
            // RDVRAMH: high byte of the prefetch register; reloads/increments per VMAIN mode
            // (same reload-before-increment ordering as $2139).
            0x213A => {
                let value = ((self.vram_prefetch >> 8) & 0x00FF) as u8;
                if self.vram_increment_after_high {
                    self.vram_prefetch = self.read_vram_word(self.translated_vram_address());
                    self.increment_vram_address();
                }
                value
            }
            // RDOAM: OAM data read (auto-incrementing byte address).
            0x2138 => {
                let index = self.oam_index();
                let value = self.oam[index];
                self.increment_oam_address();
                value
            }
            // RDCGRAM: CGRAM data read (auto-incrementing byte address). Contributes to PPU2
            // open bus like the other $213B/$213C/$213D/$213F reads (see OPHCT below).
            0x213B => {
                let index = self.cgram_index();
                let value = self.cgram[index];
                self.increment_cgram_address();
                self.ppu2_open_bus = value;
                value
            }
            // RDNMI: VBlank NMI flag (bit 7) + CPU version. Read acknowledges/clears the flag,
            // EXCEPT during intra-line clocks 2-5 of the vblank scanline: the flag rises at
            // clock 2 (anomie H=0.5) but is held un-acknowledgeable until the CPU NMI line is
            // raised at clock 6 (Mesen2 `InternalRegisters::Read` $4210, hardware-verified via
            // Terranigma). A tight $4210 poll loop whose read lands in that window therefore
            // sees the same vblank twice -- observable as the PeterLemon scroll demos' +2/+1
            // frame cadence (issue #2990).
            0x4210 => {
                let value = (if self.nmi_flag { 0x80 } else { 0x00 }) | CPU_VERSION;
                // Clocks 0-1 are excluded only for symmetry with the documented window;
                // the flag cannot be set there (it rises at clock 2), so this matches
                // Mesen2's `hClock >= 6` clear guard exactly.
                let in_hold_window = self.position.scanline == self.vblank_start_line()
                    && (2..6).contains(&self.line_clock);
                if !in_hold_window {
                    self.nmi_flag = false;
                    self.update_nmi_line();
                }
                value
            }
            // TIMEUP: H/V IRQ flag. Reading acknowledges.
            0x4211 => {
                let value = if self.timeup_flag { 0x80 } else { 0x00 };
                self.timeup_flag = false;
                self.irq_line = false;
                value
            }
            // HVBJOY: VBlank flag (bit 7), HBlank flag (bit 6), auto-joypad busy (bit 0).
            0x4212 => {
                let vblank = if self.vblank_active { 0x80 } else { 0x00 };
                let hblank = if self.hblank_active() { 0x40 } else { 0x00 };
                vblank | hblank
            }
            // SLHV: software strobe to latch the H/V counters. Doesn't drive the data bus at
            // all -- real hardware returns whatever was already on the bus (the caller,
            // `SnesSystemBus::read_mmio`, substitutes its own tracked open-bus value for this
            // address and ignores this return value).
            0x2137 => {
                self.latch_strobe();
                0
            }
            // OPHCT: horizontal counter latch. Cross-checked against three independent
            // sources (bsnes `sfc/ppu/io.cpp`, Mesen2 `SnesPpu.cpp`, and the Nocash SNES
            // spec), which all agree: this flip-flop toggles unconditionally on every read
            // -- alternating low byte, high bit, low byte, ... (bsnes: `latch.hcounter++`) --
            // and is NOT reset by a fresh SLHV/WRIO re-latch (`latch_counters` only updates
            // the latched value, not this flip-flop); only a $213F read resets it (see below).
            // The high-byte read's bits 1-7 are PPU2 open bus, not zero (bsnes:
            // `ppu2.mdr &= 0xfe; ppu2.mdr |= io.hcounter >> 8 & 1;`): since every
            // $213B/$213C/$213D/$213F read leaves its own return value sitting in open bus,
            // and consecutive OPHCT/OPVCT reads without an intervening STAT78 reset just
            // re-read the same latched position, the "high" read reflects the previous "low"
            // read's byte (bits 1-7) with bit 0 replaced by the real bit 8 -- NOT a near-zero
            // value. A ROM that reads OPHCT once per H-IRQ to compute a jump-table index
            // relies on exactly this to keep getting a usable value on every firing.
            0x213C => {
                let value = if !self.ophct_read_high {
                    (self.ophct_latch & 0x00FF) as u8
                } else {
                    (((self.ophct_latch >> 8) & 0x01) as u8) | (self.ppu2_open_bus & 0xFE)
                };
                self.ophct_read_high = !self.ophct_read_high;
                self.ppu2_open_bus = value;
                value
            }
            // OPVCT: vertical counter latch. Same alternating-toggle and open-bus semantics as
            // OPHCT above.
            0x213D => {
                let value = if !self.opvct_read_high {
                    (self.opvct_latch & 0x00FF) as u8
                } else {
                    (((self.opvct_latch >> 8) & 0x01) as u8) | (self.ppu2_open_bus & 0xFE)
                };
                self.opvct_read_high = !self.opvct_read_high;
                self.ppu2_open_bus = value;
                value
            }
            // STAT77: PPU1 status + version. Bit 7 = OBJ time over-limit, bit 6 = OBJ range
            // over-limit (cleared at end of VBlank, not during forced blank).
            0x213E => {
                PPU1_VERSION
                    | ((self.stat77_time_over as u8) << 7)
                    | ((self.stat77_range_over as u8) << 6)
            }
            // STAT78: PPU2 status + version. Reports/clears the latch flag and resets the
            // OPHCT/OPVCT read flipflops.
            0x213F => {
                let value = ((self.interlace_field as u8) << 7)
                    | ((self.counter_latch_flag as u8) << 6)
                    | PPU2_VERSION;
                self.counter_latch_flag = false;
                self.ophct_read_high = false;
                self.opvct_read_high = false;
                self.ppu2_open_bus = value;
                value
            }
            _ => 0,
        };
        if trace {
            trace_ppu!(3; "read {:04X} -> {:02X} y={} x={} inidisp={:02X} mode={} tm={:02X} ts={:02X} nmi={} vblank={} irq={} frame={}",
                addr,
                value,
                self.position.scanline,
                self.position.dot,
                self.inidisp,
                self.bg_mode,
                self.tm,
                self.ts,
                self.nmi_enable as u8,
                self.vblank_active as u8,
                self.irq_line as u8,
                (self.pending_completed_frames > 0) as u8,
            );
        }
        value
    }

    /// True when INIDISP ($2100) bit 7 force-blanks the display.
    fn forced_blank_enabled(&self) -> bool {
        self.inidisp & 0x80 != 0
    }

    /// CPU writes to VRAM are honored only during VBlank or forced blank (fullsnes: "All
    /// video memory can be accessed only during V-Blank, or Forced Blank"; Mesen2
    /// `SnesPpu::CanAccessVram`).
    fn vram_cpu_access_allowed(&self) -> bool {
        self.position.scanline >= self.vblank_start_line() || self.forced_blank_enabled()
    }

    /// CGRAM is more permissive than VRAM: also writable on scanline 0 and during the
    /// current scanline's HBlank, i.e. outside the active-fetch dots 22..274 (Mesen2
    /// `SnesPpu::CanAccessCgram`, hclock < 88 || >= 1096; fullsnes notes CGRAM writes work
    /// "during certain timespots in the Hblank-period").
    fn cgram_cpu_access_allowed(&self) -> bool {
        self.vram_cpu_access_allowed()
            || self.position.scanline == 0
            || self.position.dot < VISIBLE_DOT_START
            || self.position.dot >= HBLANK_START_DOT
    }

    /// True while the sprite unit owns OAM: display enabled and before the first VBlank
    /// scanline (Mesen2 `SnesPpu.cpp` $2104 redirect condition).
    fn oam_rendering_active(&self) -> bool {
        !self.forced_blank_enabled() && self.position.scanline < self.vblank_start_line()
    }

    fn vram_index(&self) -> usize {
        ((self.translated_vram_address() as usize) << 1) & (VRAM_SIZE - 1)
    }

    /// The VRAM word address with the VMAIN ($2115) address translation
    /// applied. fullsnes: the translation "does thrice left-rotate the lower
    /// 8, 9, or 10 bits of the Word-address" (8-bit: aaaaaaaaYYYxxxxx ->
    /// aaaaaaaaxxxxxYYY) and "is applied only temporarily upon memory
    /// accesses, it doesn't affect the value in Port 2116h-17h" (Mesen2
    /// `SnesPpu::GetVramAddress`).
    fn translated_vram_address(&self) -> u16 {
        let address = self.vram_address;
        match self.vram_address_translation {
            VramAddressTranslation::None => address,
            VramAddressTranslation::EightBit => {
                (address & 0xFF00) | ((address << 3) & 0x00F8) | ((address >> 5) & 0x0007)
            }
            VramAddressTranslation::NineBit => {
                (address & 0xFE00) | ((address << 3) & 0x01F8) | ((address >> 6) & 0x0007)
            }
            VramAddressTranslation::TenBit => {
                (address & 0xFC00) | ((address << 3) & 0x03F8) | ((address >> 7) & 0x0007)
            }
        }
    }

    fn cgram_index(&self) -> usize {
        self.cgram_address as usize & (CGRAM_SIZE - 1)
    }

    fn oam_index(&self) -> usize {
        let addr = (self.oam_address as usize) & 0x03FF;
        if addr & 0x0200 != 0 {
            // High table ($200-$21F), mirrored every 32 bytes.
            0x200 + (addr & 0x001F)
        } else {
            addr & 0x01FF
        }
    }

    fn read_vram_word(&self, word_address: u16) -> u16 {
        let base = ((word_address as usize) << 1) & (VRAM_SIZE - 1);
        let low = self.vram[base] as u16;
        let high = self.vram[base | 1] as u16;
        low | (high << 8)
    }

    fn increment_vram_address(&mut self) {
        self.vram_address = self.vram_address.wrapping_add(self.vram_increment_step);
    }

    fn increment_cgram_address(&mut self) {
        self.cgram_address = (self.cgram_address + 1) & (CGRAM_SIZE as u16 - 1);
    }

    fn increment_oam_address(&mut self) {
        // The OAM address is a 10-bit counter; $220-$3FF mirror $200-$21F (handled in oam_index).
        self.oam_address = (self.oam_address + 1) & 0x03FF;
    }
}

#[cfg(test)]
mod tests {
    use super::super::Ppu;

    #[test]
    fn vram_writes_should_store_a_word_and_increment_after_high_byte() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x34);
        ppu.write_register(0x2117, 0x12);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x2468), 0xAA);
        assert_eq!(ppu.vram_byte(0x2469), 0xBB);

        ppu.write_register(0x2116, 0x34);
        ppu.write_register(0x2117, 0x12);
        assert_eq!(ppu.read_register(0x2139), 0xAA);
        assert_eq!(ppu.read_register(0x213A), 0xBB);
    }

    #[test]
    fn vram_reads_should_return_the_first_word_twice_after_setting_the_address() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2119, 0x22);
        ppu.write_register(0x2118, 0x33);
        ppu.write_register(0x2119, 0x44);
        ppu.write_register(0x2118, 0x55);
        ppu.write_register(0x2119, 0x66);

        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);

        // fullsnes "Increment/Prefetch in detail": a read returns the OLD
        // prefetch value, reloads the prefetch register from the OLD (still
        // current) address, and only then increments -- so the first word
        // arrives twice after setting the address, further words follow from
        // properly increasing addresses.
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x33);
        assert_eq!(ppu.read_register(0x213A), 0x44);
        assert_eq!(ppu.read_register(0x2139), 0x55);
        assert_eq!(ppu.read_register(0x213A), 0x66);
    }

    #[test]
    fn vram_low_byte_reads_should_reload_the_prefetch_from_the_pre_increment_address() {
        let mut ppu = Ppu::new();

        // Seed two words in increment-after-high mode, then switch to
        // increment-after-low (bit 7 = 0), step 1, for the read phase.
        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2119, 0x22);
        ppu.write_register(0x2118, 0x33);
        ppu.write_register(0x2119, 0x44);

        ppu.write_register(0x2115, 0x00);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);

        // The low-byte read reloads from the pre-increment address, so the
        // following high-byte read still sees the first word's high byte; the
        // second low-byte read then repeats the first word's low byte before
        // the reload finally fetches the second word.
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x44);
    }

    // VMAIN ($2115) bits 3-2 address translation (fullsnes: thrice left-rotate
    // of the lower 8/9/10 bits of the word address, applied temporarily upon
    // memory accesses). All tests below use word address $21AC:
    //   8-bit rotate:  %10101100 (YYY=101 xxxxx=01100) -> %01100101 = $2165
    //   9-bit rotate: %110101100 (YYY=110 xxxxxP=101100) -> %101100110 = $2166
    //  10-bit rotate: %0110101100 (YYY=011 xxxxxPP=0101100) -> %0101100011 = $2163
    // and byte offsets are word address << 1.

    #[test]
    fn vmain_8bit_translation_should_rotate_the_low_byte_of_the_write_address() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x84);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x42CA), 0xAA);
        assert_eq!(ppu.vram_byte(0x42CB), 0xBB);
        // Nothing lands at the untranslated address.
        assert_eq!(ppu.vram_byte(0x4358), 0x00);
        assert_eq!(ppu.vram_byte(0x4359), 0x00);
    }

    #[test]
    fn vmain_9bit_translation_should_rotate_the_low_nine_bits_of_the_write_address() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x88);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x42CC), 0xAA);
        assert_eq!(ppu.vram_byte(0x42CD), 0xBB);
        assert_eq!(ppu.vram_byte(0x4358), 0x00);
        assert_eq!(ppu.vram_byte(0x4359), 0x00);
    }

    #[test]
    fn vmain_10bit_translation_should_rotate_the_low_ten_bits_of_the_write_address() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x8C);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x42C6), 0xAA);
        assert_eq!(ppu.vram_byte(0x42C7), 0xBB);
        assert_eq!(ppu.vram_byte(0x4358), 0x00);
        assert_eq!(ppu.vram_byte(0x4359), 0x00);
    }

    #[test]
    fn vmain_translation_should_apply_to_the_prefetch_when_setting_the_address() {
        let mut ppu = Ppu::new();

        // Seed the word at the translated address $2165 without translation.
        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x65);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0x5A);
        ppu.write_register(0x2119, 0xC3);

        // Setting $2116/17 with 8-bit translation active must prefetch
        // through the translated address.
        ppu.write_register(0x2115, 0x84);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);

        assert_eq!(ppu.read_register(0x2139), 0x5A);
        assert_eq!(ppu.read_register(0x213A), 0xC3);
    }

    #[test]
    fn vmain_translation_should_apply_to_the_prefetch_reload_after_reads() {
        let mut ppu = Ppu::new();

        // Seed the words at the translated addresses of $21AC and $21AD
        // (8-bit rotate: $2165 and $216D) without translation.
        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x65);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2119, 0x22);
        ppu.write_register(0x2116, 0x6D);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0x33);
        ppu.write_register(0x2119, 0x44);

        ppu.write_register(0x2115, 0x84);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);

        // First word twice (reload from the pre-increment address $21AC ->
        // $2165), then the reload must fetch through the translated
        // incremented address ($21AD -> $216D), not untranslated $21AD.
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x33);
        assert_eq!(ppu.read_register(0x213A), 0x44);
    }

    #[test]
    fn vmain_translation_should_not_affect_the_stored_address_or_increment() {
        let mut ppu = Ppu::new();

        // 8-bit translation with increment-after-low: consecutive $2118
        // writes must land at translate($21AC) = $2165 and translate($21AD) =
        // $216D -- the increment applies to the untranslated address and the
        // rotation is re-applied per access (fullsnes: the translation "is
        // applied only temporarily upon memory accesses, it doesn't affect
        // the value in Port 2116h-17h").
        ppu.write_register(0x2115, 0x04);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2118, 0x22);

        assert_eq!(ppu.vram_byte(0x42CA), 0x11);
        assert_eq!(ppu.vram_byte(0x42DA), 0x22);
        // Incrementing the translated value instead would hit $2166.
        assert_eq!(ppu.vram_byte(0x42CC), 0x00);
    }

    #[test]
    fn vmain_translation_mode_zero_should_leave_the_address_unchanged() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0xAC);
        ppu.write_register(0x2117, 0x21);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x4358), 0xAA);
        assert_eq!(ppu.vram_byte(0x4359), 0xBB);
        // The 8-bit-rotated location stays untouched.
        assert_eq!(ppu.vram_byte(0x42CA), 0x00);
    }

    #[test]
    fn cgram_writes_should_store_low_and_high_bytes_in_sequence() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        assert_eq!(ppu.cgram_byte(0x20), 0x34);
        assert_eq!(ppu.cgram_byte(0x21), 0x12);
    }

    #[test]
    fn cgram_reads_should_return_stored_bytes_and_increment() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        ppu.write_register(0x2121, 0x10);
        assert_eq!(ppu.read_register(0x213B), 0x34);
        assert_eq!(ppu.read_register(0x213B), 0x12);
    }

    #[test]
    fn oam_writes_should_store_even_and_odd_bytes() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2104, 0x56);
        ppu.write_register(0x2104, 0x78);

        assert_eq!(ppu.oam_byte(0x00), 0x56);
        assert_eq!(ppu.oam_byte(0x01), 0x78);
    }

    #[test]
    fn oam_reads_should_return_stored_bytes_and_increment() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2104, 0x56);
        ppu.write_register(0x2104, 0x78);

        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x00);
        assert_eq!(ppu.read_register(0x2138), 0x56);
        assert_eq!(ppu.read_register(0x2138), 0x78);
    }

    #[test]
    fn oam_address_reload_should_target_the_high_table_when_requested() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x01);
        ppu.write_register(0x2104, 0x56);

        assert_eq!(ppu.oam_byte(0x200), 0x56);
    }

    #[test]
    fn oam_address_should_address_the_middle_of_the_main_table() {
        let mut ppu = Ppu::new();

        // A committed write-pair at byte address 0x40/0x41 (color/word in mid-table).
        ppu.write_register(0x2102, 0x20);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2104, 0x9A);
        ppu.write_register(0x2104, 0xBC);

        assert_eq!(ppu.oam_byte(0x40), 0x9A);
        assert_eq!(ppu.oam_byte(0x41), 0xBC);
    }

    #[test]
    fn cgdata_even_write_latches_without_committing() {
        let mut ppu = Ppu::new();

        // A single (even-address) CGDATA write must only latch, not commit to CGRAM.
        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);

        assert_eq!(ppu.cgram_byte(0x20), 0x00);

        // The paired (odd-address) write commits both bytes.
        ppu.write_register(0x2122, 0x12);
        assert_eq!(ppu.cgram_byte(0x20), 0x34);
        assert_eq!(ppu.cgram_byte(0x21), 0x12);
    }

    #[test]
    fn cgdata_high_byte_drops_bit15() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2121, 0x00);
        ppu.write_register(0x2122, 0xFF);
        ppu.write_register(0x2122, 0xFF);

        // CGRAM is 15-bit BGR555: the high byte keeps only bits 0-6.
        assert_eq!(ppu.cgram_byte(0x00), 0xFF);
        assert_eq!(ppu.cgram_byte(0x01), 0x7F);
    }

    #[test]
    fn oamdata_low_table_even_write_latches_without_committing() {
        let mut ppu = Ppu::new();

        // A single (even-address) OAMDATA write into the low table must only latch.
        ppu.write_register(0x2102, 0x20);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2104, 0x9A);

        assert_eq!(ppu.oam_byte(0x40), 0x00);

        // The paired (odd-address) write commits both bytes.
        ppu.write_register(0x2104, 0xBC);
        assert_eq!(ppu.oam_byte(0x40), 0x9A);
        assert_eq!(ppu.oam_byte(0x41), 0xBC);
    }

    #[test]
    fn oamdata_high_table_writes_each_byte_directly() {
        let mut ppu = Ppu::new();

        // High table ($200-$21F): every write commits immediately (no latch).
        ppu.write_register(0x2102, 0x00);
        ppu.write_register(0x2103, 0x01);
        ppu.write_register(0x2104, 0x56);

        assert_eq!(ppu.oam_byte(0x200), 0x56);
    }

    #[test]
    fn ppu_powers_on_with_forced_blank_enabled() {
        // Real hardware powers on force-blanked (both Mesen2 `SnesPpu::PowerOn` and ares
        // `PPU::power` set forced blank at init); games must clear INIDISP.7 themselves.
        let ppu = Ppu::new();
        assert_eq!(ppu.inidisp & 0x80, 0x80);
    }

    #[test]
    fn vram_writes_during_active_display_are_dropped_but_address_still_increments() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // display enabled, full brightness
        ppu.position.scanline = 40;
        ppu.position.dot = 100;

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        // The data is silently dropped...
        assert_eq!(ppu.vram_byte(0x2000), 0x00);
        assert_eq!(ppu.vram_byte(0x2001), 0x00);

        // ...but the VRAM address still increments: re-entering forced blank, the
        // next word write lands one word past the dropped one.
        ppu.write_register(0x2100, 0x80);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2119, 0x22);
        assert_eq!(ppu.vram_byte(0x2002), 0x11);
        assert_eq!(ppu.vram_byte(0x2003), 0x22);
    }

    #[test]
    fn vram_writes_during_vblank_are_stored_with_display_enabled() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 225; // first VBlank scanline (224-line mode)
        ppu.position.dot = 100;

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x2000), 0xAA);
        assert_eq!(ppu.vram_byte(0x2001), 0xBB);
    }

    #[test]
    fn vram_writes_on_scanline_zero_are_dropped_when_display_enabled() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 0;
        ppu.position.dot = 100;

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0xAA);
        ppu.write_register(0x2119, 0xBB);

        assert_eq!(ppu.vram_byte(0x2000), 0x00);
        assert_eq!(ppu.vram_byte(0x2001), 0x00);
    }

    #[test]
    fn cgram_writes_during_active_rendering_land_at_the_renderer_fetch_address() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 40;
        ppu.position.dot = 100; // inside the active-fetch window (dots 22..274)
        ppu.cgram_render_index.set(0x05); // renderer last fetched palette word 5

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        // The CPU-addressed word is untouched; the commit is redirected to the
        // palette entry the renderer is currently reading (word 5, bytes 0x0A/0x0B).
        assert_eq!(ppu.cgram_byte(0x20), 0x00);
        assert_eq!(ppu.cgram_byte(0x21), 0x00);
        assert_eq!(ppu.cgram_byte(0x0A), 0x34);
        assert_eq!(ppu.cgram_byte(0x0B), 0x12);
    }

    #[test]
    fn cgram_render_index_tracks_the_last_palette_fetch() {
        let ppu = Ppu::new();
        ppu.cgram_render_index.set(0x00);
        let _ = ppu.cgram_color(0x42);
        assert_eq!(ppu.cgram_render_index.get(), 0x42);
    }

    #[test]
    fn cgram_render_index_parks_on_the_backdrop_when_the_sub_screen_is_empty() {
        // Mesen2 ends every rendered pixel chunk with `RenderBgColor`, which fetches
        // the backdrop for sub-screen-empty pixels; with no sub-screen layers enabled
        // the render cursor therefore always parks on palette entry 0.
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F); // display on
        ppu.write_register(0x212D, 0x00); // no sub-screen layers
        ppu.cgram_render_index.set(0x42);

        // Tick through one visible dot so render_dot runs.
        ppu.position.scanline = 40;
        ppu.position.dot = 100;
        for _ in 0..4 {
            ppu.tick();
        }

        assert_eq!(ppu.cgram_render_index.get(), 0x00);
    }

    #[test]
    fn cgram_writes_during_hblank_are_stored() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 40;
        ppu.position.dot = 280; // HBlank (>= dot 274)

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        assert_eq!(ppu.cgram_byte(0x20), 0x34);
        assert_eq!(ppu.cgram_byte(0x21), 0x12);
    }

    #[test]
    fn cgram_writes_before_the_active_fetch_window_are_stored() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 40;
        ppu.position.dot = 10; // before the active-fetch window (< dot 22)

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        assert_eq!(ppu.cgram_byte(0x20), 0x34);
        assert_eq!(ppu.cgram_byte(0x21), 0x12);
    }

    #[test]
    fn cgram_writes_on_scanline_zero_are_stored() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 0;
        ppu.position.dot = 100;

        ppu.write_register(0x2121, 0x10);
        ppu.write_register(0x2122, 0x34);
        ppu.write_register(0x2122, 0x12);

        assert_eq!(ppu.cgram_byte(0x20), 0x34);
        assert_eq!(ppu.cgram_byte(0x21), 0x12);
    }

    #[test]
    fn oam_writes_during_active_rendering_are_redirected_to_the_high_table() {
        let mut ppu = Ppu::new();
        // CPU-facing OAM address: word 0x20 -> byte address 0x40.
        ppu.write_register(0x2102, 0x20);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 40;
        ppu.position.dot = 100;

        ppu.write_register(0x2104, 0x9A);

        // The low table is untouched; the write lands in the high table at
        // 0x200 | ((0x40 & 0x1F0) >> 4) = 0x204.
        assert_eq!(ppu.oam_byte(0x40), 0x00);
        assert_eq!(ppu.oam_byte(0x41), 0x00);
        assert_eq!(ppu.oam_byte(0x204), 0x9A);
    }

    #[test]
    fn oam_writes_during_vblank_commit_normally_with_display_enabled() {
        let mut ppu = Ppu::new();
        ppu.write_register(0x2102, 0x20);
        ppu.write_register(0x2103, 0x00);
        ppu.write_register(0x2100, 0x0F);
        ppu.position.scanline = 225;
        ppu.position.dot = 100;

        ppu.write_register(0x2104, 0x9A);
        ppu.write_register(0x2104, 0xBC);

        assert_eq!(ppu.oam_byte(0x40), 0x9A);
        assert_eq!(ppu.oam_byte(0x41), 0xBC);
    }

    #[test]
    fn nmitimen_should_control_nmi_enable_not_inidisp() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2100, 0x80);
        assert!(!ppu.nmi_enabled());

        ppu.write_register(0x4200, 0x80);
        assert!(ppu.nmi_enabled());
    }
}
