# NESER Architecture

> NESER — NES Emulator in Rust

## Overview

NESER is a cycle-accurate NES (Nintendo Entertainment System) emulator written in Rust, built on an architecture that supports multiple emulated hardware targets. It supports three frontend targets: a native desktop window (winit + OpenGL), a terminal-based TUI ROM launcher, and a WebAssembly-powered browser frontend. The emulator implements the core NES hardware — CPU, PPU, APU, and bus — as well as over 200 cartridge mappers, multiple input device types, debugging tools, save states, and an autorun recording/playback system.

The codebase is roughly 183,000 lines of Rust, with additional JavaScript for the web frontend and Python tooling for ROM management.

## High-Level Architecture

```none
┌───────────────────────────────────────────────────────┐
│                     Frontends                         │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
│  │Native Frontend│ │ TUI Frontend │ │ WASM Frontend│   │
│  │(Desktop, GL) │ │ (Terminal)   │ │ (Browser)    │   │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘   │
│         │                │                │           │
│         └────────────────┼────────────────┘           │
│                          ▼                            │
│  ┌─────────────────────────────────────────────────┐  │
│  │  Console enum (src/emulator.rs)                 │  │
│  │  Hardware-agnostic interface: run_tick, render,  │  │
│  │  audio, input, save/load state, reset           │  │
│  │  Variants: Console::Nes(Nes)                    │  │
│  └──────────────────────┬─────────────────────────┘  │
│                          │                            │
│                          ▼                            │
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
│  │  AppContext · FrontendConfig · Audio · Rendering │  │
│  └─────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────┘
```

The emulator is designed around a **multi-layer architecture**:

- **Console enum** (`src/emulator.rs`): Hardware-agnostic dispatch layer. Frontends program against `Console` for common operations (run, render, audio, input, save/load state, reset). System-specific features (NES debugging, PPU viewer, Zapper) are accessed by matching on the `Console::Nes` variant.
- **NES emulator** (`src/nes/`): All NES-specific hardware lives under this namespace. The `Nes` struct in `src/nes/console/nes.rs` orchestrates the per-cycle stepping of CPU, PPU, APU, and Bus.
- **Shared platform** (`src/`): `FrontendConfig` (src/config.rs), `AppContext` (src/app_context.rs), audio infrastructure, and rendering backends are shared across all emulated systems.
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
| `scripts/test-dir.sh` | Runs Rust tests for specific source directories. Converts directory paths (e.g., `src/nes/cartridge`) to `cargo test` module filters. Supports `--skip-integration` and `--list` flags. Used by CI to conditionally run tests based on changed files. |

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

#### Platform Layer (Hardware-Agnostic)

| File | Description |
| ------ | ------------- |
| `src/emulator.rs` | `Console` enum — hardware-agnostic wrapper around system-specific emulators. Provides common interface: `run_tick`, `is_ready_to_render`, `screen_snapshot`, `get_sample`, `set_button`, `save_state_bytes`/`load_state_bytes`, `reset`. System-specific features accessed via variant matching (`Console::Nes`). |
| `src/config.rs` | `FrontendConfig` struct — generic frontend settings (audio, video, autorun, debugger, window) shared across all emulated systems. |
| `src/app_context.rs` | `AppContext` — shared application state including configuration, ROM database, and toast notification manager. Wrapped in `Rc<RefCell<>>` for interior mutability. |

#### NES Emulation (`src/nes/`)

All NES-specific hardware and supporting code lives under `src/nes/`.

