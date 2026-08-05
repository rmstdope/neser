use crate::snes::bus::SnesBus;
use crate::snes::cpu::Cpu;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const PROCESSOR_TESTS_ROOT: &str = "roms/snes/automated_tests/processor_tests/65816/v1";
const PROCESSOR_TESTS_FULL_ROOT: &str = "roms/snes/automated_tests/processor_tests/65816/full/v1";

/// Opcodes whose vectors are checked cycle-by-cycle against the bus activity the vector
/// records, not merely for total cycle count and final RAM.
///
/// The vectors have always carried per-cycle bus data; asserting only `cycles.len()` is why a
/// family of internal-cycle placement bugs (#3050, #3068) could sit unnoticed behind a green
/// suite. This list is the migration front: it is seeded with one opcode per addressing mode
/// and read-modify-write form touched by #3068, and should grow until it covers every opcode.
/// Opcodes outside it keep the length + final-state checks only, so unrelated instruction-set
/// divergences do not have to be resolved before this net can be extended.
const CYCLE_EXACT_OPCODES: &[u8] = &[
    // Direct-page addressing modes -- one representative opcode each.
    0xA5, // LDA dp
    0xB5, // LDA dp,X
    0xB6, // LDX dp,Y
    0xA1, // LDA (dp,X)
    0xB1, // LDA (dp),Y
    0xB2, // LDA (dp)
    0xA7, // LDA [dp]
    0xB7, // LDA [dp],Y
    0xD4, // PEI
    // Read-modify-write forms -- one representative addressing mode each.
    0x06, // ASL dp
    0x16, // ASL dp,X
    0x0E, // ASL abs
    0x1E, // ASL abs,X
    0x04, // TSB dp
];

/// Whether this vector's bus cycles should be compared one by one.
fn is_cycle_exact(opcode: u8) -> bool {
    CYCLE_EXACT_OPCODES.contains(&opcode)
}

/// Full-corpus ProcessorTests vector names whose expectations are known-wrong for the SNES
/// 5A22. The bulk runner (`run_vectors_from_file`) skips exactly these names; feeding one of
/// them to `run_vector_case` directly still reports the divergence.
///
/// Both groups disagree with NESER about where a direct-page indirect pointer's HIGH byte is
/// fetched in emulation mode:
///
/// * The 28 `a1 e` entries -- LDA (dp,X) with E=1, DL != 0 and the pointer straddling a page
///   boundary: the 5A22 wraps the +1 for the pointer high byte within that page (an
///   undocumented hardware quirk); the vectors carry into the next page. Evidence for
///   wrapping: gilyon cputest.sfc tests 02c9-02cc pass on real hardware and its README
///   documents the exact rule, noting it applies only to this addressing mode; Mesen2
///   implements it as `GetDirectAddressIndirectWordWithPageWrap`.
/// * `d4 e 232` -- PEI with E=1, DL == 0 and operand $FF: PEI is a "new" 65816 instruction
///   whose pointer fetch never wraps, even under the E=1/DL==0 wrap rule that governs the old
///   (dp) modes; the vector wraps. Evidence for carrying: the WDC datasheet ("except for
///   [Direct] and [Direct],Y addressing modes and the PEI instruction which will increment
///   from 0000FE or 0000FF into the Stack area"), Bruce Clark's 65C816 tutorial section 5.11,
///   gilyon cputest.sfc test 03c4, and Mesen2.
///
/// The ProcessorTests corpus is generated from an emulator model, not captured from hardware,
/// and has a history of exactly this class of emulation-mode wrap bug (upstream issue #1
/// regenerated the [dp]/[dp],Y vectors; issues #3, #6 and #8 were still open at the pinned
/// revision). Full-corpus tally: 28/10000 in a1.e.json and 1/10000 in d4.e.json diverge,
/// while a1.n.json and d4.n.json are 0/10000. Documented as intentional in #3135.
const KNOWN_DIVERGENT_VECTORS: &[&str] = &[
    // LDA (dp,X), E=1, DL != 0: the vector carries the pointer high-byte fetch into the next
    // page where the 5A22 wraps within it.
    "a1 e 847",
    "a1 e 1067",
    "a1 e 1516",
    "a1 e 1587",
    "a1 e 1708",
    "a1 e 2020",
    "a1 e 2050",
    "a1 e 2225",
    "a1 e 2640",
    "a1 e 2918",
    "a1 e 3217",
    "a1 e 4107",
    "a1 e 4328",
    "a1 e 4506",
    "a1 e 4868",
    "a1 e 5038",
    "a1 e 5114",
    "a1 e 5220",
    "a1 e 5941",
    "a1 e 6032",
    "a1 e 6120",
    "a1 e 6469",
    "a1 e 6486",
    "a1 e 7148",
    "a1 e 7805",
    "a1 e 8287",
    "a1 e 9387",
    "a1 e 9677",
    // PEI, E=1, DL == 0: the vector wraps the pointer high-byte fetch within the direct page
    // where the 5A22 carries into the next page.
    "d4 e 232",
];

