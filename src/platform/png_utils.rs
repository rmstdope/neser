use std::io::Write as _;

/// Write `rgb` as an 8-bit RGB PNG at `path`.
///
/// Missing parent directories are created, and an existing file at `path` is
/// overwritten. Every failure — including a `rgb` length that does not match
/// `width * height * 3` — is surfaced as an [`std::io::Error`] so callers can
/// report it instead of panicking.
pub fn write_rgb_png(
    path: &std::path::Path,
    rgb: &[u8],
    width: u32,
    height: u32,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png_writer = encoder.write_header().map_err(encoding_error)?;
    png_writer.write_image_data(rgb).map_err(encoding_error)?;
    // `finish` writes the trailing chunks and reports failures that dropping
    // the writer would silently discard.
    png_writer.finish().map_err(encoding_error)?;

    writer.flush()
}

/// Convert a `png` encoding failure into an [`std::io::Error`].
///
/// `png::EncodingError` already carries an `io::Error` for I/O failures; the
/// remaining variants (parameter and format problems) are mapped to
/// `InvalidInput` so the whole function can return a single error type.
fn encoding_error(error: png::EncodingError) -> std::io::Error {
    match error {
        png::EncodingError::IoError(io_error) => io_error,
        other => std::io::Error::new(std::io::ErrorKind::InvalidInput, other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// Decode a PNG from disk into `(width, height, rgb_bytes)`.
    fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
        let file = std::fs::File::open(path).expect("written PNG should be readable");
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = decoder.read_info().expect("PNG header should decode");
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .expect("PNG image data should decode");
        buffer.truncate(info.buffer_size());
        (info.width, info.height, buffer)
    }

    #[test]
    fn write_rgb_png_writes_a_decodable_image_with_the_requested_dimensions() {
        // Given a two-pixel red/green image
        let temp = TempDir::new().expect("create temp dir");
        let path = temp.path().join("out.png");
        let rgb = [255, 0, 0, 0, 255, 0];

        // When it is written
        write_rgb_png(&path, &rgb, 2, 1).expect("write should succeed");

        // Then it decodes back to the same dimensions and pixels
        let (width, height, decoded) = decode_png(&path);
        assert_eq!((width, height), (2, 1));
        assert_eq!(decoded, rgb);
    }

    #[test]
    fn write_rgb_png_creates_missing_parent_directories() {
        // Given an output path whose parent directories do not exist
        let temp = TempDir::new().expect("create temp dir");
        let path = temp.path().join("nested").join("deeper").join("out.png");

        // When the image is written
        write_rgb_png(&path, &[1, 2, 3], 1, 1).expect("write should create parent directories");

        // Then the file exists at the requested path
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn write_rgb_png_overwrites_an_existing_file() {
        // Given a path that already holds a differently sized capture
        let temp = TempDir::new().expect("create temp dir");
        let path = temp.path().join("out.png");
        write_rgb_png(&path, &[0, 0, 0, 0, 0, 0], 2, 1).expect("first write should succeed");

        // When a new capture is written to the same path
        write_rgb_png(&path, &[9, 9, 9], 1, 1).expect("second write should succeed");

        // Then the second capture replaced the first
        let (width, height, decoded) = decode_png(&path);
        assert_eq!((width, height), (1, 1));
        assert_eq!(decoded, [9, 9, 9]);
    }

    #[test]
    fn write_rgb_png_returns_error_when_the_buffer_does_not_match_the_dimensions() {
        // Given a buffer that is one pixel short of the declared 2x1 image
        let temp = TempDir::new().expect("create temp dir");
        let path = temp.path().join("out.png");

        // When it is written
        let result = write_rgb_png(&path, &[255, 0, 0], 2, 1);

        // Then the mismatch is reported rather than panicking, and the message
        // names both sizes so a miscomputed capture is diagnosable.
        let error = result.expect_err("expected a buffer size error");
        let message = error.to_string();
        assert!(
            message.contains('6') && message.contains('3'),
            "expected expected/actual sizes in {message:?}"
        );
    }

    #[test]
    fn write_rgb_png_returns_error_when_the_parent_path_is_a_file() {
        // Given a regular file occupying the output path's parent directory slot
        let temp = TempDir::new().expect("create temp dir");
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("create blocking file");
        let path = blocker.join("out.png");

        // When a capture is written beneath it
        let result = write_rgb_png(&path, &[1, 2, 3], 1, 1);

        // Then the failure is reported rather than panicking
        assert!(
            result.is_err(),
            "expected an error when the parent path is a file"
        );
    }
}
