import { defineConfig } from "@playwright/test";

// NESER_WEB_PORT is set per session by .cerebro/cerebro/scripts/smoke-port (port_base/port_env in
// .cerebro/project.conf), so parallel sessions never reuse each other's server; 8000 otherwise.
// scripts/run_web.sh reads the same variable, which the web server command inherits.
const WEB_APP_PORT = process.env.NESER_WEB_PORT || "8000";
const WEB_APP_URL = `http://127.0.0.1:${WEB_APP_PORT}`;
const WEB_APP_SERVER_COMMAND = "bash scripts/build_web.sh && bash scripts/run_web.sh";

export default defineConfig({
    testDir: ".",
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
