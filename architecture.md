# NESER Architecture

> NESER — NES Emulator in Rust

## Overview

NESER is a cycle-accurate NES (Nintendo Entertainment System) emulator written in Rust. It supports three frontend targets: a desktop SDL2 window, a terminal-based TUI ROM launcher, and a WebAssembly-powered browser frontend. The emulator implements the core NES hardware — CPU, PPU, APU, and bus — as well as over 200 cartridge mappers, multiple input device types, debugging tools, save states, and an autorun recording/playback system.

The codebase is roughly 183,000 lines of Rust, with additional JavaScript for the web frontend and Python tooling for ROM management.

## High-Level Architecture

```none
┌───────────────────────────────────────────────────────┐
│                     Frontends                         │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │ SDL Frontend │ │ TUI Frontend │ │ WASM Frontend│   │
│  │ (Desktop GUI)│ │ (Terminal)   │ │ (Browser)    │   │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘   │
│         │                │                │           │
│         └────────────────┼────────────────┘           │
│                          ▼                            │
│  ┌─────────────────────────────────────────────────┐  │
│  │              Console (Nes struct)               │  │
│  │  Orchestrates CPU ↔ PPU ↔ APU ↔ Bus per cycle   │  │
│  └──────────┬──────────────────────────┬───────────┘  │
│             │                          │              │
│    ┌────────▼────────┐       ┌─────────▼──────────┐   │
│    │    CPU (6502)   │       │    PPU (2C02)      │   │
│    │  Opcodes, DMA,  │       │  Background,       │   │
│    │  Interrupts     │       │  Sprites, Timing   │   │
│    └────────┬────────┘       └─────────┬──────────┘   │
│             │                          │              │
│    ┌────────▼──────────────────────────▼───────────┐  │
│    │                    Bus                        │  │
│    │  Address routing: CPU RAM, PPU regs, APU regs,│  │
│    │  OAM DMA, Controller I/O, Mapper/Cartridge    │  │
│    └────────┬──────────────────────────┬───────────┘  │
│             │                          │              │
│    ┌────────▼────────┐       ┌─────────▼──────────┐   │
│    │   APU (2A03)    │       │    Cartridge       │   │
│    │ Pulse, Triangle,│       │  iNES/NES2.0 parser│   │
│    │ Noise, DMC      │       │  207 Mappers       │   │
│    └─────────────────┘       └────────────────────┘   │
│                                                       │
│  ┌─────────────────────────────────────────────────┐  │
│  │  Supporting Systems                             │  │
│  │  Input · Debugging · Autorun · Save States      │  │
│  └─────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────┘
```

The emulator is designed around a **bus-centric architecture**: the `Bus` struct routes memory reads and writes between the CPU, PPU registers, APU registers, RAM, OAM DMA, controller ports, and the cartridge mapper. The `Nes` struct in `src/console/nes.rs` orchestrates the per-cycle stepping of all components.

## Binaries and Scripts

### Rust Binaries

| Binary | Source | Feature | Description |
| --------- | -------- | --------- | ------------- |
| `neser` | `src/main.rs` | `sdl` (default) | Main emulator with SDL2 desktop window, audio, gamepad input, shader filters, debugger, and autorun support. |
| `joysticks` | `src/bin/joysticks.rs` | `sdl` | Diagnostic utility that lists connected joysticks/gamepads, displays their GUID, and shows real-time axis/button state in an SDL window. |

The `src/bin/roms.rs` file is a library binary (accessed via `cargo run --bin roms`) that provides ROM management commands: `list` (scan a directory for NES ROMs), `info` (parse and display iNES/NES2.0 header details), and `infoall` (batch info for all ROMs).

### Shell Scripts

| Script | Description |
| -------- | ------------- |
| `scripts/build_web.sh` | Builds the WASM target with `cargo build --target wasm32-unknown-unknown --features wasm`, then runs `wasm-bindgen` to generate JS glue code into `web/pkg/`. |
| `scripts/run_web.sh` | Starts a local HTTP server (`python3 -m http.server`) in the `web/` directory for testing the browser frontend. |

