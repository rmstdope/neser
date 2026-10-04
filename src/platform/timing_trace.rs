//! Timing traces that line up with Mesen2's (nr-ggx).
//!
//! Finding where NESER's CPU timing first leaves Mesen2's takes two traces from each
//! emulator, written in the same format so `scripts/diff_timing_traces.py` can compare them
//! line by line:
//!
//! - **NMI clock log** ([`TraceMode::NmiClock`]): one line per NMI entry, stamped with the
//!   clock before the handler's first opcode fetch. A CPU-timing drift moves the next NMI
//!   entry, so this one line per frame finds the frame to look at in seconds (nr-4lq).
//! - **Exec trace** ([`TraceMode::Exec`]): one line per executed instruction between two
//!   NMI entries, the window the NMI log pointed at.
//!
//! The Mesen2 halves are `scripts/reference_capture/mesen2_nmi_clock.lua` and
//! `mesen2_exec_trace.lua`. Both sides stamp an instruction with the clock *before its
//! opcode fetch*, which is what a Mesen2 `exec` callback reports (nr-4cl). The clock is the
//! CPU cycle count on the NES and the master clock on the SNES, Mesen2's `masterClock` on
//! each. A sample is written only when the tick that follows it executes an instruction: a
//! tick that only dispatches an interrupt, waits in WAI, or runs OAM DMA leaves the PC on an
//! instruction Mesen2 has not reached yet.

use crate::platform::app_context::AppContext;
use crate::platform::config::{Config, FrontendConfig, RamInitMode};
use crate::platform::emulator::{Console, Emulator};
use crate::platform::rom_loader::load_console;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

/// Ticks allowed between two lines before the trace is declared stuck: a ROM that never
/// enables NMI would otherwise spin forever in an NMI-clock run. One NTSC SNES frame is
/// a few tens of thousands of ticks, so this is several hundred frames.
const MAX_TICKS_BETWEEN_LINES: u64 = 10_000_000;

/// What a console exposes so its timing can be traced against Mesen2.
pub trait TimingProbe {
    /// The address of the next instruction: the PC on the NES, `PBR << 16 | PC` on the SNES.
    fn trace_pc(&self) -> u32;
    /// The clock now: CPU cycles on the NES, master clocks on the SNES.
    fn trace_clock(&self) -> u64;
    /// NMIs the CPU has entered since power-on.
    fn nmis_taken(&self) -> u64;
    /// Instructions the CPU has executed since power-on.
    fn instructions_executed(&self) -> u64;
}

/// The slice of a console the trace loop drives; narrow so it can be tested with a double.
pub(crate) trait TraceStepper: TimingProbe {
    fn run_tick(&mut self) -> u8;
}

impl<T: TimingProbe + Emulator> TraceStepper for T {
    fn run_tick(&mut self) -> u8 {
        Emulator::run_tick(self)
    }
}

/// Which trace to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    /// One `nmi=<n> pc=<hex> clk=<dec>` line for each of the first `nmis` NMI entries.
    NmiClock { nmis: u64 },
    /// One `pc=<hex> clk=<dec>` line per instruction, from the first instruction of NMI entry
    /// `from_nmi` (0: from power-on) up to, not including, the first instruction of entry
    /// `to_nmi`.
    Exec { from_nmi: u64, to_nmi: u64 },
}

/// The `timing_trace` binary's command line: which ROM, and which trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceArgs {
    pub rom_path: String,
    /// Where the trace goes. A file rather than stdout, which also carries the cores'
    /// load messages.
    pub out: String,
    pub mode: TraceMode,
}

/// NMI entries an NMI-clock log covers by default: a minute of NTSC play.
pub const DEFAULT_NMIS: u64 = 3600;

/// The usage line `timing_trace` prints with every argument error.
pub const USAGE: &str = "usage: timing_trace nmi <rom> --out <path> [--nmis N]\n       timing_trace exec <rom> --out <path> --from-nmi A [--to-nmi B]";

