"""Unit tests for scripts/diff_screenshots.py."""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

from scripts.diff_screenshots import (
    Screenshot,
    best_row_lag,
    best_shift,
    diff_pixels,
    load_screenshot,
    main,
    render_diff,
    row_luminance,
    save_screenshot,
    shifted_diff,
)

BLACK = (0, 0, 0)
WHITE = (255, 255, 255)
RED = (255, 0, 0)


def build(rows: list[list[tuple[int, int, int]]]) -> Screenshot:
    """A screenshot from a list of pixel rows, so tests can spell images out literally."""
    height = len(rows)
    width = len(rows[0])
    pixels = bytes(channel for row in rows for pixel in row for channel in pixel)
    return Screenshot(width=width, height=height, pixels=pixels)


def banded(height: int, period: int, phase: int = 0) -> Screenshot:
    """One-pixel-wide horizontal banding: the signal these HDMA ROMs actually produce."""
    return build([[WHITE if (y + phase) % period == 0 else BLACK] for y in range(height)])


def ramp(height: int, first_row: int = 0) -> Screenshot:
    """Rows with a unique grey level each, so exactly one vertical shift can line two up.

    Periodic banding matches at both +1 and -1, which would make a best-shift assertion
    pin an arbitrary tie-break rather than the tool's actual answer.
    """
    return build([[(value, value, value)] for value in range(first_row, first_row + height)])


class DiffPixelsTest(unittest.TestCase):
    def test_identical_images_have_no_differing_pixels(self):
        image = build([[BLACK, WHITE], [WHITE, BLACK]])
        self.assertEqual(diff_pixels(image, image), 0)

    def test_a_single_changed_channel_counts_as_one_differing_pixel(self):
        a = build([[BLACK, WHITE], [WHITE, BLACK]])
        b = build([[BLACK, WHITE], [WHITE, (0, 0, 1)]])
        self.assertEqual(diff_pixels(a, b), 1)

    def test_every_pixel_can_differ(self):
        self.assertEqual(diff_pixels(build([[BLACK, BLACK]]), build([[WHITE, WHITE]])), 2)

    def test_mismatched_dimensions_are_rejected(self):
        with self.assertRaises(ValueError):
            diff_pixels(build([[BLACK]]), build([[BLACK, BLACK]]))


class ShiftedDiffTest(unittest.TestCase):
    def test_zero_shift_compares_every_pixel(self):
        a = build([[BLACK, WHITE], [WHITE, BLACK]])
        self.assertEqual(shifted_diff(a, a, 0, 0), (0, 4))

    def test_a_one_row_shift_is_cancelled_by_the_matching_dy(self):
        a = banded(8, 2, phase=0)
        b = banded(8, 2, phase=1)
        # Every row differs head-on; comparing A row y against B row y+1 lines them back up.
        self.assertEqual(shifted_diff(a, b, 0, 0), (8, 8))
        differing, compared = shifted_diff(a, b, 0, 1)
        self.assertEqual(differing, 0)
        self.assertEqual(compared, 7)

    def test_the_overlap_shrinks_by_the_shift(self):
        a = build([[BLACK, BLACK], [BLACK, BLACK], [BLACK, BLACK]])
        self.assertEqual(shifted_diff(a, a, 1, 1), (0, 2))

    def test_a_shift_past_the_image_compares_nothing(self):
        self.assertEqual(shifted_diff(build([[BLACK]]), build([[BLACK]]), 1, 0), (0, 0))


class BestShiftTest(unittest.TestCase):
    def test_identical_images_settle_on_no_shift(self):
        image = banded(8, 2)
        result = best_shift(image, image, radius=1)
        self.assertEqual((result.dx, result.dy), (0, 0))
        self.assertEqual(result.differing, 0)

    def test_a_one_row_offset_is_found(self):
        # B's row y+1 holds what A has on row y, so only dy=+1 lines the two up.
        result = best_shift(ramp(8, first_row=10), ramp(8, first_row=9), radius=1)
        self.assertEqual((result.dx, result.dy), (0, 1))
        self.assertEqual(result.differing, 0)

    def test_ties_prefer_the_smallest_shift(self):
        # A uniform image matches at every offset; the report must not claim a spurious shift.
        flat = build([[BLACK, BLACK], [BLACK, BLACK], [BLACK, BLACK]])
        result = best_shift(flat, flat, radius=1)
        self.assertEqual((result.dx, result.dy), (0, 0))

    def test_the_ratio_is_reported_over_the_compared_area(self):
        a = build([[BLACK, BLACK]])
        b = build([[BLACK, WHITE]])
        result = best_shift(a, b, radius=0)
        self.assertEqual((result.differing, result.compared), (1, 2))
        self.assertAlmostEqual(result.ratio, 0.5)


