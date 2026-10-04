"""Compare a NES or SNES ROM between NESER and Mesen2 at given frames (nr-ocx).

    python -m scripts.reference_capture.compare_mesen2 <rom> --frames 120 600 [--out-dir DIR]

This command is the Mesen2 comparison recipe; sweep beads and the README name it rather than
copying its flags, so a confound is fixed here once. For each frame it runs Mesen2's
testRunner with ``mesen2_capture.lua`` and ``neser --headless``, writes ``mesen2-<N>.png`` and
``neser-<N>.png`` into the output directory and prints the number of differing pixels. Before
the frames it prints what each emulator decided about the ROM: Mesen2's ``[iNes]``/``[DB]``
lines (or its SNES header block) beside NESER's ``Loaded rom ... mapper=`` and ``Hardware:``
lines. Exit status: 0 every frame matches, 1 a frame differs, 2 a capture failed.

The confounds it removes, each paid for by an earlier comparison:

* Battery saves (nr-kds, nr-nuf, nr-7v3). Both emulators load a save by ROM file name, and
  Mesen2 keeps its saves in ``<Mesen2 home>/Saves``, not beside the ROM. Every invocation of
  either emulator runs on its own copy of the ROM, named uniquely per invocation, and Mesen2's
  ``Saves``/``RecentGames``/``SaveStates`` entries for that copy are removed afterwards.
* The game database (nr-nwy, nr-sjt, nr-1le). Mesen2 silently replaces the header with its
  database entry, and misses the database for files with trailing data. Its decision is
  printed; ``--no-game-database`` makes it use the header (NES only).
* Controller ports (nr-0an). Mesen2 otherwise takes them from its ``settings.json``; both
  ports are pinned to a standard pad, and NESER runs with an empty ``--config`` so a local
  ``neser.conf`` cannot change its ports or palette.
* Region (nr-f6o). NESER's ``Hardware:`` line, printed with every comparison, names the region
  it picked.

Mesen2 needs ``"AllowIoOsAccess": true`` in its ``settings.json`` (see README.md); without it
the capture script prints one ``ERROR:`` line, which this command passes on.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from scripts.diff_screenshots import diff_pixels, load_screenshot

REPO = Path(__file__).resolve().parent.parent.parent
CAPTURE_SCRIPT = Path(__file__).resolve().parent / "mesen2_capture.lua"
DEFAULT_MESEN2 = "/Applications/Mesen.app/Contents/MacOS/Mesen"
DEFAULT_NESER = str(REPO / "target" / "release" / "neser")
DEFAULT_MESEN2_HOME = str(Path.home() / "Library" / "Application Support" / "Mesen2")

COMMON_MESEN2_FLAGS = [
    "--testRunner",
    "--enableStdout",
    "--timeout=30",
    "--Video.VideoFilter=None",
    "--Video.AspectRatio=NoStretching",
]
# Frame skipping off is mandatory for animated content (testRunner otherwise renders every
# other frame); zero RAM matches NESER's --headless; a standard pad in each port matches
# NESER's defaults (nr-0an).
NES_MESEN2_FLAGS = [
    "--nes.DisableFrameSkipping=true",
    "--nes.RamPowerOnState=AllZeros",
    "--nes.port1.type=NesController",
    "--nes.port2.type=NesController",
]
SNES_MESEN2_FLAGS = [
    "--snes.disableFrameSkipping=true",
    "--snes.RamPowerOnState=AllZeros",
    "--snes.port1.type=SnesController",
    "--snes.port2.type=SnesController",
]
NO_GAME_DATABASE_FLAG = "--nes.DisableGameDatabase=true"
# Mesen2's colours; without them NES whites differ.
NESER_FLAGS = {"nes": ["--nes-palette", "mesen"], "snes": []}
SYSTEMS = {".nes": "nes", ".sfc": "snes", ".smc": "snes"}
# Mesen2 keeps per-game files under its home, named after the ROM file.
MESEN2_GAME_DIRS = ("Saves", "RecentGames", "SaveStates")


class CaptureFailed(Exception):
    """One emulator produced no screenshot; the message says what it printed."""


def system_for(rom: Path) -> str:
    try:
        return SYSTEMS[rom.suffix.lower()]
    except KeyError:
        raise ValueError(f"{rom.name}: only .nes, .sfc and .smc ROMs are compared against Mesen2") from None


def mesen2_flags(system: str, no_game_database: bool) -> list[str]:
    if system == "snes":
        return list(SNES_MESEN2_FLAGS)
    return [*NES_MESEN2_FLAGS, NO_GAME_DATABASE_FLAG] if no_game_database else list(NES_MESEN2_FLAGS)


def merge_flags(pinned: list[str], extra: list[str]) -> list[str]:
    """``pinned`` with ``extra`` appended; an extra ``--key=value`` replaces the pinned flag with that key."""
    keys = {flag.split("=", 1)[0] for flag in extra if "=" in flag}
    return [*(flag for flag in pinned if flag.split("=", 1)[0] not in keys), *extra]


def fresh_copy(rom: Path, directory: Path) -> Path:
    """Copy ``rom`` into ``directory`` under a name no other invocation uses."""
    copy = directory / f"{rom.stem}-{os.getpid()}-{time.time_ns()}{rom.suffix}"
    shutil.copyfile(rom, copy)
    return copy


def remove_mesen2_leftovers(mesen2_home: Path, copy: Path) -> None:
    """Delete what Mesen2 stored under its home for this copy's name, and nothing else."""
    for sub in MESEN2_GAME_DIRS:
        directory = mesen2_home / sub
        if not directory.is_dir():
            continue
        for path in directory.iterdir():
            if path.stem == copy.stem or path.name.startswith(copy.stem + "_"):
                path.unlink()


