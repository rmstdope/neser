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

    def test_shards_are_weighted_toward_the_heavy_tail(self) -> None:
        """Playwright splits by test count in file order, and the slow specs sort last.

        An even split left shard 2 with 281 worker-seconds against shard 1's 169 on run
        36317969389, so shard 1 takes twice the tests. One weight per matrix entry.
        """

        weights = re.search(r'PWTEST_SHARD_WEIGHTS: "([0-9:]+)"', self.job)
        matrix = re.search(r"shard: \[([^\]]*)\]", self.job)
        assert weights is not None and matrix is not None
        self.assertEqual(weights.group(1), "2:1")
        self.assertEqual(len(weights.group(1).split(":")), len(matrix.group(1).split(",")))

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


class PlaywrightSystemDependenciesTest(unittest.TestCase):
    """CI stays off the apt mirror unless Chromium really lacks a library (nr-2e2l).

    ``install-deps`` took up to eleven minutes on a slow mirror while the runner image already
    had every library Chromium needs; ``scripts/ensure_playwright_deps.sh`` checks with ``ldd``
    and falls back to apt only when one is missing.
    """

    CHECK = "run: bash scripts/ensure_playwright_deps.sh"

    def test_no_job_installs_playwright_apt_packages_unconditionally(self) -> None:
        ci = CI_YML.read_text(encoding="utf-8")
        self.assertNotIn("--with-deps", ci)
        self.assertNotIn("install-deps", ci)

    def test_web_integration_checks_deps_on_every_run(self) -> None:
        job = web_integration_job()
        step = re.search(r"- name: Check Playwright system dependencies\n((?: {8}.*\n)+)", job)
        assert step is not None, "no dependency check step in web-integration"
        self.assertIn(self.CHECK, step.group(1))
        self.assertNotIn("if:", step.group(1))
        self.assertLess(job.index("npx playwright install chromium"), job.index(self.CHECK))
        self.assertLess(job.index(self.CHECK), job.index("npm run test:integration:web"))

    def test_web_container_checks_deps_after_browser_install(self) -> None:
        ci = CI_YML.read_text(encoding="utf-8")
        build = ci.split("  web-container:\n", 1)[1].split("  publish-web:\n")[0]
        self.assertLess(build.index("npx playwright install chromium"), build.index(self.CHECK))
        self.assertLess(build.index(self.CHECK), build.index("node deploy/smoke.mjs"))


class WebContainerPublicationTest(unittest.TestCase):
    def test_pr_builds_are_read_only_and_publish_waits_for_the_tested_image(self) -> None:
        ci = CI_YML.read_text(encoding="utf-8")
        build, publish = ci.split("  web-container:\n", 1)[1].split("  publish-web:\n")
        self.assertNotIn("packages: write", build)
        self.assertIn("needs: web-container", publish)
        self.assertIn("github.event_name == 'push' && github.ref == 'refs/heads/main'", publish)
        # Without a status function the implicit success() skips publishing
        # whenever path filtering skipped an upstream test job.
        self.assertIn("!cancelled() && needs.web-container.result == 'success'", publish)
        self.assertIn("packages: write", publish)
        self.assertIn("docker load --input web-container.tar", publish)
        self.assertIn("node deploy/smoke.mjs", build)
        self.assertLess(build.index("node deploy/smoke.mjs"), build.index("docker save"))

    def test_build_is_gated_by_every_other_check_and_reruns_preserve_manifest(self) -> None:
        ci = CI_YML.read_text(encoding="utf-8")
        jobs = set(re.findall(r"^  ([a-z][a-z-]+):$", ci, re.MULTILINE))
        build, publish = ci.split("  web-container:\n", 1)[1].split("  publish-web:\n")
        needs = re.search(r"needs: \[([^\]]+)\]", build)
        assert needs is not None
        self.assertEqual(
            {name.strip() for name in needs.group(1).split(",")},
            jobs - {"web-container", "publish-web", "push", "pull_request"},
        )
        self.assertIn("!contains(needs.*.result, 'failure')", build)
        self.assertIn("!contains(needs.*.result, 'cancelled')", build)
        for step in publish.split("      - ")[1:]:
            if "docker load" in step or "docker push" in step or "upload-artifact@" in step:
                self.assertIn("if: steps.existing.outputs.found != 'true'", step)


if __name__ == "__main__":
    unittest.main()