| Directory/File | Description |
| ---------------- | ------------- |
| `src/nes/mod.rs` | Module declarations for all NES sub-modules. |
| `src/nes/console/` | Top-level NES orchestration. |
| `src/nes/console/nes.rs` | The `Nes` struct — creates and owns CPU, PPU, APU, and Bus. Runs the master clock cycle loop. Handles save state capture/restore, cartridge insertion, and reset logic. |
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
| `src/gb/console.rs` | `Gb<B: GbBus>` — thin console shell that owns the CPU. `step()` executes one instruction and ticks the bus by the elapsed M-cycles. |
| `src/gb/bus/bus.rs` | `GbBus` trait — `read(&mut self, addr: u16) -> u8`, `write(&mut self, addr: u16, val: u8)`, and a default no-op `tick(&mut self, m_cycles: u8)`. `StubBus` implements the trait for unit tests. |
| `src/gb/bus/dmg_bus.rs` | `DmgBus` — full DMG memory map. Routes all 16-bit addresses to cartridge ROM/RAM, VRAM, WRAM, echo RAM, OAM, HRAM, Timer registers ($FF04–$FF07), APU registers ($FF10–$FF3F), IF ($FF0F), IE ($FFFF), and I/O stubs. Owns the cartridge, Timer, and APU. Overrides `tick()` to advance the Timer, APU, and propagate timer interrupts to IF. Exposes `sample_ready()`/`take_sample()`/`set_audio_sample_rate()` for the platform audio layer. |
| `src/gb/apu/mod.rs` | `Apu` — DMG Audio Processing Unit. 8-step frame sequencer (512 Hz), NR50/NR51/NR52 power/volume/panning control, mixer (NR51 L/R routing, NR50 master volume), sample output pipeline (fractional M-cycle accumulator). |
| `src/gb/apu/channel1.rs` | `Channel1` — Pulse channel with frequency sweep. Duty cycle (4 patterns), length counter, volume envelope, frequency sweep (period, direction, shift). |
| `src/gb/apu/channel2.rs` | `Channel2` — Pulse channel without sweep. Duty cycle, length counter, volume envelope. |
| `src/gb/apu/channel3.rs` | `Channel3` — Wave output channel. 32-nibble wave RAM ($FF30–$FF3F), 4 output levels (mute/100%/50%/25%), length counter, wave position advancing at half the pulse-channel rate. |
| `src/gb/apu/channel4.rs` | `Channel4` — Noise channel. 15-bit or 7-bit LFSR, 8 clock divisor codes × 8 shift values = 64 noise frequencies, length counter, volume envelope. |
| `src/gb/cpu/sm83.rs` | `Sm83<B: GbBus>` — SM83/LR35902 CPU core. Full instruction set (primary + CB-prefixed), HALT bug, interrupt dispatch at five vectors. Each M-cycle increments an internal counter used by the console for bus ticking. |
| `src/gb/cpu/opcode.rs` | Opcode metadata tables (BASE[256] and CB[256]) for debugging and tracing. |
| `src/gb/timer/timer.rs` | `Timer` — DIV/TIMA/TMA/TAC subsystem. `tick(m_cycles)` advances counters and sets `interrupt_pending` on TIMA overflow; caller (DmgBus) propagates this to IF. |
| `src/gb/cartridge/cartridge.rs` | `GbCartridge` trait — `read(&self, addr: u16) -> u8` and `write(&mut self, addr: u16, val: u8)`. Addresses $0000–$7FFF map to ROM; $A000–$BFFF to cartridge RAM. |
| `src/gb/cartridge/mbc0.rs` | ROM-only cartridge (MBC type 0x00). No banking; writes are silently ignored. |
| `src/gb/cartridge/mbc1.rs` | MBC1 cartridge (types 0x01–0x03). ROM bank switching ($2000–$3FFF), secondary bank register ($4000–$5FFF), banking mode ($6000–$7FFF), RAM enable ($0000–$1FFF). Supports up to 2 MB ROM and 32 KB RAM. |
| `src/gb/cartridge/mod.rs` | `load_cartridge(bytes: &[u8]) -> Result<Box<dyn GbCartridge>, RomError>` — parses a `.gb` ROM, validates the header checksum, and returns the appropriate MBC implementation. `RomError` variants: `TooShort`, `BadHeaderChecksum`, `UnsupportedMbc(u8)`. |

#### Frontends

