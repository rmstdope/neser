# Mapper Verification Test ROM Suite

A configuration-driven framework for building NES test ROMs that verify mapper implementations. Each mapper's tests are assembled from **shared test logic** combined with **per-mapper definition files** and **linker configs**, enabling broad coverage with minimal code duplication.

The ideas behind this test harness — particularly the `$6000` status byte protocol, the console text output, and the overall structure of init → run tests → report result — are derived from **Blargg's NES test ROMs**, a widely used set of hardware-verified test programs for NES emulators. This framework adapts those patterns into a parameterized, mapper-focused system that can target many different mappers from a single set of test source files.

## Architecture Overview

```
mapper_verification/
├── Makefile                     # Builds all test ROMs (combined + singles)
├── common/                      # Shared infrastructure
│   ├── shell.s                  # Test shell: init, $6000 protocol, IRQ handler
│   ├── nes_init.s               # PPU/APU init, RAM clear, VBL sync
│   ├── console.s                # Nametable-based text console (30×28)
│   ├── nes20_header.inc         # NES 2.0 header generation macro
│   ├── mmc3_core_macros.inc     # Shared MMC3 register definitions (mappers 4/12/14)
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
│   ├── test_multiplier.s        # MMC5 hardware 8×8 multiplier
│   └── test_combined.s          # Meta-runner: all tests in sequence
├── defs/                        # Per-mapper capability and register definitions
│   ├── m000.0_defs.inc          # NROM
│   ├── m001.0_defs.inc          # MMC1 submapper 0
│   ├── m001.5_defs.inc          # MMC1 submapper 5 (fixed PRG)
│   ├── m002.0_defs.inc          # UxROM submapper 0 (bus conflicts)
│   ├── m002.2_defs.inc          # UxROM submapper 2 (no bus conflicts)
│   ├── m003.0_defs.inc          # CNROM submapper 0 (bus conflicts)
│   ├── m003.1_defs.inc          # CNROM submapper 1 (no bus conflicts)
│   ├── m004.0_defs.inc          # MMC3 submapper 0 (Sharp IRQ)
│   ├── m004.1_defs.inc          # MMC3 submapper 1 (NEC IRQ)
│   ├── m005.0_defs.inc          # MMC5
│   ├── m006.0_defs.inc          # Front Fareast Magic Card
│   ├── m007.0_defs.inc          # AxROM submapper 0 (bus conflicts)
│   ├── m007.1_defs.inc          # AxROM submapper 1 (no bus conflicts)
│   ├── m008.0_defs.inc          # SMC GNROM mode 4
│   ├── m009.0_defs.inc          # MMC2
│   ├── m010.0_defs.inc          # MMC4
│   ├── m011.0_defs.inc          # Color Dreams submapper 0 (bus conflicts)
│   ├── m011.1_defs.inc          # Color Dreams submapper 1 (no bus conflicts)
│   ├── m012.0_defs.inc          # SL-5020B (MMC3 + outer CHR)
│   ├── m013.0_defs.inc          # CPROM
│   ├── m014.0_defs.inc          # SL-1632 (MMC3/VRC2 hybrid)
│   └── m015.0_defs.inc          # K-1029 multicart
├── configs/                     # Per-mapper linker configurations
│   └── m{NNN}.{S}.cfg           # One .cfg per mapper.submapper variant
└── bin/                         # Built ROM files
    ├── m{NNN}.{S}.nes           # Combined ROMs (all tests for one mapper)
    └── rom_singles/
        └── m{NNN}.{S}_{aspect}.nes  # Individual test ROMs
```

### Design Principles

1. **Configuration-driven**: Test logic lives in shared `.s` files. Per-mapper behavior is controlled entirely through capability flags and register macros in `defs/*.inc` files, plus memory layout in `configs/*.cfg` files. Adding a new mapper requires no changes to shared test code — only new definition and config files.

2. **Dual feedback**: Every test reports results through **two channels** simultaneously:
   - **`$6000` status byte** — machine-readable, for automated test runners
   - **Console text output** — human-readable, rendered to the PPU nametable

3. **Conditional assembly**: Tests use `.ifdef` / `.if` guards to include or skip functionality based on mapper capabilities. The same `test_prg_banking.s` handles 8KB (MMC3), 16KB (UxROM), and 32KB (AxROM) banking modes via `PRG_BANK_SIZE`.

