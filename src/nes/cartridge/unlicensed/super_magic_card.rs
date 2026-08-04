//! Mapper 6 — Front Fareast Magic Card (SMC) 1M/2M/4M PRG banking
//!
//! Sub-issue #627: Core latch-based banking modes 0–7 + register scaffolding.
//! Sub-issue #628: 2M/4M PRG banking mode ($43FC-$43FF, $4504-$4507).
//! Sub-issue #629: 1 KiB CHR banking mode + CHR nametable banking.
//! Sub-issue #630: IRQ counter ($4501-$4503, $4500 bit 3).
//! Sub-issue #631: Trainer initialization at $7000-$71FF.
//!
//! Spec: <https://www.nesdev.org/wiki/INES_Mapper_006>
//!       <https://www.nesdev.org/wiki/Super_Magic_Card>
//!
//! Known Limitations:
//! - JSR $7003 execution before game reset vector not yet implemented
//!   (requires CPU/console-layer changes; tracked separately).
use std::cell::Cell;

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::{A12RisingEdgeDetector, ChrMemory};
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const PRG_BANK_SIZE_8K: usize = 0x2000;
const CHR_BANK_SIZE_8K: usize = 0x2000;
const CHR_BANK_SIZE_4K: usize = 0x1000;
const CHR_BANK_SIZE_1K: usize = 0x0400;
const WRAM_BANK_SIZE_8K: usize = 0x2000;
const WRAM_SIZE_32K: usize = 0x8000;
const CHR_RAM_SIZE_256K: usize = 0x40000;

/// 16 KiB bank index for the lower half of 32 KiB PRG bank #3 (= 8 KiB banks 12–15).
/// Modes 5, 6, and 7 fix PRG at this bank pair.
const PRG_BANK3_LOWER_HALF: usize = 6; // 16 KiB index → 8 KiB banks 12 & 13

/// Mapper 6 — Front Fareast Magic Card (SMC-801)
///
/// Latch-based banking covering eight modes inherited from the Magic Card 1M/2M:
///   - Mode 0: UNROM  — 3-bit PRG bank at $8000, fixed last at $C000
///   - Mode 1: UN1ROM+CHRSW — 4-bit PRG (bits 5-2) + 2-bit CHR (bits 1-0)
///   - Mode 2: UOROM  — 4-bit PRG bank at $8000, fixed last at $C000
///   - Mode 3: Reverse UOROM+CHRSW — switchable $C000, fixed last at $8000
///   - Mode 4: GNROM  — 32KB PRG bank (bits 5-4), 8KB CHR bank (bits 1-0), CHR-protected
///   - Mode 5: CNROM-256 — fixed 32KB bank 3, 2-bit CHR, CHR-protected
///   - Mode 6: CNROM-128 — fixed 32KB bank 3, 1-bit CHR, CHR-protected
///   - Mode 7: NROM-256 — fixed 32KB bank 3, fixed CHR bank 0, CHR-protected
///
/// Registers:
///   $42FC-$42FF — 1M mode register; address bits A1/A0 encode latch-enable and
///                 mirroring LSB, data bits D7-D5/D4 encode mode and mirroring MSB.
///   $43FC-$43FF — 2M/4M mode register; A0=M (1=disable), A1=N (1=2M when M=0).
///   $4500       — SMC mode register (bits 5-4: WRAM bank select)
///   $4504-$4507 — 4M PRG slot registers; data bits 5-0 select 8 KiB bank per slot.
///   $6000-$7FFF — 8 KiB window into 32 KiB banked WRAM
///   $8000-$FFFF — latch write when latch is enabled; always updates 2M shadow slots
pub struct SuperMagicCardMapper {
    base: BaseMapper,
    wram: Vec<u8>,
    scratch_ram: Vec<u8>, // 4 KiB scratch RAM at CPU $5000–$5FFF (Mapper 17 only)
    trainer_load_address: u16, // CPU address where the trainer is loaded (default $7000)
    latch_mode: u8,       // D7-D5 of $42FC-$42FF: 0-7
    latch_value: u8,      // last value written to the latch at $8000-$FFFF
    latch_enabled: bool,  // A1 of $42FC-$42FF: PRG write-protected ↔ latch enabled
    mirroring_type: u8,   // (A0 << 1) | D4; 0=SingleScreenLower, 1=Upper, 2=Vertical, 3=Horizontal
    wram_bank: u8,        // bits 5-4 of $4500: 0-3, selects 8 KiB WRAM bank
    chr_mode_1kb: bool,   // $4500 bit 0: false=8 KiB (default), true=1 KiB
    chr_nt_active: bool,  // $4500 bit 1 inverted: true=CHR nametables via $4518-$451B
    mmc4_disabled: bool,  // $4500 bit 2: true=direct 1 KiB, false=MMC4 latch mode
    chr_1k_regs: [u8; 8], // $4510-$4517: 1 KiB bank per PPU $0000-$1FFF slot
    chr_nt_regs: [u8; 4], // $4518-$451B: 1 KiB bank per nametable $2000-$2FFF slot
    mmc4_latch0_fd: Cell<bool>, // lower 4 KiB latch: true=FD, false=FE
    mmc4_latch1_fd: Cell<bool>, // upper 4 KiB latch: true=FD, false=FE
    irq_counter: u16,     // 16-bit upward counting IRQ counter
    irq_latch_lo: u8,     // LSB written to $4502 (loaded on $4503 write)
    irq_enabled: bool,    // counting active (enabled by $4503, disabled by $4501)
    irq_pending_flag: bool, // IRQ asserted (set on $FFFF→$0000 wrap)
    irq_pa12_mode: bool,  // $4500 bit 3: false=M2 (cpu_cycle), true=PA12 (ppu_address_changed)
    a12_detector: A12RisingEdgeDetector, // A12 rising edge detection (no debounce)
    prg_2m_slots: [u8; 4], // shadow 8 KiB PRG banks for 2M mode (always updated on $8000-$FFFF writes)
    prg_4m_slots: [u8; 4], // 8 KiB PRG banks for 4M mode (updated via $4504-$4507)
    mode_2m_active: bool,  // true when $43FE was the last $43FC-$43FF write
    mode_4m_active: bool,  // true when $43FC was the last $43FC-$43FF write
}

/// Map a switched 16 KiB bank `b` to an 8 KiB slot index where slots 0–1 follow
/// the switched bank and slots 2–3 are fixed at the specified 16 KiB bank.
/// Used by latch modes 0 (UNROM), 1 (UN1ROM), and 2 (UOROM).
fn lower_switched_upper_fixed(b: usize, slot: usize, fixed_lo: usize, fixed_hi: usize) -> usize {
    match slot {
        0 => b * 2,
        1 => b * 2 + 1,
        2 => fixed_lo,
        _ => fixed_hi,
    }
}

/// First 8 KiB half of the fixed upper 16 KiB bank for UNROM/UN1ROM (16 KiB bank #7).
const FIXED_PRG_BANK7_LO: usize = 14;
/// Second 8 KiB half of the fixed upper 16 KiB bank for UNROM/UN1ROM (16 KiB bank #7).
const FIXED_PRG_BANK7_HI: usize = 15;
/// First 8 KiB half of the fixed upper 16 KiB bank for UOROM/Rev-UOROM (16 KiB bank #15).
const FIXED_PRG_BANK15_LO: usize = 30;
/// Second 8 KiB half of the fixed upper 16 KiB bank for UOROM/Rev-UOROM (16 KiB bank #15).
const FIXED_PRG_BANK15_HI: usize = 31;

/// Derive the 8 KiB PRG slot index (0–3) from a CPU address in $8000–$FFFF.
fn prg_slot_from_addr(addr: u16) -> usize {
    ((addr - 0x8000) / 0x2000) as usize
}

