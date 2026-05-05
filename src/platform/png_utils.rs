pub fn write_rgb_png(path: &std::path::Path, rgb: &[u8], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("PNG output directory should be created");
    }

    let file = std::fs::File::create(path).expect("PNG output file should be created");
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);

    let mut png_writer = encoder
        .write_header()
        .expect("PNG header should be written");
    png_writer
        .write_image_data(rgb)
        .expect("PNG image data should be written");
    drop(png_writer);

    use std::io::Write as _;
    writer.flush().expect("PNG buffer should be flushed");
}
