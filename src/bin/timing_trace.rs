//! Write an NMI-clock log or an exec trace that lines up with Mesen2's
//! (`scripts/reference_capture/mesen2_nmi_clock.lua`, `mesen2_exec_trace.lua`).
//! See `scripts/reference_capture/README.md`, "Tracing against Mesen2".

use neser::platform::timing_trace::{load_for_trace, parse_args, run_trace};
use std::io::{BufWriter, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = parse_args(&args).and_then(|args| {
        let mut console = load_for_trace(&args.rom_path)?;
        let file = std::fs::File::create(&args.out)
            .map_err(|err| format!("cannot create {}: {err}", args.out))?;
        let mut out = BufWriter::new(file);
        run_trace(&mut console, &args.mode, &mut out)?;
        out.flush()
            .map_err(|err| format!("cannot write the trace: {err}"))
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::from(2)
        }
    }
}
