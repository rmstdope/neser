"""Regression tests for release workflow packaging steps."""

import unittest
from pathlib import Path


class ReleaseWorkflowTests(unittest.TestCase):
    """Text-level checks for release archive packaging in GitHub Actions."""

    def test_release_workflow_packages_and_verifies_archives_in_build_matrix(
        self,
    ) -> None:
        """Build jobs package, verify, and upload archives instead of bare binaries."""

        workflow_path = Path(__file__).parents[1] / ".github/workflows/release.yml"
        workflow = workflow_path.read_text(encoding="utf-8")

        self.assertIn("python scripts/package_release.py", workflow)
        self.assertIn("python scripts/verify_release_package.py", workflow)
        self.assertIn("--smoke-command", workflow)
        self.assertIn("--version", workflow)
        self.assertIn(
            "neser-${{ matrix.target }}.${{ matrix.archive_format }}", workflow
        )
        self.assertNotIn("archive_extension", workflow)
        self.assertIn("*.tar.gz", workflow)
        self.assertIn("*.zip", workflow)
        self.assertNotIn("mv artifacts/neser-x86_64-unknown-linux-gnu/neser", workflow)
        self.assertNotIn("artifacts/neser-linux-x86_64", workflow)


if __name__ == "__main__":
    unittest.main()
