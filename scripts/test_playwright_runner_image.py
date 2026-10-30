"""Every CI job that installs Playwright's Chromium runs on a pinned Ubuntu image (nr-7sn1).

``ubuntu-latest`` moved to Ubuntu 26.04 in October 2026, and the Playwright the lockfile carries
(1.60) refuses it: ``playwright install chromium`` failed with "Playwright does not support
chromium on ubuntu26.04-x64" before any test ran, and web-container blocked every pull request.
A job that installs the browser therefore names the image it was proven on rather than following
``ubuntu-latest``; moving it is a deliberate change made together with a Playwright that supports
the new image. The workflow is read as text, as ``test_web_integration_ci.py`` does.
"""

import re
import unittest
from pathlib import Path

CI_YML = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "ci.yml"
PINNED_IMAGE = "ubuntu-24.04"


def jobs() -> dict[str, str]:
    """Each job of ci.yml by name, as the text of its body."""

    ci = CI_YML.read_text(encoding="utf-8")
    jobs_text = ci.split("\njobs:\n", 1)[1]
    return {
        match.group(1): match.group(2)
        for match in re.finditer(r"^  ([\w-]+):\n((?:(?: {4}.*)?\n)+)", jobs_text, re.MULTILINE)
    }


class PlaywrightRunnerImageTest(unittest.TestCase):
    def test_jobs_installing_playwright_are_found(self) -> None:
        installing = [name for name, body in jobs().items() if "playwright install" in body]
        self.assertEqual(sorted(installing), ["web-container", "web-integration"])

    def test_jobs_installing_playwright_pin_their_image(self) -> None:
        for name, body in jobs().items():
            if "playwright install" not in body:
                continue
            with self.subTest(job=name):
                runs_on = re.search(r"^    runs-on: (.+)$", body, re.MULTILINE)
                assert runs_on is not None, f"{name} has no runs-on"
                self.assertEqual(runs_on.group(1), PINNED_IMAGE)

    def test_cached_browsers_are_keyed_by_the_pinned_image(self) -> None:
        """A cached Chromium skips the install, the one step that checks the image (review of #3396).

        The cache once hid this very bug in web-integration, so moving the pin must miss the cache.
        """

        keys = [(name, key) for name, body in jobs().items() for key in re.findall(r"key: (playwright-\S+)", body)]
        self.assertTrue(keys, "no Playwright browser cache found in ci.yml")
        for name, key in keys:
            with self.subTest(job=name):
                self.assertIn(PINNED_IMAGE, key)


if __name__ == "__main__":
    unittest.main()
