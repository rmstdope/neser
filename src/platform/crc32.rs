/// Compute a CRC-32 checksum over one or more byte slices.
///
/// Uses the standard CRC-32/ISO-HDLC polynomial (0xEDB88320, reflected).
/// This is the same algorithm used by zlib, gzip, and PNG.
pub fn crc32(slices: &[&[u8]]) -> u32 {
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
    for slice in slices {
        for &byte in *slice {
            let index = ((crc ^ byte as u32) & 0xFF) as usize;
            crc = (crc >> 8) ^ CRC32_TABLE[index];
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(crc32(&[&[]]), 0x00000000);
    }

    #[test]
    fn test_crc32_known_value() {
        // "123456789" has well-known CRC-32 = 0xCBF43926
        let data = b"123456789";
        assert_eq!(crc32(&[data]), 0xCBF43926);
    }

    #[test]
    fn test_crc32_multiple_slices_equals_single() {
        let a = b"1234";
        let b = b"56789";
        assert_eq!(crc32(&[a, b]), crc32(&[b"123456789"]));
    }

    #[test]
    fn test_crc32_single_byte() {
        let data = [0x00];
        let result = crc32(&[&data]);
        assert_ne!(result, 0, "CRC of a single zero byte should be non-zero");
    }
}
