//! SNES memory-access speed classifier.
//!
//! Each CPU memory access takes a fixed number of master clock cycles depending
//! on the target address and the MEMSEL register ($420D bit 0).
//!
//! Source: fullsnes – "SNES Memory Control" and "SNES Memory Map".
//!
//! Speed summary (21.477272 MHz master clock):
//!  - **Fast  (6 master clocks, 3.58 MHz)**: B-Bus I/O $2000–$3FFF, CPU I/O
//!    $4200–$5FFF; WS2 ROM ($80–$BF:$8000–$FFFF, $C0–$FF:$0000–$FFFF) when
//!    MEMSEL=1.
//!  - **Slow  (8 master clocks, 2.68 MHz)**: WRAM ($7E–$7F), WRAM mirrors
//!    ($00–$3F/$80–$BF:$0000–$1FFF), Expansion ($6000–$7FFF), WS1 ROM
//!    ($00–$3F:$8000–$FFFF, $40–$7D:$0000–$FFFF); WS2 ROM when MEMSEL=0.
//!  - **XSlow (12 master clocks, 1.78 MHz)**: Manual joypad I/O
//!    ($00–$3F/$80–$BF:$4000–$41FF).

/// Number of master clock cycles consumed by one CPU bus access.
///
/// # Arguments
/// * `addr`     – 24-bit bus address (upper 8 bits = bank, lower 16 = offset)
/// * `fast_rom` – value of MEMSEL $420D bit 0 (true = WS2 ROM runs at 3.58 MHz)
///
/// # Returns
/// 6, 8, or 12 master clock cycles.
pub fn mem_access_cycles(addr: u32, fast_rom: bool) -> u8 {
    let bank = (addr >> 16) as u8;
    let offset = (addr & 0xFFFF) as u16;

    match bank {
        // ---- Banks $00–$3F and $80–$BF: System Area + WS ROM ----
        0x00..=0x3F | 0x80..=0xBF => match offset {
            0x0000..=0x1FFF => 8,  // WRAM mirror
            0x2000..=0x3FFF => 6,  // B-Bus I/O (PPU, APU)
            0x4000..=0x41FF => 12, // Manual joypad (XSlow)
            0x4200..=0x5FFF => 6,  // CPU I/O registers
            0x6000..=0x7FFF => 8,  // Expansion
            0x8000..=0xFFFF => {
                // WS1 ($00–$3F) is always slow; WS2 ($80–$BF) depends on MEMSEL
                if bank >= 0x80 && fast_rom { 6 } else { 8 }
            }
        },

        // ---- Banks $40–$7D: WS1 HiROM (always slow) ----
        0x40..=0x7D => 8,

        // ---- Banks $7E–$7F: WRAM ----
        0x7E..=0x7F => 8,

        // ---- Banks $C0–$FF: WS2 HiROM (speed depends on MEMSEL) ----
        0xC0..=0xFF => {
            if fast_rom {
                6
            } else {
                8
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // System Area – banks $00–$3F (MEMSEL has no effect here except WS2 ROM)
    // -------------------------------------------------------------------------

    #[test]
    fn bank00_wram_mirror_is_slow() {
        assert_eq!(mem_access_cycles(0x00_0000, false), 8);
        assert_eq!(mem_access_cycles(0x00_1FFF, false), 8);
    }

    #[test]
    fn bank00_bbus_io_is_fast() {
        assert_eq!(mem_access_cycles(0x00_2000, false), 6);
        assert_eq!(mem_access_cycles(0x00_2100, false), 6); // PPU
        assert_eq!(mem_access_cycles(0x00_2140, false), 6); // APU
        assert_eq!(mem_access_cycles(0x00_3FFF, false), 6);
    }

    #[test]
    fn bank00_joypad_io_is_xslow() {
        assert_eq!(mem_access_cycles(0x00_4000, false), 12);
        assert_eq!(mem_access_cycles(0x00_4016, false), 12); // JOYA
        assert_eq!(mem_access_cycles(0x00_41FF, false), 12);
    }

    #[test]
    fn bank00_cpu_io_is_fast() {
        assert_eq!(mem_access_cycles(0x00_4200, false), 6); // NMITIMEN
        assert_eq!(mem_access_cycles(0x00_420D, false), 6); // MEMSEL itself
        assert_eq!(mem_access_cycles(0x00_5FFF, false), 6);
    }

    #[test]
    fn bank00_expansion_is_slow() {
        assert_eq!(mem_access_cycles(0x00_6000, false), 8);
        assert_eq!(mem_access_cycles(0x00_7FFF, false), 8);
    }

    #[test]
    fn bank00_ws1_rom_is_always_slow() {
        // WS1: MEMSEL has NO effect on banks $00–$3F
        assert_eq!(mem_access_cycles(0x00_8000, false), 8);
        assert_eq!(mem_access_cycles(0x00_FFFF, false), 8);
        assert_eq!(mem_access_cycles(0x00_8000, true), 8);
        assert_eq!(mem_access_cycles(0x00_FFFF, true), 8);
    }

    #[test]
    fn bank3f_system_area_matches_bank00() {
        assert_eq!(mem_access_cycles(0x3F_0000, false), 8); // WRAM mirror
        assert_eq!(mem_access_cycles(0x3F_2100, false), 6); // I/O
        assert_eq!(mem_access_cycles(0x3F_4016, false), 12); // joypad
        assert_eq!(mem_access_cycles(0x3F_8000, false), 8); // WS1 ROM
    }

    // -------------------------------------------------------------------------
    // WS1 HiROM – banks $40–$7D
    // -------------------------------------------------------------------------

    #[test]
    fn banks_40_to_7d_are_always_slow() {
        assert_eq!(mem_access_cycles(0x40_0000, false), 8);
        assert_eq!(mem_access_cycles(0x40_0000, true), 8);
        assert_eq!(mem_access_cycles(0x7D_FFFF, false), 8);
        assert_eq!(mem_access_cycles(0x7D_FFFF, true), 8);
    }

    // -------------------------------------------------------------------------
    // WRAM – banks $7E–$7F
    // -------------------------------------------------------------------------

    #[test]
    fn wram_banks_7e_7f_are_slow() {
        assert_eq!(mem_access_cycles(0x7E_0000, false), 8);
        assert_eq!(mem_access_cycles(0x7E_FFFF, false), 8);
        assert_eq!(mem_access_cycles(0x7F_0000, false), 8);
        assert_eq!(mem_access_cycles(0x7F_FFFF, false), 8);
        // MEMSEL has no effect on WRAM
        assert_eq!(mem_access_cycles(0x7E_0000, true), 8);
    }

    // -------------------------------------------------------------------------
    // System Area – banks $80–$BF
    // -------------------------------------------------------------------------

    #[test]
    fn bank80_wram_mirror_is_slow() {
        assert_eq!(mem_access_cycles(0x80_0000, false), 8);
        assert_eq!(mem_access_cycles(0x80_1FFF, false), 8);
    }

    #[test]
    fn bank80_bbus_io_is_fast() {
        assert_eq!(mem_access_cycles(0x80_2000, false), 6);
        assert_eq!(mem_access_cycles(0x80_3FFF, false), 6);
    }

    #[test]
    fn bank80_joypad_io_is_xslow() {
        assert_eq!(mem_access_cycles(0x80_4000, false), 12);
        assert_eq!(mem_access_cycles(0x80_41FF, false), 12);
    }

    #[test]
    fn bank80_cpu_io_is_fast() {
        assert_eq!(mem_access_cycles(0x80_4200, false), 6);
        assert_eq!(mem_access_cycles(0x80_5FFF, false), 6);
    }

    #[test]
    fn bank80_expansion_is_slow() {
        assert_eq!(mem_access_cycles(0x80_6000, false), 8);
        assert_eq!(mem_access_cycles(0x80_7FFF, false), 8);
    }

    #[test]
    fn bank80_ws2_rom_slow_when_memsel_off() {
        assert_eq!(mem_access_cycles(0x80_8000, false), 8);
        assert_eq!(mem_access_cycles(0xBF_FFFF, false), 8);
    }

    #[test]
    fn bank80_ws2_rom_fast_when_memsel_on() {
        assert_eq!(mem_access_cycles(0x80_8000, true), 6);
        assert_eq!(mem_access_cycles(0xBF_FFFF, true), 6);
    }

    #[test]
    fn ws2_rom_speed_unaffected_by_memsel_in_system_area() {
        // MEMSEL only affects $8000–$FFFF portion of $80–$BF
        assert_eq!(mem_access_cycles(0x80_0000, true), 8); // WRAM mirror: always 8
        assert_eq!(mem_access_cycles(0x80_6000, true), 8); // Expansion: always 8
    }

    // -------------------------------------------------------------------------
    // WS2 HiROM – banks $C0–$FF
    // -------------------------------------------------------------------------

    #[test]
    fn ws2_hirom_slow_when_memsel_off() {
        assert_eq!(mem_access_cycles(0xC0_0000, false), 8);
        assert_eq!(mem_access_cycles(0xFF_FFFF, false), 8);
    }

    #[test]
    fn ws2_hirom_fast_when_memsel_on() {
        assert_eq!(mem_access_cycles(0xC0_0000, true), 6);
        assert_eq!(mem_access_cycles(0xFF_FFFF, true), 6);
    }

    // -------------------------------------------------------------------------
    // MEMSEL toggling
    // -------------------------------------------------------------------------

    #[test]
    fn memsel_toggles_ws2_rom_speed_independently_of_ws1() {
        // WS1 ($00:$8000) stays slow regardless of MEMSEL
        assert_eq!(mem_access_cycles(0x00_8000, false), 8);
        assert_eq!(mem_access_cycles(0x00_8000, true), 8);
        // WS2 ($80:$8000) changes with MEMSEL
        assert_eq!(mem_access_cycles(0x80_8000, false), 8);
        assert_eq!(mem_access_cycles(0x80_8000, true), 6);
        // WS2 HiROM ($C0:$0000) changes with MEMSEL
        assert_eq!(mem_access_cycles(0xC0_0000, false), 8);
        assert_eq!(mem_access_cycles(0xC0_0000, true), 6);
    }
}
