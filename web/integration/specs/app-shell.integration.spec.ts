import { readFileSync } from "node:fs";
import path from "node:path";
import { test, expect } from "@playwright/test";
import { loadRomFromFileInput, startFromBundledRom, waitForIdleState, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";

const ESSENTIAL_CONTROL_SELECTORS = ["#library", "#library-choose", "#library-open", "#settings-open"];

test.describe("web app shell", () => {
    test("renders essential controls", async ({ page }) => {
        await page.goto("/");

        for (const selector of ESSENTIAL_CONTROL_SELECTORS) {
            await expect(page.locator(selector)).toBeVisible();
        }
        // Before a game runs the library shows, not the picture and its control bar.
        await expect(page.locator("#emulation-controls")).toBeHidden();
    });

    test("starts a picked game at once, names it in the top bar, and Eject returns to the library with it ready to start again", async ({ page }) => {
        await page.goto("/");
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(page.locator("#library")).toBeHidden();
        await expect(page.locator("#pause")).toBeVisible();
        await expect(page.locator("#game-title")).toBeVisible();
        await expect(page.locator("#game-system")).not.toBeEmpty();

        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expect(page.locator("#library")).toBeVisible();
        await expect(page.locator("#game-title")).toBeHidden();
        await expect(page.locator("#library-again-name")).toHaveText("cpu");
        await page.locator("#start").click();
        await waitForRunningState(page);
    });

    test("opens the library over a running game, which waits paused, and goes back with Back to the game or Esc", async ({ page }) => {
        await startFromBundledRom(page);

        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await waitForPausedState(page);
        await page.locator("#library-back").click();
        await expect(page.locator("#library")).toBeHidden();
        await waitForRunningState(page);

        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await page.keyboard.press("Escape");
        await expect(page.locator("#library")).toBeHidden();
        await waitForRunningState(page);
    });

    test("searches the bundled list and filters it by system", async ({ page }) => {
        await page.route("**/roms/roms.json", (route) =>
            route.fulfill({ json: { roms: ["nes/one.nes", "nes/two.nes", "gb/three.gb", "gbc/four.gbc"] } })
        );
        await page.goto("/");
        const items = page.locator(".library-item .library-item-name");
        await expect(items).toHaveText(["three", "four", "one", "two"]);
        await expect(page.locator("#library-count")).toHaveText("4");

        await page.locator("#library-search").fill("TW");
        await expect(items).toHaveText(["two"]);
        await page.locator("#library-search").fill("");
        await page.locator('#library-systems [data-system="GB"]').click();
        await expect(items).toHaveText(["three", "four"]);
        await page.locator("#library-search").fill("zzz");
        await expect(page.locator(".library-empty")).toHaveText("No test ROMs match “zzz”.");
    });

    test("loads a game file dropped anywhere on the page", async ({ page }) => {
        await page.goto("/");
        const bytes = [...readFileSync(path.join(process.cwd(), "roms", "nes", "automated_tests", "blargg_nes_cpu_test5", "cpu.nes"))];
        const dataTransfer = await page.evaluateHandle((data) => {
            const transfer = new DataTransfer();
            transfer.items.add(new File([new Uint8Array(data)], "dropped.nes"));
            return transfer;
        }, bytes);
        await page.dispatchEvent("body", "dragover", { dataTransfer });
        await expect(page.locator("#library-drop")).toHaveClass(/library-hero-drag/);
        await page.dispatchEvent("body", "drop", { dataTransfer });
        await waitForRunningState(page);
        await expect(page.locator("#game-name")).toHaveText("dropped");
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
        await expect(page.locator("#library-choose")).toBeVisible();

        await loadRomFromFileInput(page);
        releaseWasm();

        await waitForRunningState(page);
        await expect(page.locator(".neser-toast", { hasText: "Unsupported file type" })).toHaveCount(0);
    });
});