/// Parse `timing_trace`'s arguments (without the program name).
pub fn parse_args(args: &[String]) -> Result<TraceArgs, String> {
    let fail = |problem: String| format!("{problem}\n{USAGE}");
    let [kind, rom_path, options @ ..] = args else {
        return Err(USAGE.to_string());
    };
    let is_exec = match kind.as_str() {
        "nmi" => false,
        "exec" => true,
        other => return Err(fail(format!("unknown trace '{other}'"))),
    };

    let (mut nmis, mut from_nmi, mut to_nmi, mut out) = (None, None, None, None);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        if option == "--out" {
            let path = options
                .next()
                .ok_or_else(|| fail("--out needs a path".to_string()))?;
            out = Some(path.clone());
            continue;
        }
        let slot = match option.as_str() {
            "--nmis" if !is_exec => &mut nmis,
            "--from-nmi" if is_exec => &mut from_nmi,
            "--to-nmi" if is_exec => &mut to_nmi,
            other => return Err(fail(format!("unknown option '{other}'"))),
        };
        let value = options
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| fail(format!("{option} needs a number")))?;
        *slot = Some(value);
    }

    let out = out.ok_or_else(|| fail("needs --out <path>".to_string()))?;
    let mode = if is_exec {
        let from_nmi = from_nmi.ok_or_else(|| fail("exec needs --from-nmi".to_string()))?;
        let to_nmi = to_nmi.unwrap_or(from_nmi.saturating_add(1));
        if to_nmi <= from_nmi {
            return Err(fail("--to-nmi must be after --from-nmi".to_string()));
        }
        TraceMode::Exec { from_nmi, to_nmi }
    } else {
        let nmis = nmis.unwrap_or(DEFAULT_NMIS);
        if nmis == 0 {
            return Err(fail("--nmis must be at least 1".to_string()));
        }
        TraceMode::NmiClock { nmis }
    };
    Ok(TraceArgs {
        rom_path: rom_path.clone(),
        out,
        mode,
    })
}

/// Load `rom_path` for a trace: default settings, never `neser.conf`, and zero-filled RAM as
/// `--headless` uses and the Mesen2 recipe pins (`RamPowerOnState=AllZeros`).
pub fn load_for_trace(rom_path: &str) -> Result<Console, String> {
    let config = Config {
        frontend: FrontendConfig {
            ram_init_mode: RamInitMode::Zero,
            ..Default::default()
        },
        ..Default::default()
    };
    load_console(
        &Rc::new(RefCell::new(AppContext::new_with_config(config))),
        rom_path,
    )
}

/// Write the trace `mode` asks for, for the console's loaded game, to `out`.
pub fn run_trace(
    console: &mut Console,
    mode: &TraceMode,
    out: &mut impl Write,
) -> Result<(), String> {
    match console {
        Console::Nes(nes) => trace(nes.as_mut(), mode, out),
        Console::Snes(snes) => trace(snes.as_mut(), mode, out),
        Console::GameBoy(_) | Console::GameBoyAdvance(_) => {
            Err("timing traces support NES and SNES only".to_string())
        }
    }
}

