"""Unit tests for scripts/diff_bus_traces.py."""

import unittest

from scripts.diff_bus_traces import Event, best_alignment, diff_traces, parse_lines


NESER = [
    "[CPU] exec PC=00:8002 STA $7E1FF0        A=004E X=0000",
    "[CPU]       read  $008007 clk=254",
    "[CPU]       internal clk=262",
    "[CPU]       write $7E1FF1 = $53 clk=276",
    "[PPU] write 2100=8F y=0 x=0 clk=280 lc=280",
]

MESEN = [
    "mesen busR 008007 ticks=254",
    "mesen idle   ------ ticks=262",
    "mesen busW 7E1FF1 ticks=276",
]


class ParseLinesTest(unittest.TestCase):
    def test_neser_bus_lines_are_normalised(self):
        self.assertEqual(
            parse_lines(NESER),
            [
                Event("read", 0x008007, 254),
                Event("idle", None, 262),
                Event("write", 0x7E1FF1, 276),
            ],
        )

    def test_mesen_bus_lines_are_normalised_to_the_same_shape(self):
        self.assertEqual(parse_lines(MESEN), parse_lines(NESER))

    def test_lines_without_a_clock_stamp_are_ignored(self):
        self.assertEqual(parse_lines(["[CPU] exec PC=00:8002 STA $7E1FF0"]), [])
        self.assertEqual(parse_lines(["", "garbage", "mesen busR"]), [])

    def test_a_write_value_does_not_leak_into_the_address(self):
        (event,) = parse_lines(["[CPU]       write $004210 = $FF clk=7"])
        self.assertEqual(event, Event("write", 0x004210, 7))


class DiffTracesTest(unittest.TestCase):
    def test_identical_traces_report_a_single_offset_and_no_divergence(self):
        result = diff_traces(parse_lines(NESER), parse_lines(MESEN))
        self.assertEqual(result.compared, 3)
        self.assertEqual(dict(result.offset_histogram), {0: 3})
        self.assertIsNone(result.first_offset_change)
        self.assertIsNone(result.first_shape_mismatch)
        self.assertTrue(result.clock_exact)

    def test_a_constant_offset_is_still_clock_exact(self):
        shifted = [Event(k, a, c + 1000) for k, a, c in parse_lines(MESEN)]
        result = diff_traces(parse_lines(NESER), shifted)
        self.assertEqual(dict(result.offset_histogram), {1000: 3})
        self.assertIsNone(result.first_offset_change)
        self.assertTrue(result.clock_exact)

    def test_the_ordinal_where_the_offset_steps_is_reported(self):
        a = [Event("read", 0x10, 0), Event("read", 0x11, 10), Event("read", 0x12, 20)]
        b = [Event("read", 0x10, 0), Event("read", 0x11, 10), Event("read", 0x12, 26)]
        result = diff_traces(a, b)
        self.assertEqual(result.first_offset_change, 2)
        self.assertEqual(dict(result.offset_histogram), {0: 2, 6: 1})
        self.assertFalse(result.clock_exact)

    def test_a_kind_mismatch_is_reported_as_a_shape_mismatch(self):
        a = [Event("read", 0x10, 0), Event("idle", None, 10)]
        b = [Event("read", 0x10, 0), Event("read", 0x8000, 10)]
        result = diff_traces(a, b)
        self.assertEqual(result.first_shape_mismatch, 1)

    def test_an_address_mismatch_is_reported_as_a_shape_mismatch(self):
        a = [Event("read", 0x10, 0), Event("read", 0x20, 10)]
        b = [Event("read", 0x10, 0), Event("read", 0x21, 10)]
        result = diff_traces(a, b)
        self.assertEqual(result.first_shape_mismatch, 1)

    def test_comparison_stops_at_the_shorter_trace_and_reports_the_gap(self):
        a = [Event("read", 0x10, 0), Event("read", 0x20, 10)]
        b = [Event("read", 0x10, 0)]
        result = diff_traces(a, b)
        self.assertEqual(result.compared, 1)
        self.assertEqual(result.length_a, 2)
        self.assertEqual(result.length_b, 1)

    def test_empty_traces_do_not_crash(self):
        result = diff_traces([], [])
        self.assertEqual(result.compared, 0)
        self.assertIsNone(result.first_offset_change)
        self.assertFalse(result.clock_exact)


class BestAlignmentTest(unittest.TestCase):
    """A clock window rarely opens on the same cycle in both emulators: whichever one is a
    few clocks ahead catches an extra event at the leading edge. Aligning on ordinal 0
    blindly would then report a spurious divergence at every single ordinal."""

    def _stream(self, n, start=0):
        return [Event("read", 0x8000 + i, 100 + 8 * i) for i in range(start, n)]

    def test_identical_streams_need_no_shift(self):
        stream = self._stream(50)
        self.assertEqual(best_alignment(stream, stream), (0, 0))

    def test_a_leading_extra_event_in_b_is_trimmed(self):
        a = self._stream(50, start=3)
        b = self._stream(50)
        self.assertEqual(best_alignment(a, b), (0, 3))

    def test_a_leading_extra_event_in_a_is_trimmed(self):
        a = self._stream(50)
        b = self._stream(50, start=2)
        self.assertEqual(best_alignment(a, b), (2, 0))

    def test_alignment_survives_a_divergence_later_in_the_stream(self):
        a = self._stream(50, start=4)
        b = self._stream(50)
        b = b[:30] + [Event("idle", None, e.clk) for e in b[30:]]
        self.assertEqual(best_alignment(a, b), (0, 4))

    def test_unrelated_streams_fall_back_to_no_shift(self):
        a = [Event("read", 0x10 + i, i) for i in range(40)]
        b = [Event("write", 0x900 + i, i) for i in range(40)]
        self.assertEqual(best_alignment(a, b), (0, 0))

    def test_diff_traces_applies_the_alignment_and_reports_clock_exact(self):
        a = self._stream(50, start=3)
        b = self._stream(50)
        result = diff_traces(a, b)
        self.assertTrue(result.clock_exact)
        self.assertEqual(result.alignment, (0, 3))
        self.assertEqual(result.compared, 47)


if __name__ == "__main__":
    unittest.main()