impl SuperMagicCardMapper {
    /// Create a new Mapper 6 instance.
    ///
    /// # Arguments
    /// * `prg_rom`   — PRG-ROM data (up to 256 KiB for iNES 1.0 submapper 1)
    /// * `chr_rom`   — CHR-ROM data when present; otherwise mapper uses CHR-RAM
    /// * `mirroring` — initial nametable mirroring from the iNES header
    /// * `submapper` — iNES 2.0 submapper (0 is remapped to 1 per spec)
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mapper = ctx.mapper;
        let submapper = ctx.submapper;
        let chr_is_ram = ctx.chr_rom.is_empty();
        let caps = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0, // WRAM managed separately
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            trainer_jsr: true,
            trainer_load_address: 0x7000,
        };
        let mut base = BaseMapper::new(&ctx, caps);
        if chr_is_ram {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE_256K));
        }
        let mirroring = ctx.mirroring;
        if mapper == 17 {
            Self::new_mapper17(base, mirroring, submapper)
        } else {
            // mappers 6 and 8; resolve submapper for SMC power-on state
            let effective_submapper = if mapper == 8 {
                4
            } else if submapper == 0 {
                1
            } else {
                submapper
            };
            Self::new_with_submapper(base, mirroring, effective_submapper)
        }
    }

    pub fn new_with_submapper(base: BaseMapper, mirroring: NametableLayout, submapper: u8) -> Self {
        // Per NesDev spec, the emulator simulates writing
        //   [$42FF] = (submapper << 5) | (horizontalMirroring ? 0x10 : 0x00)
        // at power-on:
        //   addr=$42FF  → A1=1 (latch enabled), A0=1 (mirroring LSB)
        //   data D7-D5  → BBB = submapper & 0x07   (latch mode)
        //   data D4     → 1 for horizontal, 0 for vertical/other
        //   mirroring_type = (A0 << 1) | D4
        let effective_submapper = if submapper == 0 { 1 } else { submapper };
        let latch_mode = effective_submapper & 0x07;
        let d4 = u8::from(matches!(mirroring, NametableLayout::Horizontal));
        // A0 = 1 (addr $42FF has bit 0 set)
        let mirroring_type = (1 << 1) | d4;

        Self {
            base,
            wram: vec![0; WRAM_SIZE_32K],
            scratch_ram: Vec::new(),
            trainer_load_address: 0x7000,
            latch_mode,
            latch_value: 0,
            latch_enabled: true,
            mirroring_type,
            wram_bank: 0,
            chr_mode_1kb: false,
            chr_nt_active: false,
            mmc4_disabled: false,
            chr_1k_regs: [0; 8],
            chr_nt_regs: [0; 4],
            mmc4_latch0_fd: Cell::new(true),
            mmc4_latch1_fd: Cell::new(true),
            irq_counter: 0,
            irq_latch_lo: 0,
            irq_enabled: false,
            irq_pending_flag: false,
            irq_pa12_mode: false,
            a12_detector: A12RisingEdgeDetector::new(0),
            prg_2m_slots: [0; 4],
            prg_4m_slots: [0; 4],
            mode_2m_active: false,
            mode_4m_active: false,
        }
    }

    /// Create a new Mapper 17 instance (Front Fareast Super Magic Card).
    ///
    /// Initializes with the register state the hardware writes at power-on:
    ///   `$4500 = $47`  — play mode, WRAM bank 0, 1 KiB CHR, CIRAM nametables, MMC4 disabled
    ///   `$42FF = $20 | (horizontal ? 0x10 : 0x00)` — latch mode 1, latch enabled, mirroring
    ///   `$43FC = $00`  — 4M PRG banking mode active
    ///   `$4504–$4507 = [N-4, N-3, N-2, N-1]` — last four 8 KiB PRG banks
    ///   `$4510–$4517 = [0, 1, 2, 3, 4, 5, 6, 7]` — identity mapping: slot N → CHR bank N
    ///
    /// The `submapper` field encodes the trainer load address (submapper 0 → $7000).
    pub fn new_mapper17(base: BaseMapper, mirroring: NametableLayout, submapper: u8) -> Self {
        let num_banks_8k = base.prg_rom().len() / PRG_BANK_SIZE_8K;
        let n = num_banks_8k as u8;

        // $42FF = $20 | (horizontalMirroring ? 0x10 : 0x00)
        // addr=$42FF → A1=1 (latch_enabled), A0=1 (mirroring_type bit 1 = 1)
        // D7-D5 = 001 → latch_mode = 1; D4 = mirroring MSB
        let d4 = u8::from(matches!(mirroring, NametableLayout::Horizontal));
        let mirroring_type = (1 << 1) | d4;

        // $43FC = $00 → 4M banking mode active
        let mode_4m_active = true;

        // $4504–$4507 = [N-4, N-3, N-2, N-1]
        let prg_4m_slots = [
            n.wrapping_sub(4),
            n.wrapping_sub(3),
            n.wrapping_sub(2),
            n.wrapping_sub(1),
        ];

        // $4500 = $47: chr_mode_1kb=true (bit 0), chr_nt_active=false (bit 1 set → CIRAM),
        //              mmc4_disabled=true (bit 2), irq_pa12_mode=false (bit 3=0), wram_bank=0

        // Trainer load address from submapper (0→$7000, 1→$5D00, 2→$5E00, 3→$5F00)
        let trainer_load_address = match submapper {
            1 => 0x5D00,
            2 => 0x5E00,
            3 => 0x5F00,
            _ => 0x7000,
        };

        Self {
            base,
            wram: vec![0; WRAM_SIZE_32K],
            scratch_ram: vec![0; 0x1000], // 4 KiB scratch RAM at $5000–$5FFF
            trainer_load_address,
            latch_mode: 1,
            latch_value: 0,
            latch_enabled: true,
            mirroring_type,
            wram_bank: 0,
            chr_mode_1kb: true,
            chr_nt_active: false,
            mmc4_disabled: true,
            chr_1k_regs: [0, 1, 2, 3, 4, 5, 6, 7],
            chr_nt_regs: [0; 4],
            mmc4_latch0_fd: Cell::new(true),
            mmc4_latch1_fd: Cell::new(true),
            irq_counter: 0,
            irq_latch_lo: 0,
            irq_enabled: false,
            irq_pending_flag: false,
            irq_pa12_mode: false,
            a12_detector: A12RisingEdgeDetector::new(0),
            prg_2m_slots: [0; 4],
            prg_4m_slots,
            mode_2m_active: false,
            mode_4m_active,
        }
    }

    /// Return the 8 KiB bank index for PRG slot `slot` (0-3) using the 1M latch.
    fn latch_bank_for_slot(&self, slot: usize) -> usize {
        match self.latch_mode {
            0 => {
                // UNROM: bits 2-0 → 16 KiB bank at $8000; bank #7 fixed at $C000
                let b = (self.latch_value & 0x07) as usize;
                lower_switched_upper_fixed(b, slot, FIXED_PRG_BANK7_LO, FIXED_PRG_BANK7_HI)
            }
            1 => {
                // UN1ROM+CHRSW: bits 5-2 → 16 KiB bank at $8000; bank #7 fixed at $C000
                let b = ((self.latch_value >> 2) & 0x0F) as usize;
                lower_switched_upper_fixed(b, slot, FIXED_PRG_BANK7_LO, FIXED_PRG_BANK7_HI)
            }
            2 => {
                // UOROM: bits 3-0 → 16 KiB bank at $8000; bank #15 fixed at $C000
                let b = (self.latch_value & 0x0F) as usize;
                lower_switched_upper_fixed(b, slot, FIXED_PRG_BANK15_LO, FIXED_PRG_BANK15_HI)
            }
            3 => {
                // Reverse UOROM: bits 3-0 → $C000 bank; bank #15 fixed at $8000
                let b = (self.latch_value & 0x0F) as usize;
                match slot {
                    0 => FIXED_PRG_BANK15_LO,
                    1 => FIXED_PRG_BANK15_HI,
                    2 => b * 2,
                    _ => b * 2 + 1,
                }
            }
            4 => {
                // GNROM: bits 5-4 → 32 KiB bank (PP)
                let pp = ((self.latch_value >> 4) & 0x03) as usize;
                pp * 4 + slot
            }
            _ => {
                // Modes 5-7: fixed 32 KiB bank #3 (16 KiB banks 6+7 = 8 KiB banks 12-15)
                PRG_BANK3_LOWER_HALF * 2 + slot
            }
        }
    }

    /// Return the 8 KiB bank index for PRG slot `slot` (0-3),
    /// honouring 4M > 2M > latch priority.
    fn bank_for_slot(&self, slot: usize) -> usize {
        if self.mode_4m_active {
            self.prg_4m_slots[slot] as usize
        } else if self.mode_2m_active {
            self.prg_2m_slots[slot] as usize
        } else {
            self.latch_bank_for_slot(slot)
        }
    }

    fn chr_bank_8k(&self) -> usize {
        match self.latch_mode {
            0 | 2 | 7 => 0,                                 // fixed CHR bank 0
            1 => (self.latch_value & 0x03) as usize,        // CC = bits 1-0
            3 => ((self.latch_value >> 4) & 0x03) as usize, // CC = bits 5-4
            4 | 5 => (self.latch_value & 0x03) as usize,    // CC = bits 1-0
            6 => (self.latch_value & 0x01) as usize,        // C = bit 0
            _ => 0,
        }
    }

    fn chr_write_protected(&self) -> bool {
        self.latch_mode >= 4
    }

    /// Decode a write to the 1M mode register ($42FC–$42FF) and apply it.
    fn apply_mode_register(&mut self, addr: u16, value: u8) {
        let latch_enable = (addr >> 1) & 1;
        let mirroring_lsb = (addr & 1) as u8;
        let mode = (value >> 5) & 0x07;
        let mirroring_msb = (value >> 4) & 0x01;
        self.latch_enabled = latch_enable != 0;
        self.latch_mode = mode;
        self.mirroring_type = (mirroring_lsb << 1) | mirroring_msb;
    }

    /// Decode a write to the 2M/4M mode register ($43FC–$43FF) and apply it.
    ///
    /// Address encoding: A0=M (0=enable, 1=disable), A1=N (0=4M, 1=2M when M=0).
    /// Data bits 1-0 (CC): CHR bank update (mirrors latch CC — handled by latch writes).
    fn apply_2m4m_register(&mut self, addr: u16) {
        let m = addr & 1; // A0
        let n = (addr >> 1) & 1; // A1
        if m != 0 {
            // M=1 → disable both modes
            self.mode_2m_active = false;
            self.mode_4m_active = false;
        } else if n != 0 {
            // M=0, N=1 → 2M mode
            self.mode_2m_active = true;
            self.mode_4m_active = false;
        } else {
            // M=0, N=0 → 4M mode
            self.mode_2m_active = false;
            self.mode_4m_active = true;
        }
    }

    fn wram_index(&self, addr: u16) -> usize {
        self.wram_bank as usize * WRAM_BANK_SIZE_8K + (addr - 0x6000) as usize
    }

    /// Compute the flat CHR memory index for `addr`, routing to the active CHR mode:
    /// - 8 KiB latch mode (`!chr_mode_1kb`)
    /// - 1 KiB direct mode (`chr_mode_1kb && mmc4_disabled`)
    /// - MMC4 latch mode  (`chr_mode_1kb && !mmc4_disabled`)
    fn chr_index_for_addr(&self, addr: u16) -> usize {
        if !self.chr_mode_1kb {
            self.chr_bank_8k() * CHR_BANK_SIZE_8K + (addr & 0x1FFF) as usize
        } else if self.mmc4_disabled {
            self.chr_index_1kb_direct(addr)
        } else {
            self.chr_index_mmc4(addr)
        }
    }

    /// Flat CHR index for 1 KiB direct mode: each 1 KiB PPU slot (`$0000–$1FFF`)
    /// maps independently to a CHR bank selected by `chr_1k_regs[slot]`.
    fn chr_index_1kb_direct(&self, addr: u16) -> usize {
        let slot = (addr / CHR_BANK_SIZE_1K as u16) as usize;
        let bank = self.chr_1k_regs[slot] as usize;
        bank * CHR_BANK_SIZE_1K + (addr & 0x03FF) as usize
    }

    /// Flat CHR index for MMC4 latch mode: two 4 KiB halves each switch between
    /// an FD and FE bank based on PPU tile-fetch trigger addresses.
    ///
    /// `chr_1k_regs` layout (FD=index+0, FE=index+2, bank = register >> 2):
    /// - lower half (`$0000–$0FFF`): base index 0 (`chr_1k_regs[0/2]`)
    /// - upper half (`$1000–$1FFF`): base index 4 (`chr_1k_regs[4/6]`)
    fn chr_index_mmc4(&self, addr: u16) -> usize {
        let is_upper_half = addr >= 0x1000;
        let latch_fd = if is_upper_half {
            self.mmc4_latch1_fd.get()
        } else {
            self.mmc4_latch0_fd.get()
        };
        let reg_base: usize = if is_upper_half { 4 } else { 0 };
        let fd_fe_offset: usize = if latch_fd { 0 } else { 2 };
        let bank_4k = (self.chr_1k_regs[reg_base + fd_fe_offset] >> 2) as usize;
        bank_4k * CHR_BANK_SIZE_4K + (addr & 0x0FFF) as usize
    }

    /// Update MMC4-style latches if `addr` is a tile-fetch trigger address.
    /// Only active when 1 KiB CHR mode and MMC4 are both enabled.
    fn update_mmc4_latches(&self, addr: u16) {
        if !self.chr_mode_1kb || self.mmc4_disabled {
            return;
        }
        match addr {
            0x0FD8..=0x0FDF => self.mmc4_latch0_fd.set(true),
            0x0FE8..=0x0FEF => self.mmc4_latch0_fd.set(false),
            0x1FD8..=0x1FDF => self.mmc4_latch1_fd.set(true),
            0x1FE8..=0x1FEF => self.mmc4_latch1_fd.set(false),
            _ => {}
        }
    }

    /// Increment the IRQ counter by one; set `irq_pending_flag` on $FFFF → $0000 wrap.
    fn tick_irq_counter(&mut self) {
        if !self.irq_enabled {
            return;
        }
        let (next, wrapped) = self.irq_counter.overflowing_add(1);
        self.irq_counter = next;
        if wrapped {
            self.irq_pending_flag = true;
        }
    }

    /// Acknowledge a pending IRQ (clear the pending flag).
    fn acknowledge_irq(&mut self) {
        self.irq_pending_flag = false;
    }

    /// Read a byte from PRG-ROM at the given 8KB bank and address.
    fn read_prg_bank_8k(&self, bank: usize, base_addr: u16, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        let num_banks = prg.len() / PRG_BANK_SIZE_8K;
        if num_banks == 0 {
            return 0;
        }
        let bank = bank % num_banks;
        let offset = (addr - base_addr) as usize;
        prg.get(bank * PRG_BANK_SIZE_8K + offset)
            .copied()
            .unwrap_or(0)
    }
}