pub(crate) fn trace<S: TraceStepper + ?Sized>(
    stepper: &mut S,
    mode: &TraceMode,
    out: &mut impl Write,
) -> Result<(), String> {
    let target = match *mode {
        TraceMode::NmiClock { nmis } => nmis,
        TraceMode::Exec { to_nmi, .. } => to_nmi,
    };
    let mut logged_nmis = 0u64;
    let mut last_nmis = stepper.nmis_taken();
    let mut ticks_since_entry = 0u64;
    loop {
        let (pc, clk, nmis) = (
            stepper.trace_pc(),
            stepper.trace_clock(),
            stepper.nmis_taken(),
        );
        if nmis != last_nmis {
            last_nmis = nmis;
            ticks_since_entry = 0;
        }
        let done = match *mode {
            TraceMode::NmiClock { nmis: wanted } => logged_nmis >= wanted,
            TraceMode::Exec { to_nmi, .. } => nmis >= to_nmi,
        };
        if done {
            return Ok(());
        }
        if ticks_since_entry >= MAX_TICKS_BETWEEN_LINES {
            return Err(stall_message(target, nmis));
        }

        let executed_before = stepper.instructions_executed();
        stepper.run_tick();
        ticks_since_entry += 1;
        if stepper.instructions_executed() == executed_before {
            continue;
        }
        let line = match *mode {
            TraceMode::NmiClock { .. } if nmis > logged_nmis => {
                logged_nmis = nmis;
                format!("nmi={nmis} pc={pc:06X} clk={clk}")
            }
            TraceMode::Exec { from_nmi, .. } if nmis >= from_nmi => {
                format!("pc={pc:06X} clk={clk}")
            }
            _ => continue,
        };
        writeln!(out, "{line}").map_err(|err| format!("cannot write the trace: {err}"))?;
    }
}

