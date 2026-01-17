# Copilot instructions for `neser`

## Project context
- Rust NES emulator with an optional SDL frontend (enable `sdl` feature for windowed/audio output).
- Test ROMs live in `roms/`; keep the existing files and names intact.

## Build and run
- Build release with UI: `cargo build --release --features sdl`
- Run release with UI: `cargo run --release --features sdl`

## Tests and checks
- Main regression suite: `cargo test` (no extra features needed).
- If touching Rust code, prefer `cargo fmt -- --check` before sending changes.

## Configuration notes
- `neser.conf.example` documents all runtime options; copy it to `neser.conf` or `~/.neser/neser.conf` for local runs.
