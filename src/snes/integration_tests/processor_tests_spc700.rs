use crate::snes::apu::spc700::{FlatRamBus, Spc700};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const PROCESSOR_TESTS_ROOT: &str = "roms/snes/automated_tests/processor_tests/spc700/v1";
const PROCESSOR_TESTS_FULL_ROOT: &str = "roms/snes/automated_tests/processor_tests/spc700/full/v1";

#[derive(Debug, Clone, serde::Deserialize)]
struct VectorState {
    pc: u16,
    sp: u8,
    psw: u8,
    a: u8,
    x: u8,
    y: u8,
    ram: Vec<[u16; 2]>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct VectorCycle {
    address: Option<u16>,
    value: Option<u8>,
    signals: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RawProcessorTestVector {
    name: String,
    initial: VectorState,
    #[serde(rename = "final")]
    final_state: VectorState,
    cycles: Vec<(Option<u16>, Option<u8>, String)>,
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

        if cycle.address.is_some() != cycle.value.is_some() {
            return Err(VectorFailure {
                details: format!(
                    "{}: cycle address/value presence mismatch (signals='{}')",
                    vector.name, cycle.signals
                ),
            });
        }
    }

    let mut bus = FlatRamBus::new();
    for [addr, value] in &vector.initial.ram {
        let byte = checked_byte(*value, vector, "initial RAM", Some(*addr))?;
        bus.set(*addr, byte);
    }

    if !contains_initial_ram_address(vector, vector.initial.pc) {
        return Err(VectorFailure {
            details: format!(
                "{}: initial RAM is missing opcode byte at PC ${:04X}",
                vector.name, vector.initial.pc
            ),
        });
    }

    let opcode = bus.get(vector.initial.pc);
    if (opcode == 0xE8 || opcode == 0xCD || opcode == 0x8D)
        && !contains_initial_ram_address(vector, vector.initial.pc.wrapping_add(1))
    {
        return Err(VectorFailure {
            details: format!(
                "{}: initial RAM is missing immediate byte for opcode ${opcode:02X} at ${:04X}",
                vector.name,
                vector.initial.pc.wrapping_add(1)
            ),
        });
    }

    if opcode != 0x00
        && opcode != 0xE8
        && opcode != 0xCD
        && opcode != 0x8D
        && opcode != 0x7D
        && opcode != 0x5D
        && opcode != 0xDD
        && opcode != 0xFD
        && opcode != 0x9D
        && opcode != 0xBD
        && opcode != 0xE4
        && opcode != 0xC4
        && opcode != 0xD8
        && opcode != 0xCB
        && opcode != 0xE6
        && opcode != 0xBF
        && opcode != 0xC6
        && opcode != 0xAF
        && opcode != 0xF4
        && opcode != 0xD4
        && opcode != 0xDB
        && opcode != 0xD9
    {
        return Err(VectorFailure {
            details: format!(
                "{}: unsupported opcode ${opcode:02X} at PC ${:04X} (supported in this slice: NOP $00, MOV A,#imm $E8, MOV X,#imm $CD, MOV Y,#imm $8D, MOV A,X $7D, MOV X,A $5D, MOV A,Y $DD, MOV Y,A $FD, MOV X,SP $9D, MOV SP,X $BD, MOV A,dp $E4, MOV dp,A $C4, MOV dp,X $D8, MOV dp,Y $CB, MOV A,(X) $E6, MOV A,(X)+ $BF, MOV (X),A $C6, MOV (X)+,A $AF, MOV A,dp+X $F4, MOV dp+X,A $D4, MOV dp+X,Y $DB, MOV dp+Y,X $D9)",
                vector.name, vector.initial.pc
            ),
        });
    }

    let mut cpu = Spc700::new();
    cpu.load_state_for_processor_test(
        vector.initial.a,
        vector.initial.x,
        vector.initial.y,
        vector.initial.sp,
        vector.initial.pc,
        vector.initial.psw,
    );

    let actual_cycles = cpu.step(&mut bus) as usize;
    let expected_cycles = vector.cycles.len();
    if actual_cycles != expected_cycles {
        return Err(VectorFailure {
            details: format!(
                "{}: cycle count mismatch (expected {expected_cycles}, got {actual_cycles})",
                vector.name
            ),
        });
    }

    if cpu.pc() != vector.final_state.pc
        || cpu.sp() != vector.final_state.sp
        || cpu.psw() != vector.final_state.psw
        || cpu.a() != vector.final_state.a
        || cpu.x() != vector.final_state.x
        || cpu.y() != vector.final_state.y
    {
        return Err(VectorFailure {
            details: format!(
                "{}: CPU final state mismatch\n  PC: expected ${:04X}, got ${:04X}\n  SP: expected ${:02X}, got ${:02X}\n  PSW: expected ${:02X}, got ${:02X}\n  A: expected ${:02X}, got ${:02X}\n  X: expected ${:02X}, got ${:02X}\n  Y: expected ${:02X}, got ${:02X}",
                vector.name,
                vector.final_state.pc,
                cpu.pc(),
                vector.final_state.sp,
                cpu.sp(),
                vector.final_state.psw,
                cpu.psw(),
                vector.final_state.a,
                cpu.a(),
                vector.final_state.x,
                cpu.x(),
                vector.final_state.y,
                cpu.y()
            ),
        });
    }

    for [addr, expected] in &vector.final_state.ram {
        let expected_byte = checked_byte(*expected, vector, "final RAM", Some(*addr))?;
        let actual = bus.get(*addr);
        if actual != expected_byte {
            return Err(VectorFailure {
                details: format!(
                    "{}: RAM mismatch at ${addr:04X} (expected {:#04X}, got {:#04X})",
                    vector.name, expected_byte, actual
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
    let subset_files = if subset_root.exists() {
        list_vector_files(subset_root)?
    } else {
        Vec::new()
    };
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

fn run_vectors_from_directory(subset_root: &Path, full_root: &Path) -> Result<(), VectorFailure> {
    let files = list_available_vector_files(subset_root, full_root)
        .map_err(|details| VectorFailure { details })?;
    if files.is_empty() {
        return Err(VectorFailure {
            details: format!(
                "no vector files found in {} or {}",
                subset_root.display(),
                full_root.display()
            ),
        });
    }

    for file in files {
        run_vectors_from_file(&file)?;
    }
    Ok(())
}

fn checked_byte(
    value: u16,
    vector: &ProcessorTestVector,
    field: &str,
    addr: Option<u16>,
) -> Result<u8, VectorFailure> {
    if value > 0x00FF {
        let where_str = addr.map(|a| format!(" at ${a:04X}")).unwrap_or_default();
        return Err(VectorFailure {
            details: format!(
                "{}: {field}{where_str} has out-of-range byte value ${value:04X}",
                vector.name
            ),
        });
    }
    Ok(value as u8)
}

fn contains_initial_ram_address(vector: &ProcessorTestVector, addr: u16) -> bool {
    vector.initial.ram.iter().any(|entry| entry[0] == addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_sample_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "00 nop",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[512, 0]]
    },
    "final": {
      "pc": 513,
      "sp": 239,
      "psw": 0,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[512, 0]]
    },
    "cycles": [
      [512, 0, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample vector JSON");
    }

    fn write_mov_a_immediate_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e8 mov a,#imm",
    "initial": {
      "pc": 768,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[768, 232], [769, 0]]
    },
    "final": {
      "pc": 770,
      "sp": 239,
      "psw": 3,
      "a": 0,
      "x": 52,
      "y": 86,
      "ram": [[768, 232], [769, 0]]
    },
    "cycles": [
      [768, 232, "d-r-----"],
      [769, 0, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,#imm vector JSON");
    }

    fn write_mov_x_immediate_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "cd mov x,#imm",
    "initial": {
      "pc": 800,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[800, 205], [801, 128]]
    },
    "final": {
      "pc": 802,
      "sp": 239,
      "psw": 129,
      "a": 18,
      "x": 128,
      "y": 86,
      "ram": [[800, 205], [801, 128]]
    },
    "cycles": [
      [800, 205, "d-r-----"],
      [801, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV X,#imm vector JSON");
    }

    fn write_mov_y_immediate_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "8d mov y,#imm",
    "initial": {
      "pc": 832,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[832, 141], [833, 0]]
    },
    "final": {
      "pc": 834,
      "sp": 239,
      "psw": 3,
      "a": 18,
      "x": 52,
      "y": 0,
      "ram": [[832, 141], [833, 0]]
    },
    "cycles": [
      [832, 141, "d-r-----"],
      [833, 0, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV Y,#imm vector JSON");
    }

    fn write_mov_a_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "7d mov a,x",
    "initial": {
      "pc": 864,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 128,
      "y": 86,
      "ram": [[864, 125]]
    },
    "final": {
      "pc": 865,
      "sp": 239,
      "psw": 129,
      "a": 128,
      "x": 128,
      "y": 86,
      "ram": [[864, 125]]
    },
    "cycles": [
      [864, 125, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,X vector JSON");
    }

    fn write_mov_y_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "fd mov y,a",
    "initial": {
      "pc": 865,
      "sp": 239,
      "psw": 129,
      "a": 0,
      "x": 52,
      "y": 86,
      "ram": [[865, 253]]
    },
    "final": {
      "pc": 866,
      "sp": 239,
      "psw": 3,
      "a": 0,
      "x": 52,
      "y": 0,
      "ram": [[865, 253]]
    },
    "cycles": [
      [865, 253, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV Y,A vector JSON");
    }

    fn write_mov_x_sp_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "9d mov x,sp",
    "initial": {
      "pc": 866,
      "sp": 128,
      "psw": 1,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[866, 157]]
    },
    "final": {
      "pc": 867,
      "sp": 128,
      "psw": 129,
      "a": 18,
      "x": 128,
      "y": 86,
      "ram": [[866, 157]]
    },
    "cycles": [
      [866, 157, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV X,SP vector JSON");
    }

    fn write_mov_sp_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "bd mov sp,x",
    "initial": {
      "pc": 867,
      "sp": 239,
      "psw": 129,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[867, 189]]
    },
    "final": {
      "pc": 868,
      "sp": 52,
      "psw": 129,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[867, 189]]
    },
    "cycles": [
      [867, 189, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV SP,X vector JSON");
    }

    fn write_mov_a_dp_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e4 mov a,dp",
    "initial": {
      "pc": 896,
      "sp": 239,
      "psw": 33,
      "a": 18,
      "x": 52,
      "y": 86,
      "ram": [[896, 228], [897, 128], [384, 128]]
    },
    "final": {
      "pc": 898,
      "sp": 239,
      "psw": 161,
      "a": 128,
      "x": 52,
      "y": 86,
      "ram": [[896, 228], [897, 128], [384, 128]]
    },
    "cycles": [
      [896, 228, "d-r-----"],
      [897, 128, "d-r-----"],
      [384, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,dp vector JSON");
    }

    fn write_mov_dp_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c4 mov dp,a",
    "initial": {
      "pc": 898,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 52,
      "y": 86,
      "ram": [[898, 196], [899, 129], [385, 0]]
    },
    "final": {
      "pc": 900,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 52,
      "y": 86,
      "ram": [[898, 196], [899, 129], [385, 66]]
    },
    "cycles": [
      [898, 196, "d-r-----"],
      [899, 129, "d-r-----"],
      [385, 0, "d-r-----"],
      [385, 66, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp,A vector JSON");
    }

    fn write_mov_dp_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d8 mov dp,x",
    "initial": {
      "pc": 900,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 55,
      "y": 86,
      "ram": [[900, 216], [901, 130], [386, 0]]
    },
    "final": {
      "pc": 902,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 55,
      "y": 86,
      "ram": [[900, 216], [901, 130], [386, 55]]
    },
    "cycles": [
      [900, 216, "d-r-----"],
      [901, 130, "d-r-----"],
      [386, 0, "d-r-----"],
      [386, 55, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp,X vector JSON");
    }

    fn write_mov_dp_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "cb mov dp,y",
    "initial": {
      "pc": 902,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 55,
      "y": 145,
      "ram": [[902, 203], [903, 131], [387, 0]]
    },
    "final": {
      "pc": 904,
      "sp": 239,
      "psw": 161,
      "a": 66,
      "x": 55,
      "y": 145,
      "ram": [[902, 203], [903, 131], [387, 145]]
    },
    "cycles": [
      [902, 203, "d-r-----"],
      [903, 131, "d-r-----"],
      [387, 0, "d-r-----"],
      [387, 145, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp,Y vector JSON");
    }

    fn write_mov_a_indirect_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e6 mov a,(x)",
    "initial": {
      "pc": 904,
      "sp": 239,
      "psw": 33,
      "a": 18,
      "x": 146,
      "y": 86,
      "ram": [[904, 230], [402, 128]]
    },
    "final": {
      "pc": 905,
      "sp": 239,
      "psw": 161,
      "a": 128,
      "x": 146,
      "y": 86,
      "ram": [[904, 230], [402, 128]]
    },
    "cycles": [
      [904, 230, "d-r-----"],
      [402, 128, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,(X) vector JSON");
    }

    fn write_mov_a_indirect_x_postinc_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "bf mov a,(x)+",
    "initial": {
      "pc": 905,
      "sp": 239,
      "psw": 161,
      "a": 18,
      "x": 254,
      "y": 86,
      "ram": [[905, 191], [510, 0]]
    },
    "final": {
      "pc": 906,
      "sp": 239,
      "psw": 35,
      "a": 0,
      "x": 255,
      "y": 86,
      "ram": [[905, 191], [510, 0]]
    },
    "cycles": [
      [905, 191, "d-r-----"],
      [510, 0, "d-r-----"],
      [null, null, "--------"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,(X)+ vector JSON");
    }

    fn write_mov_indirect_x_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c6 mov (x),a",
    "initial": {
      "pc": 906,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 132,
      "y": 86,
      "ram": [[906, 198], [388, 0]]
    },
    "final": {
      "pc": 907,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 132,
      "y": 86,
      "ram": [[906, 198], [388, 102]]
    },
    "cycles": [
      [906, 198, "d-r-----"],
      [388, 0, "d-r-----"],
      [388, 102, "dwr-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV (X),A vector JSON");
    }

    fn write_mov_indirect_x_postinc_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "af mov (x)+,a",
    "initial": {
      "pc": 907,
      "sp": 239,
      "psw": 161,
      "a": 119,
      "x": 255,
      "y": 86,
      "ram": [[907, 175], [511, 0]]
    },
    "final": {
      "pc": 908,
      "sp": 239,
      "psw": 161,
      "a": 119,
      "x": 0,
      "y": 86,
      "ram": [[907, 175], [511, 119]]
    },
    "cycles": [
      [907, 175, "d-r-----"],
      [511, 119, "dwr-----"],
      [null, null, "--------"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV (X)+,A vector JSON");
    }

    fn write_mov_a_dp_plus_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f4 mov a,dp+x",
    "initial": {
      "pc": 908,
      "sp": 239,
      "psw": 33,
      "a": 18,
      "x": 2,
      "y": 86,
      "ram": [[908, 244], [909, 254], [256, 128]]
    },
    "final": {
      "pc": 910,
      "sp": 239,
      "psw": 161,
      "a": 128,
      "x": 2,
      "y": 86,
      "ram": [[908, 244], [909, 254], [256, 128]]
    },
    "cycles": [
      [908, 244, "d-r-----"],
      [909, 254, "d-r-----"],
      [null, null, "--------"],
      [256, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,dp+X vector JSON");
    }

    fn write_mov_dp_plus_x_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d4 mov dp+x,a",
    "initial": {
      "pc": 910,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 2,
      "y": 86,
      "ram": [[910, 212], [911, 254], [256, 0]]
    },
    "final": {
      "pc": 912,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 2,
      "y": 86,
      "ram": [[910, 212], [911, 254], [256, 90]]
    },
    "cycles": [
      [910, 212, "d-r-----"],
      [911, 254, "d-r-----"],
      [256, 0, "d-r-----"],
      [256, 90, "dwr-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp+X,A vector JSON");
    }

    fn write_mov_dp_plus_x_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "db mov dp+x,y",
    "initial": {
      "pc": 912,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 2,
      "y": 145,
      "ram": [[912, 219], [913, 254], [256, 0]]
    },
    "final": {
      "pc": 914,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 2,
      "y": 145,
      "ram": [[912, 219], [913, 254], [256, 145]]
    },
    "cycles": [
      [912, 219, "d-r-----"],
      [913, 254, "d-r-----"],
      [256, 0, "d-r-----"],
      [256, 145, "dwr-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp+X,Y vector JSON");
    }

    fn write_mov_dp_plus_y_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d9 mov dp+y,x",
    "initial": {
      "pc": 914,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 119,
      "y": 2,
      "ram": [[914, 217], [915, 254], [256, 0]]
    },
    "final": {
      "pc": 916,
      "sp": 239,
      "psw": 161,
      "a": 90,
      "x": 119,
      "y": 2,
      "ram": [[914, 217], [915, 254], [256, 119]]
    },
    "cycles": [
      [914, 217, "d-r-----"],
      [915, 254, "d-r-----"],
      [256, 0, "d-r-----"],
      [256, 119, "dwr-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV dp+Y,X vector JSON");
    }

    #[test]
    fn given_spc700_vector_json_when_loaded_then_schema_is_parsed() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("00.json");
        write_sample_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        assert_eq!(vectors.len(), 1);
        let vector = &vectors[0];
        assert_eq!(vector.name, "00 nop");
        assert_eq!(vector.initial.pc, 0x0200);
        assert_eq!(vector.final_state.pc, 0x0201);
        assert_eq!(vector.cycles.len(), 2);
        assert_eq!(vector.cycles[0].address, Some(0x0200));
        assert_eq!(vector.cycles[0].value, Some(0x00));
        assert_eq!(vector.cycles[0].signals, "d-r-----");
    }

    #[test]
    fn given_nop_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("00.json");
        write_sample_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_immediate_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e8.json");
        write_mov_a_immediate_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_x_immediate_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("cd.json");
        write_mov_x_immediate_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_y_immediate_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("8d.json");
        write_mov_y_immediate_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("7d.json");
        write_mov_a_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_y_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("fd.json");
        write_mov_y_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_x_sp_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("9d.json");
        write_mov_x_sp_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_sp_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("bd.json");
        write_mov_sp_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_dp_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e4.json");
        write_mov_a_dp_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c4.json");
        write_mov_dp_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d8.json");
        write_mov_dp_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("cb.json");
        write_mov_dp_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_indirect_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e6.json");
        write_mov_a_indirect_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_indirect_x_postinc_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("bf.json");
        write_mov_a_indirect_x_postinc_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_indirect_x_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c6.json");
        write_mov_indirect_x_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_indirect_x_postinc_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("af.json");
        write_mov_indirect_x_postinc_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_dp_plus_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f4.json");
        write_mov_a_dp_plus_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_plus_x_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d4.json");
        write_mov_dp_plus_x_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_plus_x_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("db.json");
        write_mov_dp_plus_x_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_dp_plus_y_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d9.json");
        write_mov_dp_plus_y_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn runs_all_available_spc700_vectors() {
        let root = Path::new(PROCESSOR_TESTS_ROOT);
        let full_root = Path::new(PROCESSOR_TESTS_FULL_ROOT);
        if !root.exists() && !full_root.exists() {
            return;
        }

        let result = run_vectors_from_directory(root, full_root);
        assert!(
            result.is_ok(),
            "available spc700 vectors should pass: {result:?}"
        );
    }
}
