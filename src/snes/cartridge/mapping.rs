use serde::{Deserialize, Serialize};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mapping {
    LoRom,
    HiRom,
    ExHiRom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MappingCandidate {
    pub mapping: Mapping,
    pub header_offset: usize,
    pub score: i32,
}

const HEADER_SIZE: usize = 0x100;
const LOROM_HEADER_OFFSET: usize = 0x7FC0;
const HIROM_HEADER_OFFSET: usize = 0xFFC0;
const EXHIROM_HEADER_OFFSET: usize = 0x40FFC0;
const MIN_VALID_SCORE: i32 = 25;

pub(crate) fn detect_mapping(rom: &[u8]) -> Option<MappingCandidate> {
    let mut best: Option<MappingCandidate> = None;
    for (mapping, header_offset) in [
        (Mapping::LoRom, LOROM_HEADER_OFFSET),
        (Mapping::HiRom, HIROM_HEADER_OFFSET),
        (Mapping::ExHiRom, EXHIROM_HEADER_OFFSET),
    ] {
        if header_offset + HEADER_SIZE > rom.len() {
            continue;
        }
        let score = score_candidate(rom, mapping, header_offset);
        let candidate = MappingCandidate {
            mapping,
            header_offset,
            score,
        };
        match best {
            None => best = Some(candidate),
            Some(current) => {
                if candidate.score > current.score
                    || (candidate.score == current.score
                        && mapping_priority(candidate.mapping) > mapping_priority(current.mapping))
                {
                    best = Some(candidate);
                }
            }
        }
    }
    best.filter(|candidate| candidate.score >= MIN_VALID_SCORE)
}

fn score_candidate(rom: &[u8], mapping: Mapping, header_offset: usize) -> i32 {
    let map_mode = rom[header_offset + 0x15];
    let checksum_complement =
        u16::from_le_bytes([rom[header_offset + 0x1C], rom[header_offset + 0x1D]]);
    let checksum = u16::from_le_bytes([rom[header_offset + 0x1E], rom[header_offset + 0x1F]]);
    let reset_vector = u16::from_le_bytes([rom[header_offset + 0x3C], rom[header_offset + 0x3D]]);
    let rom_size_field = rom[header_offset + 0x17];

    let mut score = 0;

    if mapping_mode_matches(map_mode, mapping) {
        score += 30;
    }

    if checksum.wrapping_add(checksum_complement) == 0xFFFF {
        score += 20;
    }

    if reset_vector >= 0x8000 {
        score += 15;
    } else if reset_vector != 0 {
        score += 5;
    }

    if rom_size_is_sane(rom_size_field, rom.len()) {
        score += 10;
    }

    score
}

fn rom_size_is_sane(rom_size_field: u8, actual_len: usize) -> bool {
    let exp = usize::from(rom_size_field);
    if exp >= usize::BITS as usize {
        return false;
    }
    let Some(kib) = 1usize.checked_shl(exp as u32) else {
        return false;
    };
    let Some(bytes) = kib.checked_mul(1024) else {
        return false;
    };
    let lower_bound = actual_len / 8;
    bytes > 0 && bytes >= lower_bound && bytes <= actual_len.saturating_mul(2)
}

fn mapping_mode_matches(map_mode: u8, mapping: Mapping) -> bool {
    if map_mode & 0x20 == 0 {
        return false;
    }
    let mode_nibble = map_mode & 0x0F;
    match mapping {
        Mapping::LoRom => mode_nibble == 0x0,
        Mapping::HiRom => mode_nibble == 0x1,
        Mapping::ExHiRom => mode_nibble == 0x5,
    }
}

fn mapping_priority(mapping: Mapping) -> u8 {
    match mapping {
        Mapping::LoRom => 1,
        Mapping::HiRom => 2,
        Mapping::ExHiRom => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mapping_prefers_hirom_when_header_only_exists_at_ffc0() {
        let mut rom = vec![0u8; 0x20000];
        rom[0xFFC0 + 0x15] = 0x21;
        rom[0xFFC0 + 0x1C] = 0x34;
        rom[0xFFC0 + 0x1D] = 0x12;
        rom[0xFFC0 + 0x1E] = 0xCB;
        rom[0xFFC0 + 0x1F] = 0xED;
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0x80;

        let detected = detect_mapping(&rom).expect("candidate");
        assert_eq!(detected.mapping, Mapping::HiRom);
        assert_eq!(detected.header_offset, 0xFFC0);
    }

    #[test]
    fn detect_mapping_prefers_exhirom_when_exhirom_and_hirom_scores_tie() {
        let mut rom = vec![0u8; 0x500000];
        rom[0xFFC0 + 0x15] = 0x21;
        rom[0xFFFC] = 0x00;
        rom[0xFFFD] = 0x80;
        rom[0x40FFC0 + 0x15] = 0x35;
        rom[0x40FFFC] = 0x00;
        rom[0x40FFFD] = 0x80;

        let detected = detect_mapping(&rom).expect("candidate");
        assert_eq!(detected.mapping, Mapping::ExHiRom);
        assert_eq!(detected.header_offset, 0x40FFC0);
    }
}