def mesen2_cartridge_lines(stdout: str) -> list[str]:
    """Mesen2's account of the ROM: header and database lines (NES), header block (SNES)."""
    lines = stdout.splitlines()
    separators = [i for i, line in enumerate(lines) if line.startswith("---")]
    if len(separators) >= 2 and any(line.startswith("Game: ") for line in lines[separators[0] : separators[1]]):
        block = lines[separators[0] + 1 : separators[1]]
        return [line for line in block if not line.startswith("File: ")]
    return [
        line
        for line in lines
        if line.startswith("PRG+CHR CRC32")
        or (line.startswith("[DB]") and not line.startswith("[DB] Initialized"))
        or line.startswith("[iNes]")
        or line.startswith("[NES")
    ]


def neser_cartridge_lines(output: str) -> list[str]:
    """NESER's account of the ROM: the mapper it loaded and the hardware (region) it picked."""
    return [line for line in output.splitlines() if line.startswith(("Loaded rom with", "Hardware:"))]


def testrunner_pattern(mesen2: Path) -> str:
    """The ``pgrep -f`` pattern for any testRunner of this binary, however it was started."""
    return f"{re.escape(mesen2.name)} --testRunner"


def wait_for_other_mesen2(mesen2: Path) -> None:
    """A concurrent testRunner makes Mesen2 exit 0 with no output (snes-hardware-research)."""
    deadline = time.monotonic() + 300
    pattern = testrunner_pattern(mesen2)
    while subprocess.run(["pgrep", "-f", pattern], capture_output=True).returncode == 0:
        if time.monotonic() > deadline:
            raise TimeoutError(f"another {pattern} is still running")
        time.sleep(2)


def capture_mesen2(
    rom: Path, frame: int, out: Path, mesen2: Path, mesen2_home: Path, flags: list[str], work: Path
) -> str:
    """Run Mesen2 once on a fresh copy of ``rom``, saving frame ``frame`` to ``out``; return stdout."""
    out.unlink(missing_ok=True)  # a rerun into the same --out-dir must not diff the last run's file
    copy = fresh_copy(rom, work)
    env = {**os.environ, "CAPTURE_FRAME": str(frame), "CAPTURE_OUT": str(out.resolve())}
    try:
        if shutil.which("pgrep"):
            wait_for_other_mesen2(mesen2)
        cmd = [str(mesen2), *COMMON_MESEN2_FLAGS, *flags, str(copy), str(CAPTURE_SCRIPT)]
        run = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=120)
    finally:
        remove_mesen2_leftovers(mesen2_home, copy)
        copy.unlink(missing_ok=True)
    if not out.is_file():
        errors = [line for line in run.stdout.splitlines() if line.startswith("ERROR")]
        raise CaptureFailed("Mesen2 saved no screenshot: " + ("\n".join(errors) or run.stdout[-2000:]))
    return run.stdout


