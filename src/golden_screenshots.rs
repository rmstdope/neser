use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenScreenshotPolicy {
    /// Automatically accept the current frame as golden (useful for local bootstrapping).
    AutoAccept,
    /// Automatically reject when no golden exists.
    AutoReject,
    /// Ask the user interactively (to be implemented later).
    Interactive,
}

fn expected_rgb_len(width: u32, height: u32) -> Result<usize, String> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(3))
        .ok_or_else(|| "Invalid image dimensions".to_string())
}

pub fn golden_screenshot_path_for_rom(rom_path: &Path) -> PathBuf {
    let parent = rom_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = rom_path.file_stem().unwrap_or_default();

    parent
        .join("golden_screenshots")
        .join(format!("{}.png", stem.to_string_lossy()))
}

pub fn ensure_golden_screenshot(
    rom_path: &Path,
    rgb: &[u8],
    width: u32,
    height: u32,
    policy: GoldenScreenshotPolicy,
) -> Result<PathBuf, String> {
    let golden_path = golden_screenshot_path_for_rom(rom_path);

    if golden_path.exists() {
        return Ok(golden_path);
    }

    match policy {
        GoldenScreenshotPolicy::AutoAccept => {
            let expected_len = expected_rgb_len(width, height)?;
            if rgb.len() != expected_len {
                return Err(format!(
                    "RGB buffer length mismatch: got {}, expected {}",
                    rgb.len(),
                    expected_len
                ));
            }

            if let Some(parent) = golden_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create golden screenshot dir: {e}"))?;
            }

            let file = std::fs::File::create(&golden_path)
                .map_err(|e| format!("Failed to create golden screenshot file: {e}"))?;
            let mut writer = std::io::BufWriter::new(file);

            {
                let mut encoder = png::Encoder::new(&mut writer, width, height);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                let mut png_writer = encoder
                    .write_header()
                    .map_err(|e| format!("Failed to write PNG header: {e}"))?;
                png_writer
                    .write_image_data(rgb)
                    .map_err(|e| format!("Failed to write PNG image data: {e}"))?;
            }

            writer
                .flush()
                .map_err(|e| format!("Failed to flush PNG file: {e}"))?;

            Ok(golden_path)
        }
        GoldenScreenshotPolicy::AutoReject => Err("No golden screenshot exists".to_string()),
        GoldenScreenshotPolicy::Interactive => {
            Err("Interactive golden screenshot approval is not implemented yet".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let mut base = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        base.push(format!(
            "neser-golden-screenshots-{}-{}",
            std::process::id(),
            nanos
        ));
        base
    }

    #[test]
    fn test_golden_screenshot_path_replaces_extension_and_uses_subdir() {
        let rom_path = Path::new("roms/games/pac-man.nes");
        let expected = Path::new("roms/games/golden_screenshots/pac-man.png");
        assert_eq!(golden_screenshot_path_for_rom(rom_path), expected);
    }

    #[test]
    fn test_auto_accept_creates_png_file_when_missing() {
        let root = unique_temp_dir();
        let games_dir = root.join("roms").join("games");
        fs::create_dir_all(&games_dir).expect("create games dir");

        let rom_path = games_dir.join("demo.nes");
        fs::write(&rom_path, b"not a real rom").expect("write dummy rom");

        let width = 256;
        let height = 240;
        let rgb = vec![0u8; (width * height * 3) as usize];

        let golden_path = ensure_golden_screenshot(
            &rom_path,
            &rgb,
            width,
            height,
            GoldenScreenshotPolicy::AutoAccept,
        )
        .expect("should save golden when missing");

        let data = fs::read(&golden_path).expect("golden file should exist");
        assert!(data.len() > 8);
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    }
}
