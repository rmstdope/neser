//! ROM database module: CRC32 calculation and cartridge quirks.
//!
//! This module isolates ROM identification and quirk detection from mapper construction logic.

/// Calculate CRC32 of ROM data (PRG + CHR combined).
/// Uses the standard CRC-32 (ISO 3309) polynomial.
pub fn calculate_rom_crc32(prg_rom: &[u8], chr_rom: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut j = 0;
            while j < 8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
                j += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };

    let mut crc = 0xFFFFFFFFu32;
    for &byte in prg_rom.iter().chain(chr_rom.iter()) {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    // crate::debugging::log_info(format!("Calculated CRC32: {:08X}", crc ^ 0xFFFFFFFF));
    !crc
}

/// CRC32 values for ROMs that require alternate (NEC) MMC3 IRQ behavior.
const MMC3_ALTERNATE_IRQ_CRCS: &[u32] = &[
    0x633AFE6F, // 6-MMC3_alt.nes (blargg mmc3_test_2)
    0xF312D1DE, // 5.MMC3_rev_A.nes (blargg mmc3_irq_tests)
    0xA512BDF6, // 6-MMC6.nes
];

/// CRC32 values for ROMs that default to Arkanoid controller input on port 2.
const ARKANOID_PADDLE_PORT2_CRCS: &[u32] = &[
    0x32FB0583, // Arkanoid (NES, 1987)
];

/// CRC32 values for ROMs that default to Arkanoid controller input on port 1.
const ARKANOID_PADDLE_PORT1_CRCS: &[u32] = &[
    0x47F9F410, // PaddleTest3
];

/// CRC32 values for ROMs that default to Zapper input on port 2.
const ZAPPER_PORT2_CRCS: &[u32] = &[
    0x24598791, // Duck Hunt (World)
    0xFF24D794, // Hogan's Alley (World)
];

/// Check if a ROM CRC requires alternate MMC3 IRQ behavior.
pub fn requires_mmc3_alternate_irq(crc: u32) -> bool {
    MMC3_ALTERNATE_IRQ_CRCS.contains(&crc)
}

/// Return the default Arkanoid controller port for a ROM CRC.
///
/// Returns 0 for none, 1 for port 1, 2 for port 2.
pub fn default_arkanoid_on_port(crc: u32) -> u8 {
    if ARKANOID_PADDLE_PORT1_CRCS.contains(&crc) {
        1
    } else if ARKANOID_PADDLE_PORT2_CRCS.contains(&crc) {
        2
    } else {
        0
    }
}

/// Return the default Zapper controller port for a ROM CRC.
///
/// Returns 0 for none, 2 for port 2.
pub fn default_zapper_on_port(crc: u32) -> u8 {
    if ZAPPER_PORT2_CRCS.contains(&crc) {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_rom_crc32_empty() {
        let crc = calculate_rom_crc32(&[], &[]);
        assert_eq!(crc, 0);
    }

    #[test]
    fn test_calculate_rom_crc32_with_data() {
        // Test with some sample data
        let prg_rom = vec![0x4E, 0x45, 0x53, 0x1A]; // "NES" header start
        let chr_rom = vec![0xFF, 0x00, 0xFF, 0x00];
        let crc = calculate_rom_crc32(&prg_rom, &chr_rom);
        // Verify the exact CRC32 value for this specific input
        assert_eq!(crc, 0xA26D5B91);
    }

    #[test]
    fn test_mmc3_alternate_irq_known_crcs() {
        // Known CRCs that require alternate IRQ
        assert!(requires_mmc3_alternate_irq(0x633AFE6F));
        assert!(requires_mmc3_alternate_irq(0xF312D1DE));
    }

    #[test]
    fn test_mmc3_alternate_irq_unknown_crc() {
        // Random CRC should not require alternate IRQ
        assert!(!requires_mmc3_alternate_irq(0x12345678));
    }

    #[test]
    fn test_arkanoid_paddle_known_crcs() {
        assert_eq!(default_arkanoid_on_port(0x32FB0583), 2);
        assert_eq!(default_arkanoid_on_port(0x47F9F410), 1);
    }

    #[test]
    fn test_arkanoid_paddle_unknown_crc() {
        assert_eq!(default_arkanoid_on_port(0xDEADBEEF), 0);
    }

    #[test]
    fn test_zapper_known_crc() {
        assert_eq!(default_zapper_on_port(0x24598791), 2);
    }

    #[test]
    fn test_zapper_unknown_crc() {
        assert_eq!(default_zapper_on_port(0xDEADBEEF), 0);
    }
}
