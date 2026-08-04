"""Pixel-diff two emulator screenshots and localise the difference by row.

The SNES accuracy work compares a NESER capture (``NESER_CAPTURE_SCREEN=1``, written to
``target/snes_test_captures/<suite>/``) against a Mesen2 capture of the same ROM at the same
frame. README-SNES.md requires a **0-pixel diff** before a screen-CRC golden may be committed,
and forbids approving one from a visual comparison -- so that diff has to be measured, not
eyeballed. Until now the measurement lived only as prose and every investigation re-wrote a
throwaway PIL script.

Three things it reports:

* the plain per-pixel difference, which is the number the approval rule is written against;
* ``--shift-search``, the +/-N row/column offset search. Since the BG vertical-scroll fix in
  #2945 the two emulators align byte-for-byte at zero offset, so **a non-zero best shift is
  evidence of a bug, never a capture convention to allow for** -- the exit code stays non-zero
  even when some offset lines the two up;
* ``--rows``, a per-row mean-luminance vector for each image plus the lag that best aligns
  them. Scanline-banding ROMs (HDMA writing ``$2100`` per line) put their whole signal in that
  vector, and a phase error there shows up as a lag or as rows the other emulator does not
  have -- neither of which a 2D shift search can express.

Usage::

    python -m scripts.diff_screenshots a.png b.png [--shift-search 1] [--rows] [--out diff.png]

Exits 0 only when the two images are pixel-identical.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import NamedTuple

from PIL import Image

#: Rec. 601 luma weights. Any sane weighting ranks these captures the same way; 601 is the one
#: the SNES's own BGR555 output is usually discussed in.
_LUMA_R, _LUMA_G, _LUMA_B = 0.299, 0.587, 0.114

#: How much of the original image survives under the red diff overlay. Bright enough to see
#: what the picture was, dim enough that a red mark never reads as image content.
_MATCH_DIM = 0.35


class Screenshot(NamedTuple):
    """One decoded RGB888 image, row-major, ``width * height * 3`` bytes."""

    width: int
    height: int
    pixels: bytes

    def pixel(self, x: int, y: int) -> tuple[int, int, int]:
        """The RGB triple at ``(x, y)``."""
        offset = (y * self.width + x) * 3
        return (self.pixels[offset], self.pixels[offset + 1], self.pixels[offset + 2])


class ShiftResult(NamedTuple):
    """The best ``(dx, dy)`` offset found, and how well it scored over the overlap."""

    dx: int
    dy: int
    differing: int
    compared: int

    @property
    def ratio(self) -> float:
        """Fraction of compared pixels that differ; 0.0 when nothing was compared."""
        return self.differing / self.compared if self.compared else 0.0


def load_screenshot(path: str | Path) -> Screenshot:
    """Decode a PNG to RGB888, flattening whatever mode and alpha it was stored in."""
    with Image.open(path) as handle:
        rgb = handle.convert("RGB")
        return Screenshot(width=rgb.width, height=rgb.height, pixels=rgb.tobytes())


def save_screenshot(image: Screenshot, path: str | Path) -> None:
    """Write a screenshot back out as an RGB PNG."""
    Image.frombytes("RGB", (image.width, image.height), image.pixels).save(path)


def _require_same_size(a: Screenshot, b: Screenshot) -> None:
    if (a.width, a.height) != (b.width, b.height):
        raise ValueError(f"image sizes differ: {a.width}x{a.height} vs {b.width}x{b.height}")


def diff_pixels(a: Screenshot, b: Screenshot) -> int:
    """Count pixels that differ in any channel. Both images must be the same size."""
    _require_same_size(a, b)
    return sum(
        1 for offset in range(0, len(a.pixels), 3) if a.pixels[offset : offset + 3] != b.pixels[offset : offset + 3]
    )


def shifted_diff(a: Screenshot, b: Screenshot, dx: int, dy: int) -> tuple[int, int]:
    """Compare ``a`` against ``b`` displaced by ``(dx, dy)``, over their overlap only.

    Returns ``(differing, compared)``. A pixel at ``(x, y)`` in ``a`` is matched against
    ``(x + dx, y + dy)`` in ``b``; pixels whose partner falls outside the image are skipped, so
    a larger shift is scored over a smaller area and ``compared`` is what makes the two
    comparable.
    """
    _require_same_size(a, b)
    differing = 0
    compared = 0
    for y in range(max(0, -dy), min(a.height, a.height - dy)):
        for x in range(max(0, -dx), min(a.width, a.width - dx)):
            compared += 1
            if a.pixel(x, y) != b.pixel(x + dx, y + dy):
                differing += 1
    return differing, compared


def _shift_candidates(radius: int) -> list[tuple[int, int]]:
    """Every offset within ``radius``, nearest first, so ties resolve to the smallest shift."""
    candidates = [(dx, dy) for dy in range(-radius, radius + 1) for dx in range(-radius, radius + 1)]
    return sorted(candidates, key=lambda shift: (abs(shift[0]) + abs(shift[1]), abs(shift[0]), abs(shift[1])))


def best_shift(a: Screenshot, b: Screenshot, radius: int) -> ShiftResult:
    """Search every offset within ``radius`` and return the one with the lowest diff ratio.

    Ties keep the smallest shift, so a uniform image reports ``(0, 0)`` instead of whichever
    offset the iteration happened to visit first.
    """
    best: ShiftResult | None = None
    for dx, dy in _shift_candidates(radius):
        differing, compared = shifted_diff(a, b, dx, dy)
        if compared == 0:
            continue
        candidate = ShiftResult(dx=dx, dy=dy, differing=differing, compared=compared)
        if best is None or candidate.ratio < best.ratio:
            best = candidate
    return best if best is not None else ShiftResult(dx=0, dy=0, differing=0, compared=0)


def row_luminance(image: Screenshot) -> list[float]:
    """Mean luma per row -- the whole signal for a ROM that bands by scanline."""
    values = []
    stride = image.width * 3
    for y in range(image.height):
        row = image.pixels[y * stride : (y + 1) * stride]
        total = sum(
            _LUMA_R * row[offset] + _LUMA_G * row[offset + 1] + _LUMA_B * row[offset + 2]
            for offset in range(0, stride, 3)
        )
        values.append(total / image.width)
    return values


def best_row_lag(a: list[float], b: list[float], max_lag: int) -> tuple[int, float]:
    """Find the lag that best aligns two per-row vectors, scored by mean absolute difference.

    Lag ``L`` means ``a[i]`` corresponds to ``b[i + L]``. Returns ``(lag, score)`` with the
    smallest lag on a tie, and ``(0, 0.0)`` when there is nothing to compare.
    """
    best_lag = 0
    best_score: float | None = None
    for lag in sorted(range(-max_lag, max_lag + 1), key=abs):
        low = max(0, -lag)
        high = min(len(a), len(b) - lag)
        if high <= low:
            continue
        score = sum(abs(a[i] - b[i + lag]) for i in range(low, high)) / (high - low)
        if best_score is None or score < best_score:
            best_lag, best_score = lag, score
    return best_lag, best_score if best_score is not None else 0.0


def render_diff(a: Screenshot, b: Screenshot) -> Screenshot:
    """Mark differing pixels red over a dimmed greyscale copy of ``a``."""
    _require_same_size(a, b)
    out = bytearray(len(a.pixels))
    for offset in range(0, len(a.pixels), 3):
        pixel = a.pixels[offset : offset + 3]
        if pixel != b.pixels[offset : offset + 3]:
            out[offset : offset + 3] = b"\xff\x00\x00"
        else:
            grey = int(_MATCH_DIM * (_LUMA_R * pixel[0] + _LUMA_G * pixel[1] + _LUMA_B * pixel[2]))
            out[offset : offset + 3] = bytes((grey, grey, grey))
    return Screenshot(width=a.width, height=a.height, pixels=bytes(out))


def format_report(
    a: Screenshot,
    b: Screenshot,
    path_a: str,
    path_b: str,
    shift_radius: int,
    show_rows: bool,
) -> str:
    """Render the human-readable comparison, in the order the approval rule cares about."""
    lines = [
        f"A: {a.width}x{a.height}  {path_a}",
        f"B: {b.width}x{b.height}  {path_b}",
        "",
    ]
    total = a.width * a.height
    differing = diff_pixels(a, b)
    lines.append(f"differing pixels: {differing} / {total} ({100.0 * differing / total:.4f}%)")
    lines.append("IDENTICAL" if differing == 0 else "NOT IDENTICAL")

    if shift_radius > 0:
        best = best_shift(a, b, shift_radius)
        lines.append("")
        lines.append(f"best shift within +/-{shift_radius}: dx={best.dx:+d} dy={best.dy:+d}")
        lines.append(f"  {best.differing} / {best.compared} differ over the overlap ({100.0 * best.ratio:.4f}%)")
        if (best.dx, best.dy) != (0, 0):
            lines.append(
                "  WARNING: a non-zero best shift is evidence of a bug, not a capture "
                "convention -- see the golden approval workflow in README-SNES.md."
            )

    if show_rows:
        lum_a = row_luminance(a)
        lum_b = row_luminance(b)
        lag, score = best_row_lag(lum_a, lum_b, max_lag=min(8, len(lum_a)))
        lines.append("")
        lines.append(f"per-row mean luminance (best lag {lag:+d}, mean abs difference {score:.3f}):")
        lines.append(f"{'row':>5}  {'A':>8}  {'B':>8}  {'B - A':>8}")
        for row, (value_a, value_b) in enumerate(zip(lum_a, lum_b, strict=False)):
            marker = "" if abs(value_b - value_a) < 0.5 else "  <<<"
            lines.append(f"{row:>5}  {value_a:>8.2f}  {value_b:>8.2f}  {value_b - value_a:>+8.2f}{marker}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("image_a", help="first PNG, e.g. a NESER target/snes_test_captures capture")
    parser.add_argument("image_b", help="second PNG, e.g. the Mesen2 capture of the same ROM and frame")
    parser.add_argument(
        "--shift-search",
        type=int,
        default=0,
        metavar="N",
        help="also search every (dx, dy) offset within +/-N and report the best (default: 0, no search)",
    )
    parser.add_argument(
        "--rows",
        action="store_true",
        help="print the per-row mean-luminance vector of each image and the lag that best aligns them",
    )
    parser.add_argument("--out", metavar="PATH", help="write a red-marked difference image to PATH")
    args = parser.parse_args(argv)

    a = load_screenshot(args.image_a)
    b = load_screenshot(args.image_b)
    if (a.width, a.height) != (b.width, b.height):
        print(f"image sizes differ: {a.width}x{a.height} vs {b.width}x{b.height}", file=sys.stderr)
        return 2

    print(format_report(a, b, args.image_a, args.image_b, args.shift_search, args.rows))
    if args.out:
        save_screenshot(render_diff(a, b), args.out)
        print(f"\nwrote {args.out}")
    return 0 if diff_pixels(a, b) == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
