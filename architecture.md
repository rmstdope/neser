# NESER Architecture

> NESER — NES Emulator in Rust

## Overview

NESER is a Nintendo emulator suite written in Rust, built on an architecture that supports multiple emulated hardware targets. It supports three frontend targets: a native desktop window (winit + OpenGL), a terminal-based TUI ROM launcher, and a WebAssembly-powered browser frontend. The emulator implements full NES hardware (CPU, PPU, APU, bus, and over 200 cartridge mappers), plus GB/GBC, GBA, and SNES runtimes with shared input, audio/video frontend plumbing, configuration, save states, and debugging infrastructure.

The native frontend now exposes audio buffering in milliseconds through configuration, while the web frontend uses a small profile selector for balanced and low-latency audio scheduling.

The codebase is roughly 383,000 lines of Rust, with additional JavaScript for the web frontend and Python tooling for ROM management.

As of version 0.3.0, NESER has been refactored to introduce a hardware-agnostic `Emulator` trait and a `Console` enum that wraps the NES, Game Boy, Game Boy Advance, and SNES implementations. This allows the native frontend and GL backend to dispatch common operations through the trait instead of matching on specific console variants or using NES-specific types directly. Optional capability accessors on `Console` expose mouse input, debugger-facing PPU viewer snapshots, and direct system handles for the few places that still need them. The architecture is designed to be extensible for future emulated systems while maintaining a clean separation between hardware-specific logic and shared platform/frontend code.

## High-Level Architecture

```none
┌───────────────────────────────────────────────────────┐
│                     Frontends                         │
│  ┌─────────────-─┐ ┌──────────────┐ ┌──────────────┐  │
│  │Native Frontend│ │ TUI Frontend │ │ WASM Frontend│  │
│  │(Desktop, GL)  │ │ (Terminal)   │ │ (Browser)    │  │
│  └──────┬──────-─┘ └──────┬───────┘ └──────┬───────┘  │
│         │                │                │           │
│         └────────────────┼────────────────┘           │
│                          ▼                            │
│  ┌─────────────────────────────────────────────────┐  │
│  │  Console enum + Emulator trait                  │  │
│  │  (src/platform/emulator.rs)                     │  │
│  │  Hardware-agnostic interface: run_tick, render, │  │
│  │  audio, input, save/load state, reset           │  │
│  │  Variants: Nes, GameBoy, GameBoyAdvance, Snes   │  │
│  └──────────────────────┬─────────────────────────-┘  │
│                         │                             │
│                         ▼                             │
│  ┌─────────────────────────────────────────────────┐  │
│  │           NES Emulator (src/nes/)               │  │
│  │  Nes struct orchestrates CPU ↔ PPU ↔ APU ↔ Bus  │  │
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
│                                                       │
│  ┌─────────────────────────────────────────────────┐  │
│  │  Shared Platform (src/)                         │  │
│  │  AppContext · FrontendConfig · Audio · Rendering│  │
│  └─────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────┘
```

The emulator is designed around a **multi-layer architecture**:

- **Emulator trait + Console enum** (`src/platform/emulator.rs`): The `Emulator` trait defines the common interface that every emulated system must implement (run, render, audio, input, save/load state, reset — 22 methods total). `Nes`, `GameBoy`, `Gba`, and `Snes` (stub) implement the trait in their respective modules. The `Console` enum wraps all four systems and delegates common methods through `as_core()`/`as_core_mut()` (which return `&dyn Emulator`), keeping a single pair of match arms instead of one per method. System-specific features (NES debugging, PPU viewer, Zapper) are still accessed by matching on `Console::Nes`.
- **NES emulator** (`src/nes/`): All NES-specific hardware lives under this namespace. The `Nes` struct in `src/nes/console/nes.rs` orchestrates the per-cycle stepping of CPU, PPU, APU, and Bus.
- **Shared platform** (`src/platform/`): `FrontendConfig` (src/platform/config/), `AppContext` (src/platform/app_context.rs), audio infrastructure, and system-agnostic toast formatters are shared across all emulated systems.
- **Bus-centric hardware**: Within the NES, the `Bus` struct routes memory reads and writes between the CPU, PPU registers, APU registers, RAM, OAM DMA, controller ports, and the cartridge mapper.

## Binaries and Scripts

### Rust Binaries

| Binary | Source | Feature | Description |
| --------- | -------- | --------- | ------------- |
| `neser` | `src/main.rs` | `native` (default) | Main emulator with native desktop window (winit + OpenGL), audio, gamepad input, shader filters, debugger, and autorun support. |
| `joysticks` | `src/bin/joysticks.rs` | `native` | Diagnostic utility that lists connected joysticks/gamepads, displays their GUID, and shows real-time axis/button state. |

The `src/bin/roms.rs` file is a library binary (accessed via `cargo run --bin roms`) that provides ROM management commands: `list` (scan a directory for NES ROMs), `info` (parse and display iNES/NES2.0 header details), and `infoall` (batch info for all ROMs).

### Shell Scripts

| Script | Description |
| -------- | ------------- |
| `scripts/build_web.sh` | Builds the WASM target with `cargo build --target wasm32-unknown-unknown --features wasm`, runs `wasm-bindgen` to generate JS glue code into `web/pkg/`, then bundles the web frontend with `npx vite build` into `dist/`. |
| `scripts/run_web.sh` | Symlinks `web/roms/` into `dist/` for ROM directory browsing, then starts a local HTTP server (`python3 -m http.server`) in `dist/` for testing the browser frontend. |
| `scripts/test-dir.sh` | Runs Rust tests for specific source directories. Converts directory paths (e.g., `src/nes/cartridge`) to `cargo test` module filters. Supports `--list` and `--skip-integration` (skips the `nes`/`gb`/`gba`/`snes` `integration_tests` modules, matching CI's unit-only fallback). CI mirrors the same path-to-filter mapping to conditionally run tests based on changed files. |
| `scripts/refresh_65816_processor_tests_subset.sh` | Refreshes a local full-corpus cache of SNES 65816 ProcessorTests from upstream (`SingleStepTests/ProcessorTests`) into `roms/snes/automated_tests/processor_tests/65816/full/v1`. This cache is intentionally git-ignored to keep repository size manageable. |
| `scripts/refresh_65816_processor_tests_subset.py` | Deterministically selects a committed 65816 CI subset from the local full corpus, requires paired emulation/native vectors for selected opcodes when available, truncates selected files to a configurable per-file vector cap (default 32) to keep committed assets compact, writes `v1/*.json`, and emits a machine-readable coverage report with tree-integrity metadata. |
| `scripts/refresh_spc700_processor_tests_subset.py` | Deterministically selects a committed SPC700 CI subset from the local full corpus, truncates selected files to a configurable per-file vector cap (default 32) to keep committed assets compact, writes `v1/*.json`, and emits a machine-readable coverage report with tree-integrity metadata. |

### Python Tools

| Tool | Description |
| ------ | ------------- |
| `scripts/sort_roms.py` | Sorts ROM files into mapper-numbered subdirectories based on their iNES header. |
| `scripts/package_release.py` | Builds per-target release archives with a top-level `neser/` directory. Includes the binary, runtime resources, README, LICENSE, config example, fonts, and only shader files reachable from configured shader presets. Excludes development scripts. |
| `scripts/verify_release_package.py` | Verifies release archive manifests for `.tar.gz` and `.zip`, checks Unix executable permissions when requested, extracts packages to a temporary directory, and can run a smoke command such as `./neser --version` from the package root. |
| `scripts/disassemble_rom.py` | Disassembles a NES ROM file and prints 6502 assembly output. |
| `scripts/display_audio_output.py` | Visualizes APU audio output data for debugging audio issues. |
| `scripts/mappertool/` | A Textual-based TUI application for browsing and managing a ROM database, inspecting mapper assignments, and cross-referencing ROM files with the embedded ROM database. |
| `scripts/scraper/` | Scrapes NES cartridge databases (NesCartDB, NES 2.0 XML) into a local SQLite database for ROM identification and mapper research. |

## Directory Structure

### `src/` — Rust Source Code

#### Platform Layer (Hardware-Agnostic)

| File | Description |
| ------ | ------------- |
| `src/platform/emulator.rs` | `Emulator` trait — defines the common interface (22 methods) for all emulated systems: `run_tick`, `is_ready_to_render`, `screen_snapshot`, `get_sample`, `set_button`, `save_state_bytes`/`load_state_bytes`, `reset`, etc. `Console` enum wraps the system cores, delegating common methods through `as_core()`/`as_core_mut()` and exposing optional accessors for mouse input, PPU viewer, and system-specific handles. |
| `src/platform/save_state.rs` | Shared save-state primitives used by all four cores: the `Stateful` trait (component-level capture/restore with an associated serializable `State` and an infallible `restore_state`), a console-agnostic `SaveStateError`, and generic `to_bytes`/`from_bytes` (JSON) plus a `check_version` helper. Console-specific errors (NES `MapperMismatch`, SNES `RomMismatch`) live in slim per-console enums that convert `From<SaveStateError>`; fallible restores are surfaced as `SaveStateError::RestoreFailed` at the console boundary. |
| `src/platform/config/` | `FrontendConfig` struct — generic frontend settings shared across all emulated systems — split into per-domain modules: `mod.rs` (data structs + thin `apply_args`/`apply_config_value` orchestrators), `cli.rs` (shared CLI machinery: flag table, parse helpers, validation, help text), and `audio`/`video`/`autorun`/`debugger`/`cartridge` (per-domain flag parsing). The `cartridge` module holds the `resolved_metadata_db_path()`, `resolved_image_cache_path()`, and `resolved_favorites_path()` helpers. |
| `src/platform/app_context.rs` | `AppContext` — shared application state including configuration, ROM database, and toast notification manager. Wrapped in `Rc<RefCell<>>` for interior mutability. |
| `src/platform/catalog/` | Shared ROM catalog module — discovers ROMs, parses headers, enriches entries with metadata and cover art. Used by both TUI and native graphical frontends. |
| `src/platform/catalog/mod.rs` | `load_catalog()` builds sorted `Vec<RomEntry>` from disk scan; `enrich_catalog()` integrates metadata matching and cover art downloading. |
| `src/platform/catalog/rom_entry.rs` | `RomEntry` struct — a discovered ROM enriched with iNES header data, ROM DB lookup results, TheGamesDB metadata (genres, overview, rating, etc.), cover art paths, and favorite status. |
| `src/platform/catalog/favorites.rs` | `Favorites` struct — persistent favorites storage backed by `~/.neser/favorites.json`. Supports load, toggle, save, and contains operations. |
| `src/platform/metadata/` | TheGamesDB metadata access via SQLite (`rusqlite`). |
| `src/platform/metadata/db.rs` | `MetadataDb` — opens `metadata.db` and queries game metadata (title, overview, genres, release date, players, rating, image filenames). |
| `src/platform/metadata/matcher.rs` | Fuzzy title matching using `strsim::jaro_winkler` to link ROM DB names to TheGamesDB entries. |
| `src/platform/image_cache/` | Cover art download and caching system. Downloads front boxart and screenshots from TheGamesDB CDN into `~/.neser/image_cache/` using `reqwest`. |

#### NES Emulation (`src/nes/`)

All NES-specific hardware and supporting code lives under `src/nes/`.

| Directory/File | Description |
| ---------------- | ------------- |
| `src/nes/mod.rs` | Module declarations for all NES sub-modules. |
| `src/nes/console/` | Top-level NES orchestration. |
| `src/nes/console/nes.rs` | The `Nes` struct — creates and owns CPU, PPU, APU, and Bus. Runs the master clock cycle loop. Handles save state capture/restore, cartridge insertion, and reset logic. Implements the `Emulator` trait for system-agnostic dispatch. |
| `src/nes/console/config.rs` | `Config` struct (composition of `FrontendConfig` + `NesConfig`), `NesConfig` struct (NES-specific hardware settings), and CLI argument parser. Defines all command-line flags, config file loading, and hardware/timing/input settings. |
| `src/nes/console/cartridge_catalog.rs` | Scans directories for NES ROMs and builds/caches a CSV catalog of discovered cartridges for the TUI launcher. |
| `src/nes/console/ram_init.rs` | RAM initialization modes: `Zero`, `Random`, and `SeededRandom` for deterministic test setups. |
| `src/nes/cpu/` | MOS 6502 CPU implementation. |
| `src/nes/cpu/cpu.rs` | The `Cpu` struct — register state, instruction fetch/decode/execute loop, interrupt handling (NMI, IRQ, BRK), and DMA integration. |
| `src/nes/cpu/opcode.rs` | Opcode definitions and the instruction lookup table covering all official and unofficial 6502 opcodes. |
| `src/nes/cpu/master_clock.rs` | Master clock divider that coordinates CPU, PPU, and APU cycle ratios for accurate NTSC/PAL timing. |
| `src/nes/cpu/dma.rs` | OAM DMA and DMC DMA transfer logic (test-only module). |
| `src/nes/ppu/` | Picture Processing Unit (2C02/2C07) implementation. |
| `src/nes/ppu/ppu.rs` | The `Ppu` struct — coordinates all PPU subsystems per scanline/cycle. Contains a nested `ppu/` subdirectory with `tick.rs` for single-cycle PPU execution logic. |
| `src/nes/ppu/background.rs` | Background tile fetching, shift registers, and fine-scroll handling. |
| `src/nes/ppu/sprites.rs` | Sprite evaluation, OAM secondary buffer, and sprite-0 hit detection. |
| `src/nes/ppu/rendering.rs` | Pixel compositing — merges background and sprite layers with priority logic. |
| `src/nes/ppu/memory.rs` | PPU memory map — nametable mirroring, palette RAM, pattern table access through the cartridge mapper. |
| `src/nes/ppu/registers.rs` | PPU register interface ($2000–$2007) including the internal v/t scroll latches and read buffer. |
| `src/nes/ppu/timing.rs` | Scanline and dot-accurate timing, VBlank/pre-render logic, even/odd frame handling. |
| `src/nes/ppu/screen_buffer.rs` | Double-buffered 256×240 framebuffer for completed frames. |
| `src/nes/ppu/color_effects.rs` | Emphasis bits and grayscale color effects. |
| `src/nes/ppu/system_palettes.rs` | Preset NES system palettes (`NesPalette` enum + RGB tables). Selectable via `nes-palette` config and cycled at runtime with F8. |
| `src/nes/ppu/status.rs` | PPU status register ($2002) with VBlank, sprite-0 hit, and overflow flags. |
| `src/nes/apu/` | Audio Processing Unit (2A03) implementation. |
| `src/nes/apu/apu.rs` | The `Apu` struct — mixer output, frame counter sequencing, sample generation. |
| `src/nes/apu/pulse.rs` | Two pulse wave channels with sweep and envelope. |
| `src/nes/apu/triangle.rs` | Triangle wave channel with linear counter. |
| `src/nes/apu/noise.rs` | Noise channel with LFSR and envelope. |
| `src/nes/apu/dmc.rs` | Delta Modulation Channel — sample playback with DMA fetches. |
| `src/nes/apu/envelope.rs` | Shared envelope generator used by pulse and noise channels. |
| `src/nes/apu/frame_counter.rs` | APU frame counter (4-step/5-step modes) driving length counter and envelope clocks. |
| `src/nes/apu/length_counter.rs` | Shared length counter used by pulse, triangle, and noise channels. |
| `src/nes/bus/` | System bus connecting all hardware components. |
| `src/nes/bus/bus.rs` | The `Bus` struct — main address decoding and routing for the CPU address space ($0000–$FFFF). Manages device dispatch for reads/writes. |
| `src/nes/bus/ram_device.rs` | 2KB CPU RAM ($0000–$07FF, mirrored to $1FFF). |
| `src/nes/bus/ppu_device.rs` | Routes PPU register access ($2000–$3FFF). |
| `src/nes/bus/apu_device.rs` | Routes APU register access ($4000–$4017). |
| `src/nes/bus/oam_dma_device.rs` | OAM DMA transfer initiation ($4014). |
| `src/nes/bus/controller_device.rs` | Controller port I/O ($4016–$4017), supporting standard joypads, Four Score, Zapper, Arkanoid paddle, and Famicom expansion devices. |
| `src/nes/bus/mapper_device.rs` | Routes cartridge address space ($4018–$FFFF) to the mapper. |

#### Cartridge and Mapper System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/nes/cartridge/` | Cartridge loading, ROM parsing, and mapper implementations. |
| `src/nes/cartridge/cartridge.rs` | `Cartridge` struct — loads ROM files, parses iNES/NES2.0 headers, creates the appropriate mapper, and manages save files (.sav) and save states (.state). |
| `src/nes/cartridge/ines.rs` | iNES and NES 2.0 header parser — extracts mapper number, PRG/CHR ROM sizes, mirroring, battery backup, timing mode, and console type. |
| `src/nes/cartridge/mapper.rs` | `Mapper` trait definition and `mapper_registry!` macro that maps mapper numbers to concrete implementations. Contains the factory function `create_mapper()`. **207 mappers** are currently registered. |
| `src/nes/cartridge/base_mapper.rs` | `BaseMapper` — shared infrastructure for all mappers: PRG/CHR bank selection (signed index with modulo wrapping), PRG-RAM allocation, mirroring control, and save-state banking snapshots. |
| `src/nes/cartridge/common.rs` | Shared types: `ChrMemory` (CHR-ROM/RAM), `PrgRam`, `BankSwitch`, `BankedRom`, and `StateSnapshot` trait for mapper serialization. |
| `src/nes/cartridge/mapper_templates.rs` | Reusable mapper templates: `SimpleFixedPrgMapper`, `SimpleBankedPrgMapper`, `DualBank32Mapper` for common banking patterns. |
| `src/nes/cartridge/cpu_cycle_irq.rs` | CPU cycle-based IRQ counter shared by multiple mappers. |
| `src/nes/cartridge/hardware_type.rs` | Hardware type detection for NES vs Famicom variants. |
| `src/nes/cartridge/rom_db.rs` | ROM database lookup by CRC32 — identifies known ROMs for auto-detection of controller types, hardware quirks, and region hints. |
| `src/nes/cartridge/rom_db.csv` | CSV database of ~10,400 known ROMs with CRC32, name, country, hardware, mapper, submapper, mirroring, PRG/CHR sizes, battery flag, VS hardware/PPU types, and expansion type. |
| `src/nes/cartridge/test_helpers.rs` | Test utilities for mapper unit tests. |

| `src/nes/cartridge/` (cont.) | |

##### Mapper Implementations by Manufacturer

| Directory | Mapper Count | Notable Mappers |
|-----------|:------------:|-----------------|
| `src/nes/cartridge/nintendo/` | 22 | NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), MMC5 (5), AxROM (7), MMC2/MMC4 (9/10), FDS (20), VS System (99) |
| `src/nes/cartridge/konami/` | 7 | VRC1 (75), Mapper 151 (151), VRC2/VRC4 (21–25), VRC3 (73), VRC6 (24/26), VRC7 (85) |
| `src/nes/cartridge/namco/` | 5 | Namco 118 (206), Namco 163 (19), Namcot 3425/3443/3446 |
| `src/nes/cartridge/sunsoft/` | 5 | Sunsoft-2 (93), Sunsoft-3 (67), Sunsoft-4 (68), FME-7 (69) |
| `src/nes/cartridge/irem/` | 5 | G-101 (32), H-3001 (65), TAM-S1 (97), LROG017 (77), NINA/Tengen (34) |
| `src/nes/cartridge/jaleco/` | 7 | JF-10 through JF-19, SS88006 (18), Mapper 87 |
| `src/nes/cartridge/taito/` | 4 | TC0190 (33/48), TC0350 (206 variant), X1-005 (80), X1-017 (82) |
| `src/nes/cartridge/bandai/` | 3 | Bandai FCG (16/153/159), Mapper 70, Mapper 96 |
| `src/nes/cartridge/sachen/` | 4 | Sachen mappers (36, 132, 133, 243) |
| `src/nes/cartridge/camerica/` | 1 | Camerica/Codemasters (71) |
| `src/nes/cartridge/tengen/` | 1 | RAMBO-1 (64) |
| `src/nes/cartridge/unlicensed/` | 136 | Multicarts, pirate mappers, bootleg boards (Color Dreams, Action 53, JY Company, and many numbered mappers) |

