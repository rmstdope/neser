import { test, expect } from "@playwright/test";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    startFromBundledRom,
    waitForIdleState,
    waitForRunningState,
    waitForPausedState
} from "../helpers/lifecycle.helpers";

const ESSENTIAL_CONTROL_SELECTORS = ["#screen", "#start", "#pause", "#stop", "#reset", "#status"];
const START_BUTTON_SELECTOR = "#start";
const PAUSE_BUTTON_SELECTOR = "#pause";
const STOP_BUTTON_SELECTOR = "#stop";
const RESET_BUTTON_SELECTOR = "#reset";
const SAVE_STATE_BUTTON_SELECTOR = "#save-state";
const LOAD_STATE_BUTTON_SELECTOR = "#load-state";

test.describe("Phase 1 critical path lifecycle", () => {
    test("Given app is opened, when shell loads, then essential controls and idle status are visible", async ({ page }) => {
        await openApp(page);

        for (const selector of ESSENTIAL_CONTROL_SELECTORS) {
            await expect(page.locator(selector)).toBeVisible();
        }

        await waitForIdleState(page);
    });

    test("Given bundled ROM exists, when it is selected from bundled list, then emulator enters running state", async ({ page }) => {
        await startFromBundledRom(page);

        await waitForRunningState(page);
        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
    });

    test("Given emulator is running, when Pause/Resume is toggled, then paused and running states alternate", async ({ page }) => {
        await startFromBundledRom(page);

        await page.locator(PAUSE_BUTTON_SELECTOR).evaluate((button: HTMLButtonElement) => button.click());
        await waitForPausedState(page);

        await page.locator(PAUSE_BUTTON_SELECTOR).evaluate((button: HTMLButtonElement) => button.click());
        await waitForRunningState(page);
    });

    test("Given emulator is running, when Stop is clicked, then app returns to idle-safe state", async ({ page }) => {
        await startFromBundledRom(page);

        await page.locator(STOP_BUTTON_SELECTOR).click();

        await waitForIdleState(page);
        await expect(page.locator(START_BUTTON_SELECTOR)).toBeEnabled();
    });

    test("Given emulator is running, when Reset is clicked, then session resets while emulation remains active", async ({ page }) => {
        await startFromBundledRom(page);

        await page.locator(RESET_BUTTON_SELECTOR).click();

        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
    });

    test("Given no ROM has been started, when page loads, then save/load state controls are disabled", async ({ page }) => {
        await openApp(page);

        await expect(page.locator(SAVE_STATE_BUTTON_SELECTOR)).toBeDisabled();
        await expect(page.locator(LOAD_STATE_BUTTON_SELECTOR)).toBeDisabled();
    });

    test("Given a GBA ROM is loaded, when a NES ROM is loaded afterwards, then both sessions can run without reload", async ({ page }) => {
        await openApp(page);

        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
    });

    test("Given a GBA ROM is running, when Reset is clicked, then emulation remains active", async ({ page }) => {
        const webGlErrors: string[] = [];
        page.on("console", (msg) => {
            const text = msg.text();
            if (text.includes("GL_INVALID_OPERATION")) {
                webGlErrors.push(text);
            }
        });
        await openApp(page);

        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);

        await page.locator(RESET_BUTTON_SELECTOR).click();

        await waitForRunningState(page);
        await expect(page.locator(PAUSE_BUTTON_SELECTOR)).toHaveText("Pause");
        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
        expect(webGlErrors).toHaveLength(0);
    });

    test("Given a GBA ROM is running, when another GBA ROM is loaded, then the new session runs without page reload", async ({ page }) => {
        const webGlErrors: string[] = [];
        page.on("console", (msg) => {
            const text = msg.text();
            if (text.includes("GL_INVALID_OPERATION")) {
                webGlErrors.push(text);
            }
        });
        await openApp(page);

        await loadGbaRomFromFileInput(page, "first.gba");
        await waitForRunningState(page);

        await loadGbaRomFromFileInput(page, "second.gba");
        await waitForRunningState(page);

        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
        expect(webGlErrors).toHaveLength(0);
    });

    test("Given a GBA ROM is loaded, when the canvas is resized, then the GBA aspect ratio is preserved", async ({ page }) => {
        await openApp(page);

        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);

        const canvasSize = await page.locator("#screen").evaluate((canvas) => {
            if (!(canvas instanceof HTMLCanvasElement)) {
                throw new Error("Expected #screen to be a canvas");
            }
            return { width: canvas.width, height: canvas.height };
        });

        expect(canvasSize.width / canvasSize.height).toBeCloseTo(240 / 160, 2);
    });
});