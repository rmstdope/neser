import { expect, test } from "@playwright/test";
import { openApp, waitForRunningState } from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";

const SAVE_STATE_SECTION_SELECTOR = "#save-state-section";
const SAVE_STATE_BUTTON_SELECTOR = "#save-state";
const LOAD_STATE_BUTTON_SELECTOR = "#load-state";
const TOAST_SELECTOR = ".neser-toast";

test.describe("SNES save-state parity", () => {
    test("Given a SNES ROM is running, when save and load are used, then state persists", async ({ page }) => {
        await openApp(page);
        await page.locator("#rom").setInputFiles({
            name: "suite.sfc",
            mimeType: "application/octet-stream",
            buffer: makeMinimalSnesRomBytes()
        });

        await waitForRunningState(page);

        const saveStateSection = page.locator(SAVE_STATE_SECTION_SELECTOR);
        const saveButton = page.locator(SAVE_STATE_BUTTON_SELECTOR);
        const loadButton = page.locator(LOAD_STATE_BUTTON_SELECTOR);

        await expect(saveStateSection).toBeVisible();
        await expect(saveButton).toBeEnabled();
        await expect(loadButton).toBeDisabled();

        await saveButton.click({ force: true });
        // Wait for load button to become enabled — this is a durable signal that the save
        // completed successfully (saveStateAvailable becomes true and buttons are updated).
        await expect(loadButton).toBeEnabled({ timeout: 10_000 });

        await loadButton.click({ force: true });
        // Wait for the save-state button's data-save-state-status attribute to become "loaded"
        // — this is a durable, non-racy signal set after the load operation completes.
        await expect(saveButton).toHaveAttribute("data-save-state-status", "loaded", { timeout: 10_000 });
    });
});
