# NESER

NESER is a Nintendo emulation systems engine written in Rust. It provides native desktop and browser frontends for multiple emulated systems, with shared platform code for audio, video, input, save states, configuration, ROM browsing, and debugging.

Supported emulation targets:

- Nintendo Entertainment System / Famicom / related variants
- Game Boy and Game Boy Color
- Game Boy Advance
- Super Nintendo Entertainment System (SNES)

For emulator-specific options and notes, see:

- [NES-specific documentation](README-NES.md)
- [Game Boy-specific documentation](README-GB.md)
- [Game Boy Advance-specific documentation](README-GBA.md)
- [SNES-specific documentation](README-SNES.md)

## Installation

### Pre-built releases

Download the latest archive for your platform from [GitHub Releases](https://github.com/rmstdope/neser/releases).

Release archives contain a top-level `neser/` directory. Extract the archive, enter that directory, and run the included binary:

```bash
cd neser
./neser --version
./neser
```

On Windows, run `neser.exe --version` and `neser.exe` from the extracted `neser\` directory.

Release packages include the native emulator binary, runtime assets, shader presets required by configured presets, `neser.conf.example`, `README.md`, the system-specific README files, and `LICENSE`.

### Install from crates.io

```bash
cargo install neser
```

No SDL2 setup is required. The native frontend uses Rust crates for windowing, audio, and gamepad input. Linux source builds may still require system development packages for backend libraries used by those crates, such as ALSA and Wayland/X11.

## Building from source

Prerequisites:

- Rust toolchain
- Platform build tools needed by Rust native dependencies
- Node.js 20+ and npm for the web frontend

Native desktop build:

```bash
cargo build --release --bin neser
```

Native desktop run from source:

```bash
cargo run --release --bin neser -- [OPTIONS] [ROM]
```

Examples:

```bash
cargo run --release --bin neser --
cargo run --release --bin neser -- path/to/game.nes
cargo run --release --bin neser -- path/to/game.gb
cargo run --release --bin neser -- path/to/game.gba
cargo run --release --bin neser -- path/to/game.sfc
```

The repository contains multiple binaries, so include `--bin neser` when using `cargo run`.

## Running NESER

NESER can be launched with or without a ROM path:

```bash
neser
neser path/to/rom
```

When launched without a ROM path, NESER opens the ROM browser. When launched with a ROM path, it loads that ROM directly.

Use the built-in help for the complete current CLI reference:

```bash
neser --help
```

Common examples:

```bash
neser --fullscreen path/to/rom
neser --no-audio path/to/rom
neser --config path/to/neser.conf path/to/rom
```

## Web frontend

The browser frontend is built from the Rust WASM target and the JavaScript frontend under `web/`.

Supported ROM extensions in the web frontend include `.nes`, `.gb`/`.gbc`/`.cgb`, `.gba`, and SNES `.sfc`/`.smc`.

See [web/README.md](web/README.md) for detailed prerequisites, build, run, and test commands.

Convenience commands:

```bash
bash scripts/build_web.sh
bash scripts/run_web.sh
```

`scripts/run_web.sh` serves the built `dist/` directory at <http://localhost:8000>.

## Configuration

NESER can be configured with config files and command-line arguments.

Configuration priority is:

1. Built-in defaults
2. `~/.neser/neser.conf`
3. `./neser.conf`
4. `--config <file>` if specified
5. Command-line arguments

Copy the example config to get started:

```bash
mkdir -p ~/.neser
cp neser.conf.example ~/.neser/neser.conf
```

See [neser.conf.example](neser.conf.example) for all documented configuration keys and valid values.

## Controls

The native frontend supports keyboard and gamepad input.

Common hotkeys:

| Key | Action |
| --- | --- |
| `Space` | Pause/resume |
| `Escape` | Release mouse grab / close overlays |
| `Ctrl+Q` | Quit |
| `Ctrl+R` | Reset |
| `Ctrl+F` | Toggle fullscreen |
| `F1` | Toggle FPS overlay |
| `F2` / `F3` | Volume up/down |
| `F4` | Cycle shader preset |
| `F5` | Toggle debugger |
| `F6` / `F7` | Save/load state |
| `F8` | Cycle NES system palette (NES only) |
| `F10` / `F11` | Debugger step over/into |

System-specific controls and controller options are documented in:

- [README-NES.md](README-NES.md)
- [README-GB.md](README-GB.md)
- [README-GBA.md](README-GBA.md)
- [README-SNES.md](README-SNES.md)

## Development

Recommended setup after cloning:

```bash
git config core.hooksPath .githooks
```

Useful checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --no-default-features --lib
npm test
```

Run tests for a specific Rust source area:

```bash
./scripts/test-dir.sh src/gb --skip-integration
./scripts/test-dir.sh src/nes/cartridge
./scripts/test-dir.sh src/snes
```

`--skip-integration` skips the integration-test modules of all consoles
(`nes`/`gb`/`gba`/`snes`) for fast iteration; run without it before creating a
PR. SNES test commands, asset policy, and the golden-baseline approval
workflow are documented in [README-SNES.md](README-SNES.md).

Python tooling tests:

```bash
source .venv/bin/activate
python -m unittest discover -s scripts -t . -p "test_*.py"
```

## Repository guide

- `src/` contains Rust emulator, frontend, and shared platform code.
- `web/` contains the browser frontend.
- `roms/` contains automated test ROMs and test assets.
- `scripts/` contains build, packaging, test, ROM management, and metadata tools.
- `shaders/` and `vendor/slang-shaders/` contain shader presets used by the native frontend.
- [architecture.md](architecture.md) describes the codebase structure in more detail.

## License

NESER is licensed under the MIT License. See [LICENSE](LICENSE).
