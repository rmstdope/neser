import { expect, test } from "@playwright/test";
import { openApp, waitForRunningState, waitForPausedState } from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";
const START_BUTTON_SELECTOR = "#start";
const PAUSE_BUTTON_SELECTOR = "#pause";

test.describe("SNES frontend flow", () => {
    test("Given a SNES ROM is loaded from file input, when the emulator starts, then it runs and can pause and resume", async ({ page }) => {
        await openApp(page);
        await page.locator("#rom").setInputFiles({
            name: "flow.sfc",
            mimeType: "application/octet-stream",
            buffer: makeMinimalSnesRomBytes()
        });

        await waitForRunningState(page);
        await expect(page.locator(START_BUTTON_SELECTOR)).toBeDisabled();
        await expect(page.locator(PAUSE_BUTTON_SELECTOR)).toHaveText("Pause");

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
        await waitForPausedState(page);

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
        await waitForRunningState(page);
    });
});
