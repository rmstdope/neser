import { defineConfig } from "@playwright/test";

// NESER_WEB_PORT is set per session by .cerebro/cerebro/scripts/smoke-port (port_base/port_env in
// .cerebro/project.conf), so parallel sessions never reuse each other's server; 8000 otherwise.
// `pnpm run serve` reads the same variable, which the web server command inherits.
const WEB_APP_PORT = process.env.NESER_WEB_PORT || "8000";
const WEB_APP_URL = `http://127.0.0.1:${WEB_APP_PORT}`;
// NESER_WEB_PREBUILT=1 serves a dist/ an earlier step already built with `pnpm run build`, as CI
// does, so the wasm is compiled once per job (nr-1pl).
const WEB_APP_SERVER_COMMAND =
    process.env.NESER_WEB_PREBUILT === "1" ? "pnpm run serve" : "pnpm run build && pnpm run serve";

export default defineConfig({
    testDir: ".",
    // Every test opens its own page, so tests can run in any order on any worker; this is what
    // lets the CI job's --shard=1/2 and --shard=2/2 split the suite by test rather than by file.
    fullyParallel: true,
    // One browser at a time on CI. Each browser's SwiftShader GPU process burns about 1.6 cores, so
    // Playwright's default of two workers on the four-vCPU runner starved Chrome's own process:
    // new contexts hung for ~40 s ("Failed to find browser context") and clicks timed out
    // (nr-oi18). One worker finished the larger shard in half the time, with nothing flaky.
    workers: process.env.CI ? 1 : undefined,
    timeout: 45_000,
    retries: 1,
    use: {
        baseURL: WEB_APP_URL,
        headless: true,
        trace: "retain-on-failure",
        launchOptions: {
            args: ["--use-angle=swiftshader", "--use-gl=angle"]
        }
    },
    webServer: {
        command: WEB_APP_SERVER_COMMAND,
        url: WEB_APP_URL,
        reuseExistingServer: true,
        // A freshly prepared worktree builds the wasm cold: about two minutes of cargo alone.
        timeout: 600_000
    }
});
