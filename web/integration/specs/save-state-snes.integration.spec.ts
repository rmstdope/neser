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
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State saved" })).toBeVisible({
            timeout: 10_000
        });

        await expect(loadButton).toBeEnabled();
        await loadButton.click({ force: true });
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State loaded" })).toBeVisible({
            timeout: 10_000
        });
    });
});
