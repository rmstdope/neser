//! The DSP-1 as a cartridge feature: which DSP a cartridge carries, where its firmware comes
//! from, and the words a player reads when it cannot start.
//!
//! The header's chipset byte says only "DSP" (`$03-$05`); fullsnes ("SNES Cart DSP-n/ST010/
//! ST011") states that nothing in the header says which one, "except for using a list of known
//! Titles or Checksums". [`identify`] uses Mesen2's title list (`BaseCartridge::GetDspVersion`):
//! Pilotwings uses the original DSP-1, the known DSP-2/3/4 titles are those chips, and every
//! other DSP cartridge is a DSP-1B.
//!
//! The firmware is the chip's mask ROM, which game dumps lack and NESER cannot include. On the
//! desktop the player puts it in the `snes-firmware-dir` folder as `dsp1b.rom` (or `dsp1.rom`);
//! the browser version hands it over through `Snes::set_dsp1_firmware`.

use crate::snes::cartridge::{Cartridge, EnhancementChip};
use crate::snes::upd77c25::{DSP_IMAGE_SIZE, Upd77c25Firmware};
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
    /// True for the DSP-1 family, the one NESER emulates.
    pub fn is_dsp1(self) -> bool {
        matches!(self, Self::Dsp1 | Self::Dsp1B)
    }
}

/// The file name the messages always point the player at.
pub const DSP1B_FILE: &str = "dsp1b.rom";
/// The original DSP-1's file name, preferred by an original DSP-1 game when present.
pub const DSP1_FILE: &str = "dsp1.rom";

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

/// Why a DSP-1 game cannot start on the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareProblem {
    /// No usable firmware file in the folder (or the folder does not exist).
    Missing { folder: PathBuf },
    /// `dsp1b.rom` is there but is not 8192 bytes.
    WrongSize { path: PathBuf, size: u64 },
}

impl FirmwareProblem {
    /// The game browser strip's two lines; the first is shown in bold.
    pub fn strip_lines(&self, game: &str) -> (String, String) {
        match self {
            Self::Missing { folder } => (
                format!("{game} can't start: it needs the DSP-1 firmware."),
                format!("Put {DSP1B_FILE} in {} and try again.", folder.display()),
            ),
            Self::WrongSize { path, size } => (
                format!("{game} can't start: the DSP-1 firmware isn't valid."),
                format!(
                    "{} is {} bytes; it must be exactly {} bytes.",
                    path.display(),
                    thousands(*size),
                    thousands(DSP_IMAGE_SIZE as u64)
                ),
            ),
        }
    }

    /// The message printed on stderr when the game was given on the command line.
    pub fn cli_message(&self, game: &str) -> String {
        match self {
            Self::Missing { folder } => format!(
                "Error: {game} needs the SNES DSP-1 firmware, which was not found.\n\
                 Put {DSP1B_FILE} in {}, or point --snes-firmware-dir at the folder holding it.",
                folder.display()
            ),
            Self::WrongSize { path, size } => format!(
                "Error: {} is not valid SNES DSP-1 firmware: it is {} bytes; it must be exactly {} bytes.",
                path.display(),
                thousands(*size),
                thousands(DSP_IMAGE_SIZE as u64)
            ),
        }
    }
}

