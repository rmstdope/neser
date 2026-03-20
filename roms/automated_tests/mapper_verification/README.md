# Mapper Verification Test ROM Suite

A configuration-driven framework for building NES test ROMs that verify mapper implementations. Each mapper's tests are assembled from **shared test logic** combined with **per-mapper definition files** and **linker configs**, enabling broad coverage with minimal code duplication.

The ideas behind this test harness — particularly the `$6000` status byte protocol, the console text output, and the overall structure of init → run tests → report result — are derived from **Blargg's NES test ROMs**, a widely used set of hardware-verified test programs for NES emulators. This framework adapts those patterns into a parameterized, mapper-focused system that can target many different mappers from a single set of test source files.

## Architecture Overview

```
mapper_verification/
├── Makefile                     # Builds all test ROMs (combined + singles)
├── common/                      # Shared infrastructure
│   ├── shell.s                  # Test shell: init, status/console reporting, IRQ handler
│   ├── nes_init.s               # PPU/APU init, RAM clear, VBL sync
│   ├── console.s                # Nametable-based text console (30×28)
│   ├── nes20_header.inc         # NES 2.0 header generation macro
│   ├── mmc3_core_macros.inc     # Shared MMC3-family register definitions
│   ├── vrc4_core_macros.inc     # Shared VRC2/VRC4 register definitions
│   ├── taito_core_macros.inc    # Shared Taito mapper register definitions
│   ├── jy_company_macros.inc    # Shared J.Y. Company helper macros
│   ├── test_macros.inc          # Assertions (assert_a_eq, etc.) and lifecycle
│   ├── nes.inc                  # NES register and constant definitions
│   └── ascii.chr                # 8KB ASCII font (96 tiles, embedded in CHR)
├── tests/                       # Parameterized test sources
│   ├── test_prg_banking.s       # PRG bank switching (8/16/32KB modes)
│   ├── test_chr_banking.s       # CHR bank switching via PPUDATA reads
│   ├── test_nametable.s         # Nametable mirroring (H/V/1A/1B)
│   ├── test_irq.s               # Scanline/M2 counter IRQ verification
│   ├── test_prg_ram.s           # PRG-RAM read/write at $6000-$7FFF
│   ├── test_write_protect.s     # PRG-RAM write-protection verification
│   ├── test_bus_conflicts.s     # Bus conflict AND behavior
│   ├── test_chr_latch.s         # PPU-triggered CHR latch (MMC2/MMC4)
│   ├── test_chr_ram_banking.s   # CHR-RAM bank switching (CPROM)
│   ├── test_nt_from_chr.s       # Namco 163 nametable-from-CHR verification
│   ├── test_multiplier.s        # Mapper-provided hardware multiplier verification
│   ├── test_prg_mode.s          # Mapper-specific PRG mode behavior
│   ├── test_block_select.s      # Multicart outer-bank/block-select verification
│   └── test_combined.s          # Meta-runner: all tests in sequence
├── defs/                        # Per-mapper capability and register definitions
│   ├── m000.0_defs.inc          # NROM
│   ├── m004.0_defs.inc          # MMC3 submapper 0
│   ├── m019.0_defs.inc          # Namco 163
│   ├── m032.0_defs.inc          # Irem G-101
│   ├── m035.0_defs.inc          # J.Y. Company ASIC
│   ├── m045.0_defs.inc          # X-007 multicart
│   └── ...                      # One defs file per covered mapper/submapper
├── configs/                     # Per-mapper linker configurations
│   └── m{NNN}.{S}.cfg           # One .cfg per mapper.submapper variant
└── bin/                         # Built ROM files
    ├── m{NNN}.{S}.nes           # Combined ROMs (all tests for one mapper)
    └── rom_singles/
        └── m{NNN}.{S}_{aspect}.nes  # Individual test ROMs
```

### Design Principles

1. **Configuration-driven**: Test logic lives in shared `.s` files. Per-mapper behavior is controlled entirely through capability flags and register macros in `defs/*.inc` files, plus memory layout in `configs/*.cfg` files. Adding a new mapper requires no changes to shared test code — only new definition and config files.

