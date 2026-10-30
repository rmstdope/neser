# nr-7sn1: ubuntu-latest moved to 26.04 under us, and a cache hid it

**What happened:** The web-container job failed on PR #3395 with "Playwright does not support chromium on ubuntu26.04-x64". It failed before any test ran, and with nothing in the repository having changed. GitHub had moved `ubuntu-latest` to Ubuntu 26.04, and our lockfile's Playwright (1.60.0) refuses that image. web-integration ran on the same image and stayed green, but only because its Chromium came from an Actions cache keyed on the lockfile alone, so its install step never ran. That was luck, not health.

**Why:** Jobs that download a host-specific browser followed a floating runner label. A cache keyed without the image skipped the one step that checks host support.

**Cost:** Every pull request that ran web-container was blocked (PR #3395 failed twice; nr-ri8n and nr-1wbx queued behind it). One P0 bug pass.

**Prevent by:** `scripts/test_playwright_runner_image.py` requires every job that runs `playwright install` to name `ubuntu-24.04`, and every Playwright cache key to carry that image. Moving the image is therefore a deliberate change, made together with a Playwright upgrade (1.64.1 is the first release seen to list ubuntu26.04).

**Seen before:** No.
