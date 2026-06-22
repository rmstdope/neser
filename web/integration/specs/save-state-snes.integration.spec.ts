import { expect, test } from "@playwright/test";
import { openApp, waitForRunningState } from "../helpers/lifecycle.helpers";

const SAVE_STATE_SECTION_SELECTOR = "#save-state-section";
const SAVE_STATE_BUTTON_SELECTOR = "#save-state";
const LOAD_STATE_BUTTON_SELECTOR = "#load-state";
const TOAST_SELECTOR = ".neser-toast";

function makeMinimalSnesRomBytes() {
    const rom = Buffer.alloc(0x10000);
    const header = 0x7FC0;
    rom.write("SNES TEST ROM        ", header, "ascii");
    rom[header + 0x3C] = 0x00;
    rom[header + 0x3D] = 0x80;
    rom[header + 0xD5] = 0x20;
    rom[header + 0xD6] = 0x00;
    rom[header + 0xD7] = 0x07;
    rom[header + 0xD8] = 0x00;
    rom[header + 0xD9] = 0x00;
    rom[header + 0xDC] = 0x34;
    rom[header + 0xDD] = 0x12;
    rom[header + 0xDE] = 0xCB;
    rom[header + 0xDF] = 0xED;
    rom[0x0000] = 0xEA;
    return rom;
}

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

        await saveButton.click();
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State saved" })).toBeVisible({
            timeout: 5000
        });

        await expect(loadButton).toBeEnabled();
        await loadButton.click();
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "State loaded" })).toBeVisible({
            timeout: 5000
        });
    });
});