4. **Specification-driven**: Tests are designed against the [NESdev wiki](https://www.nesdev.org/wiki/) specifications for each mapper, not against any particular emulator implementation.

## The $6000 Status Byte Protocol

Inspired by Blargg's test ROMs, every test ROM uses memory address `$6000` as a status indicator:

| Value | Meaning |
|-------|---------|
| `$80` | Test is running (set during initialization) |
| `$00` | All tests passed |
| `$01`–`$7F` | Test N failed (value = failing test number) |

The test shell (`shell.s`) writes `$80` to `$6000` during reset, then calls the test-specific `run_tests` procedure. If all sub-tests pass, the `all_passed` macro writes `$00`. If any assertion fails, the `fail_test` macro writes the current test number (stored in `TEST_CODE` at `$6001`).

This protocol requires PRG-RAM at `$6000`–`$7FFF`. For mappers that don't natively provide PRG-RAM, the NES 2.0 header specifies 8KB PRG-RAM, which emulators allocate.

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

Writes known patterns to `$6004`–`$7FFF` (reserving `$6000`–`$6003` for the status protocol) and reads them back. Tests include:

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

### Hardware Multiplier (`test_multiplier.s`)

MMC5 provides an 8×8 unsigned hardware multiplier at `$5205`/`$5206`. The test verifies edge cases: `0×0=0`, `1×1=1`, `255×255=65025`, and intermediate values.

### PRG-RAM Write-Protection (`test_write_protect.s`)

For mappers with write-protection registers (MMC3, MMC5, MMC3-derivatives), this test verifies that:

1. PRG-RAM is writable when enabled
2. Write-protection blocks writes (reads return old values)
3. Re-enabling writes restores write capability
4. Write-protection preserves data integrity under attempted corruption

The test writes patterns to `$6000`+, enables write-protect via the mapper-specific `write_protect_prg_ram` macro, attempts to overwrite, and verifies original data is preserved.

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

To eliminate duplication, MMC3 derivative mappers (4, 12, 14) include `common/mmc3_core_macros.inc`, which defines:

- **Register addresses**: `MMC3_BANK_SELECT`, `MMC3_BANK_DATA`, `MMC3_MIRRORING`, `MMC3_PRG_RAM`, `MMC3_IRQ_LATCH`, etc.
- **Core macros**: `select_prg_bank`, `select_chr_bank`, `set_mirroring`, IRQ control, PRG-RAM enable/disable/write-protect

Each mapper's defs file then includes the shared macros and adds mapper-specific extensions:
- **Mapper 12**: Outer CHR A18 register (`$4132`) via `set_chr_ext` macro
- **Mapper 14**: Supervisor mode register (`$A131`) via `set_mmc3_mode` macro

This approach reduced code by ~373 lines across the three mapper definitions.

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
| Combined ROMs | 22 |
| Single test ROMs | 67 |
| **Total** | **89** |

## Automated Testing Integration

### Rust Test Runner

The emulator's Rust test suite (`src/integration_tests/mapper_tests.rs`) integrates the verification ROMs using the `setup_rom_test!` macro:

```rust
setup_rom_test!(
    test_mv_m004_0_prg_banking,
    "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_prg_banking.nes"
);
```

This generates a `#[test]` function that:

1. Loads the ROM file and creates a `Cartridge`
2. Auto-detects the TV system (NTSC/PAL) from the NES 2.0 header
3. Creates an NES instance and inserts the cartridge
4. Runs the emulator frame-by-frame, polling `$6000` every 256 CPU cycles
5. Returns **Pass** when `$6000 = $00`, **Fail** when `$6000 = $01–$7F` (with console text as the error message), or **Timeout** after 30 seconds of wall-clock time

The test runner first waits for `$6000` to become `$80` (test running), then watches for the final result. This two-phase approach prevents false positives from uninitialized RAM.

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

The pre-built ROM binaries are committed to `bin/` so that CI can run the tests without requiring the cc65 toolchain. The CI workflow runs `cargo test --lib --examples --all-features`, which includes all mapper verification tests.

## Adding a New Mapper

To add tests for a new mapper (e.g. mapper 99):

1. **Create `defs/m099.0_defs.inc`** — Define capability flags, bank sizes, and register macros based on the [NESdev wiki](https://www.nesdev.org/wiki/) specification

2. **Create `configs/m099.0.cfg`** — Define memory layout matching the mapper's banking scheme

3. **Add Makefile targets** — Add `ROM_RULE` entries for each test aspect and a `COMBINED_RULE` entry

4. **Add conditionals to shared test files** (if needed) — For example, if the mapper has a unique mirroring encoding, add a `.elseif MAPPER_NUM = 99` branch to `test_nametable.s`

5. **Add mapper-specific init to `shell.s`** (if needed) — For example, font loading for MMC3 clones, or mode switching for multicart mappers

6. **Add Rust test entries** — Add `setup_rom_test!` invocations to `src/integration_tests/mapper_tests.rs`

7. **Build and commit** — Run `make` to build the ROMs, then commit the binaries to `bin/`

## Covered Mappers

| Mapper | Name | Submappers | Tests |
|--------|------|------------|-------|
| 0 | NROM | 0 | PRG-RAM |
| 1 | MMC1 | 0, 5 | PRG banking, CHR banking, nametable, PRG-RAM |
| 2 | UxROM | 0, 2 | PRG banking, bus conflicts |
| 3 | CNROM | 0, 1 | CHR banking, bus conflicts |
| 4 | MMC3 | 0, 1 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, write-protect |
| 5 | MMC5 | 0 | PRG banking, CHR banking, nametable, IRQ, PRG-RAM, multiplier, write-protect |
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

### Known Emulator Issues

The test ROMs have successfully identified emulator bugs:

1. **Mapper 11 Submapper 1**: Not implemented in emulator (ColorDreamsMapper hardcodes bus conflicts)
   - `m011.1_prg_banking.nes`: ✅ PASS
   - `m011.1_chr_banking.nes`: ❌ FAIL (expected — emulator missing feature)
   - `m011.1_combined.nes`: ❌ FAIL (expected — emulator missing feature)

2. **Mapper 12 Write-Protection Re-Enable**: Fails to restore write capability after protection
   - `m012.0_write_protect.nes`: ❌ FAIL on test 3 (expected — emulator bug)

These failures are documented and expected. The test ROMs are correct per NESdev specifications.

## Acknowledgments

The test harness design — particularly the `$6000` status byte protocol, the console text rendering approach, and the overall init-test-report structure — is inspired by [Blargg's NES test ROMs](https://github.com/christopherpow/nes-test-roms/tree/master/blargg_nes_cpu_test5). Blargg's tests established the convention of using `$6000` as a machine-readable status indicator alongside human-readable text output, which has become a de facto standard in NES emulator testing. This framework adapts those ideas into a parameterized, configuration-driven system specifically targeting mapper verification.