#### Input System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/nes/input/` | NES input device implementations. |
| `src/nes/input/controller.rs` | `ControllerType` enum and input abstraction layer. |
| `src/nes/input/nes_joypad.rs` | Standard NES joypad with 8-button serial protocol. |
| `src/nes/input/arkanoid_controller.rs` | Arkanoid paddle controller with analog position and trigger. |
| `src/nes/input/zapper.rs` | NES Zapper light gun with light detection. |
| `src/nes/input/power_pad.rs` | Power Pad (Family Trainer) mat controller. |
| `src/nes/input/snes_adapter.rs` | SNES-to-NES controller adapter. |

#### Game Boy Emulation (`src/gb/`)

All Game Boy (DMG) hardware lives under `src/gb/`. The module is structured around the `GbBus` trait so the SM83 CPU remains bus-agnostic and unit-testable with stub buses.

| Directory/File | Description |
| ---------------- | ------------- |
| `src/gb/mod.rs` | Module declarations for all GB sub-modules. |
| `src/gb/console/mod.rs` | Module declarations and re-exports for the GB console layer. Re-exports `Gb` and `CpuTraceLine` so call sites can keep using `crate::gb::console::Gb`. |
| `src/gb/console/gb.rs` | `Gb<B: GbBus>` — thin console shell that owns the CPU. `step()` executes one instruction and ticks the bus by the elapsed M-cycles. DMG-specific impls for screen, frame-ready, and reset. |
| `src/gb/console/gameboy.rs` | `GameBoy` — platform-facing wrapper that owns a `Gb<DmgBus>` (created lazily on `load_rom`). Implements the `Emulator` trait for system-agnostic dispatch. |
| `src/gb/console/save_state.rs` | Versioned DMG/CGB save-state serialization. Captures CPU, bus, cartridge RAM, opaque MBC state, and optional minimal SGB command/input state; CGB-specific fields include double-speed accumulator state used by APU/cartridge RTC timing and CGB 0-D extra-OAM RAM state. |
| `src/gb/bus/bus.rs` | `GbBus` trait — `read(&mut self, addr: u16) -> u8`, `write(&mut self, addr: u16, val: u8)`, and a default no-op `tick(&mut self, m_cycles: u8)`. `StubBus` implements the trait for unit tests. |
| `src/gb/bus/dmg_bus.rs` | `DmgBus` — full DMG memory map. Routes all 16-bit addresses to cartridge ROM/RAM, VRAM, WRAM, echo RAM, OAM, HRAM, Timer registers ($FF04–$FF07), APU registers ($FF10–$FF3F), IF ($FF0F), IE ($FFFF), and I/O stubs. Owns the cartridge, Timer, and APU. Overrides `tick()` to advance the Timer, APU, and propagate timer interrupts to IF. Exposes `sample_ready()`/`take_sample()`/`set_audio_sample_rate()` for the platform audio layer. `DmgBus::new_sgb` enables a narrow SGB `$FF00` command/input overlay for SGB-specific test ROMs without implying full SGB emulation. |
| `src/gb/bus/cgb_bus.rs` | `CgbBus` — full CGB memory map with VRAM/WRAM banking, CGB 0-D extra-OAM RAM / CGB-E `$FEA0-$FEFF` behavior, HDMA, KEY0/KEY1 speed switching, CGB boot ROM handling, and double-speed timing. In double speed it half-rates real-time peripherals such as the APU and cartridge RTC while preserving accumulator phase in save states. |
| `src/gb/apu/mod.rs` | Module declarations and re-exports for the GB APU. Re-exports `Apu` from the sibling implementation file. |
| `src/gb/apu/apu.rs` | `Apu` — DMG Audio Processing Unit. 8-step frame sequencer (512 Hz), NR50/NR51/NR52 power/volume/panning control, mixer (NR51 L/R routing, NR50 master volume), sample output pipeline (fractional M-cycle accumulator). |
| `src/gb/apu/channel1.rs` | `Channel1` — Pulse channel with frequency sweep. Duty cycle (4 patterns), length counter, volume envelope, frequency sweep (period, direction, shift). |
| `src/gb/apu/channel2.rs` | `Channel2` — Pulse channel without sweep. Duty cycle, length counter, volume envelope. |
| `src/gb/apu/channel3.rs` | `Channel3` — Wave output channel. 32-nibble wave RAM ($FF30–$FF3F), 4 output levels (mute/100%/50%/25%), length counter, wave position advancing at half the pulse-channel rate. |
| `src/gb/apu/channel4.rs` | `Channel4` — Noise channel. 15-bit or 7-bit LFSR, 8 clock divisor codes × 8 shift values = 64 noise frequencies, length counter, volume envelope. |
| `src/gb/cpu/sm83.rs` | `Sm83<B: GbBus>` — SM83/LR35902 CPU core. Full instruction set (primary + CB-prefixed), HALT bug, STOP state/speed-switch handling, and interrupt dispatch at five vectors. Each M-cycle increments an internal counter used by the console for bus ticking. |
| `src/gb/cpu/opcode.rs` | Opcode metadata tables (BASE[256] and CB[256]) for debugging and tracing. |
| `src/gb/ppu/mod.rs` | Module declarations and re-exports for the GB PPU. Re-exports `Ppu` so the public path remains `crate::gb::ppu::Ppu`. |
| `src/gb/ppu/ppu.rs` | `Ppu` — DMG/CGB LCD controller. Owns VRAM/OAM, LCD/STAT timing state, screen buffer, CGB palette RAM/bank state, STAT/VBlank interrupt generation, OAM corruption helpers, and dispatches per-dot Pixel FIFO rendering for all GB modes. |
| `src/gb/ppu/pixel_fifo.rs` | Dot-stepped DMG/CGB pixel renderer. Samples palette and LCDC writes as pixels are emitted, schedules OBJ fetch stalls in the visible pixel stream, tracks per-OBJ-fetch LCDC samples for DMG/CGB-DMG-compat rendering, caches scanline sprite candidates, and writes completed pixels directly into the screen buffer. |
| `src/gb/ppu/obj_fifo.rs` | DMG/CGB-DMG-compat OBJ fetch policy helpers and low-level tests for LCDC.1 object-fetch start/cancel behavior. |
| `src/gb/ppu/rendering.rs` | Shared DMG and CGB palette conversion helpers used by the Pixel FIFO renderer. |
| `src/gb/ppu/sprites.rs` | GB/CGB sprite scan, pixel fetch, priority ordering, separate low/high-byte OBJ-size sampling for DMG fetches, and Mode 3 OBJ penalty helpers. |
| `src/gb/timer/timer.rs` | `Timer` — DIV/TIMA/TMA/TAC subsystem. `tick(m_cycles)` advances counters and sets `interrupt_pending` on TIMA overflow; caller (DmgBus) propagates this to IF. |
| `src/gb/cartridge/cartridge.rs` | `GbCartridge` trait plus ROM loader surface. `load_cartridge(bytes: &[u8]) -> Result<Box<dyn GbCartridge>, RomError>` validates the header checksum and returns the appropriate MBC implementation. |
| `src/gb/cartridge/mbc0.rs` | ROM-only cartridge (MBC type 0x00) and ROM+RAM cartridges (types 0x08/0x09). ROM+RAM uses a fixed 32 KiB ROM window plus externally enabled/wrapped SRAM for homebrew/test ROM compatibility. |
| `src/gb/cartridge/mbc1.rs` | MBC1 cartridge (types 0x01–0x03). ROM bank switching ($2000–$3FFF), secondary bank register ($4000–$5FFF), banking mode ($6000–$7FFF), RAM enable ($0000–$1FFF). Supports up to 2 MB ROM and 32 KB RAM. |
| `src/gb/cartridge/mod.rs` | Module declarations and re-exports for GB cartridge support. Re-exports `GbCartridge`, `RomError`, and `load_cartridge`. |
| `src/gb/sgb.rs` | Minimal Super Game Boy command/input state used by explicit SGB test helpers. Decodes SGB packets sent through `$FF00` and implements only `MLT_REQ` player selection/current-player behavior; full SGB rendering, borders, palettes, and sound remain out of scope. |
| `src/gb/integration_tests/acid_tests.rs` | Headless automation for GBEmulatorShootout acid rendering and hardware-probe ROMs. Runs `dmg-acid2.gb`, `cgb-acid2.gbc`, `cgb-acid-hell.gbc`, and `which.gb` on the relevant DMG/CGB models and asserts screen CRCs. |
| `src/gb/integration_tests/ax6_tests.rs` | Headless automation for ax6 `rtc3test` MBC3 RTC validation. Runs the GBEmulatorShootout split ROMs (`rtc3test-1.gb` through `rtc3test-3.gb`) on both DMG and CGB hardware modes, captures result-screen PNGs with `NESER_CAPTURE_SCREEN=1`, and asserts reviewed screen CRCs. |
| `src/gb/integration_tests/cpp_tests.rs` | Headless automation for CasualPokePlayer GBEmulatorShootout MBC3/RTC ROMs. Runs `rtc-invalid-banks-test.gb`, `latch-rtc-test.gb`, and `ramg-mbc3-test.gb` on DMG and CGB paths and asserts CRCs against the upstream reference PNGs. |
| `src/gb/integration_tests/samesuite_sgb_tests.rs` | SameSuite SGB command integration tests. Runs `command_mlt_req.gb` and `command_mlt_req_1_incrementing.gb` on DMG-B with the explicit SGB input overlay and asserts the Mooneye-compatible pass marker. |

