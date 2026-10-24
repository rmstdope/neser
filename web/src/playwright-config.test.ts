import { afterEach, describe, expect, it, vi } from "vitest";

// The web integration suite's server port comes from NESER_WEB_PORT, which
// .cerebro/cerebro/scripts/smoke-port sets per session, so parallel sessions never test each
// other's build (nr-bsr).
async function loadConfig() {
    vi.resetModules();
    return (await import("../../playwright.config")).default;
}

describe("playwright.config", () => {
    afterEach(() => {
        vi.unstubAllEnvs();
    });

    it("uses NESER_WEB_PORT for baseURL and the web server url", async () => {
        vi.stubEnv("NESER_WEB_PORT", "8123");

        const config = await loadConfig();

        expect(config.use?.baseURL).toBe("http://127.0.0.1:8123");
        expect(config.webServer).toMatchObject({ url: "http://127.0.0.1:8123" });
    });

    it("defaults to port 8000", async () => {
        vi.stubEnv("NESER_WEB_PORT", undefined);

        const config = await loadConfig();

        expect(config.use?.baseURL).toBe("http://127.0.0.1:8000");
        expect(config.webServer).toMatchObject({ url: "http://127.0.0.1:8000" });
    });

    it("gives the web server long enough for a cold wasm build in a fresh worktree", async () => {
        const config = await loadConfig();

        expect(config.webServer).toMatchObject({ timeout: 600_000 });
    });

    it("runs tests fully parallel so CI shards balance by test, not by file", async () => {
        // The web-integration CI job runs as --shard=1/2 and --shard=2/2 (nr-ks6). Without
        // fullyParallel Playwright shards by file, and one file holds a third of the suite.
        const config = await loadConfig();

        expect(config.fullyParallel).toBe(true);
    });

    it("runs one worker on CI, where two browsers starve the four-core runner", async () => {
        // Each browser's SwiftShader GPU process burns about 1.6 cores. With Playwright's default of
        // two workers on CI's four vCPUs, Chrome's own process fell ~40 s behind: new contexts hung
        // ("Failed to find browser context") and clicks timed out (nr-oi18). One worker ran the
        // shard in half the time with nothing flaky.
        vi.stubEnv("CI", "true");

        const config = await loadConfig();

        expect(config.workers).toBe(1);
    });

    it("leaves the worker count to Playwright off CI", async () => {
        vi.stubEnv("CI", "");

        const config = await loadConfig();

        expect(config.workers).toBeUndefined();
    });
});
