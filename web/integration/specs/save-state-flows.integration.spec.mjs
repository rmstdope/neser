import { test, expect } from "@playwright/test";
import {
    openApp,
    startFromBundledRom,
    waitForRunningState
} from "../helpers/lifecycle.helpers.mjs";

const SAVE_STATE_BUTTON_SELECTOR = "#save-state";
const LOAD_STATE_BUTTON_SELECTOR = "#load-state";
const STATUS_SELECTOR = "#status";

test.describe("Phase 2 save-state flows", () => {
    test("Given emulator has started, when save state is clicked, then state is stored successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const statusLabel = page.locator(STATUS_SELECTOR);

        // Save button should be enabled after starting
        await expect(saveButton).toBeEnabled();

        // Click save state
        await saveButton.click();

        // Verify success status message
        await expect(statusLabel).toContainText("State saved", { timeout: 5000 });

        // Verify the save completed without errors
        // The fact that we see "State saved" confirms persistence occurred
    });

    test("Given state has been saved, when load state is clicked in same session, then state restores successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);
        const statusLabel = page.locator(STATUS_SELECTOR);

        // Save state first
        await saveButton.click();
        await expect(statusLabel).toContainText("State saved", { timeout: 5000 });

        // Load button should now be enabled (state exists)
        await expect(loadButton).toBeEnabled();

        // Click load state
        await loadButton.click();

        // Verify success status message
        await expect(statusLabel).toContainText("State loaded", { timeout: 5000 });

        // Verify emulator is still running after load
        await waitForRunningState(page);
    });

    test("Given no saved state exists, when page loads, then load button is disabled", async ({ page }) => {
        // Playwright provides a fresh browser context per test, so no saved state exists
        await startFromBundledRom(page);

        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        // Load button should be disabled (no saved state in fresh context)
        await expect(loadButton).toBeDisabled();

        // We've verified graceful handling: button is disabled when no state exists
    });

    test("Given save state button exists, when clicked multiple times, then state updates successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);
        const statusLabel = page.locator(STATUS_SELECTOR);

        // First save
        await saveButton.click();
        await expect(statusLabel).toContainText("State saved", { timeout: 5000 });

        // Wait a moment for the save to complete
        await page.waitForTimeout(200);

        // Second save (should overwrite)
        await saveButton.click();
        await expect(statusLabel).toContainText("State saved", { timeout: 5000 });

        // Load should still work
        await loadButton.click();
        await expect(statusLabel).toContainText("State loaded", { timeout: 5000 });

        // Verify emulator is still running
        await waitForRunningState(page);
    });
});

