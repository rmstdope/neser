use super::rom_runner::{
    FAIL_STATUS, PASS_IDLE_PC, PASS_STATUS, RunConfig, RunExitReason, RunOracle, RunResult,
    run_rom_with_oracle,
};
use std::fs;
use std::path::{Path, PathBuf};

const ROM_PASS_FAIL_SUBSET_ROOT: &str = "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/v1";
const ROM_PASS_FAIL_FULL_ROOT: &str =
    "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/full/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BehaviorCategory {
    Timers,
    Ports,
    Dsp,
}

#[derive(Debug, Clone, Copy)]
struct PendingCatalogEntry {
    category: BehaviorCategory,
    name: &'static str,
}

fn pending_catalog() -> Vec<PendingCatalogEntry> {
    vec![
        PendingCatalogEntry {
            category: BehaviorCategory::Timers,
            name: "blargg-spc-timer-baseline",
        },
        PendingCatalogEntry {
            category: BehaviorCategory::Ports,
            name: "blargg-apu-port-handshake",
        },
        PendingCatalogEntry {
            category: BehaviorCategory::Dsp,
            name: "blargg-dsp-register-baseline",
        },
    ]
}

#[derive(Debug, Clone, Copy)]
struct RomPassFailCase {
    name: &'static str,
    oracle: RunOracle,
    max_ticks: u64,
    max_frames: u32,
}

#[derive(Debug, Clone)]
struct RomPassFailOutcome {
    name: &'static str,
    result: RunResult,
}

impl RomPassFailOutcome {
    fn passed(&self) -> bool {
        self.result.passed && self.result.exit_reason == RunExitReason::PassMarker
    }

    fn failed_with_marker(&self) -> bool {
        !self.result.passed && self.result.exit_reason == RunExitReason::FailMarker
    }
}

fn run_case(rom: &[u8], case: &RomPassFailCase) -> RomPassFailOutcome {
    let result = run_rom_with_oracle(
        rom,
        case.name,
        RunConfig::new(case.max_ticks, case.max_frames),
        case.oracle,
    );

    RomPassFailOutcome {
        name: case.name,
        result,
    }
}

fn run_catalog<'a>(catalog: &'a [(RomPassFailCase, &'a [u8])]) -> Vec<RomPassFailOutcome> {
    catalog
        .iter()
        .map(|(case, rom)| run_case(rom, case))
        .collect()
}

fn list_rom_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(root)
        .map_err(|err| format!("failed to read ROM directory {}: {err}", root.display()))?;

    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("sfc") || ext.eq_ignore_ascii_case("smc")
                })
        })
        .collect();
    files.sort();
    Ok(files)
}

fn list_available_rom_files(subset_root: &Path, full_root: &Path) -> Result<Vec<PathBuf>, String> {
    let subset_files = if subset_root.exists() {
        list_rom_files(subset_root)?
    } else {
        Vec::new()
    };

    if !subset_files.is_empty() {
        return Ok(subset_files);
    }

    if full_root.exists() {
        return list_rom_files(full_root);
    }

    Ok(Vec::new())
}

fn default_case_for_rom_path(path: &Path) -> Option<RomPassFailCase> {
    let stem = path.file_stem()?.to_str()?;

    let case_name = match stem {
        "blargg-spc-timer-baseline" => "blargg-spc-timer-baseline",
        "blargg-apu-port-handshake" => "blargg-apu-port-handshake",
        "blargg-dsp-register-baseline" => "blargg-dsp-register-baseline",
        _ => return None,
    };

    Some(RomPassFailCase {
        name: case_name,
        oracle: RunOracle::Marker,
        max_ticks: 2_000_000,
        max_frames: 240,
    })
}

