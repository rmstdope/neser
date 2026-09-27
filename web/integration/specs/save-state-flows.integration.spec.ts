import { test, expect } from "@playwright/test";
import {
    openApp,
    startFromBundledRom,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const SAVE_STATE_SECTION_SELECTOR = "#save-state-section";
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

        await saveButton.click();

        await expect(page.locator(SAVE_STATE_SECTION_SELECTOR)).toHaveAttribute("data-save-state", "saved", { timeout: 5000 });
        await expect(loadButton).toBeEnabled();
    });

    test("Given state has been saved, when load state is clicked in same session, then state restores successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        const saveStateSection = page.locator(SAVE_STATE_SECTION_SELECTOR);

        await saveButton.click();
        await expect(saveStateSection).toHaveAttribute("data-save-state", "saved", { timeout: 5000 });

        await loadButton.click();
        await expect(saveStateSection).toHaveAttribute("data-save-state", "loaded", { timeout: 5000 });
    });

    test("Given no saved state exists, when page loads, then load button is disabled", async ({ page }) => {
        // Playwright provides a fresh browser context per test, so no saved state exists
        await startFromBundledRom(page);

        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        await expect(page.locator(SAVE_STATE_SECTION_SELECTOR)).toHaveAttribute("data-save-state", "empty");
        await expect(loadButton).toBeDisabled();
    });

    test("Given save state button exists, when clicked multiple times, then state updates successfully", async ({ page }) => {
        await startFromBundledRom(page);

        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        const saveStateSection = page.locator(SAVE_STATE_SECTION_SELECTOR);

        // Alternate the presses so each one moves the slot and is observed, never pre-satisfied.
        for (let round = 0; round < 2; round++) {
            await saveButton.click();
            await expect(saveStateSection).toHaveAttribute("data-save-state", "saved", { timeout: 5000 });

            await loadButton.click();
            await expect(saveStateSection).toHaveAttribute("data-save-state", "loaded", { timeout: 5000 });
        }
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

        await saveButton.click();
        await expect(page.locator(SAVE_STATE_SECTION_SELECTOR)).toHaveAttribute("data-save-state", "saved", { timeout: 5000 });

        await loadButton.click();

        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State loaded" })).toBeVisible({
            timeout: 5000
        });
    });
});

