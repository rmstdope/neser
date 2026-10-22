import { test, expect } from "@playwright/test";
import { loadRomFromFileInput, waitForRunningState } from "../helpers/lifecycle.helpers";

const ESSENTIAL_CONTROL_SELECTORS = ["#screen", "#start", "#pause", "#stop"];

test.describe("web app shell", () => {
    test("renders essential controls", async ({ page }) => {
        await page.goto("/");

        for (const selector of ESSENTIAL_CONTROL_SELECTORS) {
            await expect(page.locator(selector)).toBeVisible();
        }
    });

    test("shows a refused file's message as one line in the theme's error colour, and no REC badge while idle", async ({ page }) => {
        await page.goto("/");
        await expect(page.locator("#rec-overlay")).toBeHidden();

        await page.locator("#rom").setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("not a game") });

        const status = page.locator("#status");
        await expect(status).toHaveText("Unsupported file type .txt");
        await expect(status).toBeVisible();
        const box = (await status.boundingBox())!;
        expect(box.height).toBeLessThan(40);
        expect(box.width).toBeGreaterThan(box.height * 3);
        await expect(status).toHaveCSS("color", "rgb(242, 113, 138)");
        // Readable: the error colours the words, never a box behind them.
        await expect(status).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
    });

    test("accepts every supported ROM file in the file picker once start-up installs the wasm table", async ({ page }) => {
        await page.goto("/");

        await expect(page.locator("#rom")).toHaveAttribute(
            "accept",
            ".nes,.gb,.gbc,.cgb,.gba,.sfc,.smc,application/octet-stream"
        );
    });
});

test.describe("a ROM chosen before the wasm module has loaded", () => {
    test("starts once the module arrives instead of being rejected as unsupported", async ({ page }) => {
        let releaseWasm: () => void = () => {};
        const wasmHeld = new Promise<void>((resolve) => {
            releaseWasm = resolve;
        });
        await page.route("**/*.wasm", async (route) => {
            await wasmHeld;
            await route.continue();
        });
        await page.goto("/");
        await expect(page.locator("#start")).toBeVisible();

        await loadRomFromFileInput(page);
        releaseWasm();

        await waitForRunningState(page);
        await expect(page.locator(".neser-toast", { hasText: "Unsupported file type" })).toHaveCount(0);
    });
});
