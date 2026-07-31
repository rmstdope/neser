"""Diff two SNES CPU bus traces by event ordinal and localise the first divergence.

NESER and a reference emulator (Mesen2, with the `NeserBusLog` hook) each emit one line per
CPU bus cycle, stamped with the master clock at the instant the cycle's data moves. Comparing
those two streams *by clock* is useless once they drift; comparing them **by ordinal** is not:
the Nth CPU cycle in one emulator is the Nth CPU cycle in the other for as long as both run
the same code, so the per-ordinal clock offset stays constant while they agree and steps at
the exact cycle where one emulator charges a different number of master clocks.

That step is the answer -- it names the instruction whose cycle accounting is wrong. A
histogram with a single bucket means clock-exact (any constant offset is just a difference in
where each emulator zeroes its clock); a histogram that steps by a constant at one ordinal
localises the divergence to that cycle.

Usage::

    python -m scripts.diff_bus_traces neser.log mesen.log [--context 12]

Both files may contain unrelated lines (NESER `exec`/`[PPU]` traces, Mesen2 stdout); anything
without a recognised clock stamp is ignored.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from dataclasses import dataclass, field
from typing import Iterable, NamedTuple, Optional


class Event(NamedTuple):
    """One CPU bus cycle: what it did, where, and when."""

    kind: str  # "read" | "write" | "idle"
    addr: Optional[int]  # None for idle cycles
    clk: int


# NESER, from `trace_cpu!(2; ...)` in Cpu::trace_bus_cycle.
_NESER_READ = re.compile(r"^\[CPU\]\s+read\s+\$([0-9A-Fa-f]+)\s+clk=(\d+)")
_NESER_WRITE = re.compile(r"^\[CPU\]\s+write\s+\$([0-9A-Fa-f]+)\s*=\s*\$[0-9A-Fa-f]+\s+clk=(\d+)")
_NESER_IDLE = re.compile(r"^\[CPU\]\s+internal\s+clk=(\d+)")

# Mesen2, from Core/SNES/NeserBusLog.h.
_MESEN = re.compile(r"^mesen\s+(busR|busW|idle)\s+(\S+)\s+ticks=(\d+)")

_MESEN_KINDS = {"busR": "read", "busW": "write", "idle": "idle"}


def parse_line(line: str) -> Optional[Event]:
    """Normalise one log line to an :class:`Event`, or ``None`` if it is not a bus cycle."""
    match = _NESER_READ.match(line)
    if match:
        return Event("read", int(match.group(1), 16), int(match.group(2)))

    match = _NESER_WRITE.match(line)
    if match:
        return Event("write", int(match.group(1), 16), int(match.group(2)))

    match = _NESER_IDLE.match(line)
    if match:
        return Event("idle", None, int(match.group(1)))

    match = _MESEN.match(line)
    if match:
        kind = _MESEN_KINDS[match.group(1)]
        addr = None if kind == "idle" else int(match.group(2), 16)
        return Event(kind, addr, int(match.group(3)))

    return None


def parse_lines(lines: Iterable[str]) -> list[Event]:
    """Normalise a whole log, dropping every line that is not a bus cycle."""
    return [event for event in (parse_line(line) for line in lines) if event is not None]


#: How many leading events either trace may have to give up to line the two up, and how many
#: ordinals a candidate shift is scored over. A clock window opens on whichever cycle each
#: emulator happens to be on, so a handful of edge events is normal; anything larger than
#: this is a real divergence, not a windowing artefact.
MAX_ALIGNMENT_SHIFT = 64
ALIGNMENT_SCORE_DEPTH = 200


def best_alignment(a: list[Event], b: list[Event]) -> tuple[int, int]:
    """Return the ``(start_a, start_b)`` that best lines two traces up at their leading edge.

    A clock window never opens on the same cycle in both emulators -- whichever one is a few
    clocks ahead catches an extra event or two at the edge. Comparing from ordinal 0 blindly
    would then report every single ordinal as divergent. Candidate shifts are scored by how
    many of the next :data:`ALIGNMENT_SCORE_DEPTH` cycles agree in *shape* (kind and address),
    which is unaffected by the clock drift we are actually hunting for. Ties prefer the
    smallest shift, and a score of zero everywhere falls back to no shift at all.
    """
    best_score = -1
    best = (0, 0)
    for shift in range(MAX_ALIGNMENT_SHIFT + 1):
        for start_a, start_b in ((shift, 0), (0, shift)):
            depth = min(ALIGNMENT_SCORE_DEPTH, len(a) - start_a, len(b) - start_b)
            if depth <= 0:
                continue
            score = sum(
                1
                for i in range(depth)
                if a[start_a + i].kind == b[start_b + i].kind
                and a[start_a + i].addr == b[start_b + i].addr
            )
            if score > best_score:
                best_score, best = score, (start_a, start_b)
    return best if best_score > 0 else (0, 0)


@dataclass
class DiffResult:
    """The outcome of an ordinal-aligned comparison of two traces."""

    #: Lengths of the traces as handed in, before any alignment trim.
    length_a: int
    length_b: int
    compared: int
    alignment: tuple[int, int] = (0, 0)
    offset_histogram: Counter = field(default_factory=Counter)
    first_offset_change: Optional[int] = None
    first_shape_mismatch: Optional[int] = None

    @property
    def clock_exact(self) -> bool:
        """True when the two traces ran the same cycles and never drifted apart.

        Both halves matter. A single offset bucket alone is not enough: if the shapes
        disagree the two runs were not executing the same cycles at all, and a uniform delta
        across a long run of same-speed accesses (the likeliest symptom of `best_alignment`
        settling on a bogus shift) would otherwise be reported as a clean all-clear.
        """
        return (
            self.compared > 0
            and len(self.offset_histogram) == 1
            and self.first_shape_mismatch is None
        )


def diff_traces(a: list[Event], b: list[Event]) -> DiffResult:
    """Align two traces by ordinal and locate the first clock and shape divergence.

    The offset baseline is ordinal 0's offset, so the two emulators need not agree on where
    their master clocks start -- only on how many clocks elapse between cycles.
    """
    start_a, start_b = best_alignment(a, b)
    length_a, length_b = len(a), len(b)
    a = a[start_a:]
    b = b[start_b:]
    compared = min(len(a), len(b))
    result = DiffResult(
        length_a=length_a,
        length_b=length_b,
        compared=compared,
        alignment=(start_a, start_b),
    )
    if compared == 0:
        return result

    baseline = b[0].clk - a[0].clk
    for ordinal in range(compared):
        offset = b[ordinal].clk - a[ordinal].clk
        result.offset_histogram[offset] += 1
        if offset != baseline and result.first_offset_change is None:
            result.first_offset_change = ordinal
        same_shape = a[ordinal].kind == b[ordinal].kind and a[ordinal].addr == b[ordinal].addr
        if not same_shape and result.first_shape_mismatch is None:
            result.first_shape_mismatch = ordinal
    return result


def _format_event(event: Optional[Event]) -> str:
    if event is None:
        return "<end of trace>"
    where = "------" if event.addr is None else f"{event.addr:06X}"
    return f"{event.kind:<5} {where} clk={event.clk}"


def format_report(a: list[Event], b: list[Event], result: DiffResult, context: int) -> str:
    """Render a human-readable report, with both traces side by side around the divergence."""
    start_a, start_b = result.alignment
    a = a[start_a:]
    b = b[start_b:]
    lines = [
        f"trace A: {result.length_a} cycles",
        f"trace B: {result.length_b} cycles",
        f"compared: {result.compared} ordinals"
        + (f" (dropped {start_a} leading A / {start_b} leading B to align)"
           if (start_a or start_b) else ""),
        "",
        "clock-offset histogram (B - A), most common first:",
    ]
    for offset, count in result.offset_histogram.most_common():
        lines.append(f"  {offset:+d} x{count}")
    lines.append("")

    if result.clock_exact:
        lines.append("CLOCK-EXACT: a single offset bucket, the traces never drift.")
    elif result.first_offset_change is None:
        lines.append("No compared ordinals.")
    else:
        lines.append(f"first clock divergence at ordinal {result.first_offset_change}")
    if result.first_shape_mismatch is not None:
        lines.append(f"first shape mismatch at ordinal {result.first_shape_mismatch}")

    anchors = [x for x in (result.first_offset_change, result.first_shape_mismatch) if x is not None]
    if anchors:
        anchor = min(anchors)
        low = max(0, anchor - context)
        high = min(result.compared, anchor + context + 1)
        lines.append("")
        lines.append(f"{'ordinal':>9}  {'A (neser)':<24}  {'B (mesen)':<24}  offset")
        for ordinal in range(low, high):
            marker = " <<<" if ordinal == anchor else ""
            offset = b[ordinal].clk - a[ordinal].clk
            lines.append(
                f"{ordinal:>9}  {_format_event(a[ordinal]):<24}  "
                f"{_format_event(b[ordinal]):<24}  {offset:+d}{marker}"
            )
    return "\n".join(lines)


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("trace_a", help="NESER trace (or any trace; A is the offset baseline)")
    parser.add_argument("trace_b", help="reference trace, e.g. Mesen2 with NESER_BUS_LOG=1")
    parser.add_argument(
        "--context",
        type=int,
        default=12,
        help="cycles of context to print on each side of the divergence (default: 12)",
    )
    args = parser.parse_args(argv)

    with open(args.trace_a, encoding="utf-8", errors="replace") as handle:
        a = parse_lines(handle)
    with open(args.trace_b, encoding="utf-8", errors="replace") as handle:
        b = parse_lines(handle)

    result = diff_traces(a, b)
    print(format_report(a, b, result, args.context))
    return 0 if result.clock_exact else 1


if __name__ == "__main__":
    sys.exit(main())
