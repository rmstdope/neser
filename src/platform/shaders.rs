/// Single source of truth for all available shader presets.
///
/// Each entry is `(short_name, relative_path_to_slangp)`.
/// - `short_name` is used in the CLI (`--nes-filter crt`, `--gb-filter gameboy`) and config file (`nes-filter=crt`, `gb-filter=gameboy`).
/// - `path` is relative to the working directory when neser runs.
///
/// To add or remove a shader preset, edit this list only.
pub const SHADER_PRESETS: &[(&str, &str)] = &[
    ("none", "shaders/stock.slangp"),
    ("crt", "vendor/slang-shaders/crt/crt-lottes.slangp"),
    (
        "smooth",
        "vendor/slang-shaders/edge-smoothing/xbrz/xbrz-freescale-multipass.slangp",
    ),
    (
        "ntsc",
        "vendor/slang-shaders/ntsc/ntsc-256px-composite.slangp",
    ),
    (
        "pal",
        "vendor/slang-shaders/pal/decoupled-guest-advanced-pal 3-RF.slangp",
    ),
    ("gameboy", "vendor/slang-shaders/handheld/gameboy.slangp"),
];