| Directory/File | Description |
| ---------------- | ------------- |
| `src/frontends/native/` | Desktop frontend using winit + OpenGL. |
| `src/frontends/native/event_loop.rs` | Main event loop — holds `Console` enum, handles input events, frame timing, VSync, autorun integration, pause/resume, and hot-reload of ROMs. NES-specific features (debugger, Zapper, SNES mouse) accessed by extracting the inner `Nes` via pattern match. |
| `src/frontends/native/audio.rs` | Native audio device setup and sample queuing. |
| `src/frontends/native/keyboard.rs` | Keyboard input handling — maps physical keys to NES buttons, debugger hotkeys, and system commands. |
| `src/frontends/native/gamepad.rs` | Gamepad input using gilrs — maps controller axes/buttons to NES joypads. |
| `src/frontends/native/mouse.rs` | Mouse input — Zapper light gun, SNES mouse, and Arkanoid paddle coordinate mapping. |
| `src/frontends/native/gl_wrapper.rs` | OpenGL context management for native windows. |
| `src/rendering/` | Shared rendering infrastructure (native feature). |
| `src/rendering/gl_backend.rs` | OpenGL framebuffer, texture management, and imgui debugger UI. |
| `src/rendering/shader_manager.rs` | Shader pipeline using librashader — loads `.slangp` presets (CRT, NTSC, xBRZ). |
| `src/frontends/tui/` | Terminal UI ROM launcher using `ratatui` + `crossterm`. |
| `src/frontends/tui/app.rs` | TUI application state and event loop. |
| `src/frontends/tui/rom_list.rs` | Scrollable ROM list widget. |
| `src/frontends/tui/catalog.rs` | Integration with the cartridge catalog for ROM discovery. |
| `src/frontends/tui/launcher.rs` | Launches the SDL emulator for a selected ROM. |
| `src/frontends/tui/action_menu.rs` | Context menu for ROM actions. |
| `src/frontends/web/` | WebAssembly frontend. |
| `src/frontends/web/wasm.rs` | `wasm-bindgen` bindings — exposes `NesEmulator` to JavaScript with methods for frame stepping, input, audio sample retrieval, and save states. |
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
| `src/nes/debugging/ui.rs` | ImGui-based debugger UI with CPU state, memory viewer, and disassembly. |
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
| `src/nes/frontend_toasts.rs` | Toast message formatters for NES-specific user notifications (cartridge loaded, hardware mode, gamepad detection, timing mode). |

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

The web frontend is bundled with **Vite** (config at `vite.config.js`, root: `web/`, build output: `dist/`). JavaScript modules are organized into feature folders under `web/src/`.

| Directory/File | Description |
| -------------- | ------------- |
| `web/index.html` | Entry point — canvas element, ROM file picker, keyboard shortcuts overlay. Loads `./src/app.js` as the main module. |
| `web/styles.css` | Application styling. |
| `web/src/app.js` | Application bootstrapper — initializes the WASM module, sets up the render loop, and coordinates all subsystems. |
| `web/src/audio/` | Audio resampling (`audio_resampler.js`), frame timing (`frame_limiter.js`, `frame_plan.js`). |
| `web/src/input/` | Gamepad API (`gamepad.js`), keyboard/gamepad routing (`input_routing.js`), mouse input (`mouse_input.js`), pointer lock (`pointer_lock.js`). |
| `web/src/display/` | Canvas sizing (`canvas_size.js`), zoom controls (`zoom_controls.js`), cursor visibility, crosshair overlay. |
| `web/src/rom/` | ROM file listing (`rom_list.js`), selection UI (`rom_selection.js`), autorun context. |
| `web/src/save-state/` | Save state persistence using IndexedDB (`save_state_storage.js`, `save_state_controller.js`, `save_state_context.js`). |
| `web/src/debugger/` | Browser-based debugger panels — disassembly, OAM viewer, watch expressions, PPU viewer layout/scroll. |
| `web/src/shortcuts/` | Keyboard shortcut actions and help overlay. |
| `web/src/ui/` | Toast overlays (`toast_overlay.js`), gamepad init toast, sine scroller. |
| `web/integration/` | Playwright-based end-to-end integration tests for the web frontend. |

Each JavaScript module has a corresponding `.test.mjs` unit test file (run with `vitest`).

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
| `vite.config.js` | Vite bundler configuration — root: `web/`, build output: `dist/`, dev/preview server on port 8000, Vitest test pattern. |
| `package.json` | Node.js project for web frontend — Vite bundler, Vitest unit tests, and Playwright integration tests. |

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
5. **JavaScript unit tests** — Web frontend JS modules tested with Vitest (`npm test`).
6. **Playwright integration tests** — End-to-end browser tests for the web frontend.
7. **Python tests** — Unit tests for the ROM scraper and mappertool utilities.
