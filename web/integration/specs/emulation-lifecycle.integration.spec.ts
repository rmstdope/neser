import { test, expect } from "@playwright/test";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    startFromBundledRom,
    waitForIdleState,
    waitForRunningState
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

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
        await expect(page.locator(PAUSE_BUTTON_SELECTOR)).toHaveText("Resume");

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
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
});