### Python Tools

| Tool | Description |
| ------ | ------------- |
| `scripts/sort_roms.py` | Sorts ROM files into mapper-numbered subdirectories based on their iNES header. |
| `scripts/disassemble_rom.py` | Disassembles a NES ROM file and prints 6502 assembly output. |
| `scripts/display_audio_output.py` | Visualizes APU audio output data for debugging audio issues. |
| `scripts/mappertool/` | A Textual-based TUI application for browsing and managing a ROM database, inspecting mapper assignments, and cross-referencing ROM files with the embedded ROM database. |
| `scripts/scraper/` | Scrapes NES cartridge databases (NesCartDB, NES 2.0 XML) into a local SQLite database for ROM identification and mapper research. |

## Directory Structure

### `src/` — Rust Source Code

#### Core Emulation

| Directory/File | Description |
| ---------------- | ------------- |
| `src/console/` | Top-level emulator orchestration. |
| `src/console/nes.rs` | The `Nes` struct — creates and owns CPU, PPU, APU, and Bus. Runs the master clock cycle loop. Handles save state capture/restore, cartridge insertion, and reset logic. |
| `src/console/config.rs` | `Config` struct and CLI argument parser. Defines all command-line flags, config file loading (with priority: defaults → `~/.neser/neser.conf` → `./neser.conf` → `--config` → CLI args), and hardware/timing/input settings. |
| `src/console/cartridge_catalog.rs` | Scans directories for NES ROMs and builds/caches a CSV catalog of discovered cartridges for the TUI launcher. |
| `src/console/ram_init.rs` | RAM initialization modes: `Zero`, `Random`, and `SeededRandom` for deterministic test setups. |
| `src/cpu/` | MOS 6502 CPU implementation. |
| `src/cpu/cpu.rs` | The `Cpu` struct — register state, instruction fetch/decode/execute loop, interrupt handling (NMI, IRQ, BRK), and DMA integration. |
| `src/cpu/opcode.rs` | Opcode definitions and the instruction lookup table covering all official and unofficial 6502 opcodes. |
| `src/cpu/master_clock.rs` | Master clock divider that coordinates CPU, PPU, and APU cycle ratios for accurate NTSC/PAL timing. |
| `src/cpu/dma.rs` | OAM DMA and DMC DMA transfer logic (test-only module). |
| `src/ppu/` | Picture Processing Unit (2C02/2C07) implementation. |
| `src/ppu/ppu.rs` | The `Ppu` struct — coordinates all PPU subsystems per scanline/cycle. Contains a nested `ppu/` subdirectory with `tick.rs` for single-cycle PPU execution logic. |
| `src/ppu/background.rs` | Background tile fetching, shift registers, and fine-scroll handling. |
| `src/ppu/sprites.rs` | Sprite evaluation, OAM secondary buffer, and sprite-0 hit detection. |
| `src/ppu/rendering.rs` | Pixel compositing — merges background and sprite layers with priority logic. |
| `src/ppu/memory.rs` | PPU memory map — nametable mirroring, palette RAM, pattern table access through the cartridge mapper. |
| `src/ppu/registers.rs` | PPU register interface ($2000–$2007) including the internal v/t scroll latches and read buffer. |
| `src/ppu/timing.rs` | Scanline and dot-accurate timing, VBlank/pre-render logic, even/odd frame handling. |
| `src/ppu/screen_buffer.rs` | Double-buffered 256×240 framebuffer for completed frames. |
| `src/ppu/color_effects.rs` | Emphasis bits and grayscale color effects. |
| `src/ppu/status.rs` | PPU status register ($2002) with VBlank, sprite-0 hit, and overflow flags. |
| `src/apu/` | Audio Processing Unit (2A03) implementation. |
| `src/apu/apu.rs` | The `Apu` struct — mixer output, frame counter sequencing, sample generation. |
| `src/apu/pulse.rs` | Two pulse wave channels with sweep and envelope. |
| `src/apu/triangle.rs` | Triangle wave channel with linear counter. |
| `src/apu/noise.rs` | Noise channel with LFSR and envelope. |
| `src/apu/dmc.rs` | Delta Modulation Channel — sample playback with DMA fetches. |
| `src/apu/envelope.rs` | Shared envelope generator used by pulse and noise channels. |
| `src/apu/frame_counter.rs` | APU frame counter (4-step/5-step modes) driving length counter and envelope clocks. |
| `src/apu/length_counter.rs` | Shared length counter used by pulse, triangle, and noise channels. |
| `src/bus/` | System bus connecting all hardware components. |
| `src/bus/bus.rs` | The `Bus` struct — main address decoding and routing for the CPU address space ($0000–$FFFF). Manages device dispatch for reads/writes. |
| `src/bus/ram_device.rs` | 2KB CPU RAM ($0000–$07FF, mirrored to $1FFF). |
| `src/bus/ppu_device.rs` | Routes PPU register access ($2000–$3FFF). |
| `src/bus/apu_device.rs` | Routes APU register access ($4000–$4017). |
| `src/bus/oam_dma_device.rs` | OAM DMA transfer initiation ($4014). |
| `src/bus/controller_device.rs` | Controller port I/O ($4016–$4017), supporting standard joypads, Four Score, Zapper, Arkanoid paddle, and Famicom expansion devices. |
| `src/bus/mapper_device.rs` | Routes cartridge address space ($4018–$FFFF) to the mapper. |

