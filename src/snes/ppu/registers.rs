//! PPU register read/write dispatch and VRAM/CGRAM/OAM access.
//!
//! The PPU register file is addressed by its 16-bit offset (`$2100-$213F`), plus the CPU I/O
//! ports the PPU owns (`$4200` NMITIMEN, `$4201` WRIO, `$4210` RDNMI, `$4211` TIMEUP,
//! `$4212` HVBJOY). The bus passes the bare offset to [`Ppu::write_register`] /
//! [`Ppu::read_register`].

use super::{
    CGRAM_SIZE, CPU_VERSION, HBLANK_START_DOT, PPU1_VERSION, PPU2_VERSION, Ppu, VRAM_SIZE,
};

impl Ppu {
    /// Write a PPU register by its 16-bit address offset.
    pub fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            // INIDISP: forced blank (bit 7) + master brightness (bits 0-3).
            0x2100 => self.inidisp = value,
            // OBSEL: OBJ size pair (bits 7-5), name gap (bits 4-3), OBJ tile name base (bits 2-0).
            0x2101 => self.obsel = value,
            // BGMODE: BG screen mode (bits 0-2), BG3 high-priority (bit 3), per-BG tile size.
            0x2105 => {
                self.bg_mode = value & 0x07;
                self.bg3_priority = value & 0x08 != 0;
                for bg in 0..4 {
                    self.bg_tile_size_16[bg] = value & (0x10 << bg) != 0;
                }
            }
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
            // CGWSEL: Color Math Control A. Only bit 0 (direct-color enable) is used here.
            0x2130 => self.cgwsel = value,
            // SETINI: Display Control 2. Only bit 6 (EXTBG enable for Mode 7) is used here.
            0x2133 => self.setini = value,
            // NMITIMEN: VBlank NMI enable (bit 7). Re-evaluate the NMI line so that enabling NMI
            // while the VBlank flag is already set raises an edge.
            0x4200 => {
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
            // VMAIN: VRAM address increment mode/step.
            0x2115 => {
                self.vram_increment_after_high = value & 0x80 != 0;
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
                self.vram_prefetch = self.read_vram_word(self.vram_address);
            }
            // VMDATAL / VMDATAH: VRAM data write (low/high byte of the addressed word).
            0x2118 => {
                let index = self.vram_index();
                self.vram[index] = value;
                if !self.vram_increment_after_high {
                    self.increment_vram_address();
                }
            }
            0x2119 => {
                let index = self.vram_index() | 1;
                self.vram[index] = value;
                if self.vram_increment_after_high {
                    self.increment_vram_address();
                }
            }
            // CGADD: CGRAM word address (color index * 2).
            0x2121 => self.cgram_address = (value as u16) << 1,
            // CGDATA: CGRAM data write. Even byte latches the low byte; the odd byte commits the
            // 15-bit word (high byte keeps only bits 0-6). The address increments after each write.
            0x2122 => {
                let index = self.cgram_index();
                if index & 1 == 0 {
                    self.cgram_latch = value;
                } else {
                    self.cgram[index - 1] = self.cgram_latch;
                    self.cgram[index] = value & 0x7F;
                }
                self.increment_cgram_address();
            }
            // OAMADDL / OAMADDH: OAM word address + high-table select.
            0x2102 => self.oam_address = (self.oam_address & 0x0200) | ((value as u16) << 1),
            0x2103 => {
                self.oam_address = (self.oam_address & 0x01FE) | (((value & 0x01) as u16) << 9)
            }
            // OAMDATA: OAM data write. In the low table ($000-$1FF) an even byte latches and the
            // odd byte commits the word; the high table ($200-$21F) writes each byte directly.
            // The address increments after each write.
            0x2104 => {
                let addr = (self.oam_address as usize) & 0x03FF;
                if addr < 0x200 {
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
            }
            _ => {}
        }
    }

