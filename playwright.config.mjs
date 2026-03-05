import { defineConfig } from "@playwright/test";

const WEB_APP_URL = "http://127.0.0.1:8000";
const WEB_APP_SERVER_COMMAND = "bash scripts/build_web.sh && bash scripts/run_web.sh";

export default defineConfig({
    testDir: ".",
    timeout: 120_000,
    retries: process.env.CI ? 1 : 0,
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
        timeout: 120_000
    }
});