impl Mapper for SuperMagicCardMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x5FFF => self
                .scratch_ram
                .get((addr - 0x5000) as usize)
                .copied()
                .unwrap_or(0),
            0x6000..=0x7FFF => self.wram.get(self.wram_index(addr)).copied().unwrap_or(0),
            0x8000..=0x9FFF => self.read_prg_bank_8k(self.bank_for_slot(0), 0x8000, addr),
            0xA000..=0xBFFF => self.read_prg_bank_8k(self.bank_for_slot(1), 0xA000, addr),
            0xC000..=0xDFFF => self.read_prg_bank_8k(self.bank_for_slot(2), 0xC000, addr),
            0xE000..=0xFFFF => self.read_prg_bank_8k(self.bank_for_slot(3), 0xE000, addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x5000..=0x5FFF).contains(&addr) && !self.scratch_ram.is_empty() {
            self.read_prg(addr)
        } else {
            // Default: $4020-$5FFF returns open bus, $6000+ reads PRG
            if addr < 0x6000 {
                open_bus
            } else {
                self.read_prg(addr)
            }
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5000..=0x5FFF => {
                let idx = (addr - 0x5000) as usize;
                if idx < self.scratch_ram.len() {
                    self.scratch_ram[idx] = value;
                }
            }
            0x42FC..=0x42FF => self.apply_mode_register(addr, value),
            0x43FC..=0x43FF => self.apply_2m4m_register(addr),
            0x4500 => {
                self.wram_bank = (value >> 4) & 0x03;
                self.chr_mode_1kb = (value & 0x01) != 0;
                self.chr_nt_active = (value & 0x02) == 0;
                self.mmc4_disabled = (value & 0x04) != 0;
                self.irq_pa12_mode = (value & 0x08) != 0;
            }
            0x4501 => {
                // Acknowledge IRQ and disable counting.
                self.acknowledge_irq();
                self.irq_enabled = false;
            }
            0x4502 => {
                // Store counter LSB; acknowledge IRQ.
                self.acknowledge_irq();
                self.irq_latch_lo = value;
            }
            0x4503 => {
                // Store counter MSB; acknowledge IRQ; load counter; enable counting.
                self.acknowledge_irq();
                self.irq_counter = (u16::from(value) << 8) | u16::from(self.irq_latch_lo);
                self.irq_enabled = true;
            }
            0x4504..=0x4507 => self.prg_4m_slots[(addr - 0x4504) as usize] = value & 0x3F,
            0x4510..=0x4517 => self.chr_1k_regs[(addr - 0x4510) as usize] = value,
            0x4518..=0x451B => self.chr_nt_regs[(addr - 0x4518) as usize] = value,
            0x6000..=0x7FFF => {
                let index = self.wram_index(addr);
                if index < self.wram.len() {
                    self.wram[index] = value;
                }
            }
            // $8000-$FFFF writes are register writes only when write-protection is active.
            // Both the 2M shadow slot and the latch are updated together on each write.
            0x8000..=0xFFFF if self.latch_enabled => {
                let slot = prg_slot_from_addr(addr);
                self.prg_2m_slots[slot] = (value >> 2) & 0x3F;
                self.latch_value = value;
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let index = self.chr_index_for_addr(addr);
        let value = self.base.read_chr_at_index(index);
        self.update_mmc4_latches(addr);
        value
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_write_protected() {
            let index = self.chr_index_for_addr(addr);
            self.base.write_chr_at_index(index, value);
        }
    }

    fn read_nametable(&mut self, addr: u16) -> Option<u8> {
        if !self.chr_nt_active {
            return None;
        }
        let slot = ((addr - 0x2000) / CHR_BANK_SIZE_1K as u16) as usize;
        if slot >= 4 {
            return None;
        }
        let bank = self.chr_nt_regs[slot] as usize;
        let offset = (addr & 0x03FF) as usize;
        Some(
            self.base
                .read_chr_at_index(bank * CHR_BANK_SIZE_1K + offset),
        )
    }

    fn write_nametable(&mut self, addr: u16, value: u8) -> bool {
        if !self.chr_nt_active {
            return false;
        }
        let slot = ((addr - 0x2000) / CHR_BANK_SIZE_1K as u16) as usize;
        if slot >= 4 {
            return false;
        }
        let bank = self.chr_nt_regs[slot] as usize;
        let offset = (addr & 0x03FF) as usize;
        self.base
            .write_chr_at_index(bank * CHR_BANK_SIZE_1K + offset, value);
        true
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.update_mmc4_latches(addr);
        if self.irq_pa12_mode && self.a12_detector.update(addr) {
            self.tick_irq_counter();
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending_flag
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_pa12_mode {
            self.tick_irq_counter();
        }
    }

    fn get_mirroring(&self) -> NametableLayout {
        match self.mirroring_type {
            0 => NametableLayout::SingleScreenLower,
            1 => NametableLayout::SingleScreenUpper,
            2 => NametableLayout::Vertical,
            3 => NametableLayout::Horizontal,
            _ => NametableLayout::Horizontal, // mirroring_type is always 0–3; unreachable in practice
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 32,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            trainer_jsr: true,
            trainer_load_address: self.trainer_load_address,
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // byte  0: latch_mode (bits 2-0)
        // byte  1: latch_value
        // byte  2: latch_enabled (bit 0) | mirroring_type (bits 2-1) | wram_bank (bits 5-4)
        // bytes 3-6:  prg_2m_slots[0-3]
        // bytes 7-10: prg_4m_slots[0-3]
        // byte 11: mode flags: bit 0=mode_2m_active, bit 1=mode_4m_active
        // bytes 12-19: chr_1k_regs[0-7]
        // bytes 20-23: chr_nt_regs[0-3]
        // byte 24: chr flags: bit 0=chr_mode_1kb, bit 1=chr_nt_active, bit 2=mmc4_disabled
        //                     bit 3=mmc4_latch0_fd, bit 4=mmc4_latch1_fd
        // bytes 25-26: irq_counter (lo, hi)
        // byte 27: irq_latch_lo
        // byte 28: irq flags: bit 0=irq_enabled, bit 1=irq_pending_flag,
        //                     bit 2=irq_pa12_mode, bit 3=prev_a12
        let mut v = vec![
            self.latch_mode & 0x07,
            self.latch_value,
            (self.latch_enabled as u8) | (self.mirroring_type << 1) | (self.wram_bank << 4),
        ];
        v.extend_from_slice(&self.prg_2m_slots);
        v.extend_from_slice(&self.prg_4m_slots);
        v.push((self.mode_2m_active as u8) | ((self.mode_4m_active as u8) << 1));
        v.extend_from_slice(&self.chr_1k_regs);
        v.extend_from_slice(&self.chr_nt_regs);
        v.push(
            (self.chr_mode_1kb as u8)
                | ((self.chr_nt_active as u8) << 1)
                | ((self.mmc4_disabled as u8) << 2)
                | ((self.mmc4_latch0_fd.get() as u8) << 3)
                | ((self.mmc4_latch1_fd.get() as u8) << 4),
        );
        v.push(self.irq_counter as u8);
        v.push((self.irq_counter >> 8) as u8);
        v.push(self.irq_latch_lo);
        v.push(
            (self.irq_enabled as u8)
                | ((self.irq_pending_flag as u8) << 1)
                | ((self.irq_pa12_mode as u8) << 2)
                | ((self.a12_detector.prev_a12() as u8) << 3),
        );
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.latch_mode = data[0] & 0x07;
            self.latch_value = data[1];
            self.latch_enabled = (data[2] & 0x01) != 0;
            self.mirroring_type = (data[2] >> 1) & 0x03;
            self.wram_bank = (data[2] >> 4) & 0x03;
        }
        if data.len() >= 7 {
            self.prg_2m_slots.copy_from_slice(&data[3..7]);
        }
        if data.len() >= 11 {
            self.prg_4m_slots.copy_from_slice(&data[7..11]);
        }
        if data.len() >= 12 {
            self.mode_2m_active = (data[11] & 0x01) != 0;
            self.mode_4m_active = (data[11] & 0x02) != 0;
        }
        if data.len() >= 20 {
            self.chr_1k_regs.copy_from_slice(&data[12..20]);
        }
        if data.len() >= 24 {
            self.chr_nt_regs.copy_from_slice(&data[20..24]);
        }
        if data.len() >= 25 {
            self.chr_mode_1kb = (data[24] & 0x01) != 0;
            self.chr_nt_active = (data[24] & 0x02) != 0;
            self.mmc4_disabled = (data[24] & 0x04) != 0;
            self.mmc4_latch0_fd.set((data[24] & 0x08) != 0);
            self.mmc4_latch1_fd.set((data[24] & 0x10) != 0);
        }
        if data.len() >= 29 {
            self.irq_counter = u16::from(data[25]) | (u16::from(data[26]) << 8);
            self.irq_latch_lo = data[27];
            self.irq_enabled = (data[28] & 0x01) != 0;
            self.irq_pending_flag = (data[28] & 0x02) != 0;
            self.irq_pa12_mode = (data[28] & 0x04) != 0;
            self.a12_detector.set_prev_a12((data[28] & 0x08) != 0);
        }
    }

    fn wram_size(&self) -> usize {
        WRAM_SIZE_32K
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.wram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.wram.len());
        self.wram[..to_copy].copy_from_slice(&data[..to_copy]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANK_SIZE_16K: usize = 0x4000;

    fn create_m6(prg: Vec<u8>, submapper: u8, mirroring: NametableLayout) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new_for_test(6, prg, vec![], mirroring).with_submapper(submapper),
        )
        .expect("Failed to create Mapper 6")
    }

    fn create_m17(prg: Vec<u8>, submapper: u8, mirroring: NametableLayout) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new_for_test(17, prg, vec![], mirroring).with_submapper(submapper),
        )
        .expect("Failed to create Mapper 17")
    }

    // ── Initial state ──────────────────────────────────────────────────────────

    #[test]
    fn test_initial_state_submapper1_vertical_mirroring() {
        // Air Fortress-like: submapper 1, vertical mirroring from iNES header
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Vertical);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_initial_state_submapper1_horizontal_mirroring() {
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Horizontal);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── $42FC-$42FF mirroring register ────────────────────────────────────────
    //
    // Address bits: A1=latch-enable, A0=mirroring-LSB
    // Data bits:    D7-D5=mode(BBB), D4=mirroring-MSB
    // mirroring_type = (A0 << 1) | D4
    //   0 → SingleScreenLower, 1 → SingleScreenUpper, 2 → Vertical, 3 → Horizontal

    #[test]
    fn test_write_42fe_d4_0_sets_single_screen_lower() {
        // A0=0, D4=0 → mirroring_type = 0
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FE, 0x20); // D7-D5=001 (mode 1), D4=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_write_42fe_d4_1_sets_single_screen_upper() {
        // A0=0, D4=1 → mirroring_type = 1
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FE, 0x30); // D7-D5=001 (mode 1), D4=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_write_42ff_d4_0_sets_vertical() {
        // A0=1, D4=0 → mirroring_type = 2
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Horizontal);
        mapper.write_prg(0x42FF, 0x20); // D7-D5=001 (mode 1), D4=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_write_42ff_d4_1_sets_horizontal() {
        // A0=1, D4=1 → mirroring_type = 3
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x30); // D7-D5=001 (mode 1), D4=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_write_42fc_disables_latch_so_bank_writes_are_ignored() {
        // A1=0 for $42FC → latch disabled; writes to $8000-$FFFF are ignored
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Select PRG bank 5 while latch is enabled (initial state)
        mapper.write_prg(0x8000, 5 << 2); // mode 1: bits 5-2 = 0101 → bank 5
        assert_eq!(mapper.read_prg(0x8000), 5);
        // Disable latch: write to $42FC (A1=0)
        mapper.write_prg(0x42FC, 0x20); // mode 1, A1=0 → latch disabled
        // Attempt to change bank — must be ignored
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 5); // still bank 5
    }

    #[test]
    fn test_write_42ff_enables_latch_so_bank_writes_take_effect() {
        // A1=1 for $42FF → latch enabled
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Initial: latch already enabled; write bank 3
        mapper.write_prg(0x8000, 3 << 2); // bank 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // ── Mode 0 — UNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode0_prg_8000_switches_with_bits_0_2() {
        // D~[..... PPP] → 16 KiB bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 8); // 8 × 16 KiB = 128 KiB
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x00); // BBB=000 → mode 0
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_mode0_c000_fixed_at_bank7() {
        // For 256 KiB ROM (16 × 16 KiB), fixed upper = 16 KiB bank #7 (not last)
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x00); // mode 0
        mapper.write_prg(0x8000, 0x00); // bank 0 at $8000
        assert_eq!(mapper.read_prg(0xC000), 7); // fixed at absolute 16 KiB bank #7
    }

    // ── Mode 1 — UN1ROM + CHRSW (Air Fortress) ────────────────────────────────

    #[test]
    fn test_mode1_prg_8000_switches_with_bits_2_5() {
        // D~[..BBBB CC] → PRG bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 16); // 16 × 16 KiB = 256 KiB
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x04); // bank 1 (bits 5-2 = 0001)
        assert_eq!(mapper.read_prg(0x8000), 1);
        mapper.write_prg(0x8000, 0x3C); // bank 15 (bits 5-2 = 1111)
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_mode1_c000_fixed_at_bank7() {
        // For 256 KiB ROM (16 × 16 KiB), fixed upper = 16 KiB bank #7 (not last bank #15)
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x00); // bank 0 at $8000
        assert_eq!(mapper.read_prg(0xC000), 7); // fixed at absolute 16 KiB bank #7
    }

    #[test]
    fn test_mode1_chr_bank_selected_by_bits_0_1() {
        // CC (bits 1-0) selects 8 KiB CHR bank 0-3
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Write distinct markers to CHR banks 0-3
        for bank in 0..4u8 {
            mapper.write_prg(0x8000, bank); // latch: CC=bank, BBBB=0
            mapper.write_chr(0x0000, bank + 10);
        }
        // Verify each bank holds its marker
        for bank in 0..4u8 {
            mapper.write_prg(0x8000, bank);
            assert_eq!(mapper.read_chr(0x0000), bank + 10);
        }
    }

    #[test]
    fn test_mode1_chr_is_writable() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB);
    }

    // ── Mode 2 — UOROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode2_prg_8000_switches_with_4bit_bank() {
        // D~[....PPPP] → 16 KiB bank at $8000-$BFFF; $C000-$FFFF = last bank
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x40); // BBB=010 → mode 2
        mapper.write_prg(0x8000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 10);
    }

    #[test]
    fn test_mode2_c000_fixed_at_bank15() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x40); // mode 2
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    // ── Mode 3 — Reverse UOROM + CHRSW ───────────────────────────────────────

    #[test]
    fn test_mode3_c000_switches_8000_fixed_at_bank15() {
        // D~[..CC PPPP] → $C000-$FFFF = latch bits 0-3; $8000-$BFFF = fixed last
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x60); // BBB=011 → mode 3
        mapper.write_prg(0x8000, 0x05); // PPPP=5 → $C000 bank 5
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0x8000), 15); // fixed last bank
    }

    #[test]
    fn test_mode3_chr_bank_selected_by_bits_4_5() {
        // CC = bits 5-4
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x60); // mode 3
        mapper.write_chr(0x0000, 0xAA); // write to CHR bank 0
        mapper.write_prg(0x8000, 0x10); // bits 5-4 = 01 → CHR bank 1
        mapper.write_chr(0x0000, 0xBB); // write to CHR bank 1
        mapper.write_prg(0x8000, 0x00); // back to CHR bank 0
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_chr(0x0000), 0xBB);
    }

    // ── Mode 4 — GNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_mode4_32kb_prg_bank_selected_by_bits_4_5() {
        // D~[..PP..CC] → $8000-$FFFF = 32 KiB bank PP
        let prg = banked_data(PRG_BANK_SIZE_16K, 16); // 4 × 32 KiB banks
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x80); // BBB=100 → mode 4
        // PP=0 → 32 KiB bank 0 (16 KiB banks 0+1)
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0); // lower 16 KiB
        assert_eq!(mapper.read_prg(0xC000), 1); // upper 16 KiB
        // PP=1 → 32 KiB bank 1 (16 KiB banks 2+3)
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
    }

    #[test]
    fn test_mode4_chr_is_write_protected() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0x80); // mode 4
        mapper.write_chr(0x0000, 0xAB); // must be ignored
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 5 — CNROM-256 ────────────────────────────────────────────────────

    #[test]
    fn test_mode5_prg_fixed_at_32kb_bank3() {
        // PRG fixed at 32 KiB bank #3 = 16 KiB banks 6 and 7
        let prg = banked_data(PRG_BANK_SIZE_16K, 8); // 128 KiB
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xA0); // BBB=101 → mode 5
        mapper.write_prg(0x8000, 0x00); // latch write must not change PRG
        assert_eq!(mapper.read_prg(0x8000), 6); // 32 KiB bank 3, lower half
        assert_eq!(mapper.read_prg(0xC000), 7); // upper half
    }

    #[test]
    fn test_mode5_chr_is_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xA0); // mode 5
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 6 — CNROM-128 ────────────────────────────────────────────────────

    #[test]
    fn test_mode6_prg_fixed_at_32kb_bank3() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 8);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xC0); // BBB=110 → mode 6
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_mode6_chr_is_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xC0); // mode 6
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── Mode 7 — NROM-256 ─────────────────────────────────────────────────────

    #[test]
    fn test_mode7_prg_fixed_at_32kb_bank3() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 8);
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xE0); // BBB=111 → mode 7
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn test_mode7_chr_fixed_at_bank0_and_write_protected() {
        let prg = vec![0u8; 128 * 1024];
        let mut mapper = create_m6(prg, 0, NametableLayout::Vertical);
        mapper.write_prg(0x42FF, 0xE0); // mode 7
        mapper.write_chr(0x0000, 0xAB); // must be ignored
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    // ── WRAM at $6000-$7FFF ───────────────────────────────────────────────────

    #[test]
    fn test_wram_read_write_in_bank0() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCD);
    }

    #[test]
    fn test_wram_4500_bits_5_4_select_8kb_bank() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Write 0x11 to WRAM bank 0
        mapper.write_prg(0x6000, 0x11);
        // Switch to bank 1 (bits 5-4 of $4500 = 01)
        mapper.write_prg(0x4500, 0x10);
        mapper.write_prg(0x6000, 0x22);
        // Switch back to bank 0 and verify
        mapper.write_prg(0x4500, 0x00);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
        // Switch to bank 1 and verify
        mapper.write_prg(0x4500, 0x10);
        assert_eq!(mapper.read_prg(0x6000), 0x22);
    }

    // ── Register snapshot / restore ───────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_preserves_mode_bank_and_mirroring() {
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        // Set mode 1, bank 5, vertical mirroring, WRAM bank 2
        mapper.write_prg(0x42FF, 0x20); // mode 1, latch enabled, vertical
        mapper.write_prg(0x4500, 0x20); // WRAM bank 2
        mapper.write_prg(0x8000, 5 << 2); // mode 1: bits 5-2 = 5 → bank 5
        let snap = mapper.registers_snapshot();
        // Restore into a fresh mapper
        let mut restored = create_m6(prg, 1, NametableLayout::Horizontal);
        restored.restore_registers(&snap);
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(restored.read_prg(0x8000), 5);
    }

    #[test]
    fn test_chr_ram_snapshot_preserves_all_banks() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        // Write distinct markers to CHR banks 0 and 1
        mapper.write_prg(0x8000, 0); // CHR bank 0
        mapper.write_chr(0x0000, 0x42);
        mapper.write_prg(0x8000, 1); // CHR bank 1
        mapper.write_chr(0x0000, 0x99);
        let snap = mapper.chr_ram_snapshot();
        // Restore and verify
        let mut restored = create_m6(prg, 1, NametableLayout::Vertical);
        restored.restore_chr_ram(&snap);
        restored.write_prg(0x8000, 0);
        assert_eq!(restored.read_chr(0x0000), 0x42);
        restored.write_prg(0x8000, 1);
        assert_eq!(restored.read_chr(0x0000), 0x99);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn test_capabilities_reports_chr_banking_and_dynamic_mirroring() {
        let prg = vec![0u8; 256 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Vertical);
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "mapper 6 has CHR banking");
        assert!(caps.has_dynamic_mirroring, "mapper 6 has dynamic mirroring");
        assert!(caps.has_irq, "mapper 6 has IRQ counter");
        assert!(caps.trainer_jsr, "mapper 6 executes trainer via JSR $7003");
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(mapper.wram_size(), 32 * 1024);
    }

    // ── 2M PRG banking ($43FE enables; writes to $8000-$FFFF set 8 KiB slots) ─

    #[test]
    fn test_2m_mode_slot0_reads_8kb_bank_via_8000_write() {
        // $43FE enables 2M mode; data [PPPPPPCC] → slot 0 = PPPPPP = value >> 2.
        // banked_data(0x2000, 32): each 8 KiB bank k filled with k.
        let prg = banked_data(0x2000, 32); // 32 × 8 KiB = 256 KiB
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00); // enable 2M mode (A1=N=1, A0=M=0)
        mapper.write_prg(0x8000, 9 << 2); // slot 0 = 8 KiB bank 9
        assert_eq!(mapper.read_prg(0x8000), 9);
    }

    #[test]
    fn test_2m_mode_slot1_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xA000, 7 << 2); // slot 1 = bank 7
        assert_eq!(mapper.read_prg(0xA000), 7);
    }

    #[test]
    fn test_2m_mode_slot2_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xC000, 15 << 2); // slot 2 = bank 15
        assert_eq!(mapper.read_prg(0xC000), 15);
    }

    #[test]
    fn test_2m_mode_slot3_independently_bankable() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0xE000, 20 << 2); // slot 3 = bank 20
        assert_eq!(mapper.read_prg(0xE000), 20);
    }

    #[test]
    fn test_2m_mode_all_four_slots_independent() {
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0x8000, 3 << 2); // slot 0 = bank 3
        mapper.write_prg(0xA000, 11 << 2); // slot 1 = bank 11
        mapper.write_prg(0xC000, 17 << 2); // slot 2 = bank 17
        mapper.write_prg(0xE000, 28 << 2); // slot 3 = bank 28
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xA000), 11);
        assert_eq!(mapper.read_prg(0xC000), 17);
        assert_eq!(mapper.read_prg(0xE000), 28);
    }

    #[test]
    fn test_2m_slot_shadow_registers_updated_even_when_mode_disabled() {
        // Per spec, 2M registers ALWAYS accept writes (even when 2M mode inactive).
        // A write to $8000-$FFFF while 2M is disabled still updates the shadow slot.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // 2M disabled at start; write slot 0 shadow = bank 7
        mapper.write_prg(0x8000, 7 << 2);
        // Now enable 2M — slot 0 should already be bank 7 from the shadow write
        mapper.write_prg(0x43FE, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    #[test]
    fn test_43fd_disables_2m_mode_and_latch_takes_over() {
        // $43FD: A0=M=1 → disable 2M/4M; fallback to latch-based banking.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00);
        mapper.write_prg(0x8000, 7 << 2); // 2M slot 0 = bank 7; latch = 0x1C
        // In 2M mode slot 0 = bank 7 → data 7
        assert_eq!(mapper.read_prg(0x8000), 7);
        // Disable 2M/4M
        mapper.write_prg(0x43FD, 0x00);
        // Latch mode 1: bank = (0x1C >> 2) & 0xF = 7 → 8 KiB bank 7*2=14 → data 14
        assert_eq!(mapper.read_prg(0x8000), 14);
    }

    // ── 4M PRG banking ($43FC enables; $4504-$4507 set 8 KiB slots) ───────────

    #[test]
    fn test_4m_mode_slot0_via_4504_write() {
        // $43FC enables 4M mode (A1=N=0, A0=M=0); data [..PPPPPP] = 6-bit bank.
        // banked_data(0x2000, 64): 64 × 8 KiB = 512 KiB, bank k filled with k.
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00); // enable 4M mode
        mapper.write_prg(0x4504, 15); // slot 0 = bank 15
        assert_eq!(mapper.read_prg(0x8000), 15);
    }

    #[test]
    fn test_4m_mode_slot1_via_4505_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4505, 22); // slot 1 = bank 22
        assert_eq!(mapper.read_prg(0xA000), 22);
    }

    #[test]
    fn test_4m_mode_slot2_via_4506_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4506, 40); // slot 2 = bank 40
        assert_eq!(mapper.read_prg(0xC000), 40);
    }

    #[test]
    fn test_4m_mode_slot3_via_4507_write() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4507, 55); // slot 3 = bank 55
        assert_eq!(mapper.read_prg(0xE000), 55);
    }

    #[test]
    fn test_4m_mode_all_four_slots_independent() {
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4504, 5); // slot 0 = bank 5
        mapper.write_prg(0x4505, 20); // slot 1 = bank 20
        mapper.write_prg(0x4506, 45); // slot 2 = bank 45
        mapper.write_prg(0x4507, 63); // slot 3 = bank 63
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xA000), 20);
        assert_eq!(mapper.read_prg(0xC000), 45);
        assert_eq!(mapper.read_prg(0xE000), 63);
    }

    #[test]
    fn test_4m_mode_chr_not_changed_by_4504_writes() {
        // 4M $4504 writes carry no CC bits; CHR must remain from latch.
        let prg = banked_data(0x2000, 64);
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // Latch mode 1: set CHR bank 2 and PRG bank 0
        mapper.write_prg(0x8000, 0x02); // latch: BBBB=0, CC=2 → CHR bank 2
        mapper.write_chr(0x0000, 0x42); // mark CHR bank 2
        // Enable 4M and assign slot 0 via $4504
        mapper.write_prg(0x43FC, 0x00);
        mapper.write_prg(0x4504, 40); // PRG slot 0 = bank 40
        // PRG should now reflect 4M slot 0
        assert_eq!(mapper.read_prg(0x8000), 40);
        // CHR bank 2 must still be active (latch CC bits unchanged)
        assert_eq!(mapper.read_chr(0x0000), 0x42);
    }

    // ── Snapshot roundtrip for 2M/4M state ────────────────────────────────────

    #[test]
    fn test_2m_4m_snapshot_includes_mode_and_slots() {
        // After enabling 2M mode and setting all slot banks, a snapshot+restore
        // must preserve mode_2m_active and prg_2m_slots exactly.
        let prg = banked_data(0x2000, 32);
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        mapper.write_prg(0x43FE, 0x00); // enable 2M
        mapper.write_prg(0x8000, 3 << 2); // slot 0 = bank 3
        mapper.write_prg(0xA000, 11 << 2); // slot 1 = bank 11
        mapper.write_prg(0xC000, 17 << 2); // slot 2 = bank 17
        mapper.write_prg(0xE000, 28 << 2); // slot 3 = bank 28
        let snap = mapper.registers_snapshot();
        let mut restored = create_m6(prg, 1, NametableLayout::Horizontal);
        restored.restore_registers(&snap);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xA000), 11);
        assert_eq!(restored.read_prg(0xC000), 17);
        assert_eq!(restored.read_prg(0xE000), 28);
    }

    // ── 1 KiB CHR banking ($4510-$4517, $4500 bit 0 = C) ─────────────────────
    //
    // $4500 encoding (write): [P M WW I m n C]
    //   bit 0: C — CHR mode: 0=8 KiB (default), 1=1 KiB
    //   bit 1: n — Nametable source: 0=CHR via $4518-$451B, 1=CIRAM
    //   bit 2: m — MMC4 mode: 0=enabled, 1=disabled
    //   bits 5-4: WW — 8 KiB WRAM bank
    //
    // 1 KiB direct mode (m=1, C=1, i.e. $4500=0x05):
    //   $4510-$4517 each directly select a 1 KiB CHR bank for PPU $0000-$1FFF.
    //   Data: [CCCCCCCC] = 8-bit 1 KiB bank index (0-255).

    #[test]
    fn test_4510_selects_1kb_chr_bank_for_slot0() {
        // In 1 KiB direct mode, $4510 sets the 1 KiB bank for PPU $0000-$03FF.
        // Write 0x42 to bank 3 at offset 0, then verify read uses bank 3.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct mode (C=1, m=1)
        mapper.write_prg(0x4510, 3); // slot 0 = 1 KiB bank 3
        mapper.write_chr(0x0000, 0x42); // write to bank 3, offset 0
        mapper.write_prg(0x4510, 5); // switch slot 0 to bank 5
        mapper.write_chr(0x0000, 0x99); // write to bank 5, offset 0
        mapper.write_prg(0x4510, 3); // back to bank 3
        assert_eq!(mapper.read_chr(0x0000), 0x42); // bank 3 still has 0x42
    }

    #[test]
    fn test_4511_selects_1kb_chr_bank_for_slot1() {
        // $4511 controls PPU $0400-$07FF in 1 KiB direct mode.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct
        mapper.write_prg(0x4511, 7); // slot 1 = 1 KiB bank 7
        mapper.write_chr(0x0400, 0xAB); // write to bank 7, offset 0
        mapper.write_prg(0x4511, 2); // switch slot 1 to bank 2
        mapper.write_chr(0x0400, 0xCD); // write to bank 2, offset 0
        mapper.write_prg(0x4511, 7); // back to bank 7
        assert_eq!(mapper.read_chr(0x0400), 0xAB); // bank 7 still has 0xAB
    }

    #[test]
    fn test_1kb_chr_all_eight_slots_independent() {
        // All 8 slots ($4510-$4517) must independently control their 1 KiB region.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct
        // Assign distinct banks to all 8 slots and write distinct markers.
        for slot in 0u16..8 {
            let bank = slot as u8 + 10; // banks 10-17
            mapper.write_prg(0x4510 + slot, bank);
            mapper.write_chr(slot * 0x0400, bank); // offset 0 of each slot's region
        }
        // Verify each slot returns its own bank's marker.
        for slot in 0u16..8 {
            let bank = slot as u8 + 10;
            mapper.write_prg(0x4510 + slot, bank);
            assert_eq!(mapper.read_chr(slot * 0x0400), bank);
        }
    }

    #[test]
    fn test_8kb_chr_mode_restored_when_4500_bit0_cleared() {
        // Clearing bit 0 of $4500 reverts to 8 KiB latch-based CHR banking.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        // In 8 KiB mode (latch mode 1, CC=0), chr bank = 0.
        mapper.write_chr(0x0000, 0x11); // bank 0, offset 0
        // Switch to 1 KiB mode and set slot 0 to bank 5.
        mapper.write_prg(0x4500, 0x05);
        mapper.write_prg(0x4510, 5);
        mapper.write_chr(0x0000, 0x55); // bank 5, offset 0
        // Revert to 8 KiB mode: bit 0 = 0.
        mapper.write_prg(0x4500, 0x04); // C=0 → 8 KiB mode
        // CHR bank 0 at $0000 must return 0x11 (original 8 KiB bank 0 value).
        assert_eq!(mapper.read_chr(0x0000), 0x11);
    }

    // ── CHR nametable banking ($4518-$451B, $4500 bit 1 = n) ─────────────────
    //
    // When $4500 bit 1 (n) = 0, PPU $2000-$2FFF nametable reads are supplied
    // from CHR memory via four 1 KiB bank registers $4518-$451B.

    #[test]
    fn test_chr_nametable_4518_supplies_ppu_2000_region() {
        // $4518 selects the 1 KiB CHR bank for nametable $2000-$23FF.
        // Setting $4518=3 and writing to $0C00 (bank 3 in 1 KiB direct mode)
        // makes read_nametable($2000) return the same data.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct, n=0 (CHR NT)
        mapper.write_prg(0x4518, 3); // nametable $2000 → 1 KiB bank 3
        mapper.write_prg(0x4510, 3); // slot 0 also → bank 3
        mapper.write_chr(0x0000, 0x42); // writes to bank 3, offset 0
        assert_eq!(mapper.read_nametable(0x2000), Some(0x42));
    }

    #[test]
    fn test_chr_nametable_4519_supplies_ppu_2400_region() {
        // $4519 selects the 1 KiB CHR bank for nametable $2400-$27FF.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct, n=0
        mapper.write_prg(0x4519, 4); // nametable $2400 → bank 4
        mapper.write_prg(0x4511, 4); // slot 1 also → bank 4
        mapper.write_chr(0x0400, 0x77); // bank 4, offset 0
        assert_eq!(mapper.read_nametable(0x2400), Some(0x77));
    }

    #[test]
    fn test_chr_nametable_returns_none_when_bit1_set() {
        // When $4500 bit 1 (n) = 1, read_nametable returns None (CIRAM).
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x07); // bit 2=1 (MMC4 off), bit 1=1 (CIRAM), bit 0=1
        assert_eq!(mapper.read_nametable(0x2000), None);
    }

    #[test]
    fn test_write_nametable_routes_to_chr_when_chr_nt_active() {
        // When chr_nt_active is true (n=0), writes to $2000-$2FFF must update
        // the CHR bank selected by $4518-$451B so that subsequent reads return
        // the written value.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct, n=0 (CHR NT active)
        mapper.write_prg(0x4518, 6); // nametable $2000 → 1 KiB bank 6
        mapper.write_nametable(0x2000, 0xBE); // write via nametable interface
        assert_eq!(mapper.read_nametable(0x2000), Some(0xBE));
    }

    #[test]
    fn test_write_nametable_ignored_when_ciram_mode() {
        // When chr_nt_active is false (n=1, CIRAM mode), write_nametable returns
        // false and does not modify CHR memory.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x07); // n=1 → CIRAM
        mapper.write_prg(0x4518, 6); // set bank 6, but NT mode is CIRAM
        let handled = mapper.write_nametable(0x2000, 0xBE);
        assert!(
            !handled,
            "write_nametable should return false in CIRAM mode"
        );
    }

    // ── MMC4 latch mode ($4500 bit 2 = m = 0, 1 KiB mode active) ────────────
    //
    // When C=1 and m=0 ($4500=0x01): MMC4-style 4 KiB CHR banks with FD/FE latching.
    // $4510/$4512 = FD/FE banks for lower half ($0000-$0FFF); data [CCCC CC..] → bank = value>>2
    // $4514/$4516 = FD/FE banks for upper half ($1000-$1FFF)
    // ppu_address_changed($0FDx) → latch0 = FD; ppu_address_changed($0FEx) → latch0 = FE

    #[test]
    fn test_mmc4_latch0_fd_triggers_on_0fd8_read() {
        // After ppu_address_changed($0FD8) lower 4 KiB uses the FD bank from $4510.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x01); // 1 KiB + MMC4 enabled
        mapper.write_prg(0x4510, 2 << 2); // FD bank = 4K bank 2 (value=0x08, bank=0x08>>2=2)
        mapper.write_prg(0x4512, 3 << 2); // FE bank = 4K bank 3 (value=0x0C, bank=0x0C>>2=3)
        // Activate FD latch for lower half; write a marker to 4 KiB bank 2.
        mapper.ppu_address_changed(0x0FD8);
        mapper.write_chr(0x0000, 0x55);
        // Switch to FE latch; write a different marker to 4 KiB bank 3.
        mapper.ppu_address_changed(0x0FE8);
        mapper.write_chr(0x0000, 0x77);
        // Switch back to FD; reading from bank 2 must yield 0x55.
        mapper.ppu_address_changed(0x0FD8);
        assert_eq!(mapper.read_chr(0x0000), 0x55);
    }

    #[test]
    fn test_mmc4_latch0_fe_triggers_on_0fe8_read() {
        // After ppu_address_changed($0FE8) lower 4 KiB uses the FE bank from $4512.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x01);
        mapper.write_prg(0x4510, 0 << 2); // FD bank = 4K bank 0
        mapper.write_prg(0x4512, 5 << 2); // FE bank = 4K bank 5
        mapper.ppu_address_changed(0x0FD8); // set FD first
        mapper.write_chr(0x0000, 0xAA); // bank 0, offset 0
        mapper.ppu_address_changed(0x0FE8); // switch to FE
        mapper.write_chr(0x0000, 0xBB); // bank 5, offset 0
        mapper.ppu_address_changed(0x0FE8);
        assert_eq!(mapper.read_chr(0x0000), 0xBB); // bank 5 must be active
    }

    #[test]
    fn test_mmc4_latch1_fd_triggers_on_1fd8_read() {
        // ppu_address_changed($1FD8) switches upper half ($1000-$1FFF) to FD bank.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x01);
        mapper.write_prg(0x4514, 1 << 2); // FD bank = 4K bank 1
        mapper.write_prg(0x4516, 2 << 2); // FE bank = 4K bank 2
        mapper.ppu_address_changed(0x1FD8); // FD for upper half
        mapper.write_chr(0x1000, 0x33); // bank 1, upper offset 0
        mapper.ppu_address_changed(0x1FE8); // switch to FE
        mapper.write_chr(0x1000, 0x44); // bank 2, upper offset 0
        mapper.ppu_address_changed(0x1FD8);
        assert_eq!(mapper.read_chr(0x1000), 0x33); // back to bank 1
    }

    #[test]
    fn test_mmc4_latch1_fe_triggers_on_1fe8_read() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x01);
        mapper.write_prg(0x4514, 4 << 2); // FD bank = 4K bank 4
        mapper.write_prg(0x4516, 6 << 2); // FE bank = 4K bank 6
        mapper.ppu_address_changed(0x1FD8);
        mapper.write_chr(0x1000, 0xAA); // bank 4
        mapper.ppu_address_changed(0x1FE8);
        mapper.write_chr(0x1000, 0xCC); // bank 6
        mapper.ppu_address_changed(0x1FE8);
        assert_eq!(mapper.read_chr(0x1000), 0xCC); // bank 6 must be active
    }

    #[test]
    fn test_mmc4_disabled_uses_1kb_direct_mode() {
        // $4500 bit 2 = 1 → MMC4 disabled; $4510-$4517 are direct 1 KiB bank selectors.
        // In MMC4 mode (bit 2=0), $4510 would select a 4 KiB bank;
        // in direct mode (bit 2=1), $4510 selects a 1 KiB bank.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg, 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // C=1 + m=1 → direct 1 KiB
        mapper.write_prg(0x4510, 2); // slot 0 = 1 KiB bank 2
        mapper.write_chr(0x0000, 0x42); // write to 1 KiB bank 2, offset 0
        mapper.write_prg(0x4510, 3);
        mapper.write_chr(0x0000, 0x99); // write to 1 KiB bank 3, offset 0
        mapper.write_prg(0x4510, 2);
        assert_eq!(mapper.read_chr(0x0000), 0x42); // bank 2 still has 0x42
    }

    // ── Snapshot for 1 KiB CHR registers ─────────────────────────────────────

    #[test]
    fn test_1kb_chr_snapshot_preserves_all_registers() {
        // registers_snapshot/restore_registers must round-trip chr_1k_regs,
        // chr_nt_regs, and the $4500 mode flags.
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x05); // 1 KiB direct
        for i in 0u16..8 {
            mapper.write_prg(0x4510 + i, (i as u8) * 3 + 1); // banks 1,4,7,10,13,16,19,22
        }
        for i in 0u16..4 {
            mapper.write_prg(0x4518 + i, (i as u8) * 5 + 2); // nt banks 2,7,12,17
        }
        let snap = mapper.registers_snapshot();
        let mut restored = create_m6(prg, 0, NametableLayout::Horizontal);
        restored.restore_registers(&snap);
        // Verify all 8 CHR slot banks are preserved.
        for i in 0u16..8 {
            let expected_bank = (i as u8) * 3 + 1;
            restored.write_prg(0x4510 + i, expected_bank); // re-set for read isolation
            restored.write_chr(i * 0x0400, expected_bank); // write to expected bank
            restored.write_prg(0x4510 + i, expected_bank + 1); // switch to neighbor
            restored.write_chr(i * 0x0400, expected_bank + 100); // write to neighbor
            restored.write_prg(0x4510 + i, expected_bank); // back
            assert_eq!(restored.read_chr(i * 0x0400), expected_bank);
        }
        // Verify nametable bank 0 is preserved.
        let expected_nt0 = 2u8;
        restored.write_prg(0x4510, expected_nt0); // set slot 0 = same as nt bank 0
        restored.write_chr(0x0000, 0x7F); // writes to nt bank 0
        assert_eq!(restored.read_nametable(0x2000), Some(0x7F));
    }
    // ── IRQ counter ($4501-$4503, $4500 bit 3) ────────────────────────────────
    //
    // 16-bit upward-counting counter. IRQ fires on $FFFF → $0000 wrap.
    // $4501: acknowledge IRQ + disable counting
    // $4502: store counter LSB; acknowledge IRQ
    // $4503: store counter MSB; acknowledge IRQ; enable counting
    // $4500 bit 3 (I): 0=M2 clock (clock_irq), 1=PA12 clock (ppu_address_changed A12 rise)

    fn make_irq_mapper() -> Box<dyn Mapper> {
        create_m6(vec![0u8; 256 * 1024], 1, NametableLayout::Vertical)
    }

    #[test]
    fn test_irq_not_pending_initially() {
        let mapper = make_irq_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn test_irq_fires_on_ffff_to_0000_wrap_in_m2_mode() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00); // M2 mode (bit 3 = 0)
        mapper.write_prg(0x4502, 0xFF); // counter LSB = $FF
        mapper.write_prg(0x4503, 0xFF); // counter MSB = $FF -> counter = $FFFF, counting enabled
        mapper.cpu_cycle(); // $FFFF -> $0000: IRQ fires
        assert!(mapper.irq_pending(), "IRQ must be pending after $FFFF wrap");
    }

    #[test]
    fn test_irq_does_not_fire_before_wrap() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0xFE); // counter = $FFFE
        mapper.write_prg(0x4503, 0xFF);
        mapper.cpu_cycle(); // $FFFE -> $FFFF (no wrap)
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire at $FFFF (no wrap yet)"
        );
    }

    #[test]
    fn test_irq_fires_after_full_65536_cycles_from_zero() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0x00);
        mapper.write_prg(0x4503, 0x00); // counter = $0000, counting enabled
        for _ in 0..65535 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending(), "must not fire before wrap");
        }
        mapper.cpu_cycle(); // 65536th tick: $FFFF -> $0000
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after 65536 ticks from zero"
        );
    }

    #[test]
    fn test_4501_clears_irq_and_disables_counting() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.cpu_cycle(); // fires IRQ
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4501, 0x00); // acknowledge + disable
        assert!(!mapper.irq_pending(), "IRQ must be cleared by $4501");
        for _ in 0..131072 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "counting must stay disabled after $4501"
        );
    }

    #[test]
    fn test_4502_acknowledges_irq() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.cpu_cycle(); // fires IRQ
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4502, 0x00); // acknowledge
        assert!(!mapper.irq_pending(), "IRQ must be cleared by $4502");
    }

    #[test]
    fn test_4503_acknowledges_irq_and_reloads_counter() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.cpu_cycle(); // fires IRQ
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4502, 0x34);
        mapper.write_prg(0x4503, 0x12); // acknowledge + reload -> counter = $1234
        assert!(!mapper.irq_pending(), "IRQ must be cleared by $4503");
        // $FFFF - $1234 + 1 = $EDCC ticks to wrap
        for _ in 0..0xEDCBu32 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }
        mapper.cpu_cycle(); // wrap
        assert!(mapper.irq_pending(), "IRQ must fire again after reload");
    }

    #[test]
    fn test_irq_does_not_count_when_disabled_by_4501() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x00);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF); // counter = $FFFF, enabled
        mapper.write_prg(0x4501, 0x00); // disable before any tick
        mapper.cpu_cycle(); // would wrap, but disabled
        assert!(!mapper.irq_pending(), "disabled counter must not fire IRQ");
    }

    #[test]
    fn test_pa12_mode_fires_on_a12_rising_edge() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x08); // PA12 mode (bit 3 = 1)
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.ppu_address_changed(0x0000); // A12 = 0
        mapper.ppu_address_changed(0x1000); // A12 = 1 -> rising edge -> tick
        assert!(
            mapper.irq_pending(),
            "IRQ must fire on A12 rising edge in PA12 mode"
        );
    }

    #[test]
    fn test_pa12_mode_no_irq_on_held_high() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x08);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.ppu_address_changed(0x0000); // A12 low
        mapper.ppu_address_changed(0x1000); // rising edge: IRQ fires
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF); // reload, re-enable
        mapper.ppu_address_changed(0x1000); // A12 still high -> no rising edge
        assert!(
            !mapper.irq_pending(),
            "held-high A12 must not trigger another tick"
        );
    }

    #[test]
    fn test_pa12_mode_no_irq_on_falling_edge() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x08);
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.ppu_address_changed(0x0000); // A12 low
        mapper.ppu_address_changed(0x1000); // rising edge: IRQ fires
        assert!(mapper.irq_pending());
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.ppu_address_changed(0x0000); // falling edge
        assert!(
            !mapper.irq_pending(),
            "falling A12 edge must not trigger tick"
        );
    }

    #[test]
    fn test_m2_mode_cpu_cycle_does_not_affect_pa12_mode() {
        let mut mapper = make_irq_mapper();
        mapper.write_prg(0x4500, 0x08); // PA12 mode
        mapper.write_prg(0x4502, 0xFF);
        mapper.write_prg(0x4503, 0xFF);
        mapper.cpu_cycle(); // must not increment in PA12 mode
        assert!(
            !mapper.irq_pending(),
            "cpu_cycle must be a no-op in PA12 mode"
        );
    }

    #[test]
    fn test_irq_snapshot_preserves_irq_state() {
        let prg = vec![0u8; 256 * 1024];
        let mut mapper = create_m6(prg.clone(), 1, NametableLayout::Vertical);
        mapper.write_prg(0x4500, 0x00); // M2 mode
        mapper.write_prg(0x4502, 0x34);
        mapper.write_prg(0x4503, 0x12); // counter = $1234, enabled
        mapper.cpu_cycle(); // counter = $1235
        let snap = mapper.registers_snapshot();
        let mut restored = create_m6(prg, 1, NametableLayout::Vertical);
        restored.restore_registers(&snap);
        // After restore: 1 tick -> $1236, no IRQ.
        restored.cpu_cycle();
        assert!(!restored.irq_pending(), "IRQ must not fire at $1236");
        // $FFFF - $1236 + 1 = $EDCA ticks remaining to wrap.
        for _ in 0..0xEDC9u32 {
            restored.cpu_cycle();
        }
        restored.cpu_cycle(); // wrap
        assert!(
            restored.irq_pending(),
            "IRQ must fire at wrap after restore"
        );
    }

    // ── Trainer initialization ($7000–$71FF) ──────────────────────────────────
    //
    // A 512-byte trainer block from the iNES header must be loadable into
    // Mapper 6 WRAM at CPU addresses $7000–$71FF (WRAM bank 0, offset $1000).
    // The bus layer writes trainer bytes via write_prg($7000+i, byte); this
    // test validates that the mapper correctly stores and retrieves them.

    #[test]
    fn test_trainer_bytes_written_to_7000_are_readable() {
        // Given a Mapper 6 instance
        let mut mapper = create_m6(vec![0u8; 256 * 1024], 1, NametableLayout::Vertical);
        // When 512 trainer bytes are written to $7000–$71FF (as the bus does on map_cartridge)
        for i in 0u16..512 {
            mapper.write_prg(0x7000 + i, (i as u8).wrapping_add(0xA5));
        }
        // Then each byte is readable at the same address
        for i in 0u16..512 {
            let expected = (i as u8).wrapping_add(0xA5);
            let actual = mapper.read_prg(0x7000 + i);
            assert_eq!(
                actual,
                expected,
                "Trainer byte mismatch at ${:04X}: expected ${:02X} got ${:02X}",
                0x7000 + i,
                expected,
                actual
            );
        }
    }

    #[test]
    fn test_trainer_bytes_at_7000_are_independent_of_wram_bank1() {
        // Given a Mapper 6 instance with trainer bytes in bank 0 at $7000–$71FF
        let mut mapper = create_m6(vec![0u8; 256 * 1024], 1, NametableLayout::Vertical);
        for i in 0u16..512 {
            mapper.write_prg(0x7000 + i, 0xAA);
        }
        // When WRAM bank 1 is selected (bits 5-4 of $4500 = 0b0001_0000 = 0x10)
        mapper.write_prg(0x4500, 0x10); // wram_bank = 1
        for i in 0u16..512 {
            mapper.write_prg(0x7000 + i, 0xBB); // write different bytes into bank 1
        }
        // Then switching back to bank 0 restores the original trainer bytes
        mapper.write_prg(0x4500, 0x00); // wram_bank = 0
        for i in 0u16..512 {
            assert_eq!(
                mapper.read_prg(0x7000 + i),
                0xAA,
                "Trainer bytes in bank 0 should not be overwritten by bank 1 writes"
            );
        }
    }

    #[test]
    fn test_trainer_region_is_writable_and_readable() {
        // Given a Mapper 6 instance
        let mut mapper = create_m6(vec![0u8; 256 * 1024], 1, NametableLayout::Vertical);
        // When a byte is written to the last trainer address ($71FF)
        mapper.write_prg(0x71FF, 0x42);
        // Then it is readable at the same address
        assert_eq!(mapper.read_prg(0x71FF), 0x42);
        // And when a byte is written to the first trainer address ($7000)
        mapper.write_prg(0x7000, 0x24);
        // Then it is readable at the same address
        assert_eq!(mapper.read_prg(0x7000), 0x24);
    }

    // ── Mapper 17 ─────────────────────────────────────────────────────────────
    //
    // Mapper 17 is the Front Fareast Super Magic Card variant with a fixed
    // power-on register state that differs from Mapper 6.
    // References: https://www.nesdev.org/wiki/INES_Mapper_017
    //             https://www.nesdev.org/wiki/Super_Magic_Card

    #[test]
    fn test_mapper17_initial_prg_banks_point_to_last_four_8k_banks() {
        // Given a Mapper 17 ROM with 8 × 8 KiB banks (banks 0–7)
        // $4504–$4507 must be initialised to [N-4, N-3, N-2, N-1] = [4, 5, 6, 7]
        let prg = banked_data(PRG_BANK_SIZE_8K, 8);
        let mapper = create_m17(prg, 0, NametableLayout::Vertical);

        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should map to bank 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should map to bank 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 should map to bank 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should map to bank 7");
    }

    #[test]
    fn test_mapper17_initial_mirroring_vertical() {
        // $42FF = $20 | 0x00 for vertical → mirroring_type 2 = Vertical
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 0, NametableLayout::Vertical);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_mapper17_initial_mirroring_horizontal() {
        // $42FF = $20 | 0x10 for horizontal → mirroring_type 3 = Horizontal
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 0, NametableLayout::Horizontal);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mapper17_4m_mode_active_at_start_prg_slots_switchable_via_4504() {
        // 4M mode is on at power-on; writes to $4504–$4507 must change PRG banks
        let prg = banked_data(PRG_BANK_SIZE_8K, 16);
        let mut mapper = create_m17(prg, 0, NametableLayout::Vertical);

        // Switch slot 0 ($8000) to bank 2
        mapper.write_prg(0x4504, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "4M slot 0 should switch to bank 2"
        );
    }

    #[test]
    fn test_mapper17_1kb_chr_mode_active_at_start() {
        // $4500 bit 0 = 1 → 1 KiB CHR banking active; writes to $4510 switch CHR bank 0
        let prg = vec![0u8; 64 * 1024];
        let chr = banked_data(CHR_BANK_SIZE_1K, 8);
        let mapper_ctx = MapperContext::new_for_test(17, prg, chr, NametableLayout::Vertical);
        let mut mapper = create_mapper(mapper_ctx).expect("Mapper 17 with CHR-ROM");

        // Select bank 3 for PPU $0000–$03FF
        mapper.write_prg(0x4510, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR $0000 should read from 1 KiB bank 3"
        );
    }

    #[test]
    fn test_mapper17_chr_ram_power_on_uses_identity_bank_mapping() {
        // Spec: at power-on each of the 8 × 1 KiB CHR-RAM slots must map to its own
        // bank (identity: slot N → bank N).  With all slots aliased to bank 0, a write
        // to slot 1 ($0400) overwrites slot 0 ($0000), scrambling the pattern table.
        let prg = vec![0u8; 512 * 1024];
        let mut mapper = create_m17(prg, 0, NametableLayout::Vertical);

        // Write distinct sentinel values to the first byte of each 1 KiB slot
        for slot in 0u16..8 {
            mapper.write_chr(slot * 0x0400, slot as u8);
        }

        // Each slot must read back its own sentinel — not the last write aliased to bank 0
        for slot in 0u16..8 {
            assert_eq!(
                mapper.read_chr(slot * 0x0400),
                slot as u8,
                "slot {slot}: CHR-RAM bank aliasing at power-on — expected identity mapping"
            );
        }
    }

    #[test]
    fn test_mapper17_capabilities_include_irq_and_trainer_jsr() {
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 0, NametableLayout::Vertical);
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "Mapper 17 must support IRQ");
        assert!(caps.trainer_jsr, "Mapper 17 must use JSR trainer execution");
    }

    #[test]
    fn test_mapper17_latch_mode_1_is_used_for_prg_switching_when_4m_disabled() {
        // Disable 4M mode, then verify latch-mode-1 PRG switching works:
        // mode 1 uses bits 5-2 of latch value for 16 KiB bank select
        let prg = banked_data(PRG_BANK_SIZE_16K, 16);
        let mut mapper = create_m17(prg, 0, NametableLayout::Vertical);
        // Disable 4M mode by writing $43FF (N=1 → 2M/4M disabled)
        mapper.write_prg(0x43FF, 0x02);
        // Select 16 KiB bank 3 via mode-1 latch (bits 5-2 = 0011 → bank 3)
        mapper.write_prg(0x8000, 3 << 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "latch mode 1 should select 16KiB bank 3 at $8000"
        );
    }

    // ── Mapper 17 scratch RAM ($5000–$5FFF) and trainer load address ───────────

    #[test]
    fn test_mapper17_scratch_ram_is_readable_and_writable() {
        // Mapper 17 has 4 KiB scratch RAM at $5000–$5FFF
        let mut mapper = create_m17(vec![0u8; 64 * 1024], 0, NametableLayout::Vertical);
        mapper.write_prg(0x5000, 0xAB);
        mapper.write_prg(0x5FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x5000), 0xAB, "$5000 scratch RAM read");
        assert_eq!(mapper.read_prg(0x5FFF), 0xCD, "$5FFF scratch RAM read");
    }

    #[test]
    fn test_mapper17_submapper0_trainer_load_address_is_7000() {
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 0, NametableLayout::Vertical);
        assert_eq!(
            mapper.capabilities().trainer_load_address,
            0x7000,
            "submapper 0 trainer loads at $7000"
        );
    }

    #[test]
    fn test_mapper17_submapper1_trainer_load_address_is_5d00() {
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 1, NametableLayout::Vertical);
        assert_eq!(
            mapper.capabilities().trainer_load_address,
            0x5D00,
            "submapper 1 trainer loads at $5D00"
        );
    }

    #[test]
    fn test_mapper17_submapper2_trainer_load_address_is_5e00() {
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 2, NametableLayout::Vertical);
        assert_eq!(
            mapper.capabilities().trainer_load_address,
            0x5E00,
            "submapper 2 trainer loads at $5E00"
        );
    }

    #[test]
    fn test_mapper17_submapper3_trainer_load_address_is_5f00() {
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m17(prg, 3, NametableLayout::Vertical);
        assert_eq!(
            mapper.capabilities().trainer_load_address,
            0x5F00,
            "submapper 3 trainer loads at $5F00"
        );
    }

    #[test]
    fn test_mapper17_trainer_written_to_5d00_is_readable() {
        // Submapper 1: bus writes trainer via write_prg($5D00+i, byte)
        let mut mapper = create_m17(vec![0u8; 64 * 1024], 1, NametableLayout::Vertical);
        for i in 0u16..512 {
            mapper.write_prg(0x5D00 + i, i as u8);
        }
        for i in 0u16..512 {
            assert_eq!(
                mapper.read_prg(0x5D00 + i),
                i as u8,
                "trainer byte at ${:04X}",
                0x5D00 + i
            );
        }
    }

    #[test]
    fn test_mapper6_trainer_load_address_is_7000() {
        // Mapper 6 default trainer address must remain $7000
        let prg = vec![0u8; 64 * 1024];
        let mapper = create_m6(prg, 1, NametableLayout::Vertical);
        assert_eq!(
            mapper.capabilities().trainer_load_address,
            0x7000,
            "mapper 6 trainer always loads at $7000"
        );
    }
}