#### Game Boy Advance Emulation (`src/gba/`)

All Game Boy Advance hardware lives under `src/gba/`. The module currently provides the ARM7TDMI CPU core, the system memory bus / I/O register foundation, the cartridge loader (header parsing + auto-detected SRAM/EEPROM/Flash save backends), the PPU foundation (display register dispatch, scanline timing, V/H-Blank IRQs, and Mode 3 bitmap rendering), the keypad / key-interrupt subsystem (`KEYINPUT` / `KEYCNT`), and a built-in open-source BIOS (`src/gba/bios/`) that eliminates the need for proprietary BIOS dumps; subsequent phases will add the remaining display modes, sprite (OBJ) rendering and the APU.

| Directory/File | Description |
| ---------------- | ------------- |
| `src/gba/mod.rs` | Game Boy Advance module root. Re-exports `Gba` (platform-facing wrapper), the `cpu`, `input`, `ppu` and `bios` sub-modules, `GbaBus`, `Ppu`, `Keypad`, and `GbaCartridge` / `SaveType` / `load_cartridge`. Includes `#[cfg(test)]` GBA integration test modules under `src/gba/integration_tests/`. |
| `src/gba/bios/mod.rs` | Open-source BIOS module. Embeds the pre-built 16KB binary via `include_bytes!` and exports `EMBEDDED_BIOS`. Contains unit tests for BIOS functional correctness (div, sqrt, checksum, boot). |
| `src/gba/bios/bios.s` | ARM assembly source for the open-source GBA BIOS. Implements exception vectors, IRQ dispatcher, SWI dispatch (Div, Sqrt, Halt, IntrWait, SoftReset, etc.), and boot sequence. |
| `src/gba/bios/bios.ld` | Linker script producing exactly 16384 bytes flat binary at base address 0x00000000. |
| `src/gba/bios/Makefile` | Build instructions for the BIOS binary using `arm-none-eabi-as`/`arm-none-eabi-ld`/`arm-none-eabi-objcopy`. |
| `src/gba/bios/bios.bin` | Pre-built 16KB BIOS binary, committed to the repo for CI/users without the ARM toolchain. |
| `src/gba/console/gba.rs` | `Gba` — platform-facing GBA wrapper implementing the `Emulator` trait. Owns ARM7TDMI + bus, executes per-instruction ticks, handles IRQ line dispatch, frame-ready signaling, ROM loading, and GBA save-state capture/restore. Falls back to the built-in BIOS when no external BIOS is available. |
| `src/gba/console/save_state.rs` | Versioned GBA save-state serialization. Captures CPU state, bus memory regions, bus-owned simple peripherals (I/O backing, interrupt controller, timers, DMA, SIO, keypad, WAITCNT-derived wait states, HALTCNT request state), PPU registers/timing/framebuffer state, APU channel/FIFO/mixer/timing state, cartridge save-backend state (SRAM/EEPROM/Flash data and command parser state), and scalar open-bus/BIOS-lock fields while intentionally excluding BIOS and ROM bytes. |
| `src/gba/integration_tests/mod.rs` | GBA integration test module root. Includes gba-suite runner and test definitions. |
| `src/gba/integration_tests/gba_suite_runner.rs` | Headless harness for GBA validation ROMs including jsmolka `gba-tests`, FuzzARM, ArmWrestler, and `mgba-emu/suite`. It loads ROM assets from `roms/gba/automated_tests/`, injects the embedded BIOS where needed, drives interactive menus, captures framebuffer CRCs, and can write PNG checkpoints under `target/gba_suite_checkpoints/` when `NESER_CAPTURE_SCREEN=1`. |
| `src/gba/integration_tests/gba_suite_tests.rs` | ROM-level GBA integration tests for CPU, memory, save, PPU, ArmWrestler, and `mgba-emu/suite` coverage. mGBA Video subtests are represented as per-subtest actual-vs-expected framebuffer assertions, with currently failing subtests individually ignored and linked to tracking issues. |
| `src/gba/integration_tests/save_state_tests.rs` | End-to-end GBA save-state integration tests that save, dirty CPU/bus/PPU state, restore, and verify screen CRC plus state markers return to the saved point. |
| `src/gba/cpu/mod.rs` | Module root for the ARM7TDMI core. Re-exports `Arm7tdmi`, `Bus`, `RamBus`, `Registers`, `CpuMode`, etc. |
| `src/gba/cpu/registers.rs` | `Registers` — ARM7TDMI register file with R0–R15, CPSR, and per-mode banked SPSR/SP/LR (and FIQ-banked R8–R12). Includes `CpuMode` (USR/FIQ/IRQ/SVC/ABT/UND/SYS) and `condition_met` for the 16 ARM condition codes. |
| `src/gba/cpu/bus.rs` | `Bus` trait used by the CPU for byte/halfword/word reads and writes, plus a flat little-endian `RamBus` implementation used by tests and boot stubs. |
| `src/gba/cpu/arm.rs` | ARM 32-bit instruction decoder/executor. Covers data processing, branch (B/BL), branch-and-exchange (BX), single-data transfer (LDR/STR/LDRB/STRB) and SWI; honours conditional execution. |
| `src/gba/cpu/thumb.rs` | Thumb 16-bit instruction decoder/executor. Covers move-shifted register, add/subtract, MOV/CMP/ADD/SUB immediate, hi-register operations, BX, PC-relative load, PUSH/POP and conditional/unconditional branches. |
| `src/gba/cpu/arm7tdmi.rs` | `Arm7tdmi` — fetch/decode/execute pipeline, S/N cycle accounting, exception vectors, and IRQ/FIQ/SWI dispatch (banked register handling, CPSR→SPSR save, vector jump). |
| `src/gba/bus/mod.rs` | GBA bus module root. Keeps declarations/re-exports minimal and delegates implementation to focused sibling files. |
| `src/gba/bus/gba_bus.rs` | `GbaBus` storage and inherent API: memory/peripheral ownership, BIOS/ROM loading, stepping peripherals, DMA trigger hooks, save-state capture/restore, open-bus helpers, and mGBA debug console state. |
| `src/gba/bus/cpu_bus.rs` | CPU `Bus` trait implementation for `GbaBus`, including full GBA address-space routing, read/write side effects, WAITCNT updates, HALTCNT handling, and Game Pak prefetch timing helpers. |
| `src/gba/bus/dma_bus.rs` | `DmaBus` implementation for `GbaBus`, preserving DMA-specific latch/open-bus behavior while routing transfers through the standard bus paths. |
| `src/gba/bus/waitstates.rs` | `Waitstates` and `WidthClass`, including WAITCNT-derived N/S cycle lookup tables for BIOS/RAM/I/O/video/Game Pak regions. |
| `src/gba/bus/addressing.rs` | Shared bus address helpers for VRAM mirroring, timer/DMA control register decoding, Game Pak wait-state predicates, and no-cart open-bus values. |
| `src/gba/bus/memory.rs` | Backing-store sizes (BIOS/EWRAM/IWRAM/PRAM/VRAM/OAM/SRAM) and helpers for little-endian halfword/word access with mirrored offsets. |
| `src/gba/bus/io.rs` | `IoRegisters` — dispatch table for the `0x0400_0000`–`0x0400_03FF` I/O window. Routes interrupt-controller and timer registers to live state and provides a backing store for the remaining ~300 registers so unimplemented PPU/APU/DMA registers don't panic. |
| `src/gba/bus/interrupt.rs` | `InterruptController` — `IE`/`IF`/`IME` registers with write-1-to-clear `IF` semantics and an `irq_line()` predicate consumed by the CPU. |
| `src/gba/bus/timer.rs` | `Timers` / `Timer` — 4-channel 16-bit timer bank with prescalers (1/64/256/1024), cascade mode, reload latching, enable rising-edge load and overflow IRQ generation. |
| `src/gba/bus/sio.rs` | `Sio` — Serial I/O controller implementing the transfer state machine. Normal 8/32-bit transfers complete after baud-rate-dependent cycle counts (512/64/2048/256); Multiplayer mode stays busy indefinitely (no peers). Raises IRQ_SIO on completion when enabled. |
| `src/gba/input/mod.rs` | Input subsystem module root. Re-exports `Keypad` and the keypad register addresses / `KEYCNT` flag constants. |
| `src/gba/input/keypad.rs` | `Keypad` — `KEYINPUT` (P1, `0x04000130`) and `KEYCNT` (`0x04000132`) registers. Tracks the 10 GBA buttons (A, B, Select, Start, Up, Down, Left, Right, L, R) with active-low read semantics, exposes `set_button` / `set_states` / `get_states` for frontend routing, and raises the keypad IRQ (`IF` bit 12 / IRQ3) per `KEYCNT`'s IRQ-enable + AND/OR condition bits. |
| `src/gba/ppu/mod.rs` | `Ppu` — display controller foundation. Owns `DISPCNT` / `DISPSTAT` / `VCOUNT`, drives scanline/dot timing (1232 cycles × 228 lines = 280 896 cycles per frame), maintains V-Blank/H-Blank/V-Counter status flags, raises the matching IRQs and renders Mode 3 (240×160 15-bit direct bitmap from VRAM) plus a backdrop fill from PRAM[0]. Owns the BG2/BG3 affine register file (write-only `0x0400_0020..=0x0400_003E`), routed in via `write_affine`. Stepped from `GbaBus::step`, which forwards V-Blank/H-Blank edges to the DMA hooks. |
| `src/gba/ppu/affine.rs` | `BgAffine` — per-background affine register file (PA/PB/PC/PD as signed 8.8 fixed-point `i16`; X/Y as signed 19.8 fixed-point `i32` sign-extended from bit 27 of the 28-bit hardware field). Provides the halfword write helpers (`write_x_low`/`write_x_high`/`write_y_low`/`write_y_high`) used by the bus dispatcher; the renderer that consumes these values is a follow-up sub-issue. |
| `src/gba/ppu/color.rs` | 15-bit BGR555 → 24-bit RGB888 palette conversion (`expand5_to_8`, `bgr555_to_rgb888`, `write_pixel`) using the canonical `c8 = (c5 << 3) | (c5 >> 2)` channel widening. |
| `src/gba/cartridge/mod.rs` | Cartridge module root. Re-exports `GbaCartridge`, `load_cartridge`, `SaveType`, and the per-backend types. |
| `src/gba/cartridge/header.rs` | `GbaHeader` parser — reads title, game code, maker code, the fixed `0x96` byte and computes/validates the header complement check (offsets `0x0A0..=0x0BC`). |
| `src/gba/cartridge/save_type.rs` | `SaveType` enum (`None` / `Sram32K` / `Eeprom512` / `Eeprom8K` / `Flash64K` / `Flash128K`) and the `detect_save_type` heuristic that scans 4-byte aligned ROM offsets for `EEPROM_V`, `SRAM_V`, `FLASH_V`, `FLASH512_V`, `FLASH1M_V`. |
| `src/gba/cartridge/sram.rs` | `Sram` — 32 KB battery-backed SRAM with mirrored read/write and `snapshot`/`restore` for `.sav` flush. |
| `src/gba/cartridge/eeprom.rs` | `Eeprom` — 512 B / 8 KB EEPROM bit-serial I²C state machine (`write_bit` / `read_bit`) handling read & write transactions over 6/14-bit address busses. |
| `src/gba/cartridge/flash.rs` | `Flash` — 64 KB single-bank / 128 KB dual-bank Flash backend implementing the JEDEC magic-write command sequence: byte program, sector erase, chip erase, ID readback (`0x90`/`0xF0`) and bank switch (`0xB0`). |
| `src/gba/cartridge/cartridge.rs` | `GbaCartridge` aggregate — owns the ROM image, parsed header and selected `SaveBackend`. `load_cartridge` validates size (≤ 32 MB), parses the header and constructs the matching backend. |
| `src/gba/debugging/mod.rs` | GBA debugging support module root (excluded from WASM builds). Re-exports `Breakpoints`, `CpuTrace`, `TraceEntry`, `GbaDebuggerController`, and the `disasm_arm` / `disasm_thumb` formatters. |
| `src/gba/debugging/disasm.rs` | ARM 32-bit and Thumb 16-bit disassembler — formats the subset of instructions implemented by the executor (data-processing, B/BL, BX, LDR/STR/LDRB/STRB, SWI for ARM; formats 1, 2, 3, 4, 5, 6, 14, 16, 18 for Thumb) and renders unimplemented opcodes as `<undefined>`. |
| `src/gba/debugging/breakpoints.rs` | `Breakpoints` — `BTreeSet<u32>` wrapper supporting insert/remove/contains/clear and ordered iteration over breakpoint addresses. |
| `src/gba/debugging/trace.rs` | `CpuTrace` ring buffer of recently retired instructions (`TraceEntry` carrying PC, raw word, Thumb flag, mnemonic, R0–R15 snapshot, CPSR and cumulative cycle count); configurable capacity, default 1024, with enable/disable toggle. |
| `src/gba/debugging/controller.rs` | `GbaDebuggerController` — glues the breakpoint set, trace ring buffer, optional file logger, and disassembler to `Arm7tdmi`. Provides `step`, `run_until_breakpoint` and trace-to-file hooks. |

