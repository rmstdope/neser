use crate::cartridge::cartridge::MirroringMode;
use crate::cartridge::mapper::Mapper;
use std::cell::Cell;

pub struct MMC5Mapper {
    prg_rom: Vec<u8>,
    chr: Chr,
    prg_ram: Vec<u8>,
    mirroring: MirroringMode,

    // PRG banking
    prg_mode: u8,
    prg_bank_5113: u8,
    prg_bank_5114: u8,
    prg_bank_5115: u8,
    prg_bank_5116: u8,
    prg_bank_5117: u8,

    // PRG-RAM write protection
    prg_ram_protect_1: u8,
    prg_ram_protect_2: u8,

    // CHR banking
    chr_mode: u8,
    chr_bank_a: [u8; 8], // $5120-$5127 for BG
    chr_bank_b: [u8; 4], // $5128-$512B for sprites
    chr_fetch_is_sprite: bool,

    // Nametable control
    nametable_mapping: u8, // $5105
    fill_tile: u8,         // $5106
    fill_attr: u8,         // $5107

    // ExRAM
    ex_ram: Vec<u8>, // 1KB ExRAM at $5C00-$5FFF
    ex_ram_mode: u8, // $5104

    // Split screen (not fully implemented yet)
    split_mode: u8,   // $5200
    split_scroll: u8, // $5201
    split_bank: u8,   // $5202

    // Scanline IRQ
    irq_scanline_compare: u8, // $5203
    irq_enabled: bool,        // $5204 bit 7
    irq_pending: Cell<bool>,  // IRQ pending flag (cleared on read of $5204)
    in_frame: bool,           // Track if PPU is in frame
    scanline_counter: u16,    // Current scanline counter

    // Hardware multiplier
    multiplicand: u8, // $5205
    multiplier: u8,   // $5206
}

enum Chr {
    Rom(Vec<u8>),
    Ram(Vec<u8>),
}

impl MMC5Mapper {
    const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
    const PRG_RAM_BANK_COUNT: usize = 8;
    const PRG_ROM_BANK_SIZE: usize = 8 * 1024;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        let prg_rom_bank_count_8k = prg_rom.len() / Self::PRG_ROM_BANK_SIZE;

        let chr = if chr_rom.is_empty() {
            Chr::Ram(vec![0u8; 8 * 1024])
        } else {
            Chr::Rom(chr_rom)
        };

        // A compatible superset (see nesdev): emulate 64KB PRG-RAM as 8 x 8KB banks.
        // Games that have less won't generally notice.
        let prg_ram = vec![0u8; Self::PRG_RAM_BANK_COUNT * Self::PRG_RAM_BANK_SIZE];

