import { test, expect } from "@playwright/test";
import {
    openApp,
    startFromBundledRom,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const SAVE_STATE_BUTTON_SELECTOR = "#save-state";
const LOAD_STATE_BUTTON_SELECTOR = "#load-state";
const STATUS_SELECTOR = "#status";
const TOAST_SELECTOR = ".neser-toast";

test.describe("Phase 2 save-state flows", () => {
    test("Given emulator has started, when save state is clicked, then state is stored successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        // Save button should be enabled after starting
        await expect(saveButton).toBeEnabled();

        // Click save state
        await saveButton.click();

        // Verify success: load button becomes enabled — durable signal that save completed
        await expect(loadButton).toBeEnabled({ timeout: 5000 });

        // Verify the save completed without errors
        // The load button becoming enabled confirms persistence occurred
    });

    test("Given state has been saved, when load state is clicked in same session, then state restores successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        // Save state first; wait for load button to be enabled — durable signal that save completed
        await saveButton.click();
        await expect(loadButton).toBeEnabled({ timeout: 5000 });

        // Click load state
        await loadButton.click();

        // Verify success: data-save-state-status becomes "loaded" — durable, non-racy signal
        await expect(saveButton).toHaveAttribute("data-save-state-status", "loaded", { timeout: 5000 });
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

        // First save; wait for load button to become enabled — durable sync signal
        await saveButton.click();
        await expect(loadButton).toBeEnabled({ timeout: 5000 });

        // Second save (should overwrite); wait for durable saved signal
        await saveButton.click();
        await expect(saveButton).toHaveAttribute("data-save-state-status", "saved", { timeout: 5000 });

        // Load should still work; wait for durable loaded signal
        await loadButton.click();
        await expect(saveButton).toHaveAttribute("data-save-state-status", "loaded", { timeout: 5000 });
    });

    test("Given emulator has started, when save state is clicked, then a toast notification is shown", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);

        await saveButton.click();

        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State saved" })).toBeVisible({
            timeout: 5000
        });
    });

    test("Given state has been saved, when load state is clicked, then a toast notification is shown", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        // Save first; use durable signal to confirm save completed before clicking load
        await saveButton.click();
        await expect(loadButton).toBeEnabled({ timeout: 5000 });

        await loadButton.click();

        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State loaded" })).toBeVisible({
            timeout: 5000
        });
    });
});