#### SNES Emulation (`src/snes/`)

The SNES (Super Nintendo Entertainment System) module now includes active 65816 CPU execution wired to a functional `SnesSystemBus`, cartridge parsing/mapping for LoROM/HiROM/ExHiROM, both general-purpose DMA and explicit HDMA execution hooks, a versioned JSON save-state path for CPU/bus/PPU/ROM identity capture and restore, an APU bootstrap path (64 KB ARAM, clean-room 64-byte IPL overlay with optional config-file IPL override, SPC700 instruction stepping via tick-driven catch-up, the `$2140-$2143` <-> `$F4-$F7` 4-port handshake, and SPC700 timers T0/T1/T2 at `$F1/$FA-$FF`), and a **dot-based PPU** (#2759, #2760, #2761): the bus owns the PPU and advances it one dot per 4 master clocks, routes the `$2100-$213F` register file plus `$4200`/`$4210`/`$4211`/`$4212`, latches the H/V counters, raises VBlank NMI to the CPU via `SnesBus::poll_nmi`, and renders the **BG tile pipeline for Modes 0-6** (per-dot fetch+composite: 8x8/16x16 tiles, 2/4/8 bpp, all tilemap sizes with SC mirroring, scroll, flip, per-tile/BG3 priority, TM enable, direct-color mode, and offset-per-tile for modes 2/4/6; modes 5/6 render **true hires** — 512 output columns, sub screen on even and main on odd, doubled horizontal fetch with forced 16-wide char pairing, hires-domain OPT, and per-dot hires color math, #3016) plus **Mode 7 affine rendering** (#2762: per-pixel rotation/scaling, screen-over, H/V flip, EXTBG, Direct-Color, and the MPYL/M/H multiply result) over the backdrop with master brightness/forced-blank to a frame-paced framebuffer. Screen interlace renders **distinct odd/even fields** (#3017): modes 5/6 fetch a doubled field line (realY = scanline*2 + field), fields interleave into the persistent framebuffer's alternating rows, interlaced even fields run one extra scanline (263 NTSC / 313 PAL, latched per frame), and enabling interlace during vblank clears the framebuffer. IRQ timing and S-DSP refinements remain incremental; HDMA scanline scheduling is intentionally exposed as bus APIs (`hdma_init` / `hdma_do_line`) until per-scanline PPU wiring lands.

| Directory/File | Description |
| ---------------- | ------------- |
| `src/snes/mod.rs` | SNES module root. Declares all SNES sub-modules (bus, console, cpu, ppu, apu, cartridge, input, integration_tests). |
| `src/snes/console/mod.rs` | Console module root. Re-exports `Snes` and exposes `config` / `save_state` as public modules. |
| `src/snes/console/save_state.rs` | Versioned JSON SNES save-state serialization (version 2). Captures CPU architectural + execution state, full system-bus state, PPU state (`SnesPpuState`: VRAM/CGRAM/OAM, scan position, register/counter latches, NMI/VBlank flags, video region), ROM identity (mapping + CRC32), and deterministic restore errors for version/ROM mismatches. |
| `src/snes/console/snes.rs` | `Snes` — platform-facing SNES wrapper implementing the `Emulator` trait. Owns `Option<Cpu<SnesSystemBus>>`; `load_rom` parses a `Cartridge`, resolves SNES hardware mode (config override, else header-country auto-detect), constructs the system bus/PPU with that region, and resets CPU state; `run_tick` executes real CPU steps when a ROM is loaded (returns 0 cycles when not loaded) and sets `ready_to_render` once per PPU frame; `screen_snapshot` returns the PPU framebuffer as RGB888; save-state bytes serialize via the versioned snapshot module. |
| `src/snes/console/config.rs` | `SnesConfig` struct for SNES-specific configuration (`snes-hardware` with `snes-ntsc`/`snes-pal`, `snes-spc-ipl-path`, and `snes-controller-port1`/`snes-controller-port2` with `standard`/`multitap`/`mouse`/`superscope`). |
| `src/snes/bus/mod.rs` | `SnesBus` trait definition plus `StubBus`, `TestBus`, and `SnesSystemBus` exports. The trait defines the memory access contract (`read`, `write`, `tick`) plus `poll_nmi`/`poll_irq` (default false) for delivering hardware interrupts to the CPU, mirroring the pattern used in GB and GBA. |
| `src/snes/bus/system_bus.rs` | `SnesSystemBus` implementation: WRAM direct/mirror mapping, LoROM/HiROM/ExHiROM ROM decode, SRAM windows with wrap semantics, strict MDR/open-bus reads, scoped MMIO support (`$2180-$2183`, mul/div `$4202-$4206`, MEMSEL `$420D`, MDMAEN `$420B`, HDMAEN `$420C`), ownership of the `Ppu` (advanced per `tick`, register routing for `$2100-$213F` + `$4200`/`$4210`/`$4211`/`$4212`, `poll_nmi` delegation, framebuffer + PPU save-state passthrough), and region-aware PPU construction (`NTSC`/`PAL`) at bus init. `tick()` calls `check_hdma_triggers()` every master clock (including through the DRAM-refresh stolen-clock window) to ARM `hdma_init()`/`hdma_do_line()` at `Ppu::hdma_init_due()`/`hdma_transfer_due()` (#2947 -- previously unwired, so HDMA never ran during emulation). Armed HDMA and a `$420B` general-purpose transfer both run from `SnesBus::gpdma_cycle_hook`, at the start of the second CPU cycle after their trigger, with HDMA taking priority (#3021, Mesen2 `ProcessPendingTransfers`); the per-line HDMA transfer is only armed when HDMAEN is non-zero at the trigger clock, so a channel enabled mid-scanline waits for the next line. A clock-based fallback in `tick()` runs an armed transfer for bus-only callers that have no CPU driving the cycle hook. `DmaABus::dma_write_b_bus` routes WMDATA/WMADD (`$2180-$2183`) through `write_mmio` so DMA writes get the same auto-incrementing WRAM port a direct CPU store would (#2945 -- previously silently dropped). CPU-status reads preserve open-bus bits while still honoring register side effects (including `$2137` strobe latching). Also owns save-state capture/restore helpers for WRAM, SRAM, MMIO latches, and DMA runtime state. |
| `src/snes/bus/dma.rs` | `DmaController` for #2745/#2746: `$43x0-$43xB` channel register file, general-purpose DMA execution, HDMA per-frame init and per-line execution (direct+indirect tables, repeat/continue semantics, terminator handling, channel 0→7 ordering), and deterministic B-bus stub storage for pre-PPU/APU validation. Every transfer runs inside the **hardware start/end envelope** (#3021, ported from Mesen2's `SnesDmaController`): a `SyncStartDma` pad of 1-8 clocks to the next 8-master-clock boundary, 8 clocks of setup, 8 clocks per channel plus 8 per byte, and a `SyncEndDma` pad rounding the charged total to a whole CPU cycle. HDMA's per-line work is two-phase -- all channel transfers first, then per-channel bookkeeping with a speculative descriptor read every line (pointer advanced only on expiry), indirect pointer loads before the termination check, and the last-active-channel single-MSB oddity. Clocks are spent live through `DmaABus::dma_tick` (so general-purpose DMA pays the once-per-scanline 40-clock DRAM-refresh stall mid-transfer, #2985), and each B-bus write lands directly at its true bus clock -- 4 clocks to the A-bus read, 4 more before the write -- which replaced #3020's per-byte deadline queue. The last visible pixel (x = 255) still renders pre-write because the burst only starts two CPU cycles after the dot-276 trigger. |
| `src/snes/cpu/mod.rs` | SNES 65816 CPU module root. Re-exports `Cpu` and memory-speed helpers; instruction behavior and timing tests live in `cpu.rs`. |
| `src/snes/cpu/cpu.rs` | `Cpu<B: SnesBus>` -- the 65816 core (also reused by SA-1's second CPU, `src/snes/sa1/mod.rs`). One `step()` call executes one full instruction (fetch/decode/execute), except WAI idling and in-progress MVN/MVP block moves which each spend exactly one `step()` call per cycle/byte. Interrupt (NMI/IRQ/ABORT) recognition is checked once per instruction boundary, at the top of `step()`, matching real hardware and Mesen2's `CheckForInterrupts` -- but the *pending state itself* is kept precise at per-CPU-cycle granularity (#3049), hooked into the start/end of the three CPU-cycle-boundary functions (`tick_read`, `tick_write`, `tick_internal_cycle`), mirroring Mesen2's `DetectNmiSignalEdge` (called once per `ProcessCpuCycle` from every `Read`/`Write`/`Idle`), rather than NESER's previous once-per-`step()` poll (which could overshoot dispatch by an entire extra instruction). NMI (edge-triggered) uses `resolve_nmi_arm_counter`/`poll_and_arm_nmi_edge`, an arm-this-cycle/latch-next-cycle counter mirroring Mesen2's `NmiFlagCounter` closely enough to reproduce its exact one-cycle edge-to-latch latency. IRQ (level-triggered, already a live non-consuming signal via `Ppu::poll_irq_dispatch`) is simpler -- `resample_irq_line` just resamples `bus.poll_irq()` into `irq_line_shadow` once per cycle, at cycle start, mirroring Mesen2's `PrevIrqSource`; the existing, separately-timed `irq_i_shadow` CLI/PLP/RTI-recognition-delay mechanic (#2985) is untouched and still composes via `irq_line_shadow && !irq_i_shadow`. `dispatch_hw_interrupt`'s two wasted cycles (dummy PC re-read + one internal tick, shared by NMI/IRQ/ABORT; BRK/COP absorb theirs into their own opcode+signature-byte fetch instead) and `op_pha`/`op_phx`/`op_phy`'s internal cycle both tick BEFORE their subsequent bus access (`tick_pre_access_internal_cycle`), matching Mesen2's `ProcessInterrupt`/`PHA`/`PHX`/`PHY` cycle ordering -- verified via a byte-level bus-trace diff against Mesen2 on KungFuFurby's `nmi.smc` (see `kungfufurby_nmi_tests.rs`/`kungfufurby_irq_tests.rs`). The pull-side mirror of that same ordering bug (PLA/PLX/PLY/PLB/PLD/PLP/PHP/PHB/PHD/PHK) is confirmed present but deliberately unfixed (out of scope; a spike proved it isn't `test_nmi.smc`'s remaining failure cause either). |
| `src/snes/ppu/mod.rs` | `Ppu` struct + power-on state, scan-position accessors, NMI/frame-complete/framebuffer accessors, and submodule declarations. The PPU is a dot-based pipeline owned by the bus, with region-aware frame length (262 NTSC / 312 PAL scanlines) and dynamic 224/239-line active height from SETINI bit 2. |
| `src/snes/ppu/registers.rs` | PPU register read/write dispatch (`$2100-$213F`, `$4200`/`$4201`/`$4210`/`$4212`) and VRAM/CGRAM/OAM access: word addressing, VMAIN increment + prefetch-on-read, CGRAM/OAM write latches, OAM high-table addressing, H/V counter latch reads (OPHCT/OPVCT), STAT77/STAT78, RDNMI, HVBJOY, and SLHV's counter-latch side effect. Also dispatches the BG registers (BGMODE, BGnSC, BGxxNBA, BGnHOFS/VOFS, TM), the Mode 7 registers (M7SEL, M7A-M7D, M7X/M7Y via the M7_old latch; M7HOFS/VOFS at `$210D`/`$210E` update both BG1 scroll and Mode 7 scroll), SETINI (EXTBG), and the MPYL/M/H multiply-result reads (`$2134-$2136`). |
| `src/snes/ppu/background.rs` | BG tile pipeline for Modes 0-6: BGnHOFS/VOFS shared write-twice (BG_old) scroll latch, and the per-dot layer resolve (`resolve_pixel_pair` → main/sub `resolve_screen_pixel`, composed via `compose_pixels`) — 8x8/16x16 tile decode (2/4/8 bpp), 32x32/64x32/32x64/64x64 tilemaps with SC0..SC3 mirroring, H/V flip, per-tile + Mode-1 BG3 high priority, per-BG TM enable, Mode-0 per-BG palette regions, direct-color mode (CGWSEL bit 0) for 256-color BGs, offset-per-tile (modes 2/4/6 via BG3), and backdrop fallback. `resolve_screen_pixel` interleaves the OBJ priority levels (OBJ.0-3) with the BG layers via a `Slot` chart (fullsnes Background Priority Chart, gated by TM bit 4); `resolve_pixel_pair` dispatches to the Mode 7 resolver when `bg_mode == 7` (non-hires). True hires (modes 5/6, #3016): `bg_pixel_hires` fetches at doubled horizontal resolution (BGnHOFS in half-pixel units, every tilemap entry a forced 16-wide char N/N+1 pair, hflip mirroring across the pair, hires-domain OPT for mode 6 with the halved-value quirk), the sub screen samples the even and the main screen the odd half-pixel, mosaic collapses pairs to the even block-start sample, the sub screen displays TS layers regardless of CGWSEL bit 1 (with the DISPLAYED sub backdrop being CGRAM color 0), and `compose_hires_pair` applies per-dot hires color math (odd/main vs the pre-math sub color or COLDATA; even/sub vs the post-math main pixel at x-1 with the x-1 source gate). Pseudo-hires (SETINI bit 3) shares the interleave and math path. Screen interlace (#3017): the modes 5/6 fetch doubles vertically too (realY = display line * 2 + field, incl. the OPT V arm), with the Mesen2 mosaic asymmetry (map row keeps the field term, chr row drops it) via split map/chr vertical offsets; mosaic also holds the OPT-replaced vertical offset in both the hires and native modes 2/4 paths. |
| `src/snes/ppu/sprites.rs` | OBJ (sprite) support (#2999, hardware pipeline per Mesen2/ares): OBSEL (`$2101`) size pairs (incl. undocumented 6/7), tile name base/gap; OAM priority rotation (OAMADDH bit 7); and the dot-incremental `ObjPipeline` — evaluation window (H=0..255 of the previous scanline, one OAM entry per 2 dots, priority-rotation order, 32-OBJ cap, horizontal-visibility test with the X=256 counts-but-never-draws quirk, forced blank pauses the cursor), reverse-order sliver fetch window (H=270..339, 34 8x1 CHR slots, off-screen columns skipped, the 35th attempted fetch raises STAT77 time over), dot-0 buffer swap compositing the presented `ObjLine` (CGRAM index resolved at query time), O(1) `obj_pixel_at` row-gated lookup (4bpp tile decode, X/Y flip, non-carrying large-tile composition, OBJ palettes at CGRAM 128+, 9-bit signed X), and dot-accurate STAT77 range over at `H = OAM_index x 2`. |
| `src/snes/ppu/mode7.rs` | Mode 7 affine (rotation/scaling) rendering: the per-pixel `resolve_mode7_screen_pixel` resolver (reached via `resolve_pixel_pair`) applies the fullsnes affine formula on live matrix/center/scroll registers (reading the interleaved 128×128 Mode 7 VRAM — BG map at even bytes, 8bpp tile data at odd bytes), with screen-over (wrap/transparent/tile-0 fill), screen H/V flip, EXTBG BG2 per-pixel priority (SETINI bit 6), Direct-Color for BG1, OBJ priority interleaving, and the signed `M7A × M7B` general-purpose multiply result. |
| `src/snes/ppu/timing.rs` | Dot/scanline counters (1 dot per 4 master clocks, 341 dots/line, region-aware 262/312 frame lines; interlaced even fields latch one extra scanline at the frame wrap, #3017), scanline-specific short/long-line exceptions for NTSC/PAL, VBlank entry/exit (line 225 in 224-line mode, line 240 in 239-line mode), the NMI rising-edge model (`nmi_enable && nmi_flag`), H/V counter latching (SLHV / WRIO falling edge), HBlank/VBlank flag timing, per-dot OBJ-pipeline advancement (`update_obj_pipeline`), and the DRAM-refresh (`dram_refresh_due`) and HDMA init/transfer (`hdma_init_due`/`hdma_transfer_due`, #2947) per-scanline trigger points, all phase-jittered by `total_master_clocks & 7`. |
| `src/snes/ppu/framebuffer.rs` | Per-dot backdrop + composited BG/OBJ rendering into a BGR555 framebuffer with dynamic active height (224/239; interlace doubles to 448/478). `render_dot` resolves each dot's main/sub screen pair into per-scanline line buffers (`line_main`/`line_sub`/`line_main_final`) before finalizing into the framebuffer. Also `screen_snapshot_rgb` (BGR555→RGB888 with INIDISP brightness/forced-blank applied per scanline via `line_inidisp`, latched at each row's first visible dot -- HDMA commonly rewrites INIDISP every scanline for fade/banding effects, #2947). |
| `src/snes/ppu/save_state.rs` | PPU save-state capture/restore (`SnesPpuState`); the transient framebuffer is excluded and redrawn after restore. |
| `src/snes/apu/mod.rs` | `SnesApu` path with S-DSP voice + echo-pipeline wiring: owns 64 KB ARAM, embedded/override IPL ROM selection, SPC700 CPU stepping via master-clock catch-up, `$2140-$2143` <-> `$F4-$F7` port handshake latches, DSP register ports `$F2/$F3`, SPC700 timer integration (`$F1/$FA-$FF`), per-SPC-cycle 32-phase DSP stepping, ARAM-backed echo mixing during native audio rendering, and save-state capture/restore including DSP state. |
| `src/snes/apu/dsp/` | Slot-accurate S-DSP pipeline split into focused submodules (`mod.rs`, `voice.rs`, `envelope.rs`, `brr.rs`, `gaussian.rs`, `echo.rs`) validated end-to-end by blargg `spc_dsp6`. The 32-slot schedule, per-sample pitch reloads at Step2/Step3a, BRR decode, Step3c output with the global-rate-counter noise clock, double-buffered ENDX/OUTX/ENVX publication at Step7-Step9), global register latches (PMON at slot 27, DIR/NON/EON at slot 28), and the echo pipeline distributed across slots 22-30 (ring reads into the FIR history at 22/23, per-slot FIR coefficient reads at 22-25, MVOL/EVOL output assembly at 26/27 with echo feedback at 26, the DAC latch at 27, and EDL/offset advance + echo writes + the ESA latch at 29/30). |
| `src/snes/apu/ipl/mod.rs` | Embedded clean-room 64-byte SNES IPL ROM as an in-source byte array with per-instruction SPC700 opcode comments. |
| `src/snes/apu/spc700/` | SPC700 CPU core + bus trait used by the APU bootstrap implementation and processor-test vectors. |
| `src/snes/apu/timers.rs` | SPC700 timer subsystem for T0/T1/T2: per-cycle divider logic (T0/T1=128 SPC cycles, T2=16 SPC cycles), target registers (`$FA-$FC`, with `0=>256` clocks), `$F1` enable-edge reset semantics, and read-and-clear 4-bit counters at `$FD-$FF`. |
| `src/snes/cartridge/mod.rs` | Cartridge module root. Re-exports `Cartridge`, `CartridgeError`, `RomSpeed`, and `Mapping`. |
| `src/snes/cartridge/cartridge.rs` | `Cartridge::from_bytes` loader for `.sfc`/`.smc` data. Handles optional 512-byte copier-header strip, invokes mapping detection/header parsing, and exposes parsed cartridge metadata (`mapping`, `title`, `sram_size`, `has_battery`, `speed`, `country`). |
| `src/snes/cartridge/header.rs` | Internal header parser for SNES candidate header locations. Extracts title, map mode, chipset, ROM/RAM size fields, region/developer/version, and checksum/complement. Public title decoding trims trailing NULs/spaces. |
| `src/snes/cartridge/mapping.rs` | Score-based mapping detector for LoROM/HiROM/ExHiROM candidates (`$7FC0`, `$FFC0`, `$40FFC0`) with deterministic tie-break priority (ExHiROM > HiROM > LoROM) and plausibility thresholding to reject garbage ROMs. |
| `src/snes/input/mod.rs` | SNES input subsystem: the `SnesController` trait, `SnesButton`/`button_from_id` (12-button mapping with X=10/Y=11), `SnesControllerType` (standard/multitap/mouse/superscope; multitap currently supported on port 2, while unsupported combinations fall back to standard), and `InputPorts` — the two controller ports plus the auto-joypad sequencer. Routes manual serial `$4016`/`$4017` (shared shift register, `$4016.0` strobe, grounded `$4017` bits 2-4, open-bus upper bits) and automatic reading into JOY1-JOY4 (`$4218-$421F`, enabled by `$4200.0`, busy via `$4212.0`, latched input committed after the 4224-master-cycle window). Uses WRIO select-bit routing (`$4201`: bit6->port1, bit7->port2) and save-state via `InputPortsState`. |
| `src/snes/input/standard_controller.rs` | `StandardController` — the 12-button joypad shift register (serial order B,Y,Select,Start,Up,Down,Left,Right,A,X,L,R; ID bits 0; connected-pad padding 1 after 16 clocks; latch held while `$4016.0` is high). |
| `src/snes/integration_tests/mod.rs` | SNES integration test module root. Includes ProcessorTests vector runners and the shared ROM runner foundation for headless SNES test ROM automation. |
| `src/snes/integration_tests/processor_tests_65816.rs` | Loader and harness for Tom Harte ProcessorTests 65816 JSON vectors. Parses upstream schema (`initial` / `final` / `cycles`), runs single-step CPU checks against a RAM-backed `SnesBus`, compares final CPU+RAM state and vector cycle count, and executes vectors from tracked subset files (`v1/*.json`) with per-filename overrides from local full-cache files (`full/v1/*.json`) when present. |
| `src/snes/integration_tests/processor_tests_spc700.rs` | Loader and harness for Tom Harte ProcessorTests SPC700 JSON vectors. Runs single-instruction SPC700 cases against a flat 64K RAM bus and validates final CPU/RAM state and cycle counts. |
| `src/snes/integration_tests/rom_runner.rs` | Shared headless SNES ROM runner used by future ROM-based suites. Loads generated or vendored `.sfc`/`.smc` bytes through `Snes`, runs with tick/frame budgets, supports pluggable `RunOracle` strategies — WRAM marker, bus-byte (for ROMs without marker support), and a screen-CRC golden oracle (`RunOracle::ScreenCrc` / `RunExitReason::ScreenCrcFrame`) that runs to a fixed frame and compares the rendered `screen_crc32()` against a human-approved value — returns diagnostics (exit reason, ticks, frames, PC, marker, screen CRC), and optionally writes PNG captures under `target/snes_test_captures/` when `NESER_CAPTURE_SCREEN` is set. Interactive ROMs are driven via `RunConfig::with_input_script`: sorted frame-stamped `InputEvent`s applied once the completed-frame counter reaches each stamp (#2879), where each event carries an `InputAction` — a per-port pad button edge, SNES Mouse relative motion, or a mouse button edge (#2886). `RunConfig::with_controller_ports` selects the device type on each controller port (default standard pad) before the ROM loads. |
| `src/snes/integration_tests/fixture_rom.rs` | Shared in-code LoROM fixture builder for the input verification suites (#2886/#2889): emits 65816 programs as raw opcode bytes (immediate/long stores, absolute loads, compares, long-branch-safe `bne_to` poll loops, strobe pulses, unrolled MSB-first serial-bit reads into WRAM) into a 64 KiB LoROM image with the program origin at `$8200` so the canonical `rom_runner` marker idle loops at `$8100`/`$8110`/`$8120` stay clear. |
| `src/snes/integration_tests/input_mouse_tests.rs` | SNES Mouse protocol verification suite (#2889), spec-first against fullsnes/SNESdev: 32-bit packet identification (zero lead byte, hardware ID `0001`, tail ones past bit 32), the scripted example sequence (four motion directions with sign/direction-bit checks, left/right/both button edges, release), sensitivity cycling on strobe-high `$4016` clocks, 7-bit magnitude clamping at ±127, and a port-2 mouse on `$4017`. Fixtures are assembled in-code (no on-disk assets) and report through the WRAM pass/fail marker. |
| `src/snes/integration_tests/input_standard_controller_tests.rs` | Standard-controller protocol verification suite (#2886), spec-first against fullsnes/SNESdev: `$4016`/`$4017` serial order (B,Y,Select,Start,Up,Down,Left,Right,A,X,L,R, four ID zeros, connected-pad padding ones), the scripted example press/release sequence with atomic directional transitions observed via auto-joypad JOY1 reads, strobe-high live-B semantics with falling-edge latch, port 2 coverage, and auto-joypad vs manual serial layout equivalence. Fixtures are assembled in-code (no on-disk assets) and report through the WRAM pass/fail marker. |
| `src/snes/integration_tests/blargg_apu_tests.rs` | SNES SPC700/APU ROM pass/fail suite. The `blargg_rom_test!` macro declares one `#[test]` per vendored blargg ROM under `roms/snes/automated_tests/blargg_apu/` (18 tests), each running the ROM through the `rom_runner` screen-CRC golden oracle to a fixed frame and comparing against a visually-approved PASS capture. ROMs needing a larger tick budget (`spc_dsp6`) or a mid-test reset trap (`timer_at_power_reset`) pass an explicit `RunConfig`. |
| `src/snes/integration_tests/gilyon_cpu_tests.rs` | gilyon/snes-tests 65816 CPU ROM suite. One test for `cputest.sfc` (the full 1610-test build, including undocumented emulation-mode DP/stack wrapping edge cases; the retired 1107-test basic build was a strict subset of it) via the `rom_runner` screen-CRC oracle; currently PASSES. |
| `src/snes/integration_tests/gilyon_spc_tests.rs` | gilyon/snes-tests SPC-700 CPU ROM suite. One test for `spctest.sfc` (1320 tests in the current upstream build) via the `rom_runner` screen-CRC oracle; currently PASSES. |
| `src/snes/integration_tests/peterlemon_cpu_tests.rs` | PeterLemon SNES-CPUTest-CPU ROM suite. One screen-CRC golden test per vendored 65816 opcode-group ROM (23 ROMs); goldens were probed by running each ROM until its screen CRC stayed stable for 600 consecutive frames and visually confirming all-PASS output. All 23 currently PASS (`CPUPHL.sfc` required the RDNMI open-bus fix from #2975). |
| `src/snes/integration_tests/peterlemon_spc_tests.rs` | PeterLemon SNES-CPUTest-SPC700 ROM suite. One screen-CRC golden test per vendored SPC700 opcode-group ROM (7 ROMs), probed and approved the same way as the CPU suite; all currently PASS. |
| `src/snes/integration_tests/peterlemon_ppu_bg_tests.rs` | PeterLemon basic PPU BG demo suite (#2878): 2/4/8bpp tile decoding, BG1-BG4 tilemaps, all four tilemap screen sizes, tile flip, backdrop and palettes. 11 of 12 vendored ROMs have screen-CRC goldens pixel-diff-approved against Mesen2 (and the 8 upstream reference screenshots); `8x8BGMap8BPP32x32.sfc` stays excluded: #2985's refresh-stall fix shrank its init frame offset from +3 to +1 (0-px match at a one-frame shift), with the residual tracked in #3042; its RDNMI poll-race scroll cadence matches Mesen2 since #2990. |
| `src/snes/integration_tests/undisbeliever_ppu_bg_tests.rs` | undisbeliever PPU BG / VMAIN suite (#2878): VRAM increment modes at all bit depths, VMAIN $2115 bits 2-3 address remapping, byte- vs word-increment uploads, 1bpp tile decode, non-DMA VRAM writes. All 18 locally-built ROMs have Mesen2-approved screen-CRC goldens, including the four animated scroll/textbuffer demos whose goldens derive from frame-skip-free Mesen2 captures at frame 120 (#2990) and the 6 `*-with-remapping` ROMs automated after the remapping implementation (#2989). |
| `src/snes/integration_tests/undisbeliever_ppu_obj_tests.rs` | undisbeliever OBJ/sprite-limit suite (#2879): `object-dropout-test.sfc` covering the 32-OBJ range-over limit, the 34-sliver time-over limit (plus flipped variant) and the X=256 bug. Committed `#[ignore]`d with NESER's CRC recorded: the frame-66 Mesen2 pixel-diff shows NESER enforces the range limit but not the time-over sliver limit (#2999). |
| `src/snes/integration_tests/byuu_test_oam_tests.rs` | byuu `test_oam.smc` interactive OAM suite (#2879), driven through `rom_runner` input scripting (Up/Down/Right/A/X taps dial the menu counters, Start applies them; 1-frame taps at an 8-frame period). 28 combos (menu, 8 OBSEL bases x 2 size bits, flips, char variants) carry Mesen2-approved goldens replayed with identical schedules via Lua `emu.setInput`; 6 SETINI combos are `#[ignore]`d (OBJ interlace #3000; interlace/overscan capture-dimension conventions #3001). Baselining also fixed `PPU2_VERSION` to 3 (STAT78). |
| `src/snes/integration_tests/neser_obj_tests.rs` | NESER-authored OBJ feature suite (#2879): 14 bass-built static scenes (sources vendored beside the builds) covering all eight OBSEL size pairs, OBJ palettes, OBJ-vs-OBJ priority, OAM X bit 8, mode-1 OBJ-vs-BG layering, OAMADDH first-sprite rotation and OBJ Y wrap-around. 13 have Mesen2-approved goldens; `obj-y-wrap.sfc` is `#[ignore]`d pending the V-flip+Y-wrap divergence (#3003). |
| `src/snes/integration_tests/undisbeliever_ppu_window_tests.rs` | undisbeliever PPU window / INIDISP fade suite (#2880): input-scripted `window-mask-logic.sfc` (21 states) and `window-shapes-single.sfc` (14 shapes, A-tap lock against the 120-frame auto-advance), two free-running precalculated window demos, and 7 mid-plateau `inidisp_fadein_fadeout.sfc` samples. Fade + no-window goldens are Mesen2-approved; the 36 window-enabled vectors are `#[ignore]`d pending the window-region inversion (#3011). |
| `src/snes/integration_tests/neser_color_math_tests.rs` | NESER-authored colour-math/window/brightness suite (#2880): 11 bass-built ROMs over one Mode 1 quadrant scene (64 main x sub math crossings plus fallback regions) covering CGADSUB add/sub/half, COLDATA fixed-colour math, the transparent-sub fallback, the OBJ palette 4-7 rule, colour-window clip/prevent, layer window masks, and 17 INIDISP brightness samples. 21 tests have Mesen2-approved goldens; 4 are `#[ignore]`d pending the fallback halve-suppression rule (#3012) and 2 pending #3011. |
| `src/snes/integration_tests/jonasquinn_math_tests.rs` | jonasquinn `color_halve_proof/demo.smc` (#2880): proves half colour math halves after the add via per-scanline COLDATA rewrites; Mesen2-approved golden (`test_math.sfc` deliberately left un-automated, see the manifest notes). |
| `src/snes/integration_tests/peterlemon_ppu_advanced_tests.rs` | PeterLemon advanced PPU mode suite (#2881): Mode 7 rotozoom (static + scripted rotate/zoom), HDMA Perspective and StarWars crawl, mosaic modes 3/5, six hires/interlace demos, four pseudo-hires HiColor demos. 20 Mesen2-approved goldens (four pseudo-hires HiColor with #3016; MosaicMode5 and the Interlace suite with #3017; Perspective and InterlaceSimpsonsHDMA with #3020); 2 vectors (StarWars f360/f600) `#[ignore]`d pending #3021 -- their residual is a CPU-side clock skew at the `$4210` NMI poll, not DMA cost (#3050). |
| `src/snes/integration_tests/undisbeliever_ppu_mode7_tests.rs` | undisbeliever Mode 7 VRAM-layout/tilemap suite (#2881): four static `vmain-mode7-image-*` demos sharing one Mesen2-approved golden by design plus the two animated tilemap demos pinned at frames 120/360/600 each; all pixel-exact vs Mesen2. |
| `src/snes/integration_tests/ddribin_hdrv_tests.rs` | ddribin HDRV display-mode suite (#2881, CC0, WLA-DX build): input-scripted test patterns; colorbars + graybars carry Mesen2-approved goldens; interlace/overscan combos content-verified but `#[ignore]`d pending #3001. |
| `src/snes/integration_tests/neser_opt_tests.rs` | NESER-authored offset-per-tile suite (#2881): 6 bass-built ROMs covering modes 2/4/6 OPT (flag gating, exempt first column, fine-scroll retention, mode 4 bit-15 H/V select). 5 Mesen2-approved goldens; `opt-m6` `#[ignore]`d pending #3019/#3016. |
| `src/snes/integration_tests/neser_mode7_tests.rs` | NESER-authored static-matrix Mode 7 suite (#2881): 8 bass-built ROMs (identity, M7SEL wrap/colour-0/tile-0 at 8x zoom-out, 30-degree rotation, both flips, Mode 7 mosaic), all with Mesen2-approved goldens. |
| `src/snes/integration_tests/undisbeliever_tests.rs` | undisbeliever/snes-test-roms hardware-glitch/timing-hammer suite. Unlike blargg/gilyon these ROMs print no PASS/FAIL text, so each of the 12 automated tests (of 29 vendored ROMs) asserts a stability-snapshot screen CRC via the `rom_runner` oracle. |
| `src/snes/integration_tests/hblank_dma_vram_tests.rs` | 93143 hblank-dma-vram suite (2 ROMs mirrored from a NESdev forum post into the `snes_test_roms` submodule). `hvdma.sfc` (cross-checked golden, 1.16% vs Mesen2 since #3020, see #3042) and `hvdma_max.sfc` (pixel-exact since #2953's OPHCT/open-bus fixes). |
| `src/snes/integration_tests/sa1_boot_tests.rs`, `sa1_iram_tests.rs`, `sa1_bwram_tests.rs`, `sa1_irq_tests.rs` | Hand-assembled SA-1 fixture-ROM integration tests (epic #2956): dual-CPU boot/release, shared I-RAM exchange with per-side write protection, shared BW-RAM exchange, and a full SA-1->SNES IRQ message round-trip through the main CPU's real IRQ handler. |
| `src/snes/integration_tests/dsp_audio_golden_tests.rs` | S-DSP audio sample golden checks (#2877). A `DspGoldenRecorder` drives `Sdsp` standalone over a synthetic ARAM (32 phases per native 32 kHz sample, reading the phase-27 DAC latch as exact i16 pairs) through eight deterministic windows: BRR decode, ADSR, GAIN modes, PMON pitch modulation, echo/FIR, gaussian interpolation, multi-voice mixing/clamping, and the noise LFSR. Baselines are navigator-approved CRC32s with sample-rate/warmup/window/source/review-note metadata inline in the source; `NESER_CAPTURE_AUDIO=1` writes review WAVs under `target/snes_test_captures/dsp_audio_golden_tests/` (never committed). |
| `src/snes/integration_tests/sa1_absindx_tests.rs` | absindx SA-1 conformance ROMs, verified with human-approved screen-CRC goldens like the blargg/gilyon suites (both ROMs are documented to misbehave on Mesen2, so the goldens are navigator-approved captures of NESER's own rendering, not Mesen2 cross-checks). `SA1RamProtectionTest.sfc`: all 222 sub-tests pass, golden captures the `Result Passed` screen. `SA1VersionCodeTest.sfc`: golden captures the hardware-accurate register-dump screen whose result line reads `Failed` even on real hardware -- disassembly shows its `CheckResult` unconditionally takes the failed path (the pass path is unreferenced dead code), deliberate since the SA-1's true version-code value is unknown (fullsnes) and `$230E` is open bus (bsnes). |
| `src/snes/integration_tests/kungfufurby_nmi_tests.rs`, `kungfufurby_irq_tests.rs` | KungFuFurby's 2005-2008 NMI/H-V-IRQ test ROM collection (#2883/#3049, epic #2724). `demo_nmitest.smc`, `nmi.smc` (#3049's per-CPU-cycle NMI dispatch fix), and `demo_irqtest.smc` (#3049's per-CPU-cycle IRQ dispatch fix) all match Mesen2 exactly with approved goldens (pixel-verified at frame 600; `nmi.smc`'s bus-trace diff against Mesen2 is 99.99% byte-identical, 55957/55962 lines, across ~310k master clocks). `test_nmi.smc`, `irq.smc`, `test_irq.smc`, `test_irq4200.smc`, `test_irq4209.smc`, `test_irqb.smc` remain `#[ignore]`d: unaffected by either #3049 dispatch fix (identical CRCs before/after), root cause not yet identified (a spike extending the NMI fix to the full push/pull opcode family was tried for `test_nmi.smc` and ruled out). See `src/snes/cpu/cpu.rs`'s `resolve_nmi_arm_counter`/`poll_and_arm_nmi_edge`/`resample_irq_line` for the dispatch mechanism. |
| `src/snes/integration_tests/sour_dma_irq_tests.rs` | Sour/SnesTests' `dma_irq_test.sfc` (#2883, epic #2724), rebuilt byte-identical from source (`ca65 -g` + `ld65 --dbgfile`) to recover its WRAM result-table address from debug symbols. Validates how many instructions run after a manual DMA (`$420B` write) before a pending IRQ/NMI dispatches, across 19 sub-cases; corrects a transcription error in the upstream README's expected-results table (`$00FF`, not `$FFFF`, for the two no-interrupt sentinels). `#[ignore]`d pending #3049: 8/19 sub-cases diverge, each off by exactly one fewer dispatched instruction than Mesen2, the same signature as the KungFuFurby suites. |
| `src/snes/sa1/mod.rs` | SA-1 coprocessor core (epic #2956): `Sa1ControlRegisters` (`$2200-$220F` control/vector registers, cross-CPU IRQ/NMI pending/enable latches, `$2300`/`$2301` SFR/CFR status reads, `$2302-$2305` H/V counter latch), `Sa1Bus` (SA-1-side memory map: vector interception from CRV/CNV/CIV, I-RAM direct+mirror, BW-RAM windowed + direct `$40-$5F`, Super MMC ROM, shared main-bus `Ppu` for HCR/VCR), and `Sa1Core` (a second `Cpu<Sa1Bus>` reusing the generic 65816 core, ticked per master clock with reset/wait gating). |
| `src/snes/sa1/iram.rs` | SA-1 2KB I-RAM with independent per-CPU-side 256-byte-chunk write protection (`$2229` SIWP / `$222A` CIWP) and the direct (`$0000-$07FF`, SA-1 only) and mirrored (`$3000-$37FF`, both sides) address decoders. CIWP is cleared by an SA-1 reset-release (absindx TEST 221). |
| `src/snes/sa1/memory_control.rs` | SA-1 Super MMC ROM banking (`$2220-$2223`) and BW-RAM mapping/protection (`$2224-$2228`): per-side 8KB window block selects, the shared either-side-enables write rule, and the BWPA comparator that folds linear offsets to the 256KB BW-RAM address space *before* physical-size wrapping (absindx TESTs 50/51). SNES-side direct banks are `$40-$4F`; the SA-1 side spans `$40-$5F`. |
| `scripts/validate_snes_test_assets.py` | Strict SNES automated-test asset manifest validator. Enforces required provenance metadata (source/ref, license, oracle type, variant status/path, notes) and committed-vs-optional corpus rules. |

**Hardware Constants** (defined in `src/snes/console/snes.rs` and `src/snes/ppu/mod.rs`):
- Base screen width: 256 pixels (512 in hi-res/pseudo-hires modes)
- Active screen height: 224 or 239 lines (doubled in interlace to 448/478)
- Region frame length: 262 (NTSC) or 312 (PAL) scanlines per frame
- Frame duration: ~16.639 ms (NTSC) or ~19.997 ms (PAL)

#### Frontends

| Directory/File | Description |
| ---------------- | ------------- |
| `src/frontends/native/` | Desktop frontend using winit + OpenGL. |
| `src/frontends/native/event_loop.rs` | Main event loop — holds `Console` enum, handles input events, frame timing, VSync, autorun integration, pause/resume, and hot-reload of ROMs. NES/Game Boy-specific features (debugger, PPU viewer, Zapper, SNES mouse) use `Console` accessors instead of variant branching. |
| `src/frontends/native/audio.rs` | Native audio device setup and sample queuing. |
| `src/frontends/native/keyboard/` | Keyboard input handling, split into focused modules: `mod.rs` (entry points `handle_key_pressed`/`handle_key_released`/`keyboard_target_ports` + `KeyOutcome`), `hotkeys.rs` (system/debugger/cartridge-switch hotkeys), `console_keyboard.rs` (per-console press dispatch), and `controller_mapping.rs` (key→button mapping tables). |
| `src/frontends/native/gamepad.rs` | Gamepad input using gilrs — maps controller axes/buttons to NES joypads. |
| `src/frontends/native/mouse.rs` | Mouse input — Zapper light gun, SNES mouse, and Arkanoid paddle coordinate mapping. |
| `src/frontends/native/gl_wrapper.rs` | OpenGL context management for native windows. |
| `src/frontends/native/gl_backend.rs` | OpenGL framebuffer, texture management, and debugger UI. |
| `src/frontends/native/egui_renderer.rs` | Shared egui frame input, frame runner, and egui_glow painter seam for the native emulator renderer cutover. |
| `src/frontends/native/egui_theme.rs` | Shared egui font and dark-theme setup used by native egui surfaces. |
| `src/frontends/native/egui_texture.rs` | Shared egui native texture metadata used by ROM browser textures and future emulator egui rendering. |
| `src/frontends/native/ui_geometry.rs` | Shared pure geometry helpers for letterboxed frames, overlays, toasts, and crosshair layout. |
| `src/frontends/native/shader_manager.rs` | Shader pipeline using librashader — loads `.slangp` presets (CRT, NTSC, xBRZ). |
| `src/frontends/native/rom_browser/` | Graphical ROM browser — a console-style launcher with cover art grid, search, a filter panel (platform, players, genre, favorites), and a detail view. |
| `src/frontends/native/rom_browser/app.rs` | `RomBrowserApp` — winit `ApplicationHandler` implementing the browser state machine, grid rendering, overlay modes (search, filter panel, detail view), input handling, and favorites. |
| `src/frontends/native/rom_browser/renderer.rs` | `BrowserGl` — egui_glow + egui_winit setup, texture loading from image files, and frame lifecycle management for the browser window. |
| `src/frontends/native/rom_browser/theme.rs` | Visual theme constants — colours, spacing, layout calculations (`grid_layout`, `cell_height`, `sidebar_width`). |
| `src/frontends/tui/` | Terminal UI ROM launcher using `ratatui` + `crossterm`. |
| `src/frontends/tui/app.rs` | TUI application state and event loop. |
| `src/frontends/tui/rom_list.rs` | Scrollable ROM list widget. |
| `src/frontends/tui/catalog.rs` | Integration with the cartridge catalog for ROM discovery. |
| `src/frontends/tui/launcher.rs` | Launches the native emulator for a selected ROM. |
| `src/frontends/tui/action_menu.rs` | Context menu for ROM actions. |
| `src/frontends/web/` | WebAssembly frontend. |
| `src/frontends/web/wasm.rs` | `wasm-bindgen` bindings — exposes `WasmNes` to JavaScript with methods for frame stepping, input, audio sample retrieval, save states, autorun, and NES debugger support. |
| `src/frontends/web/wasm_gb.rs` | `wasm-bindgen` bindings for the Game Boy frontend (`WasmGb`) with ROM loading, frame rendering, audio, reset, and joypad input. |
| `src/frontends/web/wasm_gba.rs` | `wasm-bindgen` bindings for the Game Boy Advance frontend (`WasmGba`) with ROM loading, 240×160 RGBA frame rendering, audio, reset, toast draining, and 10-button keypad input. |
| `src/frontends/web/wasm_snes.rs` | `wasm-bindgen` bindings for the SNES frontend (`WasmSnes`) with ROM loading (`.sfc`/`.smc`), 256×224 RGBA frame rendering, stereo audio sample draining, save/load state bytes, toast draining, and SNES peripherals (mouse, Super Scope, multitap-port detection). |
| `src/frontends/web/wasm_autorun_state.rs` | Autorun state management for the WASM frontend. |
| `src/frontends/web/wasm_tests.rs` | WASM-specific integration tests (run via `wasm-pack test`). |

#### Debugging

| Directory/File | Description |
| ---------------- | ------------- |
| `src/debugging/` | Generic debugging and diagnostic tools. |
| `src/debugging/breakpoints.rs` | Breakpoint system — supports address breakpoints and conditional breaks. |
| `src/debugging/tracing.rs` | CPU/PPU/APU/Mapper trace output at configurable verbosity levels. |
| `src/debugging/logging.rs` | Debug logging infrastructure. |
| `src/nes/debugging/` | NES-specific debugging tools. |
| `src/nes/debugging/ui.rs` | egui-based debugger UI with CPU state, memory viewer, and disassembly. |
| `src/nes/debugging/disasm.rs` | 6502 disassembler for real-time instruction display. |
| `src/nes/debugging/ppu_viewer.rs` | PPU nametable and pattern table viewer. |
| `src/nes/debugging/snapshot.rs` | Debugging state snapshots. |
| `src/nes/debugging/types.rs` | Shared NES debugging types and constants. |
| `src/nes/debugging/control.rs` | Debugger controller for breakpoints, stepping, and pause/continue. |

#### Autorun System

| Directory/File | Description |
| ---------------- | ------------- |
| `src/autorun/` | Input recording and deterministic playback system. |
| `src/autorun/types.rs` | `AutorunFile` format — stores per-frame joypad input with periodic CRC checkpoints for regression testing. Supports versioned format (currently v3 with run-length encoding). |
| `src/nes/autorun/headless_playback.rs` | NES headless playback engine — replays input without rendering for automated verification. Compares CRC checksums at each checkpoint. |
| `src/autorun/utils.rs` | Utilities for loading, saving, converting, and trimming autorun files. |

#### Other Core Files

| File | Description |
| ------ | ------------- |
| `src/platform/frontend_toasts.rs` | System-agnostic toast message formatters (gamepad connection/disconnection, cartridge load, gamepad initialization). |
| `src/nes/frontend_toasts.rs` | NES-specific toast message formatters (emulator timing mode, hardware mode/model selection). |

#### Tests

| Directory/File | Description |
| ---------------- | ------------- |
| `src/nes/integration_tests/` | Integration test suites. |
| `src/nes/integration_tests/cpu_tests.rs` | CPU instruction and timing tests using Blargg test ROMs. |
| `src/nes/integration_tests/ppu_tests.rs` | PPU rendering, timing, and register tests using Blargg test ROMs. |
| `src/nes/integration_tests/apu_audio_tests.rs` | APU channel audio output verification. |
| `src/nes/integration_tests/apu_visual_tests.rs` | APU tests that produce visual output (Blargg test ROMs). |
| `src/nes/integration_tests/mapper_tests.rs` | Mapper-specific tests using holy-mapperel and other test ROMs. |
| `src/nes/integration_tests/autorun_tests.rs` | Autorun recording/playback round-trip tests. |
| `src/nes/integration_tests/input_tests.rs` | Controller input tests. |
| `src/nes/integration_tests/ram_init_tests.rs` | RAM initialization mode tests. |
| `src/nes/integration_tests/rom_test_runner.rs` | Generic test ROM harness — runs a ROM headlessly and checks for pass/fail output. |
| `src/nes/integration_tests/romtest_harness.rs` | Shared infrastructure for ROM-based test assertions. |
| `src/nes/integration_tests/manual_test_cartridges.rs` | Programmatically generated minimal test ROMs for specific hardware scenarios. |
| `src/nes/integration_tests/miscellaneous_tests.rs` | Miscellaneous integration tests. |
| `build.rs` | Build script that scans `roms/games/mappers/` for `.autorun` files and generates per-ROM regression tests at compile time. |

### `web/` — Browser Frontend

The web frontend is bundled with **Vite** (config at `vite.config.ts`, root: `web/`, build output: `dist/`). Styled with **Tailwind CSS v4** and **DaisyUI v5** (night theme with neon accent colors). Uses a DaisyUI drawer layout with a sidebar for ROM/emulation controls and a top bar for screen controls. TypeScript modules are organized into feature folders under `web/src/`.

| Directory/File | Description |
| -------------- | ------------- |
| `web/index.html` | Entry point — DaisyUI drawer layout with sidebar, top bar, canvas area, footer, and autorun modal dialog. Loads `./src/app.ts` as the main module. |
| `web/main.css` | Tailwind CSS entry point with DaisyUI plugin config, neon theme overrides, and custom component styles. |
| `web/debugger.css` | Debugger panel styling (green-on-black terminal aesthetic). |
| `web/src/app.ts` | Application bootstrapper — initializes the WASM module, selects `WasmNes` / `WasmGb` / `WasmGba` by ROM extension, sets up the render loop, and coordinates all subsystems. |
| `web/src/audio/` | Audio resampling (`audio_resampler.ts`), frame timing (`frame_limiter.ts`, `frame_plan.ts`). |
| `web/src/input/` | Gamepad API (`gamepad.ts`), GBA keyboard mapping (`keyboard_mapping.ts`), keyboard/gamepad routing (`input_routing.ts`), mouse input (`mouse_input.ts`), pointer lock (`pointer_lock.ts`). |
| `web/src/display/` | Canvas sizing (`canvas_size.ts`), zoom controls (`zoom_controls.ts`), cursor visibility, crosshair overlay, and console-specific filter selection (`filters.ts`; GBA is stock-only in the web frontend). |
| `web/src/rom/` | ROM file listing (`rom_list.ts`), extension-to-console detection (`rom_extensions.ts` for `.nes`, `.gb`, `.gbc`, `.cgb`, `.gba`), selection UI (`rom_selection.ts`), autorun context. |
| `web/src/save-state/` | Save state persistence using IndexedDB (`save_state_storage.ts`, `save_state_controller.ts`, `save_state_context.ts`). |
| `web/src/debugger/` | Browser-based debugger panels — disassembly, OAM viewer, watch expressions, PPU viewer layout/scroll. |
| `web/src/shortcuts/` | Keyboard shortcut actions and help overlay. |
| `web/src/ui/` | Toast overlays (`toast_overlay.ts`), gamepad init toast, sine scroller. |
| `web/integration/` | Playwright-based end-to-end integration tests for the web frontend. |

Each TypeScript module has a corresponding `.test.ts` unit test file (run with `vitest`).

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
| `roms/gb/automated_tests/acid/` | Vendored GBEmulatorShootout acid ROMs (`which.gb`, `dmg-acid2.gb`, `cgb-acid2.gbc`, `cgb-acid-hell.gbc`) used by `src/gb/integration_tests/acid_tests.rs` for DMG/CGB screen CRC coverage. |
| `roms/gb/automated_tests/cpp/` | Vendored CasualPokePlayer GBEmulatorShootout MBC3/RTC ROMs and upstream PNG references used by `src/gb/integration_tests/cpp_tests.rs`. |
| `roms/gb/automated_tests/daid/` | Vendored daid GB/GBC accuracy ROMs and upstream PNG references from GBEmulatorShootout, used by `src/gb/integration_tests/daid_tests.rs` for screen CRC and reference-PNG auditing. |
| `roms/gb/automated_tests/rtc3test/` | Vendored ax6 `rtc3test` split MBC3 RTC test ROMs from GBEmulatorShootout, used by `src/gb/integration_tests/ax6_tests.rs` for DMG/CGB result-screen CRC testing. |
| `roms/gba/automated_tests/gba-tests/` | Git submodule snapshot of jsmolka `gba-tests` (ARM/Thumb GBA CPU validation ROMs) used by `src/gba/integration_tests/gba_suite_tests.rs`. |
| `roms/snes/automated_tests/processor_tests/65816/` | Pinned subset of Tom Harte ProcessorTests 65816 vectors (`v1/*.json`) selected by deterministic family-aware rules and paired emulation/native mode coverage, plus `subset_coverage_report.json` documenting selected opcodes and integrity metadata. Optional full-corpus vectors are downloaded into `full/v1/*.json` (git-ignored) and automatically used by the SNES integration harness when available. |
| `roms/snes/automated_tests/manifest.json` | Canonical SNES automated-test asset provenance manifest. Tracks source URL/ref, license status, oracle type, and CI-subset vs optional-local corpus variants for each SNES suite. Validated by `python -m scripts.validate_snes_test_assets`. |
| `roms/snes/automated_tests/snes_test_roms/gilyon/` | gilyon/snes-tests vendored as a git-subtree mirror (`snes-tests/`) plus prebuilt `cputest.sfc` (full 1610-test 65816 build) and `spctest.sfc` (1320-test SPC-700 build). Used by `src/snes/integration_tests/gilyon_cpu_tests.rs` and `gilyon_spc_tests.rs`. |
| `roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/` | undisbeliever/snes-test-roms (20210217 release), vendored via the `snes_test_roms` submodule: 29 hardware-glitch/timing-hammer ROMs (HDMA-to-$2100 glitches, HDMAEN latch, INIDISP brightness/force-blank hammering, S-CPU-A DMA bug). Used by `src/snes/integration_tests/undisbeliever_tests.rs` (12 automated; 17 committed but not yet automated, see #2943/#2944/#2945/#2947). |
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
| `ci.yml` | Main CI pipeline. Runs on push to `main` and PRs. Jobs: Rust tests (cargo-nextest archive built once, run across 4 shards), Clippy lint, `cargo fmt` check, WASM build + test (`wasm-pack test`), web JS unit tests (`npm test`), web Playwright integration tests, and Python script tests. Uses path-based change detection: per-console source filters (`src/{nes,gb,gba,snes}/**`) and per-console test-asset filters (`roms/<console>/automated_tests/**`, including `snes_test_roms` submodule pointer bumps) select which console suites run — either alone triggers that console's unit + integration tests; `src/platform` or crate-root changes run everything; other Rust changes run all unit tests while skipping every console's `integration_tests` module (mirroring `test-dir.sh --skip-integration`). |
| `release.yml` | Release pipeline triggered by version tags (`v*.*.*`). Runs full CI, then builds Linux x86_64, macOS x86_64, macOS aarch64, and Windows x86_64 on target-compatible runners. Each build job creates a structured release archive with `scripts/package_release.py`, verifies it with `scripts/verify_release_package.py`, and smoke-runs the packaged binary with `--version` from inside the extracted `neser/` directory. Publishes only verified `.tar.gz` and `.zip` archives to GitHub Releases with a git-cliff changelog. |

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
| `neser.conf.example` | Annotated example configuration file documenting all settings: hardware mode (NES + GB + GBA + SNES), audio, video (VSync, window size, fullscreen, shaders), input (gamepads, Four Score, NES + SNES controller types, Zapper detection), debugging, RAM initialization, OAM DRAM decay, overscan, and SNES SPC IPL override path. |
| `gamecontrollerdb.txt` | Community game controller mapping database (SDL_GameControllerDB format, consumed by gilrs) for broad gamepad compatibility. |

### Build Configuration

| File | Description |
| ------ | ------------- |
| `Cargo.toml` | Rust project manifest. Defines the feature flags `native` (default — desktop frontend), `wasm` (WebAssembly frontend), and `tui` (terminal ROM launcher), plus an internal `frontend` meta-feature. The library crate type is both `rlib` (for tests) and `cdylib` (for WASM). Debug builds use `opt-level = 1` to keep audio smooth; dependencies use `opt-level = 3`. |
| `build.rs` | Compile-time code generation — scans for `.autorun` files and generates Rust test functions for each. |
| `playwright.config.mjs` | Playwright configuration for web integration tests. |
| `vite.config.js` | Vite bundler configuration — root: `web/`, build output: `dist/`, dev/preview server on port 8000, Vitest test pattern. |
| `package.json` | Node.js project for web frontend — Vite bundler, Vitest unit tests, and Playwright integration tests. |

## Key Design Decisions

- **Bus-centric architecture**: All memory access goes through the `Bus`, enabling accurate mapper intercepts and DMA behavior.
- **Cycle-accurate timing**: CPU, PPU, and APU are synchronized via a master clock divider. PPU runs 3 cycles per CPU cycle (NTSC) or 3.2 (PAL).
- **Feature-gated frontends**: Native, WASM, and TUI frontends are behind Cargo features, so the core emulation library has no platform dependencies.
- **Interior mutability via `Rc<RefCell<>>`**: Components that need shared ownership (Bus, PPU, APU) use reference-counted cells rather than unsafe code.
- **Mapper trait pattern**: All mappers implement the `Mapper` trait with a standard interface for PRG/CHR reads/writes, IRQ management, and state snapshots. Common banking logic is provided by `BaseMapper`.
- **Deterministic testing**: RAM initialization modes and autorun recordings enable fully deterministic regression testing against reference CRC checksums.
- **Save state serialization**: The primary save-state path uses JSON (via serde) with a versioned format; mapper state is serialized as opaque byte vectors to keep the format flexible. NES autorun recordings and the NES compact-binary save path use postcard.
- **ROM browser architecture**: The native frontend includes a console-style graphical ROM browser as the default landing screen. It uses the shared `platform/catalog` module for ROM discovery, `platform/metadata` for TheGamesDB fuzzy matching via `rusqlite` + `strsim`, and `platform/image_cache` for cover art downloading via `reqwest`. The browser renders a cover art grid with egui (`egui_glow` + `egui_winit`), supports real-time search, a filter panel (platform, players, genre, favorites-only), a detail view overlay, and persistent favorites. When launched without a ROM path, the browser opens first; selecting a ROM transitions to emulation mode via an application state machine.

## Testing Strategy

1. **Unit tests** — Extensive per-module tests throughout the codebase (run with `cargo test --lib`).
2. **ROM-based integration tests** — Blargg, holy-mapperel, daid GB/GBC, Mealybug, SameSuite, ax6 rtc3test, SNES vector/ROM suites, and other community test ROMs verified via headless execution and screen CRC/reference artifact checks. Each console keeps its suites in a `<console>::integration_tests` module so `scripts/test-dir.sh --skip-integration` and CI can exclude them uniformly; the SNES commands, asset policy, and golden-baseline approval workflow are documented in [README-SNES.md](README-SNES.md).
3. **Autorun regression tests** — Build-time generated tests that replay recorded input and verify CRC checkspoints.
4. **WASM tests** — Browser-environment tests via `wasm-pack test --headless --chrome`.
5. **JavaScript unit tests** — Web frontend JS modules tested with Vitest (`npm test`).
6. **Playwright integration tests** — End-to-end browser tests for the web frontend.
7. **Python tests** — Unit tests for the ROM scraper and mappertool utilities.
