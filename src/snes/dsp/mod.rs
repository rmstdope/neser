//! The DSP-n chips as a cartridge feature: which DSP a cartridge carries, where its firmware
//! comes from, whether that firmware is genuine, and the words a player reads when it cannot
//! start.
//!
//! The header's chipset byte says only "DSP" (`$03-$05`); fullsnes ("SNES Cart DSP-n/ST010/
//! ST011") states that nothing in the header says which one, "except for using a list of known
//! Titles or Checksums". [`identify`] uses Mesen2's title list (`BaseCartridge::GetDspVersion`):
//! Pilotwings uses the original DSP-1, the known DSP-2/3/4 titles are those chips, and every
//! other DSP cartridge is a DSP-1B.
//!
//! The firmware is the chip's mask ROM, which game dumps lack and NESER cannot include. On the
//! desktop the player puts it in the `snes-firmware-dir` folder (`dsp1b.rom` or `dsp1.rom`,
//! `dsp2.rom`); the browser version hands it over through `Snes::set_dsp_firmware`.
//!
//! A chip is emulated exactly when [`FIRMWARE_FILES`] has a row for it: each row names one
//! file and the SHA-256 of that file's known good dump, and only a file matching its row
//! starts a game (nr-608).

use crate::snes::cartridge::{Cartridge, EnhancementChip};
use crate::snes::upd77c25::{DSP_IMAGE_SIZE, Upd77c25Firmware};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Which uPD77C25 program a DSP cartridge carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspModel {
    /// The original DSP-1 (and the identical DSP-1A).
    Dsp1,
    /// The bug-fixed DSP-1B.
    Dsp1B,
    Dsp2,
    Dsp3,
    Dsp4,
}

impl DspModel {
    /// The chip whose firmware the player supplies: DSP-1 and DSP-1B share one.
    pub fn chip(self) -> DspChip {
        match self {
            Self::Dsp1 | Self::Dsp1B => DspChip::Dsp1,
            Self::Dsp2 => DspChip::Dsp2,
            Self::Dsp3 => DspChip::Dsp3,
            Self::Dsp4 => DspChip::Dsp4,
        }
    }

    /// The chip's rows of `table` in the order this game tries the files: an original DSP-1
    /// game (Pilotwings) prefers `dsp1.rom`, every other game keeps the table order.
    pub fn firmware_files(self, table: FirmwareTable) -> Vec<&'static FirmwareFile> {
        let mut files: Vec<_> = table.iter().filter(|f| f.chip == self.chip()).collect();
        if self == Self::Dsp1 {
            files.sort_by_key(|f| f.name != DSP1_FILE);
        }
        files
    }

    /// The first bank of the LoROM `xx:8000-FFFF` DR/SR window (up to `$3F`, mirrored at
    /// `$80`). fullsnes "SNES Cart DSP-n" lists the 1 MB+RAM boards of the DSP-2
    /// (SHVC-1B5B-01) and DSP-3 (SHVC-1B3B-01) at `20-3F`, and ares' board database agrees
    /// (SHVC-1B5B-02: `20-3f,a0-bf:8000-ffff`); the DSP-1/DSP-4 1 MB board is at `30-3F`.
    /// Mesen2 maps every LoROM DSP at `30-3F` only.
    pub fn lorom_port_first_bank(self) -> u8 {
        match self {
            Self::Dsp2 | Self::Dsp3 => 0x20,
            Self::Dsp1 | Self::Dsp1B | Self::Dsp4 => 0x30,
        }
    }
}

/// A DSP chip as the player sees it: the firmware they supply, and the name messages use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspChip {
    Dsp1,
    Dsp2,
    Dsp3,
    Dsp4,
}

impl DspChip {
    const ALL: [Self; 4] = [Self::Dsp1, Self::Dsp2, Self::Dsp3, Self::Dsp4];

    /// The chip's name in messages: "DSP-1".
    pub fn label(self) -> &'static str {
        match self {
            Self::Dsp1 => "DSP-1",
            Self::Dsp2 => "DSP-2",
            Self::Dsp3 => "DSP-3",
            Self::Dsp4 => "DSP-4",
        }
    }

    /// The chip's key for the browser version (wasm and its firmware store): "dsp1".
    pub fn key(self) -> &'static str {
        match self {
            Self::Dsp1 => "dsp1",
            Self::Dsp2 => "dsp2",
            Self::Dsp3 => "dsp3",
            Self::Dsp4 => "dsp4",
        }
    }

    /// The chip whose [`key`](Self::key) is `key`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|chip| chip.key() == key)
    }

    /// The file every message about this chip names: `dsp1b.rom` for the DSP-1.
    pub fn message_file(self) -> &'static str {
        match self {
            Self::Dsp1 => DSP1B_FILE,
            Self::Dsp2 => "dsp2.rom",
            Self::Dsp3 => "dsp3.rom",
            Self::Dsp4 => "dsp4.rom",
        }
    }

    /// Whether NESER emulates the chip: `table` knows at least one genuine dump of it.
    pub fn is_emulated(self, table: FirmwareTable) -> bool {
        table.iter().any(|f| f.chip == self)
    }

    /// Checks a firmware image with no file name (the browser's): the right size, and one of
    /// this chip's genuine dumps.
    pub fn check(
        self,
        image: &[u8],
        table: FirmwareTable,
    ) -> Result<Upd77c25Firmware, ImageProblem> {
        check_image(image, |hash| {
            table.iter().any(|f| f.chip == self && f.sha256 == *hash)
        })
    }
}