/// One CPU bus cycle as observed on the bus: either an internal (no-access) cycle, or a read
/// or write of one byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedCycle {
    Internal,
    Read(u32, u8),
    Write(u32, u8),
}

impl fmt::Display for ObservedCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObservedCycle::Internal => write!(f, "internal"),
            ObservedCycle::Read(addr, value) => write!(f, "read  ${addr:06X} = ${value:02X}"),
            ObservedCycle::Write(addr, value) => write!(f, "write ${addr:06X} = ${value:02X}"),
        }
    }
}

/// Decode one vector cycle from its 8-character signal string.
///
/// Positions are `vda vpa vpb rwb e m x mlb`. A cycle that drives neither VDA nor VPA is an
/// internal cycle -- its recorded address is whatever was last left on the bus, which the CPU
/// does not model, so only the *kind* is compared for those. The one exception is the
/// emulation-mode read-modify-write dummy write, which asserts RWB=w with VDA low; RWB is
/// therefore tested before the address-valid flags.
fn decode_vector_cycle(cycle: &VectorCycle) -> ObservedCycle {
    let signals = cycle.signals.as_bytes();
    let writing = signals[3] == b'w';
    let address_valid = signals[0] == b'd' || signals[1] == b'p';

    match (writing, cycle.address, cycle.value) {
        (true, Some(addr), Some(value)) => ObservedCycle::Write(addr & 0xFF_FFFF, value),
        (false, Some(addr), Some(value)) if address_valid => {
            ObservedCycle::Read(addr & 0xFF_FFFF, value)
        }
        _ => ObservedCycle::Internal,
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VectorState {
    pc: u16,
    s: u16,
    p: u8,
    a: u16,
    x: u16,
    y: u16,
    dbr: u8,
    d: u16,
    pbr: u8,
    e: u8,
    ram: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VectorCycle {
    address: Option<u32>,
    value: Option<u8>,
    signals: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawProcessorTestVector {
    name: String,
    initial: VectorState,
    #[serde(rename = "final")]
    final_state: VectorState,
    cycles: Vec<(Option<u32>, Option<u8>, String)>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ProcessorTestVector {
    name: String,
    initial: VectorState,
    #[serde(rename = "final")]
    final_state: VectorState,
    cycles: Vec<VectorCycle>,
}

#[derive(Debug)]
struct VectorFailure {
    details: String,
}

impl fmt::Display for VectorFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for VectorFailure {}

fn load_vectors_from_file(path: &Path) -> Result<Vec<ProcessorTestVector>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read vector file {}: {err}", path.display()))?;

    let raw_vectors: Vec<RawProcessorTestVector> = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse vector JSON {}: {err}", path.display()))?;

    let vectors = raw_vectors
        .into_iter()
        .map(|raw_vector| ProcessorTestVector {
            name: raw_vector.name,
            initial: raw_vector.initial,
            final_state: raw_vector.final_state,
            cycles: raw_vector
                .cycles
                .into_iter()
                .map(|(address, value, signals)| VectorCycle {
                    address,
                    value,
                    signals,
                })
                .collect(),
        })
        .collect();

    Ok(vectors)
}

#[derive(Clone)]
struct HarnessBusShared {
    mem: Rc<RefCell<HashMap<u32, u8>>>,
    /// One entry per CPU bus cycle, in order. See [`HarnessBusShared::begin_cycle`].
    cycles: Rc<RefCell<Vec<ObservedCycle>>>,
}

impl HarnessBusShared {
    fn new() -> Self {
        Self {
            mem: Rc::new(RefCell::new(HashMap::new())),
            cycles: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn read(&self, addr: u32) -> u8 {
        let addr = addr & 0xFF_FFFF;
        *self.mem.borrow().get(&addr).unwrap_or(&0)
    }

    fn write(&self, addr: u32, value: u8) {
        let addr = addr & 0xFF_FFFF;
        if value == 0 {
            self.mem.borrow_mut().remove(&addr);
        } else {
            self.mem.borrow_mut().insert(addr, value);
        }
    }

    /// Open a new CPU bus cycle. The CPU publishes its speed exactly once at the start of
    /// each of its three cycle-boundary functions (`tick_read`, `tick_write`,
    /// `tick_internal_cycle`), so this fires once per CPU cycle and nowhere else. A cycle
    /// starts out classified as internal and is reclassified if an access lands inside it.
    fn begin_cycle(&self) {
        self.cycles.borrow_mut().push(ObservedCycle::Internal);
    }

    fn record_access(&self, cycle: ObservedCycle) {
        if let Some(last) = self.cycles.borrow_mut().last_mut() {
            *last = cycle;
        }
    }

    fn recorded_cycles(&self) -> Vec<ObservedCycle> {
        self.cycles.borrow().clone()
    }
}

struct HarnessBus {
    shared: HarnessBusShared,
}

impl HarnessBus {
    fn new(shared: HarnessBusShared) -> Self {
        Self { shared }
    }
}

impl SnesBus for HarnessBus {
    fn read(&self, addr: u32) -> u8 {
        let value = self.shared.read(addr);
        self.shared
            .record_access(ObservedCycle::Read(addr & 0xFF_FFFF, value));
        value
    }

    /// Debugger reads are not bus cycles -- they must never disturb the recording.
    fn read_for_debugger(&self, addr: u32) -> u8 {
        self.shared.read(addr)
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.shared.write(addr, value);
        self.shared
            .record_access(ObservedCycle::Write(addr & 0xFF_FFFF, value));
    }

    fn tick(&mut self) {}

    fn set_cpu_speed(&mut self, _speed: u8) {
        self.shared.begin_cycle();
    }
}

/// Compare the recorded bus cycles against the vector's, returning a rendered side-by-side
/// diff on the first divergence.
///
/// Internal cycles compare by kind only: the vector records whatever address was last left on
/// the bus during an internal cycle, which is a detail of the real chip's address latches that
/// the CPU model does not reproduce.
fn compare_bus_cycles(
    name: &str,
    expected: &[ObservedCycle],
    actual: &[ObservedCycle],
) -> Option<String> {
    let matches = expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| match (expected, actual) {
                (ObservedCycle::Internal, ObservedCycle::Internal) => true,
                _ => expected == actual,
            });
    if matches {
        return None;
    }

    let mut report = format!(
        "{name}: bus cycle mismatch\n  {:<28}  {}\n",
        "expected", "got"
    );
    for index in 0..expected.len().max(actual.len()) {
        let render = |cycle: Option<&ObservedCycle>| {
            cycle.map_or_else(|| "-".to_string(), ObservedCycle::to_string)
        };
        let left = render(expected.get(index));
        let right = render(actual.get(index));
        let differs = match (expected.get(index), actual.get(index)) {
            (Some(ObservedCycle::Internal), Some(ObservedCycle::Internal)) => false,
            (left, right) => left != right,
        };
        report.push_str(&format!(
            "  {left:<28}  {right}{}\n",
            if differs { "   <--" } else { "" }
        ));
    }
    Some(report)
}

fn run_vector_case(vector: &ProcessorTestVector) -> Result<(), VectorFailure> {
    for cycle in &vector.cycles {
        if cycle.signals.len() != 8 {
            return Err(VectorFailure {
                details: format!(
                    "{}: malformed cycle signal string '{}'",
                    vector.name, cycle.signals
                ),
            });
        }

        if cycle.address.is_none() && cycle.value.is_some() {
            return Err(VectorFailure {
                details: format!(
                    "{}: cycle has value but no address (signals='{}')",
                    vector.name, cycle.signals
                ),
            });
        }
    }

    let shared = HarnessBusShared::new();
    for ram_entry in &vector.initial.ram {
        let addr = ram_entry[0];
        let value = ram_entry[1] as u8;
        shared.write(addr, value);
    }

    let mut cpu = Cpu::new(HarnessBus::new(shared.clone()));
    cpu.load_state_for_processor_test(
        vector.initial.a,
        vector.initial.x,
        vector.initial.y,
        vector.initial.d,
        vector.initial.dbr,
        vector.initial.pbr,
        vector.initial.s,
        vector.initial.pc,
        vector.initial.p,
        vector.initial.e != 0,
    );

    let initial_pc = cpu.read_pc();
    let initial_opcode_addr = ((vector.initial.pbr as u32) << 16) | initial_pc as u32;
    let initial_opcode = shared.read(initial_opcode_addr);
    let repeat_instruction = matches!(initial_opcode, 0x44 | 0x54);

    let mut actual_cycles = 0usize;
    let expected_steps = if repeat_instruction {
        match vector.cycles.len() % 7 {
            0 => (vector.cycles.len() / 7).max(1),
            2 => ((vector.cycles.len() - 2) / 7).max(1),
            rem => {
                return Err(VectorFailure {
                    details: format!(
                        "{}: unsupported repeat-instruction cycle shape (cycles={}, rem={rem}, opcode=${:02X})",
                        vector.name,
                        vector.cycles.len(),
                        initial_opcode
                    ),
                });
            }
        }
    } else {
        1
    };

    for _ in 0..expected_steps {
        let step_cycles = cpu.step() as usize;
        actual_cycles += step_cycles;
    }

    if !repeat_instruction {
        let expected_cycles = vector.cycles.len();
        if actual_cycles != expected_cycles {
            return Err(VectorFailure {
                details: format!(
                    "{}: cycle count mismatch (expected {expected_cycles}, got {actual_cycles})",
                    vector.name
                ),
            });
        }

        if is_cycle_exact(initial_opcode) {
            let expected: Vec<ObservedCycle> =
                vector.cycles.iter().map(decode_vector_cycle).collect();
            let actual = shared.recorded_cycles();
            if let Some(details) = compare_bus_cycles(&vector.name, &expected, &actual) {
                return Err(VectorFailure { details });
            }
        }
    }

    let actual_pc = cpu.read_pc();
    let actual_s = cpu.read_s();
    let actual_p = cpu.read_p();
    let actual_a = cpu.read_a();
    let actual_x = cpu.read_x();
    let actual_y = cpu.read_y();
    let actual_dbr = cpu.read_dbr();
    let actual_d = cpu.read_d();
    let actual_pbr = cpu.read_pbr();
    let actual_e = cpu.emulation_mode();
    let expected_e = vector.final_state.e != 0;

    if actual_pc != vector.final_state.pc
        || actual_s != vector.final_state.s
        || actual_p != vector.final_state.p
        || actual_a != vector.final_state.a
        || actual_x != vector.final_state.x
        || actual_y != vector.final_state.y
        || actual_dbr != vector.final_state.dbr
        || actual_d != vector.final_state.d
        || actual_pbr != vector.final_state.pbr
        || actual_e != expected_e
    {
        return Err(VectorFailure {
            details: format!(
                "{}: CPU final state mismatch\n  PC: expected ${:04X}, got ${:04X}\n  S: expected ${:04X}, got ${:04X}\n  P: expected ${:02X}, got ${:02X}\n  A: expected ${:04X}, got ${:04X}\n  X: expected ${:04X}, got ${:04X}\n  Y: expected ${:04X}, got ${:04X}\n  DBR: expected ${:02X}, got ${:02X}\n  D: expected ${:04X}, got ${:04X}\n  PBR: expected ${:02X}, got ${:02X}\n  E: expected {}, got {}",
                vector.name,
                vector.final_state.pc,
                actual_pc,
                vector.final_state.s,
                actual_s,
                vector.final_state.p,
                actual_p,
                vector.final_state.a,
                actual_a,
                vector.final_state.x,
                actual_x,
                vector.final_state.y,
                actual_y,
                vector.final_state.dbr,
                actual_dbr,
                vector.final_state.d,
                actual_d,
                vector.final_state.pbr,
                actual_pbr,
                expected_e,
                actual_e
            ),
        });
    }

    for [addr, expected] in &vector.final_state.ram {
        let actual = shared.read(*addr);
        if actual != *expected as u8 {
            return Err(VectorFailure {
                details: format!(
                    "{}: RAM mismatch at ${addr:06X} (expected {expected:#04X}, got {actual:#04X})",
                    vector.name
                ),
            });
        }
    }

    Ok(())
}

fn run_vectors_from_file(path: &Path) -> Result<(), VectorFailure> {
    let vectors = load_vectors_from_file(path).map_err(|details| VectorFailure { details })?;
    for vector in &vectors {
        if KNOWN_DIVERGENT_VECTORS.contains(&vector.name.as_str()) {
            continue;
        }
        run_vector_case(vector)?;
    }
    Ok(())
}

fn list_vector_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(root)
        .map_err(|err| format!("failed to read vector directory {}: {err}", root.display()))?;

    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();

    files.sort();
    Ok(files)
}

fn list_available_vector_files(
    subset_root: &Path,
    full_root: &Path,
) -> Result<Vec<PathBuf>, String> {
    let subset_files = list_vector_files(subset_root)?;
    let full_files = if full_root.exists() {
        list_vector_files(full_root)?
    } else {
        Vec::new()
    };

    let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
    for file in subset_files {
        let Some(name) = file.file_name() else {
            continue;
        };
        by_name.insert(name.to_string_lossy().to_string(), file);
    }

    for file in full_files {
        let Some(name) = file.file_name() else {
            continue;
        };
        by_name.insert(name.to_string_lossy().to_string(), file);
    }

    Ok(by_name.into_values().collect())
}

fn run_vectors_from_directory(root: &Path) -> Result<(), VectorFailure> {
    run_vectors_from_roots(root, Path::new(PROCESSOR_TESTS_FULL_ROOT))
}

fn run_vectors_from_roots(subset_root: &Path, full_root: &Path) -> Result<(), VectorFailure> {
    let files = list_available_vector_files(subset_root, full_root)
        .map_err(|details| VectorFailure { details })?;
    if files.is_empty() {
        return Err(VectorFailure {
            details: format!("no vector files found in {}", subset_root.display()),
        });
    }

    let mut subset_by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
    for file in list_vector_files(subset_root).map_err(|details| VectorFailure { details })? {
        let Some(name) = file.file_name() else {
            continue;
        };
        subset_by_name.insert(name.to_string_lossy().to_string(), file);
    }

    let mut ran_any = false;
    for file in files {
        let is_full_file = file.starts_with(full_root);
        let file_name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string());

        let subset_fallback = file_name
            .as_ref()
            .and_then(|name| subset_by_name.get(name))
            .cloned();

        if is_full_file && !file.exists() {
            if let Some(subset_file) = subset_fallback.as_ref() {
                run_vectors_from_file(subset_file)?;
                ran_any = true;
            }
            continue;
        }

        match run_vectors_from_file(&file) {
            Ok(()) => {
                ran_any = true;
            }
            Err(err) => {
                if is_full_file && is_transient_full_vector_error(&err) {
                    if let Some(subset_file) = subset_fallback.as_ref() {
                        run_vectors_from_file(subset_file)?;
                        ran_any = true;
                    }
                    continue;
                }
                return Err(err);
            }
        }
    }

    if !ran_any {
        return Err(VectorFailure {
            details: format!(
                "no runnable vector files found in {}",
                subset_root.display()
            ),
        });
    }

    Ok(())
}

fn is_transient_full_vector_error(err: &VectorFailure) -> bool {
    err.details.starts_with("failed to read vector file")
        || err.details.starts_with("failed to parse vector JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_sample_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "ea native no-op",
    "initial": {
      "pc": 0,
      "s": 8191,
      "p": 0,
      "a": 4660,
      "x": 43981,
      "y": 3855,
      "dbr": 18,
      "d": 22136,
      "pbr": 2,
      "e": 0,
      "ram": [[131072, 234]]
    },
    "final": {
      "pc": 1,
      "s": 8191,
      "p": 0,
      "a": 4660,
      "x": 43981,
      "y": 3855,
      "dbr": 18,
      "d": 22136,
      "pbr": 2,
      "e": 0,
      "ram": [[131072, 234]]
    },
    "cycles": [
            [131072, 234, "dp-r----"],
      [null, null, "-----mx-"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample vector JSON");
    }

    #[test]
    fn loads_65816_vector_schema_from_json_file() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("00.n.json");
        write_sample_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        assert_eq!(vectors.len(), 1);

        let vector = &vectors[0];
        assert_eq!(vector.name, "ea native no-op");
        assert_eq!(vector.initial.e, 0);
        assert_eq!(vector.initial.pbr, 2);
        assert_eq!(vector.final_state.pc, 1);
        assert_eq!(vector.cycles.len(), 2);
    }

    #[test]
    fn executes_vector_with_native_mode_state_and_cycle_count() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("00.n.json");
        write_sample_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);

        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn runs_all_available_65816_vectors() {
        let root = Path::new(PROCESSOR_TESTS_ROOT);
        let result = run_vectors_from_directory(root);
        assert!(
            result.is_ok(),
            "available 65816 vectors should pass: {result:?}"
        );
    }

    #[test]
    fn list_vector_files_only_returns_json_files() {
        let temp = tempfile::tempdir().expect("create temp dir");
        fs::write(temp.path().join("ea.n.json"), "[]").expect("write ea.n.json");
        fs::write(temp.path().join("00.e.json"), "[]").expect("write 00.e.json");
        fs::write(temp.path().join("notes.txt"), "not a vector").expect("write notes.txt");

        let files = list_vector_files(temp.path()).expect("list vector files");
        let names: Vec<String> = files
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["00.e.json", "ea.n.json"]);
    }

    #[test]
    fn run_vectors_from_directory_fails_when_empty() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let full = temp.path().join("full");
        fs::create_dir_all(&full).expect("create empty full dir");
        let result = run_vectors_from_roots(temp.path(), &full);
        assert!(result.is_err(), "empty directory should fail");
    }

    #[test]
    fn list_available_vector_files_prefers_full_vectors_when_present() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let subset = temp.path().join("subset");
        let full = temp.path().join("full");
        fs::create_dir_all(&subset).expect("create subset dir");
        fs::create_dir_all(&full).expect("create full dir");

        fs::write(subset.join("00.e.json"), "subset").expect("write subset file");
        fs::write(subset.join("ea.n.json"), "subset").expect("write subset file");
        fs::write(full.join("00.e.json"), "full").expect("write full file");
        fs::write(full.join("01.e.json"), "full").expect("write full file");

        let files =
            list_available_vector_files(&subset, &full).expect("list available vector files");
        let names: Vec<String> = files
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["00.e.json", "01.e.json", "ea.n.json"]);
        assert_eq!(
            fs::read_to_string(&files[0]).expect("read merged 00.e.json"),
            "full"
        );
    }

    #[test]
    #[cfg(unix)]
    fn run_vectors_falls_back_to_subset_when_full_vector_disappears() {
        use std::os::unix::fs as unix_fs;

        let temp = tempfile::tempdir().expect("create temp dir");
        let subset = temp.path().join("subset");
        let full = temp.path().join("full");
        fs::create_dir_all(&subset).expect("create subset dir");
        fs::create_dir_all(&full).expect("create full dir");

        let name = "16.e.json";
        write_sample_vector(&subset.join(name));
        unix_fs::symlink(temp.path().join("missing.json"), full.join(name))
            .expect("create dangling symlink");

        let result = run_vectors_from_roots(&subset, &full);
        assert!(
            result.is_ok(),
            "expected subset fallback when full file is missing: {result:?}"
        );
    }

    #[test]
    fn run_vectors_skips_missing_full_vector_when_no_subset_fallback_exists() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let subset = temp.path().join("subset");
        let full = temp.path().join("full");
        fs::create_dir_all(&subset).expect("create subset dir");
        fs::create_dir_all(&full).expect("create full dir");

        write_sample_vector(&subset.join("00.n.json"));
        fs::write(full.join("16.e.json"), "[]").expect("write full vector file");
        fs::remove_file(full.join("16.e.json")).expect("remove full vector file");

        let result = run_vectors_from_roots(&subset, &full);
        assert!(
            result.is_ok(),
            "expected missing full file without subset fallback to be skipped: {result:?}"
        );
    }

    #[test]
    fn run_vectors_falls_back_to_subset_when_full_vector_is_unparseable() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let subset = temp.path().join("subset");
        let full = temp.path().join("full");
        fs::create_dir_all(&subset).expect("create subset dir");
        fs::create_dir_all(&full).expect("create full dir");

        let name = "44.e.json";
        write_sample_vector(&subset.join(name));
        fs::write(full.join(name), "").expect("write empty full vector file");

        let result = run_vectors_from_roots(&subset, &full);
        assert!(
            result.is_ok(),
            "expected subset fallback when full vector is unparseable: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_27_e_3341() {
        let vector = ProcessorTestVector {
            name: "27 e 3341".to_string(),
            initial: VectorState {
                pc: 0x9E72,
                s: 0x88BA,
                p: 0xFC,
                a: 0xBF88,
                x: 0x00D9,
                y: 0x0073,
                dbr: 0xEE,
                d: 0x2F00,
                pbr: 0xF1,
                e: 1,
                ram: vec![
                    [0x003000, 0x5E],
                    [0x002FFF, 0x6E],
                    [0x002FFE, 0x9C],
                    [0x5E6E9C, 0x2B],
                    [0xF19E73, 0xFE],
                    [0xF19E72, 0x27],
                ],
            },
            final_state: VectorState {
                pc: 0x9E74,
                s: 0x01BA,
                p: 0x7C,
                a: 0xBF08,
                x: 0x00D9,
                y: 0x0073,
                dbr: 0xEE,
                d: 0x2F00,
                pbr: 0xF1,
                e: 1,
                ram: vec![
                    [0x5E6E9C, 0x2B],
                    [0x003000, 0x5E],
                    [0x002FFF, 0x6E],
                    [0x002FFE, 0x9C],
                    [0xF19E73, 0xFE],
                    [0xF19E72, 0x27],
                ],
            },
            cycles: vec![
                VectorCycle {
                    address: Some(0xF19E72),
                    value: Some(0x27),
                    signals: "dp-remx-".to_string(),
                },
                VectorCycle {
                    address: Some(0xF19E73),
                    value: Some(0xFE),
                    signals: "-p-remx-".to_string(),
                },
                VectorCycle {
                    address: Some(0x002FFE),
                    value: Some(0x9C),
                    signals: "d--remx-".to_string(),
                },
                VectorCycle {
                    address: Some(0x002FFF),
                    value: Some(0x6E),
                    signals: "d--remx-".to_string(),
                },
                VectorCycle {
                    address: Some(0x003000),
                    value: Some(0x5E),
                    signals: "d--remx-".to_string(),
                },
                VectorCycle {
                    address: Some(0x5E6E9C),
                    value: Some(0x2B),
                    signals: "d--remx-".to_string(),
                },
            ],
        };

        let result = run_vector_case(&vector);
        assert!(result.is_ok(), "expected vector to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_44_e_1_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("44.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 44.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "44 e 1")
            .expect("find vector 44 e 1");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector 44 e 1 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_44_e_2_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("44.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 44.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "44 e 2")
            .expect("find vector 44 e 2");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector 44 e 2 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_44_e_2520_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("44.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 44.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "44 e 2520")
            .expect("find vector 44 e 2520");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 44 e 2520 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_61_n_3_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 61.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "61 n 3")
            .expect("find vector 61 n 3");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector 61 n 3 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_61_n_11_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 61.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "61 n 11")
            .expect("find vector 61 n 11");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 61 n 11 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_61_n_404_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 61.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "61 n 404")
            .expect("find vector 61 n 404");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 61 n 404 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_61_n_583_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 61.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "61 n 583")
            .expect("find vector 61 n 583");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 61 n 583 to pass: {result:?}"
        );
    }

    #[test]
    fn run_all_vectors_in_61_n_file_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let result = run_vectors_from_file(&full_file);
        assert!(
            result.is_ok(),
            "expected all vectors in 61.n.json to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_61_n_1129_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("61.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 61.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "61 n 1129")
            .expect("find vector 61 n 1129");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 61 n 1129 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_6b_e_104_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("6b.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 6b.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "6b e 104")
            .expect("find vector 6b e 104");

        assert!(
            vector.initial.ram.contains(&[512, 58])
                && vector.initial.ram.contains(&[513, 254])
                && vector.initial.ram.contains(&[514, 208]),
            "6b e 104 should include stack bytes at $0100-$0102"
        );

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 6b e 104 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_7c_e_2841_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("7c.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 7c.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "7c e 2841")
            .expect("find vector 7c e 2841");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 7c e 2841 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_c4_n_1616_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("c4.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load c4.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "c4 n 1616")
            .expect("find vector c4 n 1616");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector c4 n 1616 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_1d_n_3218_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("1d.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load 1d.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "1d n 3218")
            .expect("find vector 1d n 3218");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector 1d n 3218 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_cb_e_1_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("cb.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load cb.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "cb e 1")
            .expect("find vector cb e 1");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector cb e 1 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_reports_divergence_for_d4_e_232_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("d4.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load d4.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "d4 e 232")
            .expect("find vector d4 e 232");

        let result = run_vector_case(vector);
        assert!(
            result.is_err(),
            "expected vector d4 e 232 to diverge: it wraps PEI's pointer high-byte fetch, \
             which the 5A22 does not do (see KNOWN_DIVERGENT_VECTORS)"
        );
    }

    #[test]
    fn run_vector_case_reports_divergence_for_a1_e_847_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("a1.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load a1.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "a1 e 847")
            .expect("find vector a1 e 847");

        let result = run_vector_case(vector);
        assert!(
            result.is_err(),
            "expected vector a1 e 847 to diverge: it carries the (dp,X) pointer high-byte \
             fetch where the 5A22 wraps within the page (see KNOWN_DIVERGENT_VECTORS)"
        );
    }

    #[test]
    fn known_divergent_vectors_all_diverge_when_full_vectors_available() {
        let full_root = Path::new(PROCESSOR_TESTS_FULL_ROOT);
        if !full_root.join("a1.e.json").exists() || !full_root.join("d4.e.json").exists() {
            return;
        }

        let mut checked = 0;
        for file in ["a1.e.json", "d4.e.json"] {
            let vectors = load_vectors_from_file(&full_root.join(file)).expect("load full vectors");
            for vector in &vectors {
                if !KNOWN_DIVERGENT_VECTORS.contains(&vector.name.as_str()) {
                    continue;
                }
                checked += 1;
                let result = run_vector_case(vector);
                assert!(
                    result.is_err(),
                    "expected {} to diverge from the hardware-backed wrap rule, but it passed",
                    vector.name
                );
            }
        }
        assert_eq!(
            checked,
            KNOWN_DIVERGENT_VECTORS.len(),
            "every KNOWN_DIVERGENT_VECTORS entry should exist in the full corpus"
        );
    }

    #[test]
    fn emulation_mode_a1_and_d4_vectors_are_cycle_exact_checked() {
        // The divergent vectors are excluded by name via KNOWN_DIVERGENT_VECTORS, not by
        // downgrading every emulation-mode a1/d4 vector to state-only checks.
        assert!(is_cycle_exact(0xA1));
        assert!(is_cycle_exact(0xD4));
    }

    #[test]
    fn run_vectors_from_file_skips_known_divergent_vectors_when_full_vectors_available() {
        for file in ["a1.e.json", "d4.e.json"] {
            let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join(file);
            if !full_file.exists() {
                return;
            }

            let result = run_vectors_from_file(&full_file);
            assert!(
                result.is_ok(),
                "expected {file} to pass with known-divergent vectors skipped: {result:?}"
            );
        }
    }

    #[test]
    fn run_vector_case_matches_db_e_1_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("db.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load db.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "db e 1")
            .expect("find vector db e 1");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector db e 1 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_e1_e_8669_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("e1.e.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load e1.e.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "e1 e 8669")
            .expect("find vector e1 e 8669");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector e1 e 8669 to pass: {result:?}"
        );
    }

    #[test]
    fn run_vector_case_matches_fb_n_4_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("fb.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load fb.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "fb n 4")
            .expect("find vector fb n 4");

        let result = run_vector_case(vector);
        assert!(result.is_ok(), "expected vector fb n 4 to pass: {result:?}");
    }

    #[test]
    fn run_vector_case_matches_fc_n_7140_when_full_vectors_available() {
        let full_file = Path::new(PROCESSOR_TESTS_FULL_ROOT).join("fc.n.json");
        if !full_file.exists() {
            return;
        }

        let vectors = load_vectors_from_file(&full_file).expect("load fc.n.json vectors");
        let vector = vectors
            .iter()
            .find(|vector| vector.name == "fc n 7140")
            .expect("find vector fc n 7140");

        let result = run_vector_case(vector);
        assert!(
            result.is_ok(),
            "expected vector fc n 7140 to pass: {result:?}"
        );
    }
}