    /// Read a PPU register by its 16-bit address offset.
    pub fn read_register(&mut self, addr: u16) -> u8 {
        match addr {
            // MPYL/MPYM/MPYH: PPU1 signed multiply result, M7A (16-bit) * M7B (8-bit, most-recent
            // byte) = 24-bit signed product. We always expose the product (drawing-period
            // conflicts during Mode 7 are not modeled).
            0x2134 => (self.mode7_multiply() & 0xFF) as u8,
            0x2135 => ((self.mode7_multiply() >> 8) & 0xFF) as u8,
            0x2136 => ((self.mode7_multiply() >> 16) & 0xFF) as u8,
            // RDVRAML: low byte of the prefetch register; reloads/increments per VMAIN mode.
            0x2139 => {
                let value = (self.vram_prefetch & 0x00FF) as u8;
                if !self.vram_increment_after_high {
                    self.increment_vram_address();
                    self.vram_prefetch = self.read_vram_word(self.vram_address);
                }
                value
            }
            // RDVRAMH: high byte of the prefetch register; reloads/increments per VMAIN mode.
            0x213A => {
                let value = ((self.vram_prefetch >> 8) & 0x00FF) as u8;
                if self.vram_increment_after_high {
                    self.increment_vram_address();
                    self.vram_prefetch = self.read_vram_word(self.vram_address);
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
            // RDCGRAM: CGRAM data read (auto-incrementing byte address).
            0x213B => {
                let index = self.cgram_index();
                let value = self.cgram[index];
                self.increment_cgram_address();
                value
            }
            // RDNMI: VBlank NMI flag (bit 7) + CPU version. Read acknowledges/clears the flag.
            0x4210 => {
                let value = (if self.nmi_flag { 0x80 } else { 0x00 }) | CPU_VERSION;
                self.nmi_flag = false;
                self.update_nmi_line();
                value
            }
            // HVBJOY: VBlank flag (bit 7), HBlank flag (bit 6), auto-joypad busy (bit 0).
            0x4212 => {
                let vblank = if self.vblank_active { 0x80 } else { 0x00 };
                let hblank = if self.position.dot >= HBLANK_START_DOT {
                    0x40
                } else {
                    0x00
                };
                vblank | hblank
            }
            // SLHV: software strobe to latch the H/V counters (data value is open bus).
            0x2137 => {
                self.latch_strobe();
                0
            }
            // OPHCT: horizontal counter latch, read-twice (low byte then high bit).
            0x213C => {
                let value = if !self.ophct_read_high {
                    (self.ophct_latch & 0x00FF) as u8
                } else {
                    ((self.ophct_latch >> 8) & 0x01) as u8
                };
                self.ophct_read_high = !self.ophct_read_high;
                value
            }
            // OPVCT: vertical counter latch, read-twice (low byte then high bit).
            0x213D => {
                let value = if !self.opvct_read_high {
                    (self.opvct_latch & 0x00FF) as u8
                } else {
                    ((self.opvct_latch >> 8) & 0x01) as u8
                };
                self.opvct_read_high = !self.opvct_read_high;
                value
            }
            // STAT77: PPU1 status + version (sprite overflow flags added later).
            0x213E => PPU1_VERSION,
            // STAT78: PPU2 status + version. Reports/clears the latch flag and resets the
            // OPHCT/OPVCT read flipflops.
            0x213F => {
                let value = ((self.interlace_field as u8) << 7)
                    | ((self.counter_latch_flag as u8) << 6)
                    | PPU2_VERSION;
                self.counter_latch_flag = false;
                self.ophct_read_high = false;
                self.opvct_read_high = false;
                value
            }
            _ => 0,
        }
    }

    fn vram_index(&self) -> usize {
        ((self.vram_address as usize) << 1) & (VRAM_SIZE - 1)
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
    fn vram_reads_should_prefetch_the_next_word_after_incrementing() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2115, 0x80);
        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);
        ppu.write_register(0x2118, 0x11);
        ppu.write_register(0x2119, 0x22);
        ppu.write_register(0x2118, 0x33);
        ppu.write_register(0x2119, 0x44);

        ppu.write_register(0x2116, 0x00);
        ppu.write_register(0x2117, 0x10);

        assert_eq!(ppu.read_register(0x2139), 0x11);
        assert_eq!(ppu.read_register(0x213A), 0x22);
        assert_eq!(ppu.read_register(0x2139), 0x33);
        assert_eq!(ppu.read_register(0x213A), 0x44);
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
    fn nmitimen_should_control_nmi_enable_not_inidisp() {
        let mut ppu = Ppu::new();

        ppu.write_register(0x2100, 0x80);
        assert!(!ppu.nmi_enabled());

        ppu.write_register(0x4200, 0x80);
        assert!(ppu.nmi_enabled());
    }
}
