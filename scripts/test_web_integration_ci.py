"""Tests for the shape of the ``web-integration`` CI job in ``.github/workflows/ci.yml``.

The job runs the Playwright suite as two shards on separate runners (nr-ks6), each keeping the
failure artifact from nr-dv5 under a name of its own. The workflow is read as text, as
``test_rust_toolchain.py`` does, since the venv carries no YAML parser.
"""

import re
import unittest
from pathlib import Path

CI_YML = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "ci.yml"


def web_integration_job() -> str:
    """The text of the ``web-integration`` job, up to the next top-level job or end of file."""

    ci = CI_YML.read_text(encoding="utf-8")
    match = re.search(r"^  web-integration:\n((?:(?: {4}.*)?\n)+)", ci, re.MULTILINE)
    assert match is not None, "no web-integration job in ci.yml"
    return match.group(1)


class WebIntegrationShardsTest(unittest.TestCase):
    """The web-integration job splits the suite across two runners."""

    def setUp(self) -> None:
        self.job = web_integration_job()

    def test_job_is_a_two_shard_matrix(self) -> None:
        self.assertRegex(self.job, r"\n {6}fail-fast: false\n")
        self.assertRegex(self.job, r"\n {8}shard: \[1, 2\]\n")

    def test_each_shard_runs_its_share(self) -> None:
        self.assertIn(
            "run: npm run test:integration:web -- --shard=${{ matrix.shard }}/2",
            self.job,
        )

    def test_shard_count_matches_the_matrix(self) -> None:
        matrix = re.search(r"shard: \[([^\]]*)\]", self.job)
        total = re.search(r"--shard=\$\{\{ matrix\.shard \}\}/(\d+)", self.job)
        assert matrix is not None and total is not None
        shards = [int(entry) for entry in matrix.group(1).split(",")]
        self.assertEqual(shards, list(range(1, int(total.group(1)) + 1)))

    def test_each_shard_uploads_its_own_failure_artifact(self) -> None:
        upload = re.search(r"- name: Upload Playwright test results\n((?: {8}.*\n)+)", self.job)
        assert upload is not None, "no upload step"
        step = upload.group(1)
        self.assertIn("if: failure()", step)
        self.assertIn("path: test-results/", step)
        self.assertIn("name: web-integration-test-results-${{ matrix.shard }}", step)

    def test_check_names_say_which_shard(self) -> None:
        pattern = re.compile(r"^ {4}name: web-integration \(\$\{\{ matrix\.shard \}\}/2\)$", re.MULTILINE)
        self.assertRegex(self.job, pattern)


if __name__ == "__main__":
    unittest.main()