class RowLuminanceTest(unittest.TestCase):
    def test_a_black_row_and_a_white_row(self):
        values = row_luminance(build([[BLACK, BLACK], [WHITE, WHITE]]))
        self.assertAlmostEqual(values[0], 0.0)
        self.assertAlmostEqual(values[1], 255.0)

    def test_a_row_is_averaged_across_its_width(self):
        values = row_luminance(build([[BLACK, WHITE]]))
        self.assertAlmostEqual(values[0], 127.5)

    def test_green_weighs_more_than_blue(self):
        green = row_luminance(build([[(0, 255, 0)]]))[0]
        blue = row_luminance(build([[(0, 0, 255)]]))[0]
        self.assertGreater(green, blue)


class BestRowLagTest(unittest.TestCase):
    def test_matching_vectors_have_no_lag(self):
        vector = [0.0, 255.0] * 8
        lag, score = best_row_lag(vector, vector, max_lag=4)
        self.assertEqual(lag, 0)
        self.assertAlmostEqual(score, 0.0)

    def test_a_banding_phase_lag_is_recovered(self):
        a = [0.0, 255.0] * 8
        b = [255.0, 0.0] * 8
        lag, score = best_row_lag(a, b, max_lag=4)
        self.assertEqual(abs(lag), 1)
        self.assertAlmostEqual(score, 0.0)

    def test_a_lag_beyond_the_search_radius_is_not_invented(self):
        a = [0.0] * 4 + [255.0] * 12
        b = [0.0] * 12 + [255.0] * 4
        lag, _ = best_row_lag(a, b, max_lag=2)
        self.assertLessEqual(abs(lag), 2)

    def test_empty_vectors_report_no_lag(self):
        self.assertEqual(best_row_lag([], [], max_lag=4), (0, 0.0))


class RenderDiffTest(unittest.TestCase):
    def test_differing_pixels_are_marked_red_and_matches_are_dimmed(self):
        a = build([[WHITE, WHITE]])
        b = build([[WHITE, BLACK]])
        marked = render_diff(a, b)
        self.assertEqual(marked.width, 2)
        self.assertEqual(marked.height, 1)
        self.assertEqual(tuple(marked.pixels[3:6]), RED)
        # The matching pixel survives as a dimmed grey, never as full white.
        matched = marked.pixels[0:3]
        self.assertEqual(matched[0], matched[1])
        self.assertEqual(matched[1], matched[2])
        self.assertLess(matched[0], 255)

    def test_mismatched_dimensions_are_rejected(self):
        with self.assertRaises(ValueError):
            render_diff(build([[BLACK]]), build([[BLACK, BLACK]]))


class RoundTripTest(unittest.TestCase):
    def test_a_screenshot_survives_save_and_load(self):
        image = build([[BLACK, WHITE], [RED, (1, 2, 3)]])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "shot.png"
            save_screenshot(image, path)
            self.assertEqual(load_screenshot(path), image)


class MainTest(unittest.TestCase):
    def _write(self, directory: str, name: str, image: Screenshot) -> str:
        path = Path(directory) / name
        save_screenshot(image, path)
        return str(path)

    def _run(self, argv: list[str]) -> int:
        """Run main with its report captured, so the suite's own output stays readable."""
        with contextlib.redirect_stdout(io.StringIO()):
            return main(argv)

    def test_identical_images_exit_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            image = banded(8, 2)
            a = self._write(directory, "a.png", image)
            b = self._write(directory, "b.png", image)
            self.assertEqual(self._run([a, b]), 0)

    def test_differing_images_exit_one(self):
        with tempfile.TemporaryDirectory() as directory:
            a = self._write(directory, "a.png", banded(8, 2, phase=0))
            b = self._write(directory, "b.png", banded(8, 2, phase=1))
            self.assertEqual(self._run([a, b]), 1)

    def test_a_resolvable_shift_still_exits_one(self):
        # A non-zero best shift is evidence of a bug, never an accepted convention:
        # the tool must not report success just because some offset lines the two up.
        with tempfile.TemporaryDirectory() as directory:
            a = self._write(directory, "a.png", banded(8, 2, phase=0))
            b = self._write(directory, "b.png", banded(8, 2, phase=1))
            self.assertEqual(self._run([a, b, "--shift-search", "1"]), 1)

    def test_the_diff_image_is_written_when_asked(self):
        with tempfile.TemporaryDirectory() as directory:
            a = self._write(directory, "a.png", banded(8, 2, phase=0))
            b = self._write(directory, "b.png", banded(8, 2, phase=1))
            out = Path(directory) / "diff.png"
            self._run([a, b, "--out", str(out)])
            self.assertTrue(out.exists())

    def test_rows_mode_runs_on_differing_images(self):
        with tempfile.TemporaryDirectory() as directory:
            a = self._write(directory, "a.png", banded(8, 2, phase=0))
            b = self._write(directory, "b.png", banded(8, 2, phase=1))
            self.assertEqual(self._run([a, b, "--rows"]), 1)


if __name__ == "__main__":
    unittest.main()
