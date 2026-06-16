use crate::snes::bus::SnesBus;
use crate::snes::cpu::Cpu;
use std::cell::RefCell;
use std::fmt;
use std::fs;
use std::path::Path;
use std::rc::Rc;

const PROCESSOR_TESTS_ROOT: &str = "roms/snes/automated_tests/processor_tests/65816/v1";

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
    mem: Rc<RefCell<Vec<u8>>>,
}

impl HarnessBusShared {
    fn new() -> Self {
        Self {
            mem: Rc::new(RefCell::new(vec![0; 0x100_0000])),
        }
    }

    fn read(&self, addr: u32) -> u8 {
        let addr = (addr & 0xFF_FFFF) as usize;
        self.mem.borrow()[addr]
    }

    fn write(&self, addr: u32, value: u8) {
        let addr = (addr & 0xFF_FFFF) as usize;
        self.mem.borrow_mut()[addr] = value;
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
        self.shared.read(addr)
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.shared.write(addr, value);
    }

    fn tick(&mut self) {}
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
    for [addr, value] in &vector.initial.ram {
        shared.write(*addr, *value as u8);
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

    let actual_cycles = cpu.step() as usize;
    let expected_cycles = vector.cycles.len();
    if actual_cycles != expected_cycles {
        return Err(VectorFailure {
            details: format!(
                "{}: cycle count mismatch (expected {expected_cycles}, got {actual_cycles})",
                vector.name
            ),
        });
    }

    if cpu.read_pc() != vector.final_state.pc
        || cpu.read_s() != vector.final_state.s
        || cpu.read_p() != vector.final_state.p
        || cpu.read_a() != vector.final_state.a
        || cpu.read_x() != vector.final_state.x
        || cpu.read_y() != vector.final_state.y
        || cpu.read_dbr() != vector.final_state.dbr
        || cpu.read_d() != vector.final_state.d
        || cpu.read_pbr() != vector.final_state.pbr
        || cpu.emulation_mode() != (vector.final_state.e != 0)
    {
        return Err(VectorFailure {
            details: format!("{}: CPU final state mismatch", vector.name),
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
        run_vector_case(vector)?;
    }
    Ok(())
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
    fn runs_pinned_subset_opcode_00_emulation() {
        let path = Path::new(PROCESSOR_TESTS_ROOT).join("00.e.json");
        let result = run_vectors_from_file(&path);
        assert!(result.is_ok(), "00.e subset should pass: {result:?}");
    }

    #[test]
    fn runs_pinned_subset_opcode_00_native() {
        let path = Path::new(PROCESSOR_TESTS_ROOT).join("00.n.json");
        let result = run_vectors_from_file(&path);
        assert!(result.is_ok(), "00.n subset should pass: {result:?}");
    }

    #[test]
    fn runs_pinned_subset_opcode_ea_emulation() {
        let path = Path::new(PROCESSOR_TESTS_ROOT).join("ea.e.json");
        let result = run_vectors_from_file(&path);
        assert!(result.is_ok(), "ea.e subset should pass: {result:?}");
    }

    #[test]
    fn runs_pinned_subset_opcode_ea_native() {
        let path = Path::new(PROCESSOR_TESTS_ROOT).join("ea.n.json");
        let result = run_vectors_from_file(&path);
        assert!(result.is_ok(), "ea.n subset should pass: {result:?}");
    }
}
