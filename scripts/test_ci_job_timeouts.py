"""Every CI job that installs apt packages carries a time limit (nr-nts2).

A slow apt mirror once held the rust-lint job for 1h40m on PR #3363; without ``timeout-minutes``
GitHub lets a job run for six hours. The workflows are read as text, as
``test_web_integration_ci.py`` does, since the venv carries no YAML parser.
"""

import re
import unittest
from pathlib import Path

WORKFLOWS = Path(__file__).resolve().parent.parent / ".github" / "workflows"
FILES = ("ci.yml", "release.yml")


def jobs(workflow: str) -> dict[str, str]:
    """Each top-level job of a workflow, by name, with its body up to the next job."""

    text = (WORKFLOWS / workflow).read_text(encoding="utf-8")
    jobs_block = text.split("\njobs:\n", 1)[1]
    # A body is its blank and four-space lines; a two-space comment introduces the next job,
    # so it ends this body rather than joining it.
    found: dict[str, str] = {}
    for match in re.finditer(r"^  ([A-Za-z0-9_-]+):\n((?:(?: {4}.*)?\n)*)", jobs_block, re.M):
        found[match.group(1)] = match.group(2)
    return found


# Any spelling of an apt install (``apt-get -y install``, ``apt install``), and Playwright's
# ``--with-deps`` and ``install-deps``, which run apt-get on the runner too.
APT_INSTALLS = re.compile(r"\bapt(?:-get)?\b[^\n]*\binstall\b|playwright install(?:-deps\b|\b[^\n]*--with-deps)")


def apt_jobs() -> dict[str, str]:
    """The jobs, as ``<file>:<job>``, whose steps install apt packages."""

    return {
        f"{workflow}:{name}": body
        for workflow in FILES
        for name, body in jobs(workflow).items()
        if APT_INSTALLS.search(body)
    }


def timeout_of(body: str) -> int | None:
    """The job-level ``timeout-minutes``, or None when the job has none."""

    match = re.search(r"^    timeout-minutes: (\d+)$", body, re.M)
    return int(match.group(1)) if match else None


class CiJobTimeoutsTest(unittest.TestCase):
    def test_the_job_parser_finds_the_known_jobs(self) -> None:
        self.assertTrue(
            {
                "ci.yml:rust-lint",
                "ci.yml:rust-test-build",
                "ci.yml:rust-test-run",
                "ci.yml:web-integration",
                "release.yml:build",
            }.issubset(apt_jobs())
        )

    def test_every_spelling_of_an_apt_install_counts(self) -> None:
        for line in (
            "sudo apt-get update && sudo apt-get install -y libudev-dev",
            "sudo apt-get -y install libudev-dev",
            "sudo apt install libudev-dev",
            "npx playwright install --with-deps chromium",
            "npx playwright install chromium --with-deps",
            "npx playwright install-deps chromium",
        ):
            with self.subTest(line=line):
                self.assertRegex(line, APT_INSTALLS)
        for line in ("npx playwright install chromium", "sudo apt-get update", "npm install"):
            with self.subTest(line=line):
                self.assertNotRegex(line, APT_INSTALLS)

    def test_every_apt_installing_job_has_a_time_limit(self) -> None:
        missing = sorted(name for name, body in apt_jobs().items() if timeout_of(body) is None)
        self.assertEqual(missing, [], "jobs that install apt packages without timeout-minutes")

    def test_time_limits_stay_under_an_hour(self) -> None:
        for workflow in FILES:
            for name, body in jobs(workflow).items():
                limit = timeout_of(body)
                if limit is not None:
                    with self.subTest(job=f"{workflow}:{name}"):
                        self.assertTrue(1 <= limit <= 60, limit)


if __name__ == "__main__":
    unittest.main()
