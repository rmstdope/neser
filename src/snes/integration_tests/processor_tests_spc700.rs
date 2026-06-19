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
        && opcode != 0xE5
        && opcode != 0xC5
        && opcode != 0xF5
        && opcode != 0xF6
        && opcode != 0xD5
        && opcode != 0xD6
        && opcode != 0xF8
        && opcode != 0xEB
        && opcode != 0xE9
        && opcode != 0xEC
        && opcode != 0xF9
        && opcode != 0xFB
        && opcode != 0xC9
        && opcode != 0xCC
        && opcode != 0xE7
        && opcode != 0xF7
        && opcode != 0xC7
        && opcode != 0xD7
        && opcode != 0x7C
        && opcode != 0x3C
        && opcode != 0x24
        && opcode != 0x04
        && opcode != 0x44
        && opcode != 0x88
        && opcode != 0x84
        && opcode != 0xA8
        && opcode != 0xA4
        && opcode != 0xC8
        && opcode != 0xC0
        && opcode != 0xAD
    {
        return Err(VectorFailure {
            details: format!(
                "{}: unsupported opcode ${opcode:02X} at PC ${:04X} (supported in this slice: NOP $00, MOV A,#imm $E8, MOV X,#imm $CD, MOV Y,#imm $8D, MOV A,X $7D, MOV X,A $5D, MOV A,Y $DD, MOV Y,A $FD, MOV X,SP $9D, MOV SP,X $BD, MOV A,dp $E4, MOV dp,A $C4, MOV dp,X $D8, MOV dp,Y $CB, MOV A,(X) $E6, MOV A,(X)+ $BF, MOV (X),A $C6, MOV (X)+,A $AF, MOV A,dp+X $F4, MOV dp+X,A $D4, MOV dp+X,Y $DB, MOV dp+Y,X $D9, MOV A,!abs $E5, MOV !abs,A $C5, MOV A,!abs+X $F5, MOV A,!abs+Y $F6, MOV !abs+X,A $D5, MOV !abs+Y,A $D6, MOV X,dp $F8, MOV Y,dp $EB, MOV X,!abs $E9, MOV Y,!abs $EC, MOV X,dp+Y $F9, MOV Y,dp+X $FB, MOV !abs,X $C9, MOV !abs,Y $CC, MOV A,[dp+X] $E7, MOV A,[dp]+Y $F7, MOV [dp+X],A $C7, MOV [dp]+Y,A $D7, INC A $7C, DEC A $3C, AND A,#imm $24, OR A,#imm $04, EOR A,#imm $44, ADD A,#imm $88, ADC A,#imm $84, SUB A,#imm $A8, SBC A,#imm $A4, CMP A,#imm $C8, CMP X,#imm $C0, CMP Y,#imm $AD)",
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

    fn write_mov_a_abs_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e5 mov a,!abs",
    "initial": {
      "pc": 916,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 119,
      "y": 2,
      "ram": [[916, 229], [917, 52], [918, 18], [4660, 128]]
    },
    "final": {
      "pc": 919,
      "sp": 239,
      "psw": 129,
      "a": 128,
      "x": 119,
      "y": 2,
      "ram": [[916, 229], [917, 52], [918, 18], [4660, 128]]
    },
    "cycles": [
      [916, 229, "d-r-----"],
      [917, 52, "d-r-----"],
      [918, 18, "d-r-----"],
      [4660, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,!abs vector JSON");
    }

    fn write_mov_abs_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c5 mov !abs,a",
    "initial": {
      "pc": 919,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 2,
      "ram": [[919, 197], [920, 53], [921, 18], [4661, 0]]
    },
    "final": {
      "pc": 922,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 2,
      "ram": [[919, 197], [920, 53], [921, 18], [4661, 102]]
    },
    "cycles": [
      [919, 197, "d-r-----"],
      [920, 53, "d-r-----"],
      [921, 18, "d-r-----"],
      [4661, 0, "d-r-----"],
      [4661, 102, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV !abs,A vector JSON");
    }

    fn write_mov_a_abs_plus_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f5 mov a,!abs+x",
    "initial": {
      "pc": 922,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 2,
      "y": 2,
      "ram": [[922, 245], [923, 52], [924, 18], [4662, 128]]
    },
    "final": {
      "pc": 925,
      "sp": 239,
      "psw": 129,
      "a": 128,
      "x": 2,
      "y": 2,
      "ram": [[922, 245], [923, 52], [924, 18], [4662, 128]]
    },
    "cycles": [
      [922, 245, "d-r-----"],
      [923, 52, "d-r-----"],
      [924, 18, "d-r-----"],
      [null, null, "--------"],
      [4662, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,!abs+X vector JSON");
    }

    fn write_mov_a_abs_plus_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f6 mov a,!abs+y",
    "initial": {
      "pc": 925,
      "sp": 239,
      "psw": 1,
      "a": 18,
      "x": 2,
      "y": 2,
      "ram": [[925, 246], [926, 52], [927, 18], [4662, 128]]
    },
    "final": {
      "pc": 928,
      "sp": 239,
      "psw": 129,
      "a": 128,
      "x": 2,
      "y": 2,
      "ram": [[925, 246], [926, 52], [927, 18], [4662, 128]]
    },
    "cycles": [
      [925, 246, "d-r-----"],
      [926, 52, "d-r-----"],
      [927, 18, "d-r-----"],
      [null, null, "--------"],
      [4662, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,!abs+Y vector JSON");
    }

    fn write_mov_abs_plus_x_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d5 mov !abs+x,a",
    "initial": {
      "pc": 928,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[928, 213], [929, 52], [930, 18], [4662, 0]]
    },
    "final": {
      "pc": 931,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[928, 213], [929, 52], [930, 18], [4662, 102]]
    },
    "cycles": [
      [928, 213, "d-r-----"],
      [929, 52, "d-r-----"],
      [930, 18, "d-r-----"],
      [null, null, "--------"],
      [4662, 0, "d-r-----"],
      [4662, 102, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV !abs+X,A vector JSON");
    }

    fn write_mov_abs_plus_y_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d6 mov !abs+y,a",
    "initial": {
      "pc": 931,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[931, 214], [932, 52], [933, 18], [4662, 0]]
    },
    "final": {
      "pc": 934,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[931, 214], [932, 52], [933, 18], [4662, 102]]
    },
    "cycles": [
      [931, 214, "d-r-----"],
      [932, 52, "d-r-----"],
      [933, 18, "d-r-----"],
      [null, null, "--------"],
      [4662, 0, "d-r-----"],
      [4662, 102, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV !abs+Y,A vector JSON");
    }

    fn write_mov_x_dp_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f8 mov x,dp",
    "initial": {
      "pc": 934,
      "sp": 239,
      "psw": 33,
      "a": 102,
      "x": 0,
      "y": 2,
      "ram": [[934, 248], [935, 128], [384, 128]]
    },
    "final": {
      "pc": 936,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 128,
      "y": 2,
      "ram": [[934, 248], [935, 128], [384, 128]]
    },
    "cycles": [
      [934, 248, "d-r-----"],
      [935, 128, "d-r-----"],
      [384, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV X,dp vector JSON");
    }

    fn write_mov_y_dp_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "eb mov y,dp",
    "initial": {
      "pc": 936,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 128,
      "y": 255,
      "ram": [[936, 235], [937, 129], [385, 0]]
    },
    "final": {
      "pc": 938,
      "sp": 239,
      "psw": 35,
      "a": 102,
      "x": 128,
      "y": 0,
      "ram": [[936, 235], [937, 129], [385, 0]]
    },
    "cycles": [
      [936, 235, "d-r-----"],
      [937, 129, "d-r-----"],
      [385, 0, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV Y,dp vector JSON");
    }

    fn write_mov_x_abs_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e9 mov x,!abs",
    "initial": {
      "pc": 938,
      "sp": 239,
      "psw": 1,
      "a": 102,
      "x": 0,
      "y": 0,
      "ram": [[938, 233], [939, 52], [940, 18], [4660, 128]]
    },
    "final": {
      "pc": 941,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 128,
      "y": 0,
      "ram": [[938, 233], [939, 52], [940, 18], [4660, 128]]
    },
    "cycles": [
      [938, 233, "d-r-----"],
      [939, 52, "d-r-----"],
      [940, 18, "d-r-----"],
      [4660, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV X,!abs vector JSON");
    }

    fn write_mov_y_abs_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "ec mov y,!abs",
    "initial": {
      "pc": 941,
      "sp": 239,
      "psw": 1,
      "a": 102,
      "x": 128,
      "y": 0,
      "ram": [[941, 236], [942, 53], [943, 18], [4661, 128]]
    },
    "final": {
      "pc": 944,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 128,
      "y": 128,
      "ram": [[941, 236], [942, 53], [943, 18], [4661, 128]]
    },
    "cycles": [
      [941, 236, "d-r-----"],
      [942, 53, "d-r-----"],
      [943, 18, "d-r-----"],
      [4661, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV Y,!abs vector JSON");
    }

    fn write_mov_x_dp_plus_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f9 mov x,dp+y",
    "initial": {
      "pc": 944,
      "sp": 239,
      "psw": 33,
      "a": 102,
      "x": 0,
      "y": 2,
      "ram": [[944, 249], [945, 255], [257, 128]]
    },
    "final": {
      "pc": 946,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 128,
      "y": 2,
      "ram": [[944, 249], [945, 255], [257, 128]]
    },
    "cycles": [
      [944, 249, "d-r-----"],
      [945, 255, "d-r-----"],
      [null, null, "--------"],
      [257, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV X,dp+Y vector JSON");
    }

    fn write_mov_y_dp_plus_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "fb mov y,dp+x",
    "initial": {
      "pc": 946,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 2,
      "y": 255,
      "ram": [[946, 251], [947, 255], [257, 0]]
    },
    "final": {
      "pc": 948,
      "sp": 239,
      "psw": 35,
      "a": 102,
      "x": 2,
      "y": 0,
      "ram": [[946, 251], [947, 255], [257, 0]]
    },
    "cycles": [
      [946, 251, "d-r-----"],
      [947, 255, "d-r-----"],
      [null, null, "--------"],
      [257, 0, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV Y,dp+X vector JSON");
    }

    fn write_mov_abs_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c9 mov !abs,x",
    "initial": {
      "pc": 948,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 0,
      "ram": [[948, 201], [949, 52], [950, 18], [4660, 0]]
    },
    "final": {
      "pc": 951,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 0,
      "ram": [[948, 201], [949, 52], [950, 18], [4660, 119]]
    },
    "cycles": [
      [948, 201, "d-r-----"],
      [949, 52, "d-r-----"],
      [950, 18, "d-r-----"],
      [4660, 0, "d-r-----"],
      [4660, 119, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV !abs,X vector JSON");
    }

    fn write_mov_abs_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "cc mov !abs,y",
    "initial": {
      "pc": 951,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 51,
      "ram": [[951, 204], [952, 53], [953, 18], [4661, 0]]
    },
    "final": {
      "pc": 954,
      "sp": 239,
      "psw": 129,
      "a": 102,
      "x": 119,
      "y": 51,
      "ram": [[951, 204], [952, 53], [953, 18], [4661, 51]]
    },
    "cycles": [
      [951, 204, "d-r-----"],
      [952, 53, "d-r-----"],
      [953, 18, "d-r-----"],
      [4661, 0, "d-r-----"],
      [4661, 51, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV !abs,Y vector JSON");
    }

    fn write_mov_a_indirect_dp_plus_x_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "e7 mov a,[dp+x]",
    "initial": {
      "pc": 954,
      "sp": 239,
      "psw": 33,
      "a": 0,
      "x": 2,
      "y": 0,
      "ram": [[954, 231], [955, 128], [386, 52], [387, 18], [4660, 128]]
    },
    "final": {
      "pc": 956,
      "sp": 239,
      "psw": 161,
      "a": 128,
      "x": 2,
      "y": 0,
      "ram": [[954, 231], [955, 128], [386, 52], [387, 18], [4660, 128]]
    },
    "cycles": [
      [954, 231, "d-r-----"],
      [955, 128, "d-r-----"],
      [null, null, "--------"],
      [386, 52, "d-r-----"],
      [387, 18, "d-r-----"],
      [4660, 128, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,[dp+X] vector JSON");
    }

    fn write_mov_a_indirect_dp_plus_y_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "f7 mov a,[dp]+y",
    "initial": {
      "pc": 956,
      "sp": 239,
      "psw": 161,
      "a": 255,
      "x": 2,
      "y": 2,
      "ram": [[956, 247], [957, 132], [388, 52], [389, 18], [4662, 0]]
    },
    "final": {
      "pc": 958,
      "sp": 239,
      "psw": 35,
      "a": 0,
      "x": 2,
      "y": 2,
      "ram": [[956, 247], [957, 132], [388, 52], [389, 18], [4662, 0]]
    },
    "cycles": [
      [956, 247, "d-r-----"],
      [957, 132, "d-r-----"],
      [388, 52, "d-r-----"],
      [389, 18, "d-r-----"],
      [null, null, "--------"],
      [4662, 0, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV A,[dp]+Y vector JSON");
    }

    fn write_mov_indirect_dp_plus_x_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c7 mov [dp+x],a",
    "initial": {
      "pc": 958,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[958, 199], [959, 136], [394, 64], [395, 18], [4672, 0]]
    },
    "final": {
      "pc": 960,
      "sp": 239,
      "psw": 161,
      "a": 102,
      "x": 2,
      "y": 2,
      "ram": [[958, 199], [959, 136], [394, 64], [395, 18], [4672, 102]]
    },
    "cycles": [
      [958, 199, "d-r-----"],
      [959, 136, "d-r-----"],
      [null, null, "--------"],
      [394, 64, "d-r-----"],
      [395, 18, "d-r-----"],
      [4672, 0, "d-r-----"],
      [4672, 102, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV [dp+X],A vector JSON");
    }

    fn write_mov_indirect_dp_plus_y_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "d7 mov [dp]+y,a",
    "initial": {
      "pc": 960,
      "sp": 239,
      "psw": 161,
      "a": 119,
      "x": 2,
      "y": 2,
      "ram": [[960, 215], [961, 140], [396, 64], [397, 18], [4674, 0]]
    },
    "final": {
      "pc": 962,
      "sp": 239,
      "psw": 161,
      "a": 119,
      "x": 2,
      "y": 2,
      "ram": [[960, 215], [961, 140], [396, 64], [397, 18], [4674, 119]]
    },
    "cycles": [
      [960, 215, "d-r-----"],
      [961, 140, "d-r-----"],
      [396, 64, "d-r-----"],
      [397, 18, "d-r-----"],
      [null, null, "--------"],
      [4674, 0, "d-r-----"],
      [4674, 119, "dwr-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample MOV [dp]+Y,A vector JSON");
    }

    fn write_inc_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "7c inc a",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 127,
      "x": 0,
      "y": 0,
      "ram": [[512, 124]]
    },
    "final": {
      "pc": 513,
      "sp": 239,
      "psw": 128,
      "a": 128,
      "x": 0,
      "y": 0,
      "ram": [[512, 124]]
    },
    "cycles": [
      [512, 124, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample INC A vector JSON");
    }

    fn write_dec_a_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "3c dec a",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 128,
      "x": 0,
      "y": 0,
      "ram": [[512, 60]]
    },
    "final": {
      "pc": 513,
      "sp": 239,
      "psw": 0,
      "a": 127,
      "x": 0,
      "y": 0,
      "ram": [[512, 60]]
    },
    "cycles": [
      [512, 60, "d-r-----"],
      [null, null, "--------"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample DEC A vector JSON");
    }

    fn write_and_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "24 and a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 128,
      "a": 255,
      "x": 0,
      "y": 0,
      "ram": [[512, 36], [513, 15]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 0,
      "a": 15,
      "x": 0,
      "y": 0,
      "ram": [[512, 36], [513, 15]]
    },
    "cycles": [
      [512, 36, "d-r-----"],
      [513, 15, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample AND A,#imm vector JSON");
    }

    fn write_or_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "04 or a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 240,
      "x": 0,
      "y": 0,
      "ram": [[512, 4], [513, 15]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 128,
      "a": 255,
      "x": 0,
      "y": 0,
      "ram": [[512, 4], [513, 15]]
    },
    "cycles": [
      [512, 4, "d-r-----"],
      [513, 15, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample OR A,#imm vector JSON");
    }

    fn write_eor_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "44 eor a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 15,
      "x": 0,
      "y": 0,
      "ram": [[512, 68], [513, 255]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 128,
      "a": 240,
      "x": 0,
      "y": 0,
      "ram": [[512, 68], [513, 255]]
    },
    "cycles": [
      [512, 68, "d-r-----"],
      [513, 255, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample EOR A,#imm vector JSON");
    }

    fn write_add_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "88 add a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 32,
      "x": 0,
      "y": 0,
      "ram": [[512, 136], [513, 16]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 0,
      "a": 48,
      "x": 0,
      "y": 0,
      "ram": [[512, 136], [513, 16]]
    },
    "cycles": [
      [512, 136, "d-r-----"],
      [513, 16, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample ADD A,#imm vector JSON");
    }

    fn write_adc_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "84 adc a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 1,
      "a": 32,
      "x": 0,
      "y": 0,
      "ram": [[512, 132], [513, 16]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 0,
      "a": 49,
      "x": 0,
      "y": 0,
      "ram": [[512, 132], [513, 16]]
    },
    "cycles": [
      [512, 132, "d-r-----"],
      [513, 16, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample ADC A,#imm vector JSON");
    }

    fn write_sub_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "a8 sub a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 48,
      "x": 0,
      "y": 0,
      "ram": [[512, 168], [513, 16]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 1,
      "a": 32,
      "x": 0,
      "y": 0,
      "ram": [[512, 168], [513, 16]]
    },
    "cycles": [
      [512, 168, "d-r-----"],
      [513, 16, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample SUB A,#imm vector JSON");
    }

    fn write_sbc_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "a4 sbc a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 48,
      "x": 0,
      "y": 0,
      "ram": [[512, 164], [513, 16]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 1,
      "a": 31,
      "x": 0,
      "y": 0,
      "ram": [[512, 164], [513, 16]]
    },
    "cycles": [
      [512, 164, "d-r-----"],
      [513, 16, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample SBC A,#imm vector JSON");
    }

    fn write_cmp_a_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c8 cmp a,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 85,
      "x": 0,
      "y": 0,
      "ram": [[512, 200], [513, 85]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 3,
      "a": 85,
      "x": 0,
      "y": 0,
      "ram": [[512, 200], [513, 85]]
    },
    "cycles": [
      [512, 200, "d-r-----"],
      [513, 85, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample CMP A,#imm vector JSON");
    }

    fn write_cmp_x_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "c0 cmp x,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 0,
      "x": 48,
      "y": 0,
      "ram": [[512, 192], [513, 48]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 3,
      "a": 0,
      "x": 48,
      "y": 0,
      "ram": [[512, 192], [513, 48]]
    },
    "cycles": [
      [512, 192, "d-r-----"],
      [513, 48, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample CMP X,#imm vector JSON");
    }

    fn write_cmp_y_imm_vector(path: &Path) {
        let sample = r#"[
  {
    "name": "ad cmp y,#imm",
    "initial": {
      "pc": 512,
      "sp": 239,
      "psw": 0,
      "a": 0,
      "x": 0,
      "y": 64,
      "ram": [[512, 173], [513, 64]]
    },
    "final": {
      "pc": 514,
      "sp": 239,
      "psw": 3,
      "a": 0,
      "x": 0,
      "y": 64,
      "ram": [[512, 173], [513, 64]]
    },
    "cycles": [
      [512, 173, "d-r-----"],
      [513, 64, "d-r-----"]
    ]
  }
]
"#;
        fs::write(path, sample).expect("write sample CMP Y,#imm vector JSON");
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
    fn given_mov_a_abs_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e5.json");
        write_mov_a_abs_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_abs_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c5.json");
        write_mov_abs_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_abs_plus_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f5.json");
        write_mov_a_abs_plus_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_abs_plus_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f6.json");
        write_mov_a_abs_plus_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_abs_plus_x_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d5.json");
        write_mov_abs_plus_x_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_abs_plus_y_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d6.json");
        write_mov_abs_plus_y_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_x_dp_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f8.json");
        write_mov_x_dp_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_y_dp_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("eb.json");
        write_mov_y_dp_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_x_abs_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e9.json");
        write_mov_x_abs_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_y_abs_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("ec.json");
        write_mov_y_abs_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_x_dp_plus_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f9.json");
        write_mov_x_dp_plus_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_y_dp_plus_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("fb.json");
        write_mov_y_dp_plus_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_abs_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c9.json");
        write_mov_abs_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_abs_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("cc.json");
        write_mov_abs_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_indirect_dp_plus_x_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("e7.json");
        write_mov_a_indirect_dp_plus_x_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_a_indirect_dp_plus_y_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("f7.json");
        write_mov_a_indirect_dp_plus_y_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_indirect_dp_plus_x_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c7.json");
        write_mov_indirect_dp_plus_x_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_mov_indirect_dp_plus_y_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("d7.json");
        write_mov_indirect_dp_plus_y_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_inc_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("7c.json");
        write_inc_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_dec_a_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("3c.json");
        write_dec_a_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_and_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("24.json");
        write_and_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_or_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("04.json");
        write_or_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_eor_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("44.json");
        write_eor_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_add_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("88.json");
        write_add_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_adc_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("84.json");
        write_adc_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_sub_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("a8.json");
        write_sub_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_sbc_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("a4.json");
        write_sbc_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_cmp_a_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c8.json");
        write_cmp_a_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_cmp_x_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("c0.json");
        write_cmp_x_imm_vector(&path);

        let vectors = load_vectors_from_file(&path).expect("load vectors from sample file");
        let result = run_vector_case(&vectors[0]);
        assert!(result.is_ok(), "expected vector case to pass: {result:?}");
    }

    #[test]
    fn given_cmp_y_imm_vector_when_executed_then_final_state_matches() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("ad.json");
        write_cmp_y_imm_vector(&path);

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