#### Cartridge and Mapper System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/cartridge/` | Cartridge loading, ROM parsing, and mapper implementations. |
| `src/cartridge/cartridge.rs` | `Cartridge` struct — loads ROM files, parses iNES/NES2.0 headers, creates the appropriate mapper, and manages save files (.sav) and save states (.state). |
| `src/cartridge/ines.rs` | iNES and NES 2.0 header parser — extracts mapper number, PRG/CHR ROM sizes, mirroring, battery backup, timing mode, and console type. |
| `src/cartridge/mapper.rs` | `Mapper` trait definition and `mapper_registry!` macro that maps mapper numbers to concrete implementations. Contains the factory function `create_mapper()`. **207 mappers** are currently registered. |
| `src/cartridge/base_mapper.rs` | `BaseMapper` — shared infrastructure for all mappers: PRG/CHR bank selection (signed index with modulo wrapping), PRG-RAM allocation, mirroring control, and save-state banking snapshots. |
| `src/cartridge/common.rs` | Shared types: `ChrMemory` (CHR-ROM/RAM), `PrgRam`, `BankSwitch`, `BankedRom`, and `StateSnapshot` trait for mapper serialization. |
| `src/cartridge/mapper_templates.rs` | Reusable mapper templates: `SimpleFixedPrgMapper`, `SimpleBankedPrgMapper`, `DualBank32Mapper` for common banking patterns. |
| `src/cartridge/cpu_cycle_irq.rs` | CPU cycle-based IRQ counter shared by multiple mappers. |
| `src/cartridge/hardware_type.rs` | Hardware type detection for NES vs Famicom variants. |
| `src/cartridge/rom_db.rs` | ROM database lookup by CRC32 — identifies known ROMs for auto-detection of controller types, hardware quirks, and region hints. |
| `src/cartridge/rom_db.csv` | CSV database of ~10,400 known ROMs with CRC32, name, country, hardware, mapper, submapper, mirroring, PRG/CHR sizes, battery flag, VS hardware/PPU types, and expansion type. |
| `src/cartridge/test_helpers.rs` | Test utilities for mapper unit tests. |

##### Mapper Implementations by Manufacturer

