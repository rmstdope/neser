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
    !crc
}

/// CRC32 values for ROMs that require alternate (NEC) MMC3 IRQ behavior.
const MMC3_ALTERNATE_IRQ_CRCS: &[u32] = &[
    0x633AFE6F, // 6-MMC3_alt.nes (blargg mmc3_test_2)
    0xF312D1DE, // 5.MMC3_rev_A.nes (blargg mmc3_irq_tests)
];

/// Check if a ROM CRC requires alternate MMC3 IRQ behavior.
pub fn requires_mmc3_alternate_irq(crc: u32) -> bool {
    MMC3_ALTERNATE_IRQ_CRCS.contains(&crc)
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
        // CRC should be deterministic for this input
        assert_ne!(crc, 0);
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
}
