use serde::{Deserialize, Serialize};

/// Pre-computed wait-state cycle counts for each memory region.
///
/// Each region stores `[N16, N32, S16, S32]` — non-sequential and sequential
/// access times for 16-bit and 32-bit widths. Updated when WAITCNT is written.
///
/// Game Pak WAITCNT fields store waitstates; actual access time is one clock
/// plus the configured waitstate count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Waitstates {
    /// Per-region cycle lookup: indexed by `(addr >> 24) & 0xF`, then by
    /// `[N16, N32, S16, S32]`.
    regions: [[u32; 4]; 16],
    /// Raw WAITCNT register value (writable bits 0-14).
    pub waitcnt: u16,
    /// Whether the prefetch buffer is enabled (bit 14 of WAITCNT).
    pub prefetch_enabled: bool,
}

/// Indices into the per-region `[N16, N32, S16, S32]` array.
const N16: usize = 0;
const N32: usize = 1;
const S16: usize = 2;
const S32: usize = 3;

/// LUT for SRAM and ROM non-sequential wait states (2-bit index → cycles).
const WAIT_N_LUT: [u32; 4] = [4, 3, 2, 8];

/// LUT for ROM WS0 sequential access (1-bit index → cycles).
const WAIT_S0_LUT: [u32; 2] = [2, 1];
/// LUT for ROM WS1 sequential access (1-bit index → cycles).
const WAIT_S1_LUT: [u32; 2] = [4, 1];
/// LUT for ROM WS2 sequential access (1-bit index → cycles).
const WAIT_S2_LUT: [u32; 2] = [8, 1];

impl Default for Waitstates {
    fn default() -> Self {
        Self::new()
    }
}

impl Waitstates {
    /// Create with power-on defaults (WAITCNT = 0x0000).
    pub fn new() -> Self {
        let mut ws = Self {
            regions: [[1; 4]; 16],
            waitcnt: 0,
            prefetch_enabled: false,
        };
        ws.recalculate(0);
        ws
    }

    /// Recalculate all region timings from a new WAITCNT value.
    pub fn recalculate(&mut self, waitcnt: u16) {
        self.waitcnt = waitcnt & 0x5FFF; // mask unused bits 13, 15
        self.prefetch_enabled = waitcnt & (1 << 14) != 0;

        // Fixed regions (not affected by WAITCNT)
        let bios = [1, 1, 1, 1];
        let ewram = [3, 6, 3, 6];
        let iwram = [1, 1, 1, 1];
        let io = [1, 1, 1, 1];
        let pram = [1, 2, 1, 2];
        let vram = [1, 2, 1, 2];
        let oam = [1, 1, 1, 1];

        self.regions[0x0] = bios;
        self.regions[0x1] = bios; // mirror / unused
        self.regions[0x2] = ewram;
        self.regions[0x3] = iwram;
        self.regions[0x4] = io;
        self.regions[0x5] = pram;
        self.regions[0x6] = vram;
        self.regions[0x7] = oam;

        // SRAM wait (bits 0-1)
        let sram_n16 = WAIT_N_LUT[(waitcnt & 0x3) as usize] + 1;
        // SRAM is always non-sequential (8-bit bus), 32-bit = 2×N16 + 1
        let sram_n32 = 2 * sram_n16 + 1;
        let sram = [sram_n16, sram_n32, sram_n16, sram_n32];
        self.regions[0xE] = sram;
        self.regions[0xF] = sram;

        // WS0 (bits 2-4): regions 0x8, 0x9
        let ws0_n16 = WAIT_N_LUT[((waitcnt >> 2) & 0x3) as usize] + 1;
        let ws0_s16 = WAIT_S0_LUT[((waitcnt >> 4) & 0x1) as usize] + 1;
        let ws0_n32 = ws0_n16 + ws0_s16;
        let ws0_s32 = 2 * ws0_s16;
        let ws0 = [ws0_n16, ws0_n32, ws0_s16, ws0_s32];
        self.regions[0x8] = ws0;
        self.regions[0x9] = ws0;

        // WS1 (bits 5-7): regions 0xA, 0xB
        let ws1_n16 = WAIT_N_LUT[((waitcnt >> 5) & 0x3) as usize] + 1;
        let ws1_s16 = WAIT_S1_LUT[((waitcnt >> 7) & 0x1) as usize] + 1;
        let ws1_n32 = ws1_n16 + ws1_s16;
        let ws1_s32 = 2 * ws1_s16;
        let ws1 = [ws1_n16, ws1_n32, ws1_s16, ws1_s32];
        self.regions[0xA] = ws1;
        self.regions[0xB] = ws1;

        // WS2 (bits 8-10): regions 0xC, 0xD
        let ws2_n16 = WAIT_N_LUT[((waitcnt >> 8) & 0x3) as usize] + 1;
        let ws2_s16 = WAIT_S2_LUT[((waitcnt >> 10) & 0x1) as usize] + 1;
        let ws2_n32 = ws2_n16 + ws2_s16;
        let ws2_s32 = 2 * ws2_s16;
        let ws2 = [ws2_n16, ws2_n32, ws2_s16, ws2_s32];
        self.regions[0xC] = ws2;
        self.regions[0xD] = ws2;
    }

    /// Non-sequential cycle count for a given address and width.
    #[inline]
    pub fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let region = ((addr >> 24) & 0xF) as usize;
        match width {
            WidthClass::HalfwordOrByte => self.regions[region][N16],
            WidthClass::Word => self.regions[region][N32],
        }
    }

    /// Sequential cycle count for a given address and width.
    #[inline]
    pub fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let region = ((addr >> 24) & 0xF) as usize;
        match width {
            WidthClass::HalfwordOrByte => self.regions[region][S16],
            WidthClass::Word => self.regions[region][S32],
        }
    }
}

/// Access width for [`GbaBus::n_cycles`] / [`GbaBus::s_cycles`] cycle stubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    /// 8-bit or 16-bit access.
    HalfwordOrByte,
    /// 32-bit access (counts as two halfword accesses on 16-bit buses).
    Word,
}