| Directory | Mapper Count | Notable Mappers |
|-----------|:------------:|-----------------|
| `nintendo/` | 22 | NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), MMC5 (5), AxROM (7), MMC2/MMC4 (9/10), FDS (20), VS System (99) |
| `konami/` | 7 | VRC1 (75), Mapper 151 (151), VRC2/VRC4 (21–25), VRC3 (73), VRC6 (24/26), VRC7 (85) |
| `namco/` | 5 | Namco 118 (206), Namco 163 (19), Namcot 3425/3443/3446 |
| `sunsoft/` | 5 | Sunsoft-2 (93), Sunsoft-3 (67), Sunsoft-4 (68), FME-7 (69) |
| `irem/` | 5 | G-101 (32), H-3001 (65), TAM-S1 (97), LROG017 (77), NINA/Tengen (34) |
| `jaleco/` | 7 | JF-10 through JF-19, SS88006 (18), Mapper 87 |
| `taito/` | 4 | TC0190 (33/48), TC0350 (206 variant), X1-005 (80), X1-017 (82) |
| `bandai/` | 3 | Bandai FCG (16/153/159), Mapper 70, Mapper 96 |
| `sachen/` | 4 | Sachen mappers (36, 132, 133, 243) |
| `camerica/` | 1 | Camerica/Codemasters (71) |
| `tengen/` | 1 | RAMBO-1 (64) |
| `unlicensed/` | 136 | Multicarts, pirate mappers, bootleg boards (Color Dreams, Action 53, JY Company, and many numbered mappers) |

#### Input System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/input/` | Input device implementations. |
| `src/input/controller.rs` | `ControllerType` enum and input abstraction layer. |
| `src/input/nes_joypad.rs` | Standard NES joypad with 8-button serial protocol. |
| `src/input/arkanoid_controller.rs` | Arkanoid paddle controller with analog position and trigger. |
| `src/input/zapper.rs` | NES Zapper light gun with light detection. |
| `src/input/power_pad.rs` | Power Pad (Family Trainer) mat controller. |
| `src/input/snes_adapter.rs` | SNES-to-NES controller adapter. |

#### Frontends

| Directory/File | Description |
| ---------------- | ------------- |
| `src/sdl_frontend/` | Desktop frontend using SDL2. |
| `src/sdl_frontend/sdl_eventloop.rs` | Main event loop — handles input events, frame timing, VSync, autorun integration, pause/resume, and hot-reload of ROMs. |
| `src/sdl_frontend/sdl_audio.rs` | SDL2 audio device setup and sample queuing. |
| `src/sdl_frontend/sdl_audio_callback.rs` | Audio callback that pulls samples from a ring buffer fed by the APU. |
| `src/sdl_frontend/sdl_audio_resampler.rs` | Audio resampling to match SDL's actual sample rate. |
| `src/sdl_frontend/sdl_gl_wrapper.rs` | OpenGL context management for SDL2 windows. |
| `src/sdl_frontend/sdl_render_target.rs` | Renders the NES framebuffer to the SDL2 window via OpenGL. |
| `src/sdl_frontend/autorun_state.rs` | Autorun recording/playback state machine for the SDL frontend. |
| `src/rendering/` | Shared rendering infrastructure (SDL feature). |
| `src/rendering/gl_backend.rs` | OpenGL framebuffer and texture management. |
| `src/rendering/shader_manager.rs` | Shader pipeline using librashader — loads `.slangp` presets (CRT, NTSC, xBRZ). |
| `src/rendering/input.rs` | Input handling abstraction for the rendering layer. |
| `src/tui_frontend/` | Terminal UI ROM launcher using `ratatui` + `crossterm`. |
| `src/tui_frontend/app.rs` | TUI application state and event loop. |
| `src/tui_frontend/rom_list.rs` | Scrollable ROM list widget. |
| `src/tui_frontend/catalog.rs` | Integration with the cartridge catalog for ROM discovery. |
| `src/tui_frontend/launcher.rs` | Launches the SDL emulator for a selected ROM. |
| `src/tui_frontend/action_menu.rs` | Context menu for ROM actions. |
| `src/web_frontend/` | WebAssembly frontend. |
| `src/web_frontend/wasm.rs` | `wasm-bindgen` bindings — exposes `NesEmulator` to JavaScript with methods for frame stepping, input, audio sample retrieval, and save states. |
| `src/web_frontend/wasm_autorun_state.rs` | Autorun state management for the WASM frontend. |
| `src/web_frontend/wasm_tests.rs` | WASM-specific integration tests (run via `wasm-pack test`). |