2. **Dual feedback**: The framework supports two result-reporting channels:
   - **`$6000` status byte** — machine-readable, for automated test runners on mappers with usable PRG-RAM at `$6000-$7FFF`
   - **Console text output** — human-readable, rendered to the PPU nametable

   Most ROMs use both channels. ROMs whose hardware maps registers or PRG-ROM at `$6000-$7FFF` instead use **console verification** only. The test harness automatically re-enables rendering when displaying pass/fail results. Some tests (e.g., CHR banking) may disable rendering during execution to avoid PPU bus conflicts, but `all_passed` and `do_fail_test` both call `console_show` to ensure the console output is always visible to the user.

3. **Conditional assembly**: Tests use `.ifdef` / `.if` guards to include or skip functionality based on mapper capabilities. The same `test_prg_banking.s` handles 8KB (MMC3), 16KB (UxROM), and 32KB (AxROM) banking modes via `PRG_BANK_SIZE`.

4. **Specification-driven**: Tests are designed against the [NESdev wiki](https://www.nesdev.org/wiki/) specifications for each mapper, not against any particular emulator implementation.

## The $6000 Status Byte Protocol

Inspired by Blargg's test ROMs, most test ROMs use memory address `$6000` as a status indicator:

| Value | Meaning |
|-------|---------|
| `$80` | Test is running (set during initialization) |
| `$00` | All tests passed |
| `$01`–`$7F` | Test N failed (value = failing test number) |

In status-byte mode, the test shell (`shell.s`) writes `$80` to `$6000` during reset, then calls the test-specific `run_tests` procedure. If all sub-tests pass, the `all_passed` macro writes `$00`. If any assertion fails, the `fail_test` macro writes the current test number.

This protocol requires PRG-RAM at `$6000`–`$7FFF`. For mappers that don't natively provide PRG-RAM, the NES 2.0 header specifies 8KB PRG-RAM, which emulators allocate.

For mappers where `$6000-$7FFF` is used for banking registers or PRG-ROM, the suite instead uses **console verification**. Those ROMs still print the same `PASSED` / `FAILED` result text, and the Rust integration runner checks the console output rather than polling a status byte.

## How Various Functionality Is Verified

### PRG Banking (`test_prg_banking.s`)

Each PRG bank in the ROM contains an **embedded signature** at a known offset: the 4-byte sequence `$A5, bank_number, ~bank_number, $5A`. The test:

1. Selects a bank via the mapper's `select_prg_bank` macro
2. Reads from the bank window (e.g. `$8000` for 16KB mappers)
3. Verifies the 4-byte signature matches the expected bank number
4. Repeats for all available banks

The test handles three banking granularities:
- **32KB** (AxROM, GNROM): Trampoline-based — the test code itself lives in the switched bank, so it copies a small verification routine to RAM before switching
- **16KB** (UxROM, MMC1 mode 2/3): Reads from the `$8000` window while code runs from the fixed `$C000` bank
- **8KB** (MMC3, MMC5): Tests multiple independently switchable slots

For mappers where the bank number isn't in the low bits of the register value (e.g. mapper 8 uses bits 5–4), the `TRAMPOLINE_BANK_SHIFT` define controls how many left-shifts to apply.

### CHR Banking (`test_chr_banking.s`)

Similar to PRG, each CHR bank contains a signature: `$B6, bank_number, ~bank_number, $6B`. The test:

1. Disables PPU rendering
2. Selects a CHR bank via `select_chr_bank`
3. Sets PPUADDR to the bank window start
4. Reads signature bytes via PPUDATA (`$2007`)
5. Verifies against the expected bank number

The double-read quirk of PPUDATA (first read returns stale buffer) is accounted for with a dummy read before each verification sequence.

### CHR Latch (`test_chr_latch.s`)

MMC2 and MMC4 have a unique CHR banking mechanism: the PPU automatically switches CHR banks when it reads from specific tile addresses (`$0FD8`/`$0FE8` for the left pattern table, `$1FD8`/`$1FE8` for the right). The test:

1. Programs both FD and FE bank registers with different CHR banks
2. Triggers the latch by having the PPU read from the trigger address
3. Reads back CHR data to verify the correct bank is now active
4. Tests both latch states (FD→FE and FE→FD transitions)

### Nametable Mirroring (`test_nametable.s`)

Tests the four standard mirroring modes by writing unique values to one nametable and reading from others:

| Mode | Expected Mirrors |
|------|-----------------|
| Vertical | `$2000`=`$2800`, `$2400`=`$2C00` |
| Horizontal | `$2000`=`$2400`, `$2800`=`$2C00` |
| Single-screen A | All four nametables mirror `$2000` |
| Single-screen B | All four nametables mirror `$2400` |

The mirroring mode is set via the mapper's `set_mirroring` macro, which abstracts away the vastly different register encodings (MMC1 shift register, AxROM bit 4, MMC3 `$A000`, mapper 6 `$42FE`/`$42FF`, etc.).

### PRG Mode (`test_prg_mode.s`)

Some mappers expose mode bits that rearrange which PRG window is fixed and which is switchable without changing the underlying bank register format. `test_prg_mode.s` verifies those mode transitions directly. The first current use is mapper 32, where `$9000` bit 1 selects whether the fixed 16KB window lives at `$8000` or `$C000`.

### IRQ (`test_irq.s`)

Tests mapper-generated interrupt requests. The approach varies by IRQ type:

- **Scanline counters** (MMC3, MMC5): Set the counter to a known value, enable IRQ, enable rendering, wait for the IRQ handler to fire. Verify `irq_count` equals the expected number.
- **M2 cycle counters** (mapper 6, mapper 8): Load a 16-bit countdown value, enable the counter, and poll for the IRQ.

Sub-tests include:
1. IRQ fires at the correct time
2. IRQ does not fire when disabled
3. Counter reload works correctly
4. Acknowledge clears the pending IRQ

The IRQ handler in `shell.s` increments `irq_count`, sets `irq_fired`, and performs mapper-specific acknowledgment (e.g. reading MMC3's `$C000`, writing mapper 6's `$4502`).

### PRG-RAM (`test_prg_ram.s`)

Writes known patterns to `$6004`–`$7FFF` (leaving the low `$6000` area available for status-byte compatibility) and reads them back. Tests include:

1. Write ascending values, verify readback
2. Write `$FF` pattern, verify readback
3. For mappers with multiple 8KB PRG-RAM banks (e.g. mapper 6 with 32KB): switch banks and verify each bank retains its data independently

### Bus Conflicts (`test_bus_conflicts.s`)

Discrete logic mappers (UxROM submapper 0, CNROM submapper 0, AxROM submapper 0) have bus conflicts: the effective register value is the AND of the CPU write and the ROM byte at the write address. The test:

1. Places a `bank_table` in ROM where `bank_table[N] = N`
2. Writes a value that intentionally conflicts with the ROM value
3. Verifies the resulting bank selection matches `write_value AND rom_value`

### CHR-RAM Banking (`test_chr_ram_banking.s`)

For CPROM (mapper 13), CHR is RAM rather than ROM, so there are no pre-embedded signatures. The test:

1. Writes a unique pattern to each 4KB CHR-RAM bank via PPUDATA
2. Switches banks and reads back
3. Verifies each bank preserved its data independently
4. Confirms the fixed bank at `$0000`–`$0FFF` is unaffected by switches

### Nametable from CHR (`test_nt_from_chr.s`)

Namco 163 can route nametable fetches to CHR ROM instead of CIRAM. `test_nt_from_chr.s` programs the nametable mapping slots, then reads back PPU nametable data to verify that the selected CHR signature banks appear at the expected nametable quadrants and that CIRAM fallback still works when CHR mapping is disabled.

### Hardware Multiplier (`test_multiplier.s`)

`test_multiplier.s` verifies mapper-provided 8×8 unsigned hardware multipliers via mapper-defined register symbols. It is currently used for MMC5 (`$5205`/`$5206`) and the J.Y. Company ASIC (`$5800`/`$5801`). The test covers edge cases such as `0×0=0`, `1×1=1`, `255×255=65025`, and representative intermediate values.

### PRG-RAM Write-Protection (`test_write_protect.s`)

For mappers with write-protection registers (MMC3, MMC5, MMC3-derivatives), this test verifies that:

1. PRG-RAM is writable when enabled
2. Write-protection blocks writes (reads return old values)
3. Re-enabling writes restores write capability
4. Write-protection preserves data integrity under attempted corruption

The test writes patterns to `$6000`+, enables write-protect via the mapper-specific `write_protect_prg_ram` macro, attempts to overwrite, and verifies original data is preserved.

### Block Select / Outer Banking (`test_block_select.s`)

Multicart mappers often add an outer block selector on top of an inner banking core. `test_block_select.s` copies a small trampoline into CPU RAM, changes the outer block there, reads a PRG signature from the remapped `$8000` window, then restores the original block before returning to ROM code. This lets the same shared test cover mapper 44 and the later console-verified multicart mappers 37, 45, and 47.

## Mapper Definition Files

Each `defs/m{NNN}.{S}_defs.inc` file defines:

### Capability Flags
```asm
HAS_PRG_BANKING      = 1    ; Mapper supports PRG bank switching
HAS_CHR_BANKING      = 1    ; Mapper supports CHR bank switching
HAS_MIRRORING_CONTROL = 1   ; Mapper can change nametable mirroring
HAS_IRQ              = 1    ; Mapper has an IRQ source
HAS_PRG_RAM          = 1    ; Mapper provides PRG-RAM at $6000
HAS_PRG_RAM_PROTECT  = 1    ; Mapper supports PRG-RAM write-protection
HAS_BUS_CONFLICTS    = 0    ; Mapper has bus conflict behavior
MAX_MIRRORING_MODES  = 4    ; Number of mirroring modes (0/2/4)
```

### Size and Count Parameters
```asm
MAPPER_NUM     = 4           ; iNES mapper number
SUBMAPPER_NUM  = 0           ; NES 2.0 submapper
PRG_BANK_SIZE  = 8           ; Switchable PRG bank size in KB
CHR_BANK_SIZE  = 1           ; Switchable CHR bank size in KB
PRG_BANK_COUNT = 4           ; Number of switchable PRG banks
CHR_BANK_COUNT = 8           ; Number of switchable CHR banks
```

### Register Macros

Every defs file implements a standard set of macros that abstract the mapper's register interface:

```asm
.macro select_prg_bank bank    ; Select PRG bank (slot depends on mapper)
.macro select_chr_bank bank    ; Select CHR bank
.macro set_mirroring mode      ; Set mirroring (0=H, 1=V, 2=1A, 3=1B)
.macro enable_irq              ; Enable mapper IRQ
.macro set_irq_counter value   ; Load IRQ counter
.macro disable_irq             ; Disable mapper IRQ
.macro enable_prg_ram          ; Enable PRG-RAM writes
.macro disable_prg_ram         ; Disable PRG-RAM chip
.macro write_protect_prg_ram   ; Enable chip but block writes
```

These macros vary dramatically per mapper:
- **MMC1**: 5 serial writes to shift register
- **MMC3**: Bank select register (`$8000`) + bank data register (`$8001`)
- **UxROM**: Simple write to `$8000` (value = bank number)
- **AxROM**: Write to `$8000` (bits 0–2 = bank, bit 4 = nametable select)
- **Mapper 6/8**: Latch write with encoded bank bits, separate mirroring registers

### Shared MMC3 Macros

To eliminate duplication, MMC3-based boards and wrappers (such as 4, 12, 14, 37, 44, 45, and 47) include `common/mmc3_core_macros.inc`, which defines:

- **Register addresses**: `MMC3_BANK_SELECT`, `MMC3_BANK_DATA`, `MMC3_MIRRORING`, `MMC3_PRG_RAM`, `MMC3_IRQ_LATCH`, etc.
- **Core macros**: `select_prg_bank`, `select_chr_bank`, `set_mirroring`, IRQ control, PRG-RAM enable/disable/write-protect

Each mapper's defs file then includes the shared macros and adds mapper-specific extensions:
- **Mapper 12**: Outer CHR A18 register (`$4132`) via `set_chr_ext` macro
- **Mapper 14**: Supervisor mode register (`$A131`) via `set_mmc3_mode` macro

### Shared VRC2/VRC4, Taito, and J.Y. Company Macros

Other mapper families also share helper macro layers:

- `common/vrc4_core_macros.inc` centralizes the register encodings used by the VRC2/VRC4-family definitions (`21`, `23`, `25`)
- `common/taito_core_macros.inc` centralizes common Taito register layouts reused by mapper `33` and mapper `48`
- `common/jy_company_macros.inc` provides the banking / IRQ / multiplier helpers shared by mapper `35`

This approach keeps mapper-specific definitions small while letting MMC3-style boards share one core register vocabulary.

## Linker Configurations

Each `configs/m{NNN}.{S}.cfg` file defines the ROM's memory layout:

- **HEADER** segment (16 bytes): NES 2.0 header with correct mapper, submapper, PRG/CHR sizes
- **ZEROPAGE** / **RAM** / **SRAM**: CPU address space segments
- **PRG bank segments**: Sized according to the mapper's banking scheme, with signature bytes embedded at known offsets
- **CHR bank segments**: For CHR-ROM mappers, includes signature data and the ASCII font
- **VECTORS**: NMI/Reset/IRQ vectors at `$FFFA`–`$FFFF`

For example, MMC3 (mapper 4) defines 8KB PRG banks and 1KB CHR banks, while AxROM (mapper 7) defines 32KB PRG banks with no CHR-ROM.

Some mappers require special segments:
- **Mapper 15**: A `BOOT` segment in PRG bank 1 with bootstrap code that switches from the power-on banking mode to the test-compatible mode before jumping to the main reset handler

## Build System (Makefile)

### Requirements

The [cc65](https://cc65.github.io/cc65/) toolchain (`ca65` assembler and `ld65` linker) must be installed. On macOS: `brew install cc65`.

### Build Targets

```bash
make              # Build all ROMs (combined + singles)
make clean        # Remove all built ROMs and intermediate files
make combined     # Build only combined ROMs
make singles      # Build only individual test ROMs
```

### How It Works

The Makefile uses a `ROM_RULE` macro that generates build rules for each ROM:

```makefile
# $(call ROM_RULE, target_path, test_aspect, mapper_id)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_prg_banking.nes,prg_banking,m004.0))
```

Each invocation:

1. **Generates `mapper_config.inc`** — a one-line file that includes the mapper's definition file:
   ```asm
   .include "m004.0_defs.inc"
   ```

2. **Assembles** the test source, shell, nes_init, and console with `ca65`, passing include paths for `common/`, `defs/`, and the generated config

3. **Links** all object files with `ld65` using the mapper's `.cfg` file

Combined ROMs use a `COMBINED_RULE` that assembles all test sources with `-DCOMBINED=1`, which makes `test_combined.s` the entry point. It calls each individual test's `run_tests` in sequence.

### ROM Naming Convention

```
m{NNN}.{S}_{aspect}.nes
```

- `{NNN}` — 3-digit zero-padded mapper number
- `{S}` — single-digit submapper number
- `{aspect}` — test aspect in snake_case

Examples:
- `m004.0_prg_banking.nes` — MMC3, submapper 0, PRG banking test
- `m007.1_nametable.nes` — AxROM, submapper 1 (no bus conflicts), nametable test
- `m004.0.nes` — MMC3, submapper 0, combined (all tests)

### Current ROM Count

| Type | Count |
|------|-------|
| Combined ROMs | 62 |
| Single test ROMs | 190 |
| **Total** | **252** |

## Automated Testing Integration

### Rust Test Runner

The emulator's Rust test suite (`src/integration_tests/mapper_tests.rs`) integrates the verification ROMs using both `setup_rom_test!` and `setup_rom_console_test!`:

```rust
setup_rom_test!(
    test_mv_m004_0_prg_banking,
    "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_prg_banking.nes"
);

setup_rom_console_test!(
    test_mv_m045_0_block_select,
    "roms/automated_tests/mapper_verification/bin/rom_singles/m045.0_block_select.nes"
);
```

These macros generate `#[test]` functions that:

1. Loads the ROM file and creates a `Cartridge`
2. Auto-detects the TV system (NTSC/PAL) from the NES 2.0 header
3. Creates an NES instance and inserts the cartridge
4. Runs the emulator frame-by-frame
5. Verifies either:
   - a `$6000` status byte transition (`setup_rom_test!`), or
   - console text containing the configured pass string (`setup_rom_console_test!`)

For status-byte tests, the runner first waits for `$6000` to become `$80` (test running), then watches for the final result. This two-phase approach prevents false positives from uninitialized RAM. Console-verified tests instead watch the rendered nametable text for `PASSED` / `FAILED` output.

### Running Tests

```bash
# Run all mapper verification tests
cargo test --no-default-features -- "test_mv_m"

# Run tests for a specific mapper
cargo test --no-default-features -- "test_mv_m004"

# Run a specific test
cargo test --no-default-features -- "test_mv_m004_0_prg_banking"
```

### CI Integration

The pre-built ROM binaries are committed to `bin/` so that CI can run the tests without requiring the cc65 toolchain. Locally, `cargo test --no-default-features` exercises the mapper verification suite in this repository. CI consumes the same committed ROM binaries as part of the broader Rust test jobs.

### CRC-Based Rendering Verification

Some mapper features only become observable through actual PPU rendering — they cannot be verified through PPUDATA register reads. Examples include:

- MMC5 extended attribute mode (per-tile CHR bank and palette via ExRAM)
- MMC5 vertical split screen (split region with separate nametable, CHR bank, and scroll)
- MMC5 8×16 sprite CHR A/B register separation (sprites vs background use different CHR banks)

For these features, the framework uses **CRC-based framebuffer verification**:

1. **ROM side**: A standalone rendering program sets up deterministic CHR, nametable, palette, and mapper state, enables PPU rendering, and loops forever. The ROM uses the standard `ROM_RULE` build flow but never returns from `run_tests`.

2. **Rust side**: The `setup_rom_crc_test!` macro runs the emulator for a fixed number of frames and verifies the screen buffer's CRC-32 against an approved baseline.

```rust
setup_rom_crc_test!(
    test_mv_m005_0_mmc5_ext_attr,
    "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_mmc5_ext_attr.nes",
    [(60, 38994255u32)]
);
```

#### Expected Visual Output

**`m005.0_mmc5_ext_attr`** — Three horizontal bands, each 10 tile rows (80 px) tall, filling the full width:
- **Top third (rows 0–9):** White solid 8×8 tiles on black — ExRAM selects CHR bank 0, palette 0
- **Middle third (rows 10–19):** Red horizontal-striped tiles on black — ExRAM selects CHR bank 2, palette 1
- **Bottom third (rows 20–29):** Cyan vertical-striped tiles on black — ExRAM selects CHR bank 4, palette 2

**`m005.0_mmc5_split`** — Vertical split with left-side split threshold at tile 16:
- **Columns 0–15:** Red horizontal-striped tiles on black — split region active, ExRAM provides tile indices and attributes, split CHR bank 2 provides the stripe pattern, palette 1 (color 3 = red).
- **Columns 16–31:** White solid tiles on black — main region, CIRAM nametable tile $01, CHR bank 0, palette 0 (color 3 = white).
- The split region has a 32-pixel vertical scroll offset ($5201 = $20), so the left side's content is shifted up by 4 tile rows compared to the right.
- **CL mode (default):** All commercial ExROM boards use CL mode wiring, where the PPU's own fine Y bits drive CHR A0–A2 (the MMC5's split scroll fine Y is NOT connected to CHR ROM). This means the split region's fine Y matches the PPU's fine Y. The ROM sets $5201 so its low 3 bits match the PPU's fine Y scroll to avoid tile "rolling." No fine Y offset is visible on tiles 0–1 in CL mode.

**`m005.0_mmc5_sprite_chr`** — 8×16 sprite CHR A/B register separation:
- **Background (full screen):** White solid 8×8 tiles — B registers ($5128–$512B) select CHR bank 0 (solid pattern), palette 0 (color 3 = white).
- **Sprites (4×2 grid):** Red/black checkerboard 8×16 sprites — A registers ($5120–$5127) select CHR bank 2 (checkerboard pattern), sprite palette 0 (color 3 = red).
  - Top row: 4 sprites at screen positions (80, 49), (96, 49), (112, 49), (128, 49)
  - Bottom row: 4 sprites at screen positions (80, 81), (96, 81), (112, 81), (128, 81)
  - Note: screen Y = OAM Y + 1, which is standard NES hardware behavior (the PPU adds 1 to the stored Y value).
- The key verification: sprites use different CHR data (checkerboard from A registers) than the background (solid from B registers), proving A/B separation works in CHR mode 3 with 8×16 sprites.

**Adding a new rendering verification ROM:**

1. Create `tests/test_my_feature.s` — set up the rendering state and loop forever
2. Embed CHR tile data in `CHR_SIG*` segments with distinct per-bank patterns
3. Add a `ROM_RULE` entry in the Makefile (not `COMBINED_RULE` — rendering tests are standalone)
4. Build the ROM and run with `NESER_CAPTURE_SCREEN=1` to capture a screenshot and CRC
5. Visually verify the screenshot shows the expected pattern
6. Add a `setup_rom_crc_test!` entry with the approved CRC value

## Adding a New Mapper

To add tests for a new mapper (e.g. mapper 99):

1. **Create `defs/m099.0_defs.inc`** — Define capability flags, bank sizes, and register macros based on the [NESdev wiki](https://www.nesdev.org/wiki/) specification

2. **Create `configs/m099.0.cfg`** — Define memory layout matching the mapper's banking scheme

3. **Add Makefile targets** — Add `ROM_RULE` entries for each test aspect and a `COMBINED_RULE` entry

4. **Add conditionals to shared test files** (if needed) — For example, if the mapper has a unique mirroring encoding, add a `.elseif MAPPER_NUM = 99` branch to `test_nametable.s`

5. **Add mapper-specific init to `shell.s`** (if needed) — For example, font loading for MMC3 clones, or mode switching for multicart mappers

6. **Add Rust test entries** — Add `setup_rom_test!` for `$6000`-status ROMs or `setup_rom_console_test!` for console-verified ROMs

7. **Build and commit** — Run `make` to build the ROMs, then commit the binaries to `bin/`

## Covered Mappers

All mapper numbers from `0` through `48` have been reviewed against NESdev documentation.

- **Implemented coverage:** 45 of 49 mapper numbers have at least one verification ROM.
- **Not yet implemented:** 4 mapper numbers (`17, 20, 27, 36`)
- **Main reasons for the remaining deferrals:** FDS/dump-specific formats, unusual one-off hardware control schemes, or low incremental value compared to already-covered equivalents.

| Mapper | Name | Submappers | Tests |
|--------|------|------------|-------|
| 0 | NROM | 0 | PRG-RAM |
| 1 | MMC1 | 0, 5 | PRG banking, CHR banking, nametable, PRG-RAM |
| 2 | UxROM | 0, 2 | PRG banking, bus conflicts |
| 3 | CNROM | 0, 1 | CHR banking, bus conflicts |
| 4 | MMC3 | 0, 1 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, write-protect |
| 5 | MMC5 | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, multiplier, write-protect, ext-attr†, split†, sprite-chr† |
| 6 | Front Fareast | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM |
| 7 | AxROM | 0, 1 | PRG banking, nametable |
| 8 | SMC GNROM | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM |
| 9 | MMC2 | 0 | PRG banking, CHR latch, nametable |
| 10 | MMC4 | 0 | PRG banking, CHR latch, nametable, PRG-RAM |
| 11 | Color Dreams | 0, 1 | PRG banking, CHR banking |
| 12 | SL-5020B | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, write-protect |
| 13 | CPROM | 0 | CHR-RAM banking |
| 14 | SL-1632 | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, write-protect |
| 15 | K-1029 | 0 | PRG banking, nametable |
| 16 | Bandai FCG | 4, 5 | PRG banking, CHR banking, nametable, IRQ |
| 18 | Jaleco SS 88006 | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM |
| 19 | Namco 163 | 0, 2 | PRG banking, CHR banking, nametable, IRQ, NT from CHR |
| 21 | VRC4a/c | 1, 2 | PRG banking, CHR banking, nametable, IRQ |
| 22 | VRC2a | 0 | PRG banking, CHR banking, nametable |
| 23 | VRC4e/f, VRC2b | 1, 2, 3 | PRG banking, CHR banking, nametable, IRQ |
| 24 | VRC6a | 0 | PRG banking, CHR banking, nametable, IRQ |
| 25 | VRC4b/d, VRC2c | 1, 2, 3 | PRG banking, CHR banking, nametable, IRQ |
| 26 | VRC6b | 0 | PRG banking, CHR banking, nametable, IRQ |
| 28 | Action 53 | 0 | PRG banking, nametable |
| 29 | RET-CUFROM | 0 | PRG banking, CHR-RAM banking |
| 30 | UNROM 512 | 0, 2 | PRG banking, CHR-RAM banking, nametable |
| 31 | NSF subset | 0 | PRG banking |
| 32 | Irem G-101 | 0, 1 | PRG banking, CHR banking, nametable, PRG-RAM, PRG mode |
| 33 | Taito TC0190 | 0 | PRG banking, CHR banking, nametable, PRG-RAM |
| 34 | BNROM/NINA-001 | 0, 1, 2 | PRG banking, CHR banking, PRG-RAM, bus conflicts |
| 35 | J.Y. Company ASIC | 0 | PRG banking, CHR banking, nametable, PRG-RAM, multiplier |
| 37 | MMC3 multicart | 0 | Block select |
| 38 | Bit Corp. PCI556 | 0 | PRG banking, CHR banking, PRG-RAM |
| 39 | BNROM clone | 0 | PRG banking |
| 40 | SMB2j pirate | 0 | PRG banking |
| 41 | Caltron 6-in-1 | 0 | PRG banking |
| 42 | Sachen SA-72007 | 0 | PRG banking, CHR banking, nametable |
| 43 | Sachen SA-0036 | 0 | PRG banking |
| 44 | Super Big 7-in-1 | 0 | PRG banking, CHR banking, nametable, IRQ, block select |
| 45 | X-007 multicart | 0 | Block select |
| 46 | Game Station / Rumblestation | 0 | PRG banking, CHR banking |
| 47 | Super Spike / Nintendo World Cup multicart | 0 | Block select |
| 48 | Taito TC0690 | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM |

Several of the later multicart / register-at-`$6000` mappers (`37`, `40`, `41`, `42`, `43`, `45`, `46`, `47`) are covered through console verification rather than the `$6000` status-byte protocol.

†Features marked with † use CRC-based rendering verification instead of the `$6000` status-byte protocol. See [CRC-Based Rendering Verification](#crc-based-rendering-verification) above.

### Mappers Reviewed but Not Yet Implemented

| Mapper | Reason not implemented yet |
|--------|-----------------------------|
| 17 | Super Magic Card dump format rather than a normal cartridge target; trainer relocation and dump-specific startup behavior are outside the current cartridge verification framework. |
| 20 | Reserved for Famicom Disk System in iNES; not a standard cartridge mapper, so it is outside the scope of this ROM suite. |
| 27 | Pirate VRC2/VRC4 variant believed to be effectively a mapper 23-style duplicate; lower priority because existing VRC2/VRC4 coverage already exercises the interesting banking and IRQ behavior. |
| 36 | Safe for the `$6000` protocol, but the TXC board uses a `$4100-$4103` state machine plus `$8000` commit step, making it a specialized one-off that was deferred in favor of broader coverage first. |

Mappers `37`, `40`, `41`, `42`, `43`, `45`, `46`, and `47` were added after the suite gained console-verification support for boards that cannot safely use the `$6000` status-byte protocol. Mapper 36 is now the only remaining unimplemented mapper in the `32-48` range.

## Acknowledgments

The test harness design — particularly the `$6000` status byte protocol, the console text rendering approach, and the overall init-test-report structure — is inspired by [Blargg's NES test ROMs](https://github.com/christopherpow/nes-test-roms/tree/master/blargg_nes_cpu_test5). Blargg's tests established the convention of using `$6000` as a machine-readable status indicator alongside human-readable text output, which has become a de facto standard in NES emulator testing. This framework adapts those ideas into a parameterized, configuration-driven system specifically targeting mapper verification.
