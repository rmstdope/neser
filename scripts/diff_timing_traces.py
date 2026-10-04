"""Diff a NESER timing trace against Mesen2's and name the first divergent line.

The traces come from `timing_trace` (NESER) and `scripts/reference_capture/mesen2_nmi_clock.lua`
or `mesen2_exec_trace.lua` (Mesen2): one line per NMI entry (``nmi=<n> pc=<hex> clk=<dec>``) or
per instruction (``pc=<hex> clk=<dec>``). Lines are compared by ordinal. Each emulator starts
its clock at its own point, so the clock offset of the first pair is the baseline. The first
divergence is the first line where the PC (or NMI number) differs, or where the offset leaves
the baseline and stays off it.

In an exec trace, an offset that leaves the baseline for one line and comes straight back
is a *stamp difference*, not a divergence; in an NMI log a line is a frame, so there it is
one. A stall that falls on an instruction boundary (NES OAM DMA, SNES DRAM refresh) can be
charged by one emulator to the instruction before the boundary and
by the other to the one after it. The totals still agree. These lines are listed, never
failed. See scripts/reference_capture/README.md, "Tracing against Mesen2".

Usage::

    python -m scripts.diff_timing_traces neser.txt mesen.txt [--context 5] [--baseline N]

The baseline is the first pair's offset unless ``--baseline`` gives it, so a drift before
the first line is invisible without it.

Exit status: 0 when the traces match, 1 at a divergence, 2 when a file cannot be read or
holds no trace lines.
Lines that are not trace lines are ignored.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import NamedTuple

_LINE = re.compile(r"^(?:nmi=(\d+) )?pc=([0-9A-Fa-f]+) clk=(\d+)\s*$")


class Line(NamedTuple):
    """One trace line: the instruction's address, its clock, and the NMI number if any."""

    pc: int
    clk: int
    nmi: int | None


def parse_lines(lines: Iterable[str]) -> list[Line]:
    """Parse every trace line, dropping anything else (load messages, blank lines)."""
    parsed = []
    for line in lines:
        match = _LINE.match(line.strip())
        if match:
            nmi = int(match.group(1)) if match.group(1) is not None else None
            parsed.append(Line(int(match.group(2), 16), int(match.group(3)), nmi))
    return parsed


@dataclass
class DiffResult:
    """The baseline offset, the first divergence (index, why) if any, and the stamp differences."""

    baseline: int
    compared: int = 0
    divergence: int | None = None
    reason: str = ""
    stamp_differences: list[int] = field(default_factory=list)


def diff_traces(a: list[Line], b: list[Line], baseline: int | None = None) -> DiffResult:
    """Compare two non-empty traces line by line.

    ``baseline`` is the expected clock offset; by default the first pair's. Pass it when the
    trace may start after a drift: 0 for the NES, or the offset of an ``exec --from-nmi 0``
    trace for the SNES.
    """
    result = DiffResult(baseline=a[0].clk - b[0].clk if baseline is None else baseline)
    for index, (left, right) in enumerate(zip(a, b, strict=False)):
        result.compared = index + 1
        if left.nmi != right.nmi:
            return _diverge(result, index, f"nmi {left.nmi} vs {right.nmi}")
        if left.pc != right.pc:
            return _diverge(result, index, f"pc {left.pc:06X} vs {right.pc:06X}")
        offset = left.clk - right.clk
        if offset == result.baseline:
            continue
        # One NMI-log line is a whole frame, so an excursion there is a real late entry.
        following = index + 1
        is_exec = left.nmi is None
        if is_exec and following < min(len(a), len(b)) and a[following].clk - b[following].clk == result.baseline:
            result.stamp_differences.append(index)
            continue
        return _diverge(result, index, f"clock offset {result.baseline} -> {offset}")
    if len(a) != len(b):
        shorter = "second" if len(a) > len(b) else "first"
        return _diverge(result, min(len(a), len(b)), f"the {shorter} trace ends here")
    return result


def _diverge(result: DiffResult, index: int, reason: str) -> DiffResult:
    result.divergence = index
    result.reason = reason
    return result


def _format(line: Line | None) -> str:
    if line is None:
        return "<end of trace>"
    nmi = f"nmi={line.nmi} " if line.nmi is not None else ""
    return f"{nmi}pc={line.pc:06X} clk={line.clk}"


def format_report(a: list[Line], b: list[Line], result: DiffResult, context: int) -> str:
    """A one-line verdict, and at a divergence both traces side by side around it."""
    stamps = ""
    if result.stamp_differences:
        shown = ", ".join(str(index + 1) for index in result.stamp_differences[:10])
        more = " ..." if len(result.stamp_differences) > 10 else ""
        stamps = f" ({len(result.stamp_differences)} one-line stamp differences: lines {shown}{more})"
    if result.divergence is None:
        return f"match: {result.compared} lines, clock offset {result.baseline} throughout{stamps}"

    at = result.divergence
    lines = [f"first divergence at line {at + 1}: {result.reason}{stamps}"]
    for index in range(max(0, at - context), min(max(len(a), len(b)), at + context + 1)):
        left = a[index] if index < len(a) else None
        right = b[index] if index < len(b) else None
        offset = f"offset {left.clk - right.clk}" if left and right else ""
        marker = ">" if index == at else " "
        lines.append(f"{marker} {_format(left)} | {_format(right)} | {offset}".rstrip(" |"))
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("first", help="NESER's trace (timing_trace --out)")
    parser.add_argument("second", help="Mesen2's trace (TRACE_OUT)")
    parser.add_argument("--context", type=int, default=5, help="lines shown around a divergence")
    parser.add_argument(
        "--baseline",
        type=int,
        help="expected clock offset (default: the first pair's); 0 on the NES, the "
        "offset of an `exec --from-nmi 0` trace on the SNES",
    )
    args = parser.parse_args(argv)

    traces = []
    for path in (args.first, args.second):
        try:
            with open(path, encoding="utf-8") as handle:
                lines = parse_lines(handle)
        except (OSError, UnicodeDecodeError) as error:
            print(f"cannot read {path}: {error}", file=sys.stderr)
            return 2
        if not lines:
            print(f"{path} has no trace lines", file=sys.stderr)
            return 2
        traces.append(lines)

    result = diff_traces(traces[0], traces[1], args.baseline)
    print(format_report(traces[0], traces[1], result, args.context))
    return 0 if result.divergence is None else 1


if __name__ == "__main__":
    sys.exit(main())