#### Debugging

| Directory/File | Description |
| ---------------- | ------------- |
| `src/debugging/` | Debugging and diagnostic tools. |
| `src/debugging/ui.rs` | ImGui-based debugger UI with CPU state, memory viewer, and disassembly. |
| `src/debugging/disasm.rs` | 6502 disassembler for real-time instruction display. |
| `src/debugging/breakpoints.rs` | Breakpoint system — supports address breakpoints and conditional breaks. |
| `src/debugging/ppu_viewer.rs` | PPU nametable and pattern table viewer. |
| `src/debugging/tracing.rs` | CPU/PPU/APU/Mapper trace output at configurable verbosity levels. |
| `src/debugging/logging.rs` | Debug logging infrastructure. |
| `src/debugging/snapshot.rs` | Debugging state snapshots. |
| `src/debugging/types.rs` | Shared debugging types and constants. |

#### Autorun System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/autorun/` | Input recording and deterministic playback system. |
| `src/autorun/types.rs` | `AutorunFile` format — stores per-frame joypad input with periodic CRC checkpoints for regression testing. Supports versioned format (currently v3 with run-length encoding). |
| `src/autorun/headless_playback.rs` | Headless playback engine — replays input without rendering for automated verification. Compares CRC checksums at each checkpoint. |
| `src/autorun/utils.rs` | Utilities for loading, saving, converting, and trimming autorun files. |

#### Other Core Files

| File | Description |
| ------ | ------------- |
| `src/app_context.rs` | `AppContext` — shared application state including configuration, ROM database, and toast notification manager. Wrapped in `Rc<RefCell<>>` for interior mutability. |
| `src/frontend_toasts.rs` | Toast message formatters for user-facing notifications (cartridge loaded, hardware mode, gamepad detection, timing mode). |

#### Tests

| Directory/File | Description |
| ---------------- | ------------- |
| `src/integration_tests/` | Integration test suites. |
| `src/integration_tests/cpu_tests.rs` | CPU instruction and timing tests using Blargg test ROMs. |
| `src/integration_tests/ppu_tests.rs` | PPU rendering, timing, and register tests using Blargg test ROMs. |
| `src/integration_tests/apu_audio_tests.rs` | APU channel audio output verification. |
| `src/integration_tests/apu_visual_tests.rs` | APU tests that produce visual output (Blargg test ROMs). |
| `src/integration_tests/mapper_tests.rs` | Mapper-specific tests using holy-mapperel and other test ROMs. |
| `src/integration_tests/autorun_tests.rs` | Autorun recording/playback round-trip tests. |
| `src/integration_tests/input_tests.rs` | Controller input tests. |
| `src/integration_tests/ram_init_tests.rs` | RAM initialization mode tests. |
| `src/integration_tests/rom_test_runner.rs` | Generic test ROM harness — runs a ROM headlessly and checks for pass/fail output. |
| `src/integration_tests/romtest_harness.rs` | Shared infrastructure for ROM-based test assertions. |
| `src/integration_tests/manual_test_cartridges.rs` | Programmatically generated minimal test ROMs for specific hardware scenarios. |
| `src/integration_tests/miscellaneous_tests.rs` | Miscellaneous integration tests. |
| `build.rs` | Build script that scans `roms/games/mappers/` for `.autorun` files and generates per-ROM regression tests at compile time. |