/// The file every DSP-1 message names, and the DSP-1B's.
pub const DSP1B_FILE: &str = "dsp1b.rom";
/// The original DSP-1's file name, preferred by an original DSP-1 game when present.
pub const DSP1_FILE: &str = "dsp1.rom";

/// One firmware file NESER recognises: its chip, its usual name, and the SHA-256 of its known
/// good dump, taken over the 8192-byte little-endian image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareFile {
    pub chip: DspChip,
    pub name: &'static str,
    pub sha256: [u8; 32],
}

/// A list of recognised firmware files; [`FIRMWARE_FILES`] outside tests.
pub type FirmwareTable = &'static [FirmwareFile];

/// Every firmware NESER accepts. The hashes are Mesen2's (`UI/Interop/
/// FirmwareTypeExtensions.cs`); BizHawk's firmware list gives the same `dsp1b.rom` and
/// `dsp2.rom` values. Within a chip, the first row is the file its messages name.
pub const FIRMWARE_FILES: FirmwareTable = &[
    FirmwareFile {
        chip: DspChip::Dsp1,
        name: DSP1B_FILE,
        sha256: hex32("D789CB3C36B05C0B23B6C6F23BE7AA37C6E78B6EE9CEAC8D2D2AA9D8C4D35FA9"),
    },
    FirmwareFile {
        chip: DspChip::Dsp1,
        name: DSP1_FILE,
        sha256: hex32("91E87D11E1C30D172556BED2211CCE2EFA94BA595F58C5D264809EF4D363A97B"),
    },
    FirmwareFile {
        chip: DspChip::Dsp2,
        name: "dsp2.rom",
        sha256: hex32("03EF4EF26C9F701346708CB5D07847B5203CF1B0818BF2930ACD34510FFDD717"),
    },
    FirmwareFile {
        chip: DspChip::Dsp3,
        name: "dsp3.rom",
        sha256: hex32("0971B08F396C32E61989D1067DDDF8E4B14649D548B2188F7C541B03D7C69E4E"),
    },
    FirmwareFile {
        chip: DspChip::Dsp4,
        name: "dsp4.rom",
        sha256: hex32("752D03B2D74441E430B7F713001FA241F8BBCFC1A0D890ED4143F174DBE031DA"),
    },
];

/// A 64-digit hex string as 32 bytes, at compile time.
const fn hex32(hex: &str) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'A'..=b'F' => c - b'A' + 10,
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not a hex digit"),
        }
    }
    let bytes = hex.as_bytes();
    assert!(bytes.len() == 64);
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(bytes[2 * i]) << 4) | nibble(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

/// Why a firmware image was refused. The size is checked first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProblem {
    /// Not 8192 bytes; carries the actual size.
    WrongSize(usize),
    /// The right size, but not a genuine dump of the chip.
    NotGenuine,
}

/// The SHA-256 a genuine dump of this image would be listed under: the hash of its
/// little-endian layout, so an old big-endian copy of a genuine dump matches too.
fn canonical_sha256(firmware: &Upd77c25Firmware) -> [u8; 32] {
    Sha256::digest(firmware.to_le_image()).into()
}

/// Parses `image` and accepts it when `is_genuine` recognises its canonical hash.
pub fn check_image(
    image: &[u8],
    is_genuine: impl Fn(&[u8; 32]) -> bool,
) -> Result<Upd77c25Firmware, ImageProblem> {
    let firmware = Upd77c25Firmware::from_image(image).map_err(ImageProblem::WrongSize)?;
    if is_genuine(&canonical_sha256(&firmware)) {
        Ok(firmware)
    } else {
        Err(ImageProblem::NotGenuine)
    }
}