fn stall_message(wanted: u64, seen: u64) -> String {
    format!(
        "no NMI entry within {MAX_TICKS_BETWEEN_LINES} ticks; the trace wanted entry {wanted} and saw {seen}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One point a scripted CPU passes through: the state a sample sees, and whether the
    /// tick from it executes an instruction.
    #[derive(Clone, Copy)]
    struct Point {
        pc: u32,
        clk: u64,
        nmis: u64,
        executes: bool,
    }

    fn at(pc: u32, clk: u64, nmis: u64) -> Point {
        Point {
            pc,
            clk,
            nmis,
            executes: true,
        }
    }

    fn idle(pc: u32, clk: u64, nmis: u64) -> Point {
        Point {
            executes: false,
            ..at(pc, clk, nmis)
        }
    }

    /// A CPU that walks through `points`, then repeats the last one forever without
    /// executing (so a trace that wants more stalls rather than panics).
    struct Scripted {
        points: Vec<Point>,
        index: usize,
        executed: u64,
    }

    impl Scripted {
        fn new(points: Vec<Point>) -> Self {
            Self {
                points,
                index: 0,
                executed: 0,
            }
        }

        fn now(&self) -> Point {
            self.points[self.index.min(self.points.len() - 1)]
        }
    }

    impl TimingProbe for Scripted {
        fn trace_pc(&self) -> u32 {
            self.now().pc
        }
        fn trace_clock(&self) -> u64 {
            self.now().clk
        }
        fn nmis_taken(&self) -> u64 {
            self.now().nmis
        }
        fn instructions_executed(&self) -> u64 {
            self.executed
        }
    }

    impl TraceStepper for Scripted {
        fn run_tick(&mut self) -> u8 {
            if self.index < self.points.len() {
                if self.points[self.index].executes {
                    self.executed += 1;
                }
                self.index += 1;
            }
            1
        }
    }

    fn run(points: Vec<Point>, mode: TraceMode) -> Result<String, String> {
        let mut cpu = Scripted::new(points);
        let mut out = Vec::new();
        trace(&mut cpu, &mode, &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn nmi_clock_logs_the_first_instruction_after_each_entry() {
        let points = vec![
            at(0x8000, 7, 0),
            at(0x8001, 9, 0),
            at(0x9000, 30, 1), // entry 1: the handler's first instruction
            at(0x9001, 32, 1),
            at(0x8001, 40, 1),
            at(0x9000, 70, 2), // entry 2
        ];
        assert_eq!(
            run(points, TraceMode::NmiClock { nmis: 2 }).unwrap(),
            "nmi=1 pc=009000 clk=30\nnmi=2 pc=009000 clk=70\n"
        );
    }

    #[test]
    fn a_sample_whose_tick_executes_nothing_is_skipped() {
        // SNES: a dispatch-only step leaves the counter at its old value and the PC on an
        // instruction that does not run; WAI keeps the PC still for several ticks. Only the
        // sample before an executing tick may be stamped.
        let points = vec![
            at(0x808000, 100, 0),
            idle(0x808001, 106, 0), // dispatch-only step
            idle(0x009000, 190, 1), // e.g. a DMA tick before the handler's first fetch
            at(0x009000, 220, 1),
            at(0x009002, 236, 1),
        ];
        assert_eq!(
            run(points.clone(), TraceMode::NmiClock { nmis: 1 }).unwrap(),
            "nmi=1 pc=009000 clk=220\n"
        );
        assert_eq!(
            run(
                points,
                TraceMode::Exec {
                    from_nmi: 0,
                    to_nmi: 5
                }
            )
            .unwrap_err(),
            "no NMI entry within 10000000 ticks; the trace wanted entry 5 and saw 1"
        );
    }

    #[test]
    fn exec_writes_every_instruction_from_one_entry_up_to_the_next() {
        let points = vec![
            at(0x8000, 7, 0),
            at(0x9000, 30, 1),
            at(0x9001, 32, 1),
            at(0x9001, 35, 1), // a self-loop executes the same PC again: both lines stay
            at(0x8001, 40, 1),
            at(0x9000, 70, 2),
            at(0x9001, 72, 2),
        ];
        assert_eq!(
            run(
                points,
                TraceMode::Exec {
                    from_nmi: 1,
                    to_nmi: 2
                }
            )
            .unwrap(),
            "pc=009000 clk=30\npc=009001 clk=32\npc=009001 clk=35\npc=008001 clk=40\n"
        );
    }

    #[test]
    fn exec_from_entry_zero_starts_at_power_on() {
        let points = vec![at(0x8000, 7, 0), at(0x8001, 9, 0), at(0x9000, 30, 1)];
        assert_eq!(
            run(
                points,
                TraceMode::Exec {
                    from_nmi: 0,
                    to_nmi: 1
                }
            )
            .unwrap(),
            "pc=008000 clk=7\npc=008001 clk=9\n"
        );
    }

    #[test]
    fn a_rom_that_never_takes_an_nmi_fails_instead_of_spinning() {
        let points = vec![at(0x8000, 7, 0)];
        assert_eq!(
            run(points, TraceMode::NmiClock { nmis: 1 }).unwrap_err(),
            "no NMI entry within 10000000 ticks; the trace wanted entry 1 and saw 0"
        );
    }

    #[test]
    fn a_game_boy_is_refused() {
        let context = crate::platform::app_context::AppContext::new_with_config(
            crate::nes::console::Config::default(),
        );
        let mut console = Console::new_gameboy(std::rc::Rc::new(std::cell::RefCell::new(context)));
        let mut out = Vec::new();
        assert_eq!(
            run_trace(&mut console, &TraceMode::NmiClock { nmis: 1 }, &mut out).unwrap_err(),
            "timing traces support NES and SNES only"
        );
    }

    fn parse(items: &[&str]) -> Result<TraceArgs, String> {
        parse_args(&items.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parse_nmi_defaults_to_a_minute_of_entries() {
        assert_eq!(
            parse(&["nmi", "game.sfc", "--out", "n.txt"]).unwrap(),
            TraceArgs {
                rom_path: "game.sfc".to_string(),
                out: "n.txt".to_string(),
                mode: TraceMode::NmiClock { nmis: DEFAULT_NMIS }
            }
        );
        assert_eq!(
            parse(&["nmi", "game.nes", "--nmis", "50", "--out", "n.txt"])
                .unwrap()
                .mode,
            TraceMode::NmiClock { nmis: 50 }
        );
    }

    #[test]
    fn parse_exec_defaults_to_the_one_frame_after_from() {
        assert_eq!(
            parse(&["exec", "game.nes", "--from-nmi", "41", "--out", "e.txt"])
                .unwrap()
                .mode,
            TraceMode::Exec {
                from_nmi: 41,
                to_nmi: 42
            }
        );
        assert_eq!(
            parse(&[
                "exec",
                "g.nes",
                "--out",
                "e.txt",
                "--to-nmi",
                "9",
                "--from-nmi",
                "7"
            ])
            .unwrap()
            .mode,
            TraceMode::Exec {
                from_nmi: 7,
                to_nmi: 9
            }
        );
    }

    #[test]
    fn parse_rejects_what_cannot_be_run() {
        let o = ["--out", "t.txt"];
        let with_out = |items: &[&'static str]| [items, &o[..]].concat();
        for (items, problem) in [
            (vec![], USAGE.to_string()),
            (vec!["nmi"], USAGE.to_string()),
            (vec!["nmi", "g.nes"], format!("needs --out <path>\n{USAGE}")),
            (
                vec!["nmi", "g.nes", "--out"],
                format!("--out needs a path\n{USAGE}"),
            ),
            (
                with_out(&["frames", "g.nes"]),
                format!("unknown trace 'frames'\n{USAGE}"),
            ),
            (
                with_out(&["exec", "g.nes"]),
                format!("exec needs --from-nmi\n{USAGE}"),
            ),
            (
                vec!["nmi", "g.nes", "--out", "t.txt", "--nmis"],
                format!("--nmis needs a number\n{USAGE}"),
            ),
            (
                with_out(&["nmi", "g.nes", "--nmis", "x"]),
                format!("--nmis needs a number\n{USAGE}"),
            ),
            (
                with_out(&["nmi", "g.nes", "--nmis", "0"]),
                format!("--nmis must be at least 1\n{USAGE}"),
            ),
            (
                with_out(&["nmi", "g.nes", "--from-nmi", "1"]),
                format!("unknown option '--from-nmi'\n{USAGE}"),
            ),
            (
                with_out(&["exec", "g.nes", "--from-nmi", "5", "--to-nmi", "5"]),
                format!("--to-nmi must be after --from-nmi\n{USAGE}"),
            ),
            (
                with_out(&["exec", "g.nes", "--from-nmi", "1", "extra"]),
                format!("unknown option 'extra'\n{USAGE}"),
            ),
        ] {
            assert_eq!(parse(&items).unwrap_err(), problem, "{items:?}");
        }
    }

    fn nmi_log(rom: &str, nmis: u64) -> String {
        let mut console = load_for_trace(rom).unwrap();
        let mut out = Vec::new();
        run_trace(&mut console, &TraceMode::NmiClock { nmis }, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_real_nes_rom_logs_its_nmi_entries_at_mesen2s_clocks() {
        // Mesen2 2.1.1's mesen2_nmi_clock.lua on the same ROM wrote exactly these lines
        // (2026-10-04); NESER's NES CPU cycle count starts where Mesen2's masterClock does.
        assert_eq!(
            nmi_log("roms/nes/automated_tests/nmi_sync/demo_ntsc.nes", 3),
            "nmi=1 pc=008100 clk=652795\nnmi=2 pc=008100 clk=682575\nnmi=3 pc=008100 clk=712358\n"
        );
    }

    #[test]
    fn a_real_snes_rom_logs_its_nmi_entries_six_clocks_after_mesen2s() {
        // Mesen2 2.1.1 wrote clk=2451178, 2808542 and 3165908 for these entries (2026-10-04):
        // a constant 6 master clocks, where each emulator starts its clock, which the diff
        // takes as its baseline. A change in that offset is a timing change to look at.
        assert_eq!(
            nmi_log(
                "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/window-precalculated-single.sfc",
                3
            ),
            "nmi=1 pc=0087A4 clk=2451184\nnmi=2 pc=0087A4 clk=2808548\nnmi=3 pc=0087A4 clk=3165914\n"
        );
    }
}