### `web/` — Browser Frontend

The web frontend is a standalone HTML/JavaScript application that loads the WASM-compiled emulator core.

| File | Description |
| ------ | ------------- |
| `web/index.html` | Main page — canvas element, ROM file picker, keyboard shortcuts overlay. |
| `web/app.js` | Application bootstrapper — initializes the WASM module, sets up the render loop, and coordinates all subsystems. |
| `web/audio_resampler.js` | Resamples APU output to the Web Audio API's sample rate. |
| `web/gamepad.js` | Gamepad API integration for browser-based controller input. |
| `web/input_routing.js` | Keyboard and gamepad input routing to the emulator. |
| `web/frame_limiter.js` | Frame timing to maintain 60 FPS (NTSC) or 50 FPS (PAL). |
| `web/frame_plan.js` | Frame scheduling and render planning. |
| `web/rom_list.js` / `web/rom_selection.js` | ROM file management and selection UI. |
| `web/save_state_*.js` | Save state persistence using IndexedDB. |
| `web/debugger_*.js` | Browser-based debugger panels (disassembly, OAM viewer, watch expressions). |
| `web/ppu_viewer_*.js` | PPU pattern/nametable viewer for the browser. |
| `web/styles.css` | Application styling. |
| `web/integration/` | Playwright-based end-to-end integration tests for the web frontend. |

Each JavaScript module has a corresponding `.test.mjs` unit test file (run with `node --test`).

### `shaders/` — Visual Filters

Shader presets using the Slang shading language, loaded via librashader:

| File | Description |
| ------ | ------------- |
| `crt-lottes.slangp` | CRT simulation — scanlines, shadow mask, bloom, and curvature. |
| `ntsc-256px-composite.slangp` | NTSC composite video simulation with color bleeding and artifacts. |
| `xbrz-freescale.slangp` | xBRZ smooth pixel upscaling for clean, sharp output. |
| `stock.slang` / `stock.slangp` | Passthrough shader (no effect). |

### `roms/` — Test ROMs

| Directory | Description |
| --------- | ------------- |
| `roms/automated_tests/` | **70+ test ROM suites** used by the integration test harness. Includes Blargg's CPU/PPU/APU tests, DMA timing tests, mapper-specific tests (MMC3, MMC5, FME-7, VRC6), sprite tests, and more. |
| `roms/automated_tests/mapper_verification/` | Custom mapper verification ROMs built from assembly source with per-mapper test definitions. |
| `roms/manual_tests/` | ROMs for manual visual/audio verification (e.g., volume tests). |
| `roms/games/` | Game ROMs (not checked into version control). Subdirectories organized by mapper number for autorun regression tests. |

### `docs/` — Documentation

| File | Description |
| ------ | ------------- |
| `docs/MAPPER_SUPPORT.md` | Mapper support status and compatibility notes. |
| `docs/MAPPER_CAPABILITIES.md` | Per-mapper capability matrix (banking, IRQ, audio expansion, etc.). |
| `docs/MAPPERTOOL_UI_DESIGN.md` | Design document for the mappertool TUI. |
| `docs/architecture-diagrams.md` | Save-state architecture diagrams (current vs proposed). |

### `.github/` — CI/CD and Automation

#### CI Workflows

| Workflow | Description |
| ---------- | ------------- |
| `ci.yml` | Main CI pipeline. Runs on push to `main` and PRs. Jobs: Rust tests (`cargo test --lib --all-features`), Clippy lint, `cargo fmt` check, WASM build + test (`wasm-pack test`), web JS unit tests (`npm test`), web Playwright integration tests, and Python script tests. Uses path-based change detection to skip unchanged jobs. |
| `release.yml` | Release pipeline triggered by version tags (`v*.*.*`). Runs full CI, then cross-compiles release binaries for Linux (x86_64), macOS (x86_64 + aarch64), and Windows (x86_64). Windows builds bundle SDL2.dll and SDL2_ttf.dll. Publishes to GitHub Releases with a git-cliff changelog. |