def capture_neser(rom: Path, frame: int, out: Path, neser: Path, flags: list[str], work: Path) -> str:
    """Run NESER once on a fresh copy of ``rom`` with an empty config; return its output."""
    out.unlink(missing_ok=True)
    copy = fresh_copy(rom, work)
    config = work / f"empty-{copy.stem}.conf"
    config.write_text("")
    try:
        cmd = [str(neser), "--config", str(config), "--headless", "--frames", str(frame)]
        cmd += ["--output", str(out), *flags, str(copy)]
        run = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    finally:
        copy.unlink(missing_ok=True)
        copy.with_suffix(".sav").unlink(missing_ok=True)
        config.unlink(missing_ok=True)
    if run.returncode != 0 or not out.is_file():
        raise CaptureFailed(f"NESER saved no screenshot (exit {run.returncode}): {(run.stdout + run.stderr)[-2000:]}")
    return run.stdout + run.stderr


def compare(rom: Path, frames: list[int], out_dir: Path, args: argparse.Namespace) -> int:
    system = system_for(rom)
    flags_m = merge_flags(mesen2_flags(system, args.no_game_database), args.mesen2_arg)
    flags_n = [*NESER_FLAGS[system], *args.neser_arg]
    print(f"ROM: {rom}")
    print(f"Output: {out_dir}")
    worst = 0
    with tempfile.TemporaryDirectory(prefix="compare-mesen2-") as tmp:
        work = Path(tmp)
        for index, frame in enumerate(frames):
            mesen2_png, neser_png = out_dir / f"mesen2-{frame}.png", out_dir / f"neser-{frame}.png"
            try:
                m_out = capture_mesen2(rom, frame, mesen2_png, args.mesen2_bin, args.mesen2_home, flags_m, work)
                n_out = capture_neser(rom, frame, neser_png, args.neser_bin, flags_n, work)
            except (CaptureFailed, OSError, subprocess.TimeoutExpired, TimeoutError) as failure:
                # Exit 1 means "a frame differs"; a run that never captured must not read as one.
                print(f"frame {frame}: capture failed: {failure}")
                return 2
            if index == 0:
                m_lines = mesen2_cartridge_lines(m_out) or ["(no cartridge lines printed)"]
                n_lines = neser_cartridge_lines(n_out) or ["(no cartridge lines printed)"]
                for line in m_lines:
                    print(f"Mesen2: {line}")
                for line in n_lines:
                    print(f"NESER:  {line}")
            neser_shot, mesen2_shot = load_screenshot(neser_png), load_screenshot(mesen2_png)
            if (neser_shot.width, neser_shot.height) != (mesen2_shot.width, mesen2_shot.height):
                sizes = f"NESER {neser_shot.width}x{neser_shot.height}, Mesen2 {mesen2_shot.width}x{mesen2_shot.height}"
                print(f"frame {frame}: sizes differ ({sizes})")
                worst = 1
                continue
            differing = diff_pixels(neser_shot, mesen2_shot)
            print(f"frame {frame}: {differing} differing pixels")
            worst = max(worst, 1 if differing else 0)
    return worst


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="python -m scripts.reference_capture.compare_mesen2",
        description="Compare a NES or SNES ROM between NESER and Mesen2 at given frames.",
    )
    parser.add_argument("rom", type=Path)
    parser.add_argument("--frames", type=int, nargs="+", required=True, help="frames to compare, from power-on")
    parser.add_argument("--out-dir", type=Path, help="where the PNGs go (default: a new temporary directory)")
    parser.add_argument(
        "--no-game-database",
        action="store_true",
        help="make Mesen2 use the iNES header instead of its game database (NES)",
    )
    parser.add_argument("--neser-arg", action="append", default=[], help="extra NESER argument (repeatable)")
    parser.add_argument("--mesen2-arg", action="append", default=[], help="extra Mesen2 argument (repeatable)")
    parser.add_argument("--neser-bin", type=Path, default=Path(os.environ.get("NESER_BIN", DEFAULT_NESER)))
    parser.add_argument("--mesen2-bin", type=Path, default=Path(os.environ.get("MESEN2_BIN", DEFAULT_MESEN2)))
    parser.add_argument("--mesen2-home", type=Path, default=Path(os.environ.get("MESEN2_HOME", DEFAULT_MESEN2_HOME)))
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        system_for(args.rom)
    except ValueError as error:
        print(error)
        return 2
    out_dir = args.out_dir or Path(tempfile.mkdtemp(prefix="mesen2-comparison-"))
    out_dir.mkdir(parents=True, exist_ok=True)
    return compare(args.rom, args.frames, out_dir, args)


if __name__ == "__main__":
    sys.exit(main())