        // MMC5 PRG mode defaults to 3 at power-on.
        // $5117 defaults to $FF on real hardware; for our bank-indexed model, we map it to the
        // last available 8KB PRG ROM bank when present.
        Self {
            prg_rom,
            chr,
            prg_ram,
            mirroring,

            // PRG banking
            prg_mode: 3,
            prg_bank_5113: 0,
            prg_bank_5114: 0x80,
            prg_bank_5115: 0x80,
            prg_bank_5116: 0x80,
            prg_bank_5117: prg_rom_bank_count_8k.saturating_sub(1) as u8,

            // PRG-RAM write protection (default: writable - $02 and $01)
            prg_ram_protect_1: 0x02,
            prg_ram_protect_2: 0x01,

            // CHR banking
            chr_mode: 0,
            chr_bank_a: [0; 8],
            chr_bank_b: [0; 4],
            chr_fetch_is_sprite: false,

            // Nametable control
            nametable_mapping: 0,
            fill_tile: 0,
            fill_attr: 0,

            // ExRAM
            ex_ram: vec![0u8; 1024],
            ex_ram_mode: 0,

            // Split screen
            split_mode: 0,
            split_scroll: 0,
            split_bank: 0,

            // Scanline IRQ
            irq_scanline_compare: 0,
            irq_enabled: false,
            irq_pending: Cell::new(false),
            in_frame: false,
            scanline_counter: 0,

            // Hardware multiplier
            multiplicand: 0,
            multiplier: 0,
        }
    }

    fn prg_rom_bank_count_8k(&self) -> usize {
        self.prg_rom.len() / Self::PRG_ROM_BANK_SIZE
    }

    fn read_prg_rom_8k(&self, bank: u8, addr: u16, base: u16) -> u8 {
        let num_banks = self.prg_rom_bank_count_8k();
        if num_banks == 0 {
            return 0;
        }

        let bank_index = (bank as usize) % num_banks;
        let offset = (addr - base) as usize;
        self.prg_rom[bank_index * Self::PRG_ROM_BANK_SIZE + offset]
    }

    fn prg_ram_bank_index_8k(bank: u8) -> usize {
        // $5113 ignores bits 7..4; for $5114-$5116, bit 7 selects ROM/RAM.
        ((bank & 0x07) as usize) % Self::PRG_RAM_BANK_COUNT
    }

    fn read_prg_ram_8k(&self, bank: u8, addr: u16, base: u16) -> u8 {
        let bank_index = Self::prg_ram_bank_index_8k(bank);
        let offset = (addr - base) as usize;
        let index = bank_index * Self::PRG_RAM_BANK_SIZE + offset;
        self.prg_ram.get(index).copied().unwrap_or(0)
    }

    fn write_prg_ram_8k(&mut self, bank: u8, addr: u16, base: u16, value: u8) {
        // Check if PRG-RAM writes are protected
        if !self.is_prg_ram_writable() {
            return;
        }
        let bank_index = Self::prg_ram_bank_index_8k(bank);
        let offset = (addr - base) as usize;
        let index = bank_index * Self::PRG_RAM_BANK_SIZE + offset;
        if let Some(slot) = self.prg_ram.get_mut(index) {
            *slot = value;
        }
    }

    fn is_prg_ram_writable(&self) -> bool {
        // PRG-RAM is writable when both protect registers are set to the magic values
        // $5102 = %xxxx_xx10 and $5103 = %xxxx_xx01
        (self.prg_ram_protect_1 & 0x03) == 0x02 && (self.prg_ram_protect_2 & 0x03) == 0x01
    }

    fn read_window_8k(&self, reg: u8, addr: u16, base: u16) -> u8 {
        if (reg & 0x80) != 0 {
            self.read_prg_rom_8k(reg & 0x7F, addr, base)
        } else {
            self.read_prg_ram_8k(reg, addr, base)
        }
    }

    fn write_window_8k(&mut self, reg: u8, addr: u16, base: u16, value: u8) {
        if (reg & 0x80) == 0 {
            self.write_prg_ram_8k(reg, addr, base, value);
        }
    }

    fn read_window_16k_mode2(&self, reg: u8, addr: u16) -> u8 {
        let second_8k = if addr >= 0xA000 { 1u8 } else { 0u8 };
        if (reg & 0x80) != 0 {
            // ROM bank index in 8KB units; even-aligned for 16KB.
            let bank_base = (reg & 0x7F) & !1;
            if addr >= 0xA000 {
                self.read_prg_rom_8k(bank_base.wrapping_add(second_8k), addr, 0xA000)
            } else {
                self.read_prg_rom_8k(bank_base, addr, 0x8000)
            }
        } else if addr >= 0xA000 {
            self.read_prg_ram_8k(reg.wrapping_add(second_8k), addr, 0xA000)
        } else {
            self.read_prg_ram_8k(reg, addr, 0x8000)
        }
    }

    fn write_window_16k_mode2(&mut self, reg: u8, addr: u16, value: u8) {
        if (reg & 0x80) != 0 {
            return;
        }

        let second_8k = if addr >= 0xA000 { 1u8 } else { 0u8 };
        if addr >= 0xA000 {
            self.write_prg_ram_8k(reg.wrapping_add(second_8k), addr, 0xA000, value);
        } else {
            self.write_prg_ram_8k(reg, addr, 0x8000, value);
        }
    }

    fn read_window_32k_mode0(&self, reg: u8, addr: u16) -> u8 {
        // Mode 0: Single 32KB bank at $8000-$FFFF
        // Use $5117 with bits 1-0 ignored (align to 32KB)
        let bank_base = (reg & 0x7F) & !3; // Align to 32KB boundary (4 x 8KB banks)
        let offset_8k = ((addr >> 13) & 0x03) as u8; // Which 8KB within the 32KB
        let base_addr = addr & 0xE000; // Start of the current 8KB segment
        self.read_prg_rom_8k(bank_base.wrapping_add(offset_8k), addr, base_addr)
    }

    fn read_window_16k_mode1(&self, reg: u8, addr: u16, is_high: bool) -> u8 {
        // Mode 1: Two 16KB banks
        // $8000-$BFFF uses $5115 (bit 0 ignored)
        // $C000-$FFFF uses $5117 (bit 0 ignored)
        let bank_base = (reg & 0x7F) & !1; // Align to 16KB boundary (2 x 8KB banks)
        let offset_8k = if is_high {
            if addr >= 0xE000 { 1u8 } else { 0u8 }
        } else {
            if addr >= 0xA000 { 1u8 } else { 0u8 }
        };
        let base_addr = if is_high {
            if addr >= 0xE000 { 0xE000 } else { 0xC000 }
        } else {
            if addr >= 0xA000 { 0xA000 } else { 0x8000 }
        };
        self.read_prg_rom_8k(bank_base.wrapping_add(offset_8k), addr, base_addr)
    }

    fn get_chr_bank(&self, addr: u16) -> u8 {
        fn bank_idx_1k(addr: u16) -> u8 {
            ((addr >> 10) & 0x07) as u8
        }

        // MMC5 CHR banking supports 4 modes:
        // Mode 0: 8KB (single bank)
        // Mode 1: 4KB (two banks)
        // Mode 2: 2KB (four banks)
        // Mode 3: 1KB (eight banks, with separate BG/sprite banks)

        let chr_mode = self.chr_mode & 0x03;
        match chr_mode {
            0 => {
                // 8KB mode: use $5127
                self.chr_bank_a[7]
            }
            1 => {
                // 4KB mode: use $5123 (low) or $5127 (high)
                let high = addr >= 0x1000;
                self.chr_bank_a[if high { 7 } else { 3 }]
            }
            2 => {
                // 2KB mode: use $5121, $5123, $5125, $5127
                let bank_idx = (addr >> 11) & 0x03;
                self.chr_bank_a[(bank_idx * 2 + 1) as usize]
            }
            3 => {
                // 1KB mode:
                // - BG fetches use $5120-$5127 (A, 8 x 1KB)
                // - Sprite fetches use $5128-$512B (B, 4 x 1KB)
                let bank_idx = bank_idx_1k(addr);
                if self.chr_fetch_is_sprite {
                    // Sprite pattern table is 4KB wide, so select within 4 banks.
                    self.chr_bank_b[(bank_idx & 0x03) as usize]
                } else {
                    self.chr_bank_a[bank_idx as usize]
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_chr_banked(&self, bank: u8, addr: u16) -> u8 {
        // Calculate the actual address in CHR ROM/RAM
        let bank_size = match self.chr_mode {
            0 => 8 * 1024, // 8KB
            1 => 4 * 1024, // 4KB
            2 => 2 * 1024, // 2KB
            3 => 1 * 1024, // 1KB
            _ => 1 * 1024,
        };

        let offset = (addr as usize) % bank_size;
        let chr_addr = (bank as usize) * bank_size + offset;

        match &self.chr {
            Chr::Rom(data) => {
                if data.is_empty() {
                    0
                } else {
                    data.get(chr_addr % data.len()).copied().unwrap_or(0)
                }
            }
            Chr::Ram(data) => data.get(chr_addr % data.len()).copied().unwrap_or(0),
        }
    }

    fn write_chr_banked(&mut self, bank: u8, addr: u16, value: u8) {
        // Calculate the actual address in CHR RAM
        let bank_size = match self.chr_mode {
            0 => 8 * 1024, // 8KB
            1 => 4 * 1024, // 4KB
            2 => 2 * 1024, // 2KB
            3 => 1 * 1024, // 1KB
            _ => 1 * 1024,
        };

        let offset = (addr as usize) % bank_size;
        let chr_addr = (bank as usize) * bank_size + offset;

        if let Chr::Ram(data) = &mut self.chr {
            let data_len = data.len();
            if let Some(slot) = data.get_mut(chr_addr % data_len) {
                *slot = value;
            }
        }
    }

    fn nametable_mapping_for_addr(&self, addr: u16) -> u8 {
        const NAMETABLE_MASK: u16 = 0x2FFF;
        const NAMETABLE_BASE: u16 = 0x2000;
        // $5105: 2 bits per nametable quadrant:
        // bits 1-0: $2000, 3-2: $2400, 5-4: $2800, 7-6: $2C00
        // values: 0 = VRAM A, 1 = VRAM B, 2 = ExRAM, 3 = fill mode
        let addr = addr & NAMETABLE_MASK;
        debug_assert!(addr >= NAMETABLE_BASE && addr <= NAMETABLE_MASK);

        let quadrant = ((addr - NAMETABLE_BASE) >> 10) & 0x03;
        (self.nametable_mapping >> (quadrant * 2)) & 0x03
    }

    fn fill_attribute_byte(&self) -> u8 {
        // $5107 stores a 2-bit attribute value that is replicated across an attribute byte.
        let a = self.fill_attr & 0x03;
        a | (a << 2) | (a << 4) | (a << 6)
    }
}

impl Mapper for MMC5Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            // Hardware multiplier output (read-only)
            0x5205 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                (result & 0xFF) as u8
            }
            0x5206 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                ((result >> 8) & 0xFF) as u8
            }

            // IRQ status (read clears pending flag)
            0x5204 => {
                const IRQ_PENDING_BIT: u8 = 0x80;
                const IN_FRAME_BIT: u8 = 0x40;

                let result = (if self.irq_pending.get() {
                    IRQ_PENDING_BIT
                } else {
                    0x00
                }) | (if self.in_frame { IN_FRAME_BIT } else { 0x00 });
                self.irq_pending.set(false);
                result
            }

            // ExRAM
            0x5C00..=0x5FFF => {
                let index = (addr - 0x5C00) as usize;
                self.ex_ram.get(index).copied().unwrap_or(0)
            }

            0x6000..=0x7FFF => self.read_prg_ram_8k(self.prg_bank_5113, addr, 0x6000),

            0x8000..=0xFFFF => {
                let prg_mode = self.prg_mode & 0x03;
                match prg_mode {
                    0 => {
                        // Mode 0: 32KB bank at $8000-$FFFF via $5117
                        self.read_window_32k_mode0(self.prg_bank_5117, addr)
                    }

                    1 => match addr {
                        // Mode 1: Two 16KB banks
                        // $8000-$BFFF: 16KB bank via $5115 (bit 0 ignored)
                        0x8000..=0xBFFF => {
                            self.read_window_16k_mode1(self.prg_bank_5115, addr, false)
                        }
                        // $C000-$FFFF: 16KB bank via $5117 (bit 0 ignored)
                        0xC000..=0xFFFF => {
                            self.read_window_16k_mode1(self.prg_bank_5117, addr, true)
                        }
                        _ => 0,
                    },

                    2 => match addr {
                        // $8000-$BFFF: 16KB bank via $5115 (bit 0 ignored)
                        0x8000..=0xBFFF => self.read_window_16k_mode2(self.prg_bank_5115, addr),

                        // $C000-$DFFF: 8KB bank via $5116
                        0xC000..=0xDFFF => self.read_window_8k(self.prg_bank_5116, addr, 0xC000),

                        // $E000-$FFFF: 8KB fixed ROM bank via $5117
                        0xE000..=0xFFFF => {
                            self.read_prg_rom_8k(self.prg_bank_5117 & 0x7F, addr, 0xE000)
                        }

                        _ => 0,
                    },

                    3 => match addr {
                        // Four 8KB banks.
                        0x8000..=0x9FFF => self.read_window_8k(self.prg_bank_5114, addr, 0x8000),
                        0xA000..=0xBFFF => self.read_window_8k(self.prg_bank_5115, addr, 0xA000),
                        0xC000..=0xDFFF => self.read_window_8k(self.prg_bank_5116, addr, 0xC000),
                        0xE000..=0xFFFF => {
                            // $5117 always maps ROM.
                            self.read_prg_rom_8k(self.prg_bank_5117 & 0x7F, addr, 0xE000)
                        }
                        _ => 0,
                    },

                    _ => unreachable!(),
                }
            }

            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5100 => {
                self.prg_mode = value & 0x03;
            }

            0x5101 => {
                self.chr_mode = value & 0x03;
            }

            // PRG-RAM write protection
            0x5102 => {
                self.prg_ram_protect_1 = value;
            }
            0x5103 => {
                self.prg_ram_protect_2 = value;
            }

            // ExRAM mode
            0x5104 => {
                self.ex_ram_mode = value & 0x03;
            }

            // Nametable mapping
            0x5105 => {
                self.nametable_mapping = value;
            }

            // Fill mode tile
            0x5106 => {
                self.fill_tile = value;
            }

            // Fill mode attribute
            0x5107 => {
                self.fill_attr = value & 0x03;
            }

            // PRG bankswitch registers
            0x5113 => self.prg_bank_5113 = value,
            0x5114 => self.prg_bank_5114 = value,
            0x5115 => self.prg_bank_5115 = value,
            0x5116 => self.prg_bank_5116 = value,
            0x5117 => self.prg_bank_5117 = value,

            // CHR bank registers
            0x5120..=0x5127 => {
                let index = (addr - 0x5120) as usize;
                self.chr_bank_a[index] = value;
            }
            0x5128..=0x512B => {
                let index = (addr - 0x5128) as usize;
                self.chr_bank_b[index] = value;
            }

            // Split screen
            0x5200 => {
                self.split_mode = value;
            }
            0x5201 => {
                self.split_scroll = value;
            }
            0x5202 => {
                self.split_bank = value;
            }

            // IRQ
            0x5203 => {
                self.irq_scanline_compare = value;
            }
            0x5204 => {
                self.irq_enabled = (value & 0x80) != 0;
                if !self.irq_enabled {
                    self.irq_pending.set(false);
                }
            }

            // Hardware multiplier
            0x5205 => {
                self.multiplicand = value;
            }
            0x5206 => {
                self.multiplier = value;
            }

            // ExRAM
            0x5C00..=0x5FFF => {
                let index = (addr - 0x5C00) as usize;
                if let Some(slot) = self.ex_ram.get_mut(index) {
                    *slot = value;
                }
            }

            0x6000..=0x7FFF => {
                self.write_prg_ram_8k(self.prg_bank_5113, addr, 0x6000, value);
            }

            // Support basic PRG-RAM writes when a window is mapped to RAM.
            0x8000..=0xDFFF => {
                let prg_mode = self.prg_mode & 0x03;
                match prg_mode {
                    2 => match addr {
                        0x8000..=0xBFFF => {
                            self.write_window_16k_mode2(self.prg_bank_5115, addr, value);
                        }
                        0xC000..=0xDFFF => {
                            self.write_window_8k(self.prg_bank_5116, addr, 0xC000, value);
                        }
                        _ => {}
                    },

                    3 => match addr {
                        0x8000..=0x9FFF => {
                            self.write_window_8k(self.prg_bank_5114, addr, 0x8000, value);
                        }
                        0xA000..=0xBFFF => {
                            self.write_window_8k(self.prg_bank_5115, addr, 0xA000, value);
                        }
                        0xC000..=0xDFFF => {
                            self.write_window_8k(self.prg_bank_5116, addr, 0xC000, value);
                        }
                        _ => {}
                    },

                    _ => {}
                }
            }

            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank = self.get_chr_bank(addr);
        self.read_chr_banked(bank, addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.get_chr_bank(addr);
        self.write_chr_banked(bank, addr, value);
    }

    fn ppu_set_chr_fetch_is_sprite(&mut self, is_sprite: bool) {
        self.chr_fetch_is_sprite = is_sprite;
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return None;
        }

        match self.nametable_mapping_for_addr(addr) {
            2 => {
                // ExRAM (1KB). Multiple quadrants mapped to ExRAM will alias.
                let index = (addr & 0x03FF) as usize;
                Some(self.ex_ram.get(index).copied().unwrap_or(0))
            }
            3 => {
                // Fill mode: tile area returns $5106, attribute area returns replicated $5107.
                if (addr & 0x03FF) < 0x03C0 {
                    Some(self.fill_tile)
                } else {
                    Some(self.fill_attribute_byte())
                }
            }
            _ => None,
        }
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        let addr = addr & 0x2FFF;
        if !(0x2000..=0x2FFF).contains(&addr) {
            return false;
        }

        match self.nametable_mapping_for_addr(addr) {
            2 => {
                let index = (addr & 0x03FF) as usize;
                if let Some(slot) = self.ex_ram.get_mut(index) {
                    *slot = value;
                }
                true
            }
            3 => {
                // Fill mode is not backed by RAM.
                let _ = value;
                true
            }
            _ => false,
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        // MMC5 scanline IRQ: increment counter on A12 rising edge during rendering
        // Simplified: we'll rely on in_frame tracking instead of full A12 detection
        // The IRQ system will be updated in cpu_cycle and via external scanline notification
        let _ = addr; // Suppress unused warning for now
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending.get()
    }

    fn ppu_scanline(&mut self, scanline: u16, rendering_enabled: bool) {
        // MMC5 scanline IRQ behavior (current scope: scanline-notify based):
        // - Only active during rendering.
        // - Track in-frame while rendering is enabled.
        // - Assert IRQ when scanline matches compare and IRQ is enabled.
        if !rendering_enabled {
            self.in_frame = false;
            return;
        }

        self.in_frame = true;
        self.scanline_counter = scanline;

        if rendering_enabled && self.irq_enabled && (scanline as u8) == self.irq_scanline_compare {
            self.irq_pending.set(true);
        }
    }

    fn ppu_end_frame(&mut self) {
        // End-of-frame bookkeeping; does not clear irq_pending (that is read-to-clear via $5204).
        self.in_frame = false;
    }

    fn get_mirroring(&self) -> MirroringMode {
        // MMC5's $5105 register controls nametable mapping
        // Each 2 bits control one quadrant (bits 1-0: $2000, 3-2: $2400, 5-4: $2800, 7-6: $2C00)
        // Values: 0 = $2000 (A), 1 = $2400 (B), 2 = ExRAM, 3 = fill mode

        // For basic compatibility, map common patterns to standard mirroring modes
        let mapping = self.nametable_mapping;

        // Check for standard patterns
        if mapping == 0b00_00_00_00 {
            // All to A -> Single screen
            return MirroringMode::SingleScreen;
        } else if mapping == 0b01_01_01_01 {
            // All to B -> Single screen
            return MirroringMode::SingleScreen;
        } else if mapping == 0b00_00_01_01 {
            // Vertical mirroring (A|A, B|B)
            return MirroringMode::Vertical;
        } else if mapping == 0b01_00_01_00 {
            // Horizontal mirroring (A|B, A|B)
            return MirroringMode::Horizontal;
        }

        // Default to the original iNES mirroring for other cases
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::cartridge::MirroringMode;
    use crate::cartridge::mapper::Mapper;
    use crate::cartridge::mapper::create_mapper;

    use super::MMC5Mapper;

    fn banked_data(bank_size: usize, num_banks: usize) -> Vec<u8> {
        let mut data = vec![0u8; bank_size * num_banks];
        for bank in 0..num_banks {
            let start = bank * bank_size;
            let end = start + bank_size;
            data[start..end].fill(bank as u8);
        }
        data
    }

    fn new_mmc5_for_irq_test() -> MMC5Mapper {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1 * 1024, 8);
        MMC5Mapper::new(prg_rom, chr_rom, MirroringMode::Horizontal)
    }

    #[test]
    fn test_mmc5_irq_triggers_when_scanline_matches_compare_and_enabled() {
        let mut mmc5 = new_mmc5_for_irq_test();

        // $5203: scanline compare
        mmc5.write_prg(0x5203, 5);
        // $5204: enable IRQ (bit 7)
        mmc5.write_prg(0x5204, 0x80);

        // No rendering => should not trigger.
        mmc5.ppu_scanline(5, false);
        assert!(!mmc5.irq_pending());

        // Rendering enabled: should trigger when scanline == compare.
        mmc5.ppu_scanline(4, true);
        assert!(!mmc5.irq_pending());
        mmc5.ppu_scanline(5, true);
        assert!(mmc5.irq_pending());
    }

    #[test]
    fn test_mmc5_irq_status_register_reports_pending_and_in_frame() {
        let mut mmc5 = new_mmc5_for_irq_test();

        // Start of visible frame (rendering enabled) should set the in-frame flag (bit 6).
        mmc5.ppu_scanline(0, true);
        let status = mmc5.read_prg(0x5204);
        assert_eq!(status & 0x40, 0x40);

        // Pending flag (bit 7) becomes set when the IRQ condition triggers.
        mmc5.write_prg(0x5203, 2);
        mmc5.write_prg(0x5204, 0x80);
        mmc5.ppu_scanline(2, true);
        let status = mmc5.read_prg(0x5204);
        assert_eq!(status & 0x80, 0x80);
    }

    #[test]
    fn test_mmc5_irq_pending_clears_on_read_of_5204() {
        let mut mmc5 = new_mmc5_for_irq_test();

        mmc5.write_prg(0x5203, 1);
        mmc5.write_prg(0x5204, 0x80);
        mmc5.ppu_scanline(1, true);
        assert!(mmc5.irq_pending());

        // Reading $5204 should clear the pending flag.
        let _ = mmc5.read_prg(0x5204);
        assert!(!mmc5.irq_pending());
    }

    #[test]
    fn test_mmc5_mapper_5_is_wired_in_factory() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");
    }

    #[test]
    fn test_mmc5_prg_mode_3_8kb_bank_mapping() {
        // MMC5 PRG mode 3: four 8KB banks at $8000-$FFFF.
        // - $8000-$9FFF uses $5114
        // - $A000-$BFFF uses $5115
        // - $C000-$DFFF uses $5116
        // - $E000-$FFFF uses $5117 (ROM only)
        //
        // For $5114-$5116 bit7 selects ROM (1) vs RAM (0). This test uses ROM.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Explicitly select PRG mode 3.
        mapper.write_prg(0x5100, 0x03);

        // Map banks 2/3/4/7 into the 4x 8KB slots.
        mapper.write_prg(0x5114, 0b1000_0000 | 2);
        mapper.write_prg(0x5115, 0b1000_0000 | 3);
        mapper.write_prg(0x5116, 0b1000_0000 | 4);
        mapper.write_prg(0x5117, 7);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc5_chr_mode3_uses_bank_a_for_bg_and_bank_b_for_sprites() {
        // In MMC5 CHR mode 3 (1KB), background uses bank A regs ($5120-$5127)
        // while sprite fetches use bank B regs ($5128-$512B).

        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1 * 1024, 16);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // CHR mode 3 (1KB).
        mapper.write_prg(0x5101, 0x03);

        // Program A banks to 0..7.
        for i in 0..8u8 {
            mapper.write_prg(0x5120 + (i as u16), i);
        }
        // Program B banks to 8..11.
        for i in 0..4u8 {
            mapper.write_prg(0x5128 + (i as u16), 8 + i);
        }

        // Background fetches should use A banks.
        mapper.ppu_set_chr_fetch_is_sprite(false);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0400), 1);
        assert_eq!(mapper.read_chr(0x1000), 4);

        // Sprite fetches should use B banks (indexed within the 4KB region).
        mapper.ppu_set_chr_fetch_is_sprite(true);
        assert_eq!(mapper.read_chr(0x0000), 8);
        assert_eq!(mapper.read_chr(0x0400), 9);
        assert_eq!(mapper.read_chr(0x1000), 8);
    }

    #[test]
    fn test_mmc5_nametable_mapping_exram_routes_reads_and_writes() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Map $2000 quadrant to ExRAM (value 2 in bits 1-0).
        mapper.write_prg(0x5105, 0b00_00_00_10);

        // Mapper should own nametable access when mapped to ExRAM.
        assert!(mapper.write_nametable(0x2000, 0xAB));
        assert_eq!(mapper.read_nametable(0x2000), Some(0xAB));
    }

    #[test]
    fn test_mmc5_nametable_mapping_fill_mode_returns_fill_tile_and_attr() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Map $2000 quadrant to fill mode (value 3 in bits 1-0).
        mapper.write_prg(0x5105, 0b00_00_00_11);
        mapper.write_prg(0x5106, 0x55);
        mapper.write_prg(0x5107, 0x02);

        // Tile fetches return fill tile.
        assert_eq!(mapper.read_nametable(0x2000), Some(0x55));

        // Attribute fetches return a byte derived from $5107 (2-bit value).
        // For now, require that at least the low 2 bits match.
        let attr = mapper
            .read_nametable(0x23C0)
            .expect("fill-mode attribute read should be overridden");
        assert_eq!(attr & 0x03, 0x02);

        // Writes to fill-mode nametable should not fall through to internal VRAM.
        assert!(mapper.write_nametable(0x2000, 0x99));
    }

    #[test]
    fn test_mmc5_nametable_mapping_internal_vram_passthrough() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Map $2000 quadrant to internal VRAM (value 0 in bits 1-0).
        mapper.write_prg(0x5105, 0b00_00_00_00);

        assert_eq!(mapper.read_nametable(0x2000), None);
        assert!(!mapper.write_nametable(0x2000, 0xAB));
    }

    #[test]
    fn test_mmc5_prg_ram_bank_switching_via_5113() {
        // MMC5 has switchable PRG-RAM; $5113 selects the PRG-RAM bank.
        // This test checks that selecting different banks changes what data is visible at $6000.

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG-RAM bank 0 and write a value.
        mapper.write_prg(0x5113, 0);
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Select PRG-RAM bank 1; the value should not be present.
        mapper.write_prg(0x5113, 1);
        assert_eq!(mapper.read_prg(0x6000), 0x00);

        // Write a different value in bank 1.
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);

        // Switch back to bank 0; original value should be visible again.
        mapper.write_prg(0x5113, 0);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn test_mmc5_prg_mode_2_16kb_plus_8kb_plus_fixed_8kb_mapping() {
        // MMC5 PRG mode 2:
        // - $8000-$BFFF: 16KB bank selected via $5115 (bit 0 ignored)
        // - $C000-$DFFF: 8KB bank selected via $5116
        // - $E000-$FFFF: 8KB fixed bank selected via $5117 (ROM only)

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG mode 2.
        mapper.write_prg(0x5100, 0x02);

        // Select a 16KB bank for $8000-$BFFF using an odd value; bit 0 must be ignored,
        // so $8000 should still map to the even bank, and $A000 to the following bank.
        mapper.write_prg(0x5115, 0b1000_0011); // ROM, bank index 3 -> treated as 2 for 16KB

        // Select an 8KB bank at $C000.
        mapper.write_prg(0x5116, 0b1000_0101); // ROM, bank 5

        // Fixed last bank window uses ROM only.
        mapper.write_prg(0x5117, 7);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc5_prg_mode_0_32kb_bank() {
        // MMC5 PRG mode 0: Single 32KB bank at $8000-$FFFF via $5117

        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG mode 0
        mapper.write_prg(0x5100, 0x00);

        // Select 32KB bank (bits 1-0 ignored, so bank 7 becomes 4)
        mapper.write_prg(0x5117, 0x87); // ROM bit set, bank 7 -> aligned to 4

        // All 4 x 8KB segments should come from banks 4, 5, 6, 7
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc5_prg_mode_1_two_16kb_banks() {
        // MMC5 PRG mode 1: Two 16KB banks

        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Select PRG mode 1
        mapper.write_prg(0x5100, 0x01);

        // Low 16KB bank via $5115 (bit 0 ignored, so 3 -> 2)
        mapper.write_prg(0x5115, 0x83); // ROM, bank 3 -> aligned to 2

        // High 16KB bank via $5117 (bit 0 ignored, so 7 -> 6)
        mapper.write_prg(0x5117, 0x87); // ROM, bank 7 -> aligned to 6

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn test_mmc5_hardware_multiplier() {
        // MMC5 has a hardware multiplier at $5205/$5206

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Write multiplicand and multiplier
        mapper.write_prg(0x5205, 123);
        mapper.write_prg(0x5206, 45);

        // Result should be 123 * 45 = 5535 = 0x159F
        assert_eq!(mapper.read_prg(0x5205), 0x9F); // Low byte
        assert_eq!(mapper.read_prg(0x5206), 0x15); // High byte

        // Test another multiplication
        mapper.write_prg(0x5205, 255);
        mapper.write_prg(0x5206, 255);

        // Result should be 255 * 255 = 65025 = 0xFE01
        assert_eq!(mapper.read_prg(0x5205), 0x01); // Low byte
        assert_eq!(mapper.read_prg(0x5206), 0xFE); // High byte
    }

    #[test]
    fn test_mmc5_exram_access() {
        // MMC5 has 1KB ExRAM at $5C00-$5FFF

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // Write to ExRAM
        mapper.write_prg(0x5C00, 0xAA);
        mapper.write_prg(0x5C01, 0xBB);
        mapper.write_prg(0x5FFF, 0xCC);

        // Read back
        assert_eq!(mapper.read_prg(0x5C00), 0xAA);
        assert_eq!(mapper.read_prg(0x5C01), 0xBB);
        assert_eq!(mapper.read_prg(0x5FFF), 0xCC);
    }

    #[test]
    fn test_mmc5_prg_ram_write_protection() {
        // MMC5 PRG-RAM write protection via $5102/$5103

        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1 * 1024, 8);

        let mut mapper = create_mapper(5, prg_rom, chr_rom, MirroringMode::Horizontal)
            .expect("MMC5 (mapper 5) should be implemented");

        // By default, PRG-RAM should be writable (protect registers initialized to 0x02/0x01)
        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Protect PRG-RAM by writing wrong values
        mapper.write_prg(0x5102, 0x00);
        mapper.write_prg(0x5103, 0x00);

        // Now writes should be ignored
        mapper.write_prg(0x6000, 0xBB);
        assert_eq!(mapper.read_prg(0x6000), 0xAA); // Still old value

        // Unprotect by writing correct values
        mapper.write_prg(0x5102, 0x02);
        mapper.write_prg(0x5103, 0x01);

        // Now writes should work again
        mapper.write_prg(0x6000, 0xCC);
        assert_eq!(mapper.read_prg(0x6000), 0xCC);
    }
}
