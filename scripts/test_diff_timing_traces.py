"""Unit tests for scripts/diff_timing_traces.py."""

import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from scripts.diff_timing_traces import Line, diff_traces, main, parse_lines


def trace(*points: tuple[int, int]) -> list[Line]:
    return [Line(pc, clk, None) for pc, clk in points]


class ParseLinesTest(unittest.TestCase):
    def test_exec_and_nmi_lines_parse_and_anything_else_is_dropped(self):
        self.assertEqual(
            parse_lines(
                [
                    "Loaded rom with CRC32: 5CE951EA",
                    "nmi=3 pc=0087A4 clk=3165914",
                    "pc=8087CD clk=3166402\n",
                    "",
                ]
            ),
            [Line(0x0087A4, 3165914, 3), Line(0x8087CD, 3166402, None)],
        )


class DiffTracesTest(unittest.TestCase):
    def test_a_constant_offset_is_a_match(self):
        result = diff_traces(
            trace((0x8000, 13), (0x8001, 15), (0x8003, 19)),
            trace((0x8000, 7), (0x8001, 9), (0x8003, 13)),
        )
        self.assertIsNone(result.divergence)
        self.assertEqual(result.baseline, 6)
        self.assertEqual(result.compared, 3)
        self.assertEqual(result.stamp_differences, [])

    def test_a_one_line_spike_is_a_stamp_difference_not_a_divergence(self):
        # A stall at an instruction boundary (NES OAM DMA, SNES DRAM refresh) is charged to
        # the instruction before it by one emulator and after it by the other: one line's
        # offset jumps and the next is back at the baseline.
        result = diff_traces(
            trace((0x8000, 6), (0x8003, 76), (0x8006, 110)),
            trace((0x8000, 0), (0x8003, 30), (0x8006, 104)),
        )
        self.assertIsNone(result.divergence)
        self.assertEqual(result.stamp_differences, [1])

    def test_an_offset_that_steps_and_stays_is_the_divergence(self):
        result = diff_traces(
            trace((0x8000, 6), (0x8003, 20), (0x8006, 32), (0x8008, 40)),
            trace((0x8000, 0), (0x8003, 14), (0x8006, 24), (0x8008, 32)),
        )
        self.assertEqual(result.divergence, 2)
        self.assertEqual(result.reason, "clock offset 6 -> 8")

    def test_a_different_pc_is_the_divergence(self):
        result = diff_traces(
            trace((0x8000, 6), (0x8003, 20)),
            trace((0x8000, 0), (0x9000, 14)),
        )
        self.assertEqual(result.divergence, 1)
        self.assertEqual(result.reason, "pc 008003 vs 009000")

    def test_a_step_on_the_last_line_is_a_divergence(self):
        result = diff_traces(trace((0x8000, 6), (0x8003, 30)), trace((0x8000, 0), (0x8003, 14)))
        self.assertEqual(result.divergence, 1)

    def test_one_trace_running_longer_is_a_divergence_at_its_extra_line(self):
        result = diff_traces(trace((0x8000, 6), (0x8003, 20)), trace((0x8000, 0)))
        self.assertEqual(result.divergence, 1)
        self.assertEqual(result.reason, "the second trace ends here")

    def test_a_one_line_excursion_in_an_nmi_log_is_a_divergence(self):
        # One NMI log line is a whole frame: an entry late in one frame and on time in the
        # next is the kind of NMI-latency bug the log exists to find (review finding 4).
        result = diff_traces(
            [Line(0x9000, 100, 1), Line(0x9000, 212, 2), Line(0x9000, 300, 3)],
            [Line(0x9000, 100, 1), Line(0x9000, 200, 2), Line(0x9000, 300, 3)],
        )
        self.assertEqual(result.divergence, 1)
        self.assertEqual(result.stamp_differences, [])

    def test_a_baseline_given_up_front_is_used_instead_of_the_first_line(self):
        # A drift before a trace's first line would otherwise become its baseline.
        result = diff_traces(trace((0x8000, 6), (0x8003, 20)), trace((0x8000, 0), (0x8003, 14)), baseline=0)
        self.assertEqual(result.divergence, 0)
        self.assertEqual(result.reason, "clock offset 0 -> 6")

    def test_nmi_numbers_are_compared_like_pcs(self):
        result = diff_traces(
            [Line(0x9000, 100, 1), Line(0x9000, 200, 2)],
            [Line(0x9000, 100, 1), Line(0x9000, 200, 3)],
        )
        self.assertEqual(result.divergence, 1)
        self.assertEqual(result.reason, "nmi 2 vs 3")


class MainTest(unittest.TestCase):
    def run_main(self, first: str, second: str, *extra: str) -> tuple[int, str]:
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory, "neser.txt"), Path(directory, "mesen.txt")
            a.write_text(first)
            b.write_text(second)
            out = io.StringIO()
            with redirect_stdout(out), redirect_stderr(out):
                code = main([str(a), str(b), "--context", "1", *extra])
        return code, out.getvalue()

    def test_a_match_exits_0_and_says_so(self):
        code, report = self.run_main(
            "nmi=1 pc=0087A4 clk=2451184\nnmi=2 pc=0087A4 clk=2808548\n",
            "nmi=1 pc=0087A4 clk=2451178\nnmi=2 pc=0087A4 clk=2808542\n",
        )
        self.assertEqual(code, 0)
        self.assertIn("match: 2 lines, clock offset 6 throughout", report)

    def test_a_divergence_exits_1_and_shows_both_sides_around_it(self):
        code, report = self.run_main(
            "pc=008000 clk=6\npc=008003 clk=20\npc=008006 clk=32\npc=008008 clk=40\n",
            "pc=008000 clk=0\npc=008003 clk=14\npc=008006 clk=24\npc=008008 clk=32\n",
        )
        self.assertEqual(code, 1)
        self.assertIn("first divergence at line 3: clock offset 6 -> 8", report)
        self.assertIn("> pc=008006 clk=32 | pc=008006 clk=24 | offset 8", report)
        self.assertIn("  pc=008003 clk=20 | pc=008003 clk=14 | offset 6", report)

    def test_an_empty_trace_exits_2(self):
        code, report = self.run_main("Loaded rom\n", "pc=008000 clk=0\n")
        self.assertEqual(code, 2)
        self.assertIn("has no trace lines", report)

    def test_baseline_option_is_passed_through(self):
        code, report = self.run_main("pc=008000 clk=6\n", "pc=008000 clk=0\n", "--baseline", "0")
        self.assertEqual(code, 1)
        self.assertIn("first divergence at line 1: clock offset 0 -> 6", report)

    def test_a_missing_file_exits_2_not_1(self):
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            code = main(["/nonexistent/neser.txt", "/nonexistent/mesen.txt"])
        self.assertEqual(code, 2)
        self.assertIn("cannot read /nonexistent/neser.txt", out.getvalue())


if __name__ == "__main__":
    unittest.main()