/// Reads the DSP-1 firmware for `model` from `dir`, fresh on every call.
///
/// A DSP-1B game tries `dsp1b.rom` then `dsp1.rom`; an original DSP-1 game tries them the other
/// way round. The first file of the right size wins. When none does, the problem reported
/// names `dsp1b.rom`, as every message does: "wrong size" when `dsp1b.rom` exists with the wrong
/// size, otherwise "missing".
pub fn load_from_dir(dir: &Path, model: DspModel) -> Result<Upd77c25Firmware, FirmwareProblem> {
    let order = if model == DspModel::Dsp1 {
        [DSP1_FILE, DSP1B_FILE]
    } else {
        [DSP1B_FILE, DSP1_FILE]
    };
    let mut dsp1b_size = None;
    for name in order {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            match Upd77c25Firmware::from_image(&bytes) {
                Ok(firmware) => return Ok(firmware),
                Err(size) if name == DSP1B_FILE => dsp1b_size = Some(size as u64),
                Err(_) => {}
            }
        }
    }
    Err(match dsp1b_size {
        Some(size) => FirmwareProblem::WrongSize {
            path: dir.join(DSP1B_FILE),
            size,
        },
        None => FirmwareProblem::Missing {
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

    fn image(fill: u8) -> Vec<u8> {
        vec![fill; DSP_IMAGE_SIZE]
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

    #[test]
    fn load_from_dir_prefers_dsp1b_for_dsp1b_game() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B).unwrap();
        assert_eq!(fill_of(&fw), 0x1B);
    }

    #[test]
    fn falls_back_to_dsp1_rom() {
        let dir = dir_with(&[(DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B).unwrap();
        assert_eq!(fill_of(&fw), 0x01);
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1).unwrap();
        assert_eq!(fill_of(&fw), 0x1B);
    }

    #[test]
    fn original_dsp1_game_prefers_dsp1_rom() {
        let dir = dir_with(&[(DSP1B_FILE, image(0x1B)), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1).unwrap();
        assert_eq!(fill_of(&fw), 0x01);
    }

    #[test]
    fn missing_folder_file_path_or_file_is_missing() {
        let dir = dir_with(&[]);
        let missing = |path: &Path| FirmwareProblem::Missing {
            folder: path.to_path_buf(),
        };
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B).err(),
            Some(missing(dir.path()))
        );
        let nowhere = dir.path().join("nowhere");
        assert_eq!(
            load_from_dir(&nowhere, DspModel::Dsp1B).err(),
            Some(missing(&nowhere))
        );
        let file = dir.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            load_from_dir(&file, DspModel::Dsp1B).err(),
            Some(missing(&file))
        );
        // A directory named dsp1b.rom is not a firmware file.
        std::fs::create_dir(dir.path().join(DSP1B_FILE)).unwrap();
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B).err(),
            Some(missing(dir.path()))
        );
    }

    #[test]
    fn wrong_size_dsp1b_reports_actual_size() {
        let dir = dir_with(&[(DSP1B_FILE, vec![0; 12288])]);
        assert_eq!(
            load_from_dir(dir.path(), DspModel::Dsp1B).err(),
            Some(FirmwareProblem::WrongSize {
                path: dir.path().join(DSP1B_FILE),
                size: 12288
            })
        );
        // For Pilotwings too, since dsp1.rom is absent.
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1).err(),
            Some(FirmwareProblem::WrongSize { size: 12288, .. })
        ));
    }

    #[test]
    fn wrong_size_file_skipped_when_fallback_valid() {
        let dir = dir_with(&[(DSP1B_FILE, vec![0; 100]), (DSP1_FILE, image(0x01))]);
        let fw = load_from_dir(dir.path(), DspModel::Dsp1B).unwrap();
        assert_eq!(fill_of(&fw), 0x01);

        // Only a wrong-size dsp1.rom: dsp1b.rom is genuinely missing.
        let dir = dir_with(&[(DSP1_FILE, vec![0; 100])]);
        assert!(matches!(
            load_from_dir(dir.path(), DspModel::Dsp1).err(),
            Some(FirmwareProblem::Missing { .. })
        ));
    }

    #[test]
    fn strip_lines_missing_and_wrong_size_words() {
        let folder = PathBuf::from("/Users/henrik/.neser/firmware");
        let missing = FirmwareProblem::Missing {
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
        let folder = PathBuf::from("/Users/henrik/.neser/firmware");
        assert_eq!(
            FirmwareProblem::Missing {
                folder: folder.clone()
            }
            .cli_message("Super Mario Kart (USA)"),
            "Error: Super Mario Kart (USA) needs the SNES DSP-1 firmware, which was not found.\n\
             Put dsp1b.rom in /Users/henrik/.neser/firmware, or point --snes-firmware-dir at the folder holding it."
        );
        assert_eq!(
            FirmwareProblem::WrongSize {
                path: folder.join("dsp1b.rom"),
                size: 12288
            }
            .cli_message("Super Mario Kart (USA)"),
            "Error: /Users/henrik/.neser/firmware/dsp1b.rom is not valid SNES DSP-1 firmware: it is 12,288 bytes; it must be exactly 8,192 bytes."
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