/// A table recognising exactly `entries`' images, for tests whose firmware is synthetic.
#[cfg(test)]
pub(crate) fn test_table(entries: &[(DspChip, &'static str, &[u8])]) -> FirmwareTable {
    let rows: Vec<FirmwareFile> = entries
        .iter()
        .map(|(chip, name, image)| FirmwareFile {
            chip: *chip,
            name,
            sha256: canonical_sha256(&Upd77c25Firmware::from_image(image).expect("8192 bytes")),
        })
        .collect();
    Box::leak(rows.into_boxed_slice())
}

/// Identifies a DSP cartridge's chip by its header title, or `None` for a cartridge without a
/// DSP.
pub fn identify(cart: &Cartridge) -> Option<DspModel> {
    if cart.enhancement_chip() != Some(EnhancementChip::Dsp) {
        return None;
    }
    let title = trim_title(cart.title_bytes());
    Some(match title {
        b"DUNGEON MASTER" => DspModel::Dsp2,
        b"PILOTWINGS" => DspModel::Dsp1,
        // "SD Gundam GX", in half-width katakana.
        b"SD\xB6\xDE\xDD\xC0\xDE\xD1GX" => DspModel::Dsp3,
        b"PLANETS CHAMP TG3000" | b"TOP GEAR 3000" => DspModel::Dsp4,
        _ => DspModel::Dsp1B,
    })
}

/// Identifies the DSP of a raw ROM image, `None` if it is not a SNES ROM with a DSP.
pub fn identify_rom(rom: &[u8]) -> Option<DspModel> {
    Cartridge::from_bytes(rom).ok().as_ref().and_then(identify)
}

fn trim_title(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map_or(0, |i| i + 1);
    &bytes[..end]
}

/// Why a DSP game cannot start on the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareProblem {
    /// No usable firmware file in the folder (or the folder does not exist).
    Missing { chip: DspChip, folder: PathBuf },
    /// The chip's message file is there but is not 8192 bytes.
    WrongSize {
        chip: DspChip,
        path: PathBuf,
        size: u64,
    },
    /// A file of the right size that is not the chip's genuine firmware.
    NotGenuine { chip: DspChip, path: PathBuf },
}

impl FirmwareProblem {
    /// The game browser strip's two lines; the first is shown in bold.
    pub fn strip_lines(&self, game: &str) -> (String, String) {
        match self {
            Self::Missing { chip, folder } => (
                format!(
                    "{game} can't start: it needs the {} firmware.",
                    chip.label()
                ),
                format!(
                    "Put {} in {} and try again.",
                    chip.message_file(),
                    folder.display()
                ),
            ),
            Self::WrongSize { chip, path, size } => (
                format!(
                    "{game} can't start: the {} firmware isn't valid.",
                    chip.label()
                ),
                format!("{} is {}.", path.display(), size_detail(*size)),
            ),
            Self::NotGenuine { chip, path } => (
                format!(
                    "{game} can't start: the {} firmware isn't valid.",
                    chip.label()
                ),
                format!("{} is {}.", path.display(), not_genuine_detail(*chip)),
            ),
        }
    }

    /// The message printed on stderr when the game was given on the command line.
    pub fn cli_message(&self, game: &str) -> String {
        match self {
            Self::Missing { chip, folder } => format!(
                "Error: {game} needs the SNES {} firmware, which was not found.\n\
                 Put {} in {}, or point --snes-firmware-dir at the folder holding it.",
                chip.label(),
                chip.message_file(),
                folder.display()
            ),
            Self::WrongSize { chip, path, size } => format!(
                "Error: {} is not valid SNES {} firmware: it is {}.",
                path.display(),
                chip.label(),
                size_detail(*size)
            ),
            Self::NotGenuine { chip, path } => format!(
                "Error: {} is not valid SNES {} firmware: it is {}.",
                path.display(),
                chip.label(),
                not_genuine_detail(*chip)
            ),
        }
    }
}

/// "12,288 bytes; it must be exactly 8,192 bytes"
fn size_detail(size: u64) -> String {
    format!(
        "{} bytes; it must be exactly {} bytes",
        thousands(size),
        thousands(DSP_IMAGE_SIZE as u64)
    )
}

/// "not the DSP-2 firmware (it may be the firmware of a different chip)"
fn not_genuine_detail(chip: DspChip) -> String {
    format!(
        "not the {} firmware (it may be the firmware of a different chip)",
        chip.label()
    )
}

/// Reads the firmware for `model` from `dir`, fresh on every call.
///
/// The model's files ([`DspModel::firmware_files`]) are tried in order and each is checked
/// against its own genuine dump; the first that passes wins. When none does, the problem
/// reported is: the first right-size file that is not genuine (the file the game would have
/// used); else the chip's message file having the wrong size; else "missing".
pub fn load_from_dir(
    dir: &Path,
    model: DspModel,
    table: FirmwareTable,
) -> Result<Upd77c25Firmware, FirmwareProblem> {
    let chip = model.chip();
    let mut not_genuine = None;
    let mut wrong_size = None;
    for file in model.firmware_files(table) {
        let path = dir.join(file.name);
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match check_image(&bytes, |hash| *hash == file.sha256) {
            Ok(firmware) => return Ok(firmware),
            Err(ImageProblem::NotGenuine) => {
                not_genuine.get_or_insert(path);
            }
            Err(ImageProblem::WrongSize(size)) if file.name == chip.message_file() => {
                wrong_size = Some((path, size as u64));
            }
            Err(ImageProblem::WrongSize(_)) => {}
        }
    }
    Err(match (not_genuine, wrong_size) {
        (Some(path), _) => FirmwareProblem::NotGenuine { chip, path },
        (None, Some((path, size))) => FirmwareProblem::WrongSize { chip, path, size },
        (None, None) => FirmwareProblem::Missing {
            chip,
            folder: dir.to_path_buf(),
        },
    })
}

/// A byte count with thousands separators: 12288 becomes "12,288".
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::test_support::dsp_rom;

    #[test]
    fn identifies_pilotwings_as_dsp1_and_others_as_dsp1b() {
        let cart = Cartridge::from_bytes(&dsp_rom(b"PILOTWINGS", false)).unwrap();
        assert_eq!(identify(&cart), Some(DspModel::Dsp1));
        let cart = Cartridge::from_bytes(&dsp_rom(b"SUPER MARIOKART", false)).unwrap();
        assert_eq!(identify(&cart), Some(DspModel::Dsp1B));
        let cart = Cartridge::from_bytes(&dsp_rom(b"SUPER MARIOKART", true)).unwrap();
        assert_eq!(identify(&cart), Some(DspModel::Dsp1B));
    }

    #[test]
    fn identifies_dsp2_dsp3_dsp4_titles() {
        let model = |title: &[u8]| identify_rom(&dsp_rom(title, false));
        assert_eq!(model(b"DUNGEON MASTER"), Some(DspModel::Dsp2));
        assert_eq!(model(b"SD\xB6\xDE\xDD\xC0\xDE\xD1GX"), Some(DspModel::Dsp3));
        assert_eq!(model(b"TOP GEAR 3000"), Some(DspModel::Dsp4));
        assert_eq!(model(b"PLANETS CHAMP TG3000"), Some(DspModel::Dsp4));
    }

    #[test]
    fn a_rom_without_a_dsp_is_not_identified() {
        assert_eq!(
            identify_rom(&crate::snes::test_support::minimal_lorom(b"PLAIN GAME")),
            None
        );
        assert_eq!(identify_rom(&[0u8; 16]), None);
    }

    #[test]
    fn model_chip_maps_dsp1_family_to_dsp1() {
        assert_eq!(DspModel::Dsp1.chip(), DspChip::Dsp1);
        assert_eq!(DspModel::Dsp1B.chip(), DspChip::Dsp1);
        assert_eq!(DspModel::Dsp2.chip(), DspChip::Dsp2);
        assert_eq!(DspModel::Dsp3.chip(), DspChip::Dsp3);
        assert_eq!(DspModel::Dsp4.chip(), DspChip::Dsp4);
    }

    #[test]
    fn chip_labels_keys_and_message_files() {
        let facts: Vec<_> = DspChip::ALL
            .iter()
            .map(|c| (c.label(), c.key(), c.message_file()))
            .collect();
        assert_eq!(
            facts,
            [
                ("DSP-1", "dsp1", "dsp1b.rom"),
                ("DSP-2", "dsp2", "dsp2.rom"),
                ("DSP-3", "dsp3", "dsp3.rom"),
                ("DSP-4", "dsp4", "dsp4.rom"),
            ]
        );
    }

    #[test]
    fn from_key_round_trips() {
        for chip in DspChip::ALL {
            assert_eq!(DspChip::from_key(chip.key()), Some(chip));
        }
        assert_eq!(DspChip::from_key("DSP1"), None);
        assert_eq!(DspChip::from_key(""), None);
    }

    #[test]
    fn every_dsp_chip_is_emulated() {
        assert!(DspChip::Dsp1.is_emulated(FIRMWARE_FILES));
        assert!(DspChip::Dsp2.is_emulated(FIRMWARE_FILES));
        assert!(DspChip::Dsp3.is_emulated(FIRMWARE_FILES));
        assert!(DspChip::Dsp4.is_emulated(FIRMWARE_FILES));
    }

    #[test]
    fn firmware_table_hashes_match_mesen2() {
        let hex = |f: &FirmwareFile| {
            f.sha256
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<String>()
        };
        let rows: Vec<_> = FIRMWARE_FILES
            .iter()
            .map(|f| (f.chip, f.name, hex(f)))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    DspChip::Dsp1,
                    "dsp1b.rom",
                    "D789CB3C36B05C0B23B6C6F23BE7AA37C6E78B6EE9CEAC8D2D2AA9D8C4D35FA9".to_string()
                ),
                (
                    DspChip::Dsp1,
                    "dsp1.rom",
                    "91E87D11E1C30D172556BED2211CCE2EFA94BA595F58C5D264809EF4D363A97B".to_string()
                ),
                (
                    DspChip::Dsp2,
                    "dsp2.rom",
                    "03EF4EF26C9F701346708CB5D07847B5203CF1B0818BF2930ACD34510FFDD717".to_string()
                ),
                (
                    DspChip::Dsp3,
                    "dsp3.rom",
                    "0971B08F396C32E61989D1067DDDF8E4B14649D548B2188F7C541B03D7C69E4E".to_string()
                ),
                (
                    DspChip::Dsp4,
                    "dsp4.rom",
                    "752D03B2D74441E430B7F713001FA241F8BBCFC1A0D890ED4143F174DBE031DA".to_string()
                ),
            ]
        );
    }

    #[test]
    fn canonical_hash_is_sha256_of_the_little_endian_image() {
        // SHA-256 of 8192 zero bytes, which is also the little-endian layout of an all-zero
        // firmware.
        let fw = Upd77c25Firmware::from_image(&image(0)).unwrap();
        let hex: String = canonical_sha256(&fw)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex,
            "9f1dcbc35c350d6027f98be0f5c8b43b42ca52b7604459c0c42be3aa88913d47"
        );
    }

    #[test]
    fn model_file_order_prefers_dsp1_rom_for_pilotwings_only() {
        let names = |m: DspModel| -> Vec<_> {
            m.firmware_files(FIRMWARE_FILES)
                .iter()
                .map(|f| f.name)
                .collect()
        };
        assert_eq!(names(DspModel::Dsp1B), ["dsp1b.rom", "dsp1.rom"]);
        assert_eq!(names(DspModel::Dsp1), ["dsp1.rom", "dsp1b.rom"]);
        assert_eq!(names(DspModel::Dsp2), ["dsp2.rom"]);
        assert_eq!(names(DspModel::Dsp3), ["dsp3.rom"]);
        assert_eq!(names(DspModel::Dsp4), ["dsp4.rom"]);
    }

    #[test]
    fn lorom_port_first_bank_follows_fullsnes_boards() {
        assert_eq!(DspModel::Dsp1.lorom_port_first_bank(), 0x30);
        assert_eq!(DspModel::Dsp1B.lorom_port_first_bank(), 0x30);
        assert_eq!(DspModel::Dsp2.lorom_port_first_bank(), 0x20);
        assert_eq!(DspModel::Dsp3.lorom_port_first_bank(), 0x20);
        assert_eq!(DspModel::Dsp4.lorom_port_first_bank(), 0x30);
    }

    /// An 8192-byte image whose every byte is `fill`; the first data word tells images apart.
    fn image(fill: u8) -> Vec<u8> {
        vec![fill; DSP_IMAGE_SIZE]
    }

    /// `image` in the old big-endian layout (each opcode's three bytes and each data word's
    /// two bytes reversed), which parses to the same firmware.
    fn big_endian(image: &[u8]) -> Vec<u8> {
        let (program, data) = image.split_at(2048 * 3);
        let mut out = Vec::with_capacity(image.len());
        for op in program.chunks(3) {
            out.extend_from_slice(&[op[2], op[1], op[0]]);
        }
        for word in data.chunks(2) {
            out.extend_from_slice(&[word[1], word[0]]);
        }
        out
    }

    /// A little-endian image whose first opcodes are `JRQM $`, so its big-endian copy is
    /// detected as big-endian (fullsnes "ROM-Images").
    fn jrqm_image(data_fill: u8) -> Vec<u8> {
        let mut img = image(data_fill);
        img[..3].copy_from_slice(&[0x00, 0xC0, 0x97]);
        img
    }

    #[test]
    fn check_image_reports_size_before_genuineness() {
        let genuine = |_: &[u8; 32]| false;
        assert_eq!(
            check_image(&[0u8; 12288], genuine).err(),
            Some(ImageProblem::WrongSize(12288))
        );
        assert_eq!(
            check_image(&image(0), genuine).err(),
            Some(ImageProblem::NotGenuine)
        );
    }

    #[test]
    fn check_image_accepts_a_genuine_dump_in_either_byte_order() {
        let le = jrqm_image(0x22);
        let table = test_table(&[(DspChip::Dsp2, "dsp2.rom", &le)]);
        assert_eq!(
            DspChip::Dsp2.check(&le, table).map(|fw| fw.to_le_image()),
            Ok(le.clone())
        );
        let be = big_endian(&le);
        assert_ne!(be, le);
        assert_eq!(
            DspChip::Dsp2.check(&be, table).map(|fw| fw.to_le_image()),
            Ok(le)
        );
    }

    #[test]
    fn check_image_rejects_an_8k_image_of_another_chip() {
        let dsp1b = image(0x1B);
        let table = test_table(&[(DspChip::Dsp1, DSP1B_FILE, &dsp1b)]);
        assert!(DspChip::Dsp1.check(&dsp1b, table).is_ok());
        assert_eq!(
            DspChip::Dsp2.check(&dsp1b, table).err(),
            Some(ImageProblem::NotGenuine)
        );
        assert_eq!(
            DspChip::Dsp2.check(&[0u8; 100], table).err(),
            Some(ImageProblem::WrongSize(100))
        );
    }

    #[test]
    fn browser_check_accepts_either_dsp1_dump() {
        let (dsp1b, dsp1) = (image(0x1B), image(0x01));
        let table = test_table(&[
            (DspChip::Dsp1, DSP1B_FILE, &dsp1b),
            (DspChip::Dsp1, DSP1_FILE, &dsp1),
        ]);
        assert!(DspChip::Dsp1.check(&dsp1b, table).is_ok());
        assert!(DspChip::Dsp1.check(&dsp1, table).is_ok());
    }

    fn dir_with(files: &[(&str, Vec<u8>)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in files {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        dir
    }

    /// The first data-ROM word identifies which file was loaded.
    fn fill_of(fw: &Upd77c25Firmware) -> u8 {
        fw.data_word(0) as u8
    }

    /// A table in which `image(0x1B)` is the genuine `dsp1b.rom`, `image(0x01)` the genuine
    /// `dsp1.rom` and `image(0x22)` the genuine `dsp2.rom`.
    fn table() -> FirmwareTable {
        test_table(&[
            (DspChip::Dsp1, DSP1B_FILE, &image(0x1B)),
            (DspChip::Dsp1, DSP1_FILE, &image(0x01)),
            (DspChip::Dsp2, "dsp2.rom", &image(0x22)),
        ])
    }

    #[test]
    fn load_from_dir_prefers_dsp1b_for_dsp1b_game() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x1B);
    }

    #[test]
    fn falls_back_to_dsp1_rom() {
        let dir = dir_with(&[(DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x01);
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x1B);
    }

    #[test]
    fn original_dsp1_game_prefers_dsp1_rom() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x01);
    }

    #[test]
    fn missing_folder_file_path_or_file_is_missing() {
        let dir = dir_with(&[]);
        let missing = |path: &Path| FirmwareProblem::Missing {
            chip: DspChip::Dsp1,
            folder: path.to_path_buf(),
        };
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(missing(dir.path()))
        );
        let nowhere = dir.path().join("nowhere");
        assert_eq!(
            load_from_dir(&nowhere, DspModel::Dsp1B, table()).err(),
            Some(missing(&nowhere))
        );
        let file = dir.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            load_from_dir(&file, DspModel::Dsp1B, table()).err(),
            Some(missing(&file))
        );
        // A directory named dsp1b.rom is not a firmware file.
        std::fs::create_dir(dir.path().join(DSP1B_FILE)).unwrap();
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(missing(dir.path()))
        );
    }

    #[test]
    fn wrong_size_dsp1b_reports_actual_size() {
        let dir = dir_with(&[(DSP1B_FILE, vec![0; 12288])]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(FirmwareProblem::WrongSize {
                chip: DspChip::Dsp1,
                path: dir.path().join(DSP1B_FILE),
                size: 12288
            })
        );
        // For Pilotwings too, since dsp1.rom is absent.
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1, table()).err(),
            Some(FirmwareProblem::WrongSize { size: 12288, .. })
        ));
    }

    #[test]
    fn wrong_size_file_skipped_when_fallback_valid() {
        let dir = dir_with(&[(DSP1B_FILE, vec![0; 100]), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x01);

        // Only a wrong-size dsp1.rom: dsp1b.rom is genuinely missing.
        let dir = dir_with(&[(DSP1_FILE, vec![0; 100])]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1, table()).err(),
            Some(FirmwareProblem::Missing { .. })
        ));
    }

    #[test]
    fn load_from_dir_for_dsp2_reads_dsp2_rom() {
        let dir = dir_with(&[("dsp2.rom", image(0x22)), (DSP1B_FILE, image(0x1B))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp2, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x22);
    }

    #[test]
    fn dsp2_missing_wrong_size_and_not_genuine() {
        let chip = DspChip::Dsp2;
        // A DSP-1 file alone does not start a DSP-2 game.
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp2, table()).err(),
            Some(FirmwareProblem::Missing {
                chip,
                folder: dir.path().to_path_buf()
            })
        );
        let dir = dir_with(&[("dsp2.rom", vec![0; 12288])]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp2, table()).err(),
            Some(FirmwareProblem::WrongSize {
                chip,
                path: dir.path().join("dsp2.rom"),
                size: 12288
            })
        );
        let dir = dir_with(&[("dsp2.rom", image(0x77))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp2, table()).err(),
            Some(FirmwareProblem::NotGenuine {
                chip,
                path: dir.path().join("dsp2.rom")
            })
        );
    }

    /// [`table`] plus `image(0x33)` as the genuine `dsp3.rom`.
    fn table_with_dsp3() -> FirmwareTable {
        test_table(&[
            (DspChip::Dsp1, DSP1B_FILE, &image(0x1B)),
            (DspChip::Dsp2, "dsp2.rom", &image(0x22)),
            (DspChip::Dsp3, "dsp3.rom", &image(0x33)),
        ])
    }

    #[test]
    fn dsp3_reads_dsp3_rom_only_and_refuses_another_chips_file() {
        let chip = DspChip::Dsp3;
        let dir = dir_with(&[("dsp3.rom", image(0x33)), ("dsp2.rom", image(0x22))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp3, table_with_dsp3()).unwrap();
        assert_eq!(fill_of(&fw), 0x33);
        // DSP-1 and DSP-2 files do not start a DSP-3 game: dsp3.rom has no fallback name.
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), ("dsp2.rom", image(0x22))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp3, table_with_dsp3()).err(),
            Some(FirmwareProblem::Missing {
                chip,
                folder: dir.path().to_path_buf()
            })
        );
        let dir = dir_with(&[("dsp3.rom", vec![0; 12288])]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp3, table_with_dsp3()).err(),
            Some(FirmwareProblem::WrongSize {
                chip,
                path: dir.path().join("dsp3.rom"),
                size: 12288
            })
        );
        // A renamed dsp1b.rom or dsp2.rom is the right size but not the DSP-3 firmware.
        for other in [image(0x1B), image(0x22)] {
            let dir = dir_with(&[("dsp3.rom", other)]);
            assert_eq!(
                load_from_dir(dir.path(), DspModel::Dsp3, table_with_dsp3()).err(),
                Some(FirmwareProblem::NotGenuine {
                    chip,
                    path: dir.path().join("dsp3.rom")
                })
            );
        }
    }

    /// [`table`] plus `image(0x44)` as the genuine `dsp4.rom`.
    fn table_with_dsp4() -> FirmwareTable {
        test_table(&[
            (DspChip::Dsp1, DSP1B_FILE, &image(0x1B)),
            (DspChip::Dsp2, "dsp2.rom", &image(0x22)),
            (DspChip::Dsp4, "dsp4.rom", &image(0x44)),
        ])
    }

    #[test]
    fn load_from_dir_for_dsp4_reads_dsp4_rom() {
        let dir = dir_with(&[
            ("dsp4.rom", image(0x44)),
            (DSP1B_FILE, image(0x1B)),
            ("dsp2.rom", image(0x22)),
        ]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp4, table_with_dsp4()).unwrap();
        assert_eq!(fill_of(&fw), 0x44);
    }

    #[test]
    fn dsp4_missing_wrong_size_and_not_genuine() {
        let chip = DspChip::Dsp4;
        // Other chips' files do not start a DSP-4 game: dsp4.rom has no fallback name.
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), ("dsp2.rom", image(0x22))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp4, table_with_dsp4()).err(),
            Some(FirmwareProblem::Missing {
                chip,
                folder: dir.path().to_path_buf()
            })
        );
        let dir = dir_with(&[("dsp4.rom", vec![0; 12288])]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp4, table_with_dsp4()).err(),
            Some(FirmwareProblem::WrongSize {
                chip,
                path: dir.path().join("dsp4.rom"),
                size: 12288
            })
        );
        // A renamed dsp1b.rom or dsp2.rom is the right size but not the DSP-4 firmware.
        for other in [image(0x1B), image(0x22)] {
            let dir = dir_with(&[("dsp4.rom", other)]);
            assert_eq!(
                load_from_dir(dir.path(), DspModel::Dsp4, table_with_dsp4()).err(),
                Some(FirmwareProblem::NotGenuine {
                    chip,
                    path: dir.path().join("dsp4.rom")
                })
            );
        }
    }

    #[test]
    fn a_dsp1b_image_named_dsp2_rom_is_not_genuine() {
        let dir = dir_with(&[("dsp2.rom", image(0x1B))]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp2, table()).err(),
            Some(FirmwareProblem::NotGenuine { .. })
        ));
        // Each DSP-1 file is checked against its own dump: the genuine dsp1.rom image under
        // the name dsp1b.rom is refused.
        let dir = dir_with(&[(DSP1B_FILE, image(0x01))]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(FirmwareProblem::NotGenuine { .. })
        ));
    }

    #[test]
    fn dsp1_not_genuine_dsp1b_names_dsp1b() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x77))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(FirmwareProblem::NotGenuine {
                chip: DspChip::Dsp1,
                path: dir.path().join(DSP1B_FILE)
            })
        );
    }

    #[test]
    fn pilotwings_not_genuine_dsp1_names_dsp1() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x78)), (DSP1_FILE, image(0x77))]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1, table()).err(),
            Some(FirmwareProblem::NotGenuine {
                chip: DspChip::Dsp1,
                path: dir.path().join(DSP1_FILE)
            })
        );
        // A wrong-size dsp1b.rom does not hide the non-genuine dsp1.rom the game would use.
        let dir = dir_with(&[(DSP1B_FILE, vec![0; 100]), (DSP1_FILE, image(0x77))]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1B, table()).err(),
            Some(FirmwareProblem::NotGenuine { path, .. }) if path.ends_with(DSP1_FILE)
        ));
    }

    #[test]
    fn not_genuine_preferred_file_skipped_when_fallback_genuine() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x77)), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B, table()).unwrap();
        assert_eq!(fill_of(&fw), 0x01);
    }

    #[test]
    fn the_real_table_refuses_a_synthetic_image() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), ("dsp2.rom", image(0x22))]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1B, FIRMWARE_FILES).err(),
            Some(FirmwareProblem::NotGenuine { .. })
        ));
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp2, FIRMWARE_FILES).err(),
            Some(FirmwareProblem::NotGenuine { .. })
        ));
    }

    const FOLDER: &str = "/Users/henrik/.neser/firmware";

    #[test]
    fn strip_lines_missing_and_wrong_size_words() {
        let folder = PathBuf::from(FOLDER);
        let missing = FirmwareProblem::Missing {
            chip: DspChip::Dsp1,
            folder: folder.clone(),
        };
        assert_eq!(
            missing.strip_lines("Super Mario Kart (USA)"),
            (
                "Super Mario Kart (USA) can't start: it needs the DSP-1 firmware.".to_string(),
                "Put dsp1b.rom in /Users/henrik/.neser/firmware and try again.".to_string()
            )
        );
        let wrong = FirmwareProblem::WrongSize {
            chip: DspChip::Dsp1,
            path: folder.join("dsp1b.rom"),
            size: 12288,
        };
        assert_eq!(
            wrong.strip_lines("Super Mario Kart (USA)"),
            (
                "Super Mario Kart (USA) can't start: the DSP-1 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp1b.rom is 12,288 bytes; it must be exactly 8,192 bytes."
                    .to_string()
            )
        );
    }

    #[test]
    fn cli_message_missing_and_wrong_size_words() {
        let folder = PathBuf::from(FOLDER);
        assert_eq!(
            FirmwareProblem::Missing {
                chip: DspChip::Dsp1,
                folder: folder.clone()
            }
            .cli_message("Super Mario Kart (USA)"),
            "Error: Super Mario Kart (USA) needs the SNES DSP-1 firmware, which was not found.\n\
             Put dsp1b.rom in /Users/henrik/.neser/firmware, or point --snes-firmware-dir at the folder holding it."
        );
        assert_eq!(
            FirmwareProblem::WrongSize {
                chip: DspChip::Dsp1,
                path: folder.join("dsp1b.rom"),
                size: 12288
            }
            .cli_message("Super Mario Kart (USA)"),
            "Error: /Users/henrik/.neser/firmware/dsp1b.rom is not valid SNES DSP-1 firmware: it is 12,288 bytes; it must be exactly 8,192 bytes."
        );
    }

    #[test]
    fn dsp1_not_genuine_words() {
        let problem = FirmwareProblem::NotGenuine {
            chip: DspChip::Dsp1,
            path: PathBuf::from(FOLDER).join("dsp1b.rom"),
        };
        assert_eq!(
            problem.strip_lines("Super Mario Kart (USA)"),
            (
                "Super Mario Kart (USA) can't start: the DSP-1 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp1b.rom is not the DSP-1 firmware (it may be the firmware of a different chip)."
                    .to_string()
            )
        );
        assert_eq!(
            problem.cli_message("Super Mario Kart (USA)"),
            "Error: /Users/henrik/.neser/firmware/dsp1b.rom is not valid SNES DSP-1 firmware: it is not the DSP-1 firmware (it may be the firmware of a different chip)."
        );
    }

    #[test]
    fn strip_lines_and_cli_words_for_dsp2() {
        let folder = PathBuf::from(FOLDER);
        let chip = DspChip::Dsp2;
        let game = "Dungeon Master (Japan)";
        let missing = FirmwareProblem::Missing {
            chip,
            folder: folder.clone(),
        };
        assert_eq!(
            missing.strip_lines(game),
            (
                "Dungeon Master (Japan) can't start: it needs the DSP-2 firmware.".to_string(),
                "Put dsp2.rom in /Users/henrik/.neser/firmware and try again.".to_string()
            )
        );
        assert_eq!(
            missing.cli_message(game),
            "Error: Dungeon Master (Japan) needs the SNES DSP-2 firmware, which was not found.\n\
             Put dsp2.rom in /Users/henrik/.neser/firmware, or point --snes-firmware-dir at the folder holding it."
        );
        let wrong = FirmwareProblem::WrongSize {
            chip,
            path: folder.join("dsp2.rom"),
            size: 12288,
        };
        assert_eq!(
            wrong.strip_lines(game),
            (
                "Dungeon Master (Japan) can't start: the DSP-2 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp2.rom is 12,288 bytes; it must be exactly 8,192 bytes."
                    .to_string()
            )
        );
        assert_eq!(
            wrong.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp2.rom is not valid SNES DSP-2 firmware: it is 12,288 bytes; it must be exactly 8,192 bytes."
        );
        let fake = FirmwareProblem::NotGenuine {
            chip,
            path: folder.join("dsp2.rom"),
        };
        assert_eq!(
            fake.strip_lines(game),
            (
                "Dungeon Master (Japan) can't start: the DSP-2 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp2.rom is not the DSP-2 firmware (it may be the firmware of a different chip)."
                    .to_string()
            )
        );
        assert_eq!(
            fake.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp2.rom is not valid SNES DSP-2 firmware: it is not the DSP-2 firmware (it may be the firmware of a different chip)."
        );
    }

    #[test]
    fn strip_lines_and_cli_words_for_dsp3() {
        let folder = PathBuf::from(FOLDER);
        let chip = DspChip::Dsp3;
        let game = "SD Gundam GX (Japan)";
        let missing = FirmwareProblem::Missing {
            chip,
            folder: folder.clone(),
        };
        assert_eq!(
            missing.strip_lines(game),
            (
                "SD Gundam GX (Japan) can't start: it needs the DSP-3 firmware.".to_string(),
                "Put dsp3.rom in /Users/henrik/.neser/firmware and try again.".to_string()
            )
        );
        assert_eq!(
            missing.cli_message(game),
            "Error: SD Gundam GX (Japan) needs the SNES DSP-3 firmware, which was not found.\n\
             Put dsp3.rom in /Users/henrik/.neser/firmware, or point --snes-firmware-dir at the folder holding it."
        );
        let wrong = FirmwareProblem::WrongSize {
            chip,
            path: folder.join("dsp3.rom"),
            size: 12288,
        };
        assert_eq!(
            wrong.strip_lines(game),
            (
                "SD Gundam GX (Japan) can't start: the DSP-3 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp3.rom is 12,288 bytes; it must be exactly 8,192 bytes."
                    .to_string()
            )
        );
        assert_eq!(
            wrong.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp3.rom is not valid SNES DSP-3 firmware: it is 12,288 bytes; it must be exactly 8,192 bytes."
        );
        let fake = FirmwareProblem::NotGenuine {
            chip,
            path: folder.join("dsp3.rom"),
        };
        assert_eq!(
            fake.strip_lines(game),
            (
                "SD Gundam GX (Japan) can't start: the DSP-3 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp3.rom is not the DSP-3 firmware (it may be the firmware of a different chip)."
                    .to_string()
            )
        );
        assert_eq!(
            fake.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp3.rom is not valid SNES DSP-3 firmware: it is not the DSP-3 firmware (it may be the firmware of a different chip)."
        );
    }

    #[test]
    fn strip_lines_and_cli_words_for_dsp4() {
        let folder = PathBuf::from(FOLDER);
        let chip = DspChip::Dsp4;
        let game = "Top Gear 3000 (USA)";
        let missing = FirmwareProblem::Missing {
            chip,
            folder: folder.clone(),
        };
        assert_eq!(
            missing.strip_lines(game),
            (
                "Top Gear 3000 (USA) can't start: it needs the DSP-4 firmware.".to_string(),
                "Put dsp4.rom in /Users/henrik/.neser/firmware and try again.".to_string()
            )
        );
        assert_eq!(
            missing.cli_message(game),
            "Error: Top Gear 3000 (USA) needs the SNES DSP-4 firmware, which was not found.\n\
             Put dsp4.rom in /Users/henrik/.neser/firmware, or point --snes-firmware-dir at the folder holding it."
        );
        let wrong = FirmwareProblem::WrongSize {
            chip,
            path: folder.join("dsp4.rom"),
            size: 12288,
        };
        assert_eq!(
            wrong.strip_lines(game),
            (
                "Top Gear 3000 (USA) can't start: the DSP-4 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp4.rom is 12,288 bytes; it must be exactly 8,192 bytes."
                    .to_string()
            )
        );
        assert_eq!(
            wrong.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp4.rom is not valid SNES DSP-4 firmware: it is 12,288 bytes; it must be exactly 8,192 bytes."
        );
        let fake = FirmwareProblem::NotGenuine {
            chip,
            path: folder.join("dsp4.rom"),
        };
        assert_eq!(
            fake.strip_lines(game),
            (
                "Top Gear 3000 (USA) can't start: the DSP-4 firmware isn't valid.".to_string(),
                "/Users/henrik/.neser/firmware/dsp4.rom is not the DSP-4 firmware (it may be the firmware of a different chip)."
                    .to_string()
            )
        );
        assert_eq!(
            fake.cli_message(game),
            "Error: /Users/henrik/.neser/firmware/dsp4.rom is not valid SNES DSP-4 firmware: it is not the DSP-4 firmware (it may be the firmware of a different chip)."
        );
    }

    #[test]
    fn thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(8192), "8,192");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