#### Agentic Workflows (Copilot-powered)

| Workflow | Description |
| ---------- | ------------- |
| `bug-of-the-day.md` | Selects the highest-priority open bug issue, fixes it using the bug-hunter workflow, and creates a pull request. |
| `next-mapper.md` | After a PR is closed, selects a random open mapper issue, implements it with TDD, and creates a PR. |
| `code-simplifier.md` | Analyzes recently modified code and creates PRs with readability/maintainability improvements. |
| `daily-repo-status.md` | Generates daily repository activity reports as GitHub issues. |
| `issue-enhancer.md` | Automatically enhances issues with proper labeling and quality improvements. |

### Configuration

| File | Description |
|------|-------------|
| `neser.conf.example` | Annotated example configuration file documenting all settings: hardware mode (NES-NTSC/NES-PAL/Famicom), audio, video (VSync, window size, fullscreen, shaders), input (gamepads, Four Score, controller types, Zapper detection), debugging, RAM initialization, OAM DRAM decay, and overscan. |
| `gamecontrollerdb.txt` | SDL2 game controller mapping database for broad gamepad compatibility. |

### Build Configuration

| File | Description |
| ------ | ------------- |
| `Cargo.toml` | Rust project manifest. Defines three feature flags: `sdl` (default — desktop frontend), `wasm` (WebAssembly frontend), `tui` (terminal ROM launcher). The library crate type is both `rlib` (for tests) and `cdylib` (for WASM). Debug builds use `opt-level = 1` to keep audio smooth; dependencies use `opt-level = 3`. |
| `build.rs` | Compile-time code generation — scans for `.autorun` files and generates Rust test functions for each. |
| `playwright.config.mjs` | Playwright configuration for web integration tests. |
| `package.json` | Node.js project for web frontend testing (unit tests via `node --test`, integration via Playwright). |

## Key Design Decisions

- **Bus-centric architecture**: All memory access goes through the `Bus`, enabling accurate mapper intercepts and DMA behavior.
- **Cycle-accurate timing**: CPU, PPU, and APU are synchronized via a master clock divider. PPU runs 3 cycles per CPU cycle (NTSC) or 3.2 (PAL).
- **Feature-gated frontends**: SDL, WASM, and TUI frontends are behind Cargo features, so the core emulation library has no platform dependencies.
- **Interior mutability via `Rc<RefCell<>>`**: Components that need shared ownership (Bus, PPU, APU) use reference-counted cells rather than unsafe code.
- **Mapper trait pattern**: All mappers implement the `Mapper` trait with a standard interface for PRG/CHR reads/writes, IRQ management, and state snapshots. Common banking logic is provided by `BaseMapper`.
- **Deterministic testing**: RAM initialization modes and autorun recordings enable fully deterministic regression testing against reference CRC checksums.
- **Save state serialization**: Uses JSON (via serde) with a versioned format. Mapper state is serialized as opaque byte vectors to keep the format flexible.

## Testing Strategy

1. **Unit tests** — Extensive per-module tests throughout the codebase (run with `cargo test --lib`).
2. **ROM-based integration tests** — Blargg, holy-mapperel, and other community test ROMs verified via headless execution.
3. **Autorun regression tests** — Build-time generated tests that replay recorded input and verify CRC checkspoints.
4. **WASM tests** — Browser-environment tests via `wasm-pack test --headless --chrome`.
5. **JavaScript unit tests** — Web frontend JS modules tested with Node.js built-in test runner.
6. **Playwright integration tests** — End-to-end browser tests for the web frontend.
7. **Python tests** — Unit tests for the ROM scraper and mappertool utilities.