fn run_available_roms(
    subset_root: &Path,
    full_root: &Path,
) -> Result<Vec<RomPassFailOutcome>, String> {
    let files = list_available_rom_files(subset_root, full_root)?;

    let mut outcomes = Vec::new();
    for path in files {
        let Some(case) = default_case_for_rom_path(&path) else {
            continue;
        };

        let rom = fs::read(&path)
            .map_err(|err| format!("failed to read ROM file {}: {err}", path.display()))?;
        outcomes.push(run_case(&rom, &case));
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::super::rom_runner::{FAIL_IDLE_PC, MARKER_ADDR, MARKER_MAGIC};
    use super::*;

    fn pass_bus_byte_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        emit_write_long(&mut rom, &mut cursor, 0x7E_1FE1, PASS_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, PASS_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        rom
    }

    fn fail_marker_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
            emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + offset as u32, byte);
        }
        emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + 4, FAIL_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, FAIL_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        write_idle_loop(&mut rom, FAIL_IDLE_PC);
        rom
    }

    fn pass_marker_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
            emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + offset as u32, byte);
        }
        emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + 4, PASS_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, PASS_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        rom
    }

    fn write_lorom_header(rom: &mut [u8]) {
        let header = 0x7FC0;
        let title = b"SNES ROM SUITE T";
        rom[header..header + 21].fill(b' ');
        rom[header..header + title.len()].copy_from_slice(title);
        rom[header + 0x15] = 0x20;
        rom[header + 0x16] = 0x00;
        rom[header + 0x17] = 0x07;
        rom[header + 0x18] = 0x00;
        rom[header + 0x1C] = 0x34;
        rom[header + 0x1D] = 0x12;
        rom[header + 0x1E] = 0xCB;
        rom[header + 0x1F] = 0xED;
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
    }

    fn emit_write_long(rom: &mut [u8], cursor: &mut usize, addr: u32, value: u8) {
        rom[*cursor] = 0xA9;
        rom[*cursor + 1] = value;
        rom[*cursor + 2] = 0x8F;
        rom[*cursor + 3] = (addr & 0xFF) as u8;
        rom[*cursor + 4] = ((addr >> 8) & 0xFF) as u8;
        rom[*cursor + 5] = ((addr >> 16) & 0xFF) as u8;
        *cursor += 6;
    }

    fn emit_jmp_abs(rom: &mut [u8], cursor: &mut usize, addr: u16) {
        rom[*cursor] = 0x4C;
        rom[*cursor + 1] = (addr & 0x00FF) as u8;
        rom[*cursor + 2] = (addr >> 8) as u8;
        *cursor += 3;
    }

    fn write_idle_loop(rom: &mut [u8], pc: u16) {
        let mut cursor = usize::from(pc - 0x8000);
        emit_jmp_abs(rom, &mut cursor, pc);
    }

    #[test]
    fn given_bus_oracle_case_when_run_case_then_reports_pass() {
        let case = RomPassFailCase {
            name: "fixture-bus-pass",
            oracle: RunOracle::BusByte {
                addr: 0x7E_1FE1,
                pass_value: PASS_STATUS,
                fail_value: FAIL_STATUS,
            },
            max_ticks: 10_000,
            max_frames: 2,
        };

        let outcome = run_case(&pass_bus_byte_rom(), &case);

        assert!(
            outcome.passed(),
            "expected pass outcome for {}",
            outcome.name
        );
    }

    #[test]
    fn given_marker_oracle_fail_rom_when_run_case_then_reports_marker_fail() {
        let case = RomPassFailCase {
            name: "fixture-marker-fail",
            oracle: RunOracle::Marker,
            max_ticks: 10_000,
            max_frames: 2,
        };

        let outcome = run_case(&fail_marker_rom(), &case);

        assert!(
            outcome.failed_with_marker(),
            "expected marker fail outcome for {}",
            outcome.name
        );
    }

    #[test]
    fn given_mixed_catalog_when_run_catalog_then_reports_each_case_outcome() {
        let pass_rom = pass_bus_byte_rom();
        let fail_rom = fail_marker_rom();
        let catalog: Vec<(RomPassFailCase, &[u8])> = vec![
            (
                RomPassFailCase {
                    name: "catalog-bus-pass",
                    oracle: RunOracle::BusByte {
                        addr: 0x7E_1FE1,
                        pass_value: PASS_STATUS,
                        fail_value: FAIL_STATUS,
                    },
                    max_ticks: 10_000,
                    max_frames: 2,
                },
                pass_rom.as_slice(),
            ),
            (
                RomPassFailCase {
                    name: "catalog-marker-fail",
                    oracle: RunOracle::Marker,
                    max_ticks: 10_000,
                    max_frames: 2,
                },
                fail_rom.as_slice(),
            ),
        ];

        let outcomes = run_catalog(&catalog);

        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].passed());
        assert!(outcomes[1].failed_with_marker());
    }

    #[test]
    fn pending_catalog_covers_required_behavior_categories() {
        let entries = pending_catalog();
        let categories: std::collections::BTreeSet<BehaviorCategory> =
            entries.iter().map(|entry| entry.category).collect();

        for required in [
            BehaviorCategory::Timers,
            BehaviorCategory::Ports,
            BehaviorCategory::Dsp,
        ] {
            assert!(
                categories.contains(&required),
                "pending catalog missing required category {required:?}"
            );
        }

        for entry in &entries {
            assert!(
                !entry.name.is_empty(),
                "pending catalog entry must have a non-empty name"
            );
        }
    }

    #[test]
    fn pending_catalog_entries_are_documented_in_manifest_notes() {
        let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("roms/snes/automated_tests/manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));

        for entry in pending_catalog() {
            assert!(
                manifest_text.contains(entry.name),
                "expected manifest notes to include pending catalog entry '{}'",
                entry.name
            );
        }
    }

    #[test]
    fn given_full_root_contains_baseline_rom_when_running_available_roms_then_it_executes() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let full_root = temp.path().join("full");
        fs::create_dir_all(&full_root).expect("create full root");

        let rom_path = full_root.join("blargg-spc-timer-baseline.sfc");
        fs::write(&rom_path, pass_marker_rom()).expect("write baseline ROM");

        let outcomes =
            run_available_roms(temp.path().join("subset").as_path(), full_root.as_path())
                .expect("run available ROMs");

        assert_eq!(outcomes.len(), 1);
        assert!(
            outcomes[0].passed(),
            "expected discovered baseline ROM to pass"
        );
    }

    #[test]
    fn runs_available_spc_apu_rom_pass_fail_cases() {
        let subset_root = Path::new(ROM_PASS_FAIL_SUBSET_ROOT);
        let full_root = Path::new(ROM_PASS_FAIL_FULL_ROOT);
        if !subset_root.exists() && !full_root.exists() {
            return;
        }

        let outcomes = run_available_roms(subset_root, full_root).expect("run available ROMs");

        for outcome in outcomes {
            assert!(
                outcome.passed(),
                "expected {} to pass with marker oracle: {:?}",
                outcome.name,
                outcome.result
            );
        }
    }
}
