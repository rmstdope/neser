import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
    loadRomFromFileInput,
    openApp,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";

const FILTER_SELECTOR = "#filter-toggle";

async function loadSnesRom(page: Page) {
    await page.locator("#rom").setInputFiles({
        name: "filter.sfc",
        mimeType: "application/octet-stream",
        buffer: makeMinimalSnesRomBytes()
    });
    await waitForRunningState(page);
}

test.describe("SNES screen filters (nr-gtx)", () => {
    test("Given the first SNES game on the page, then it starts on None and Filter and F4 cycle None, NTSC, CRT, also while paused", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await loadSnesRom(page);
        await expect(filter).toHaveText("Filter: None");

        await filter.click();
        await expect(filter).toHaveText("Filter: NTSC");
        await filter.click();
        await expect(filter).toHaveText("Filter: CRT");
        await filter.click();
        await expect(filter).toHaveText("Filter: None");

        await filter.blur();
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: NTSC");

        await page.locator("#pause").click();
        await waitForPausedState(page);
        await filter.click();
        await expect(filter).toHaveText("Filter: CRT");
        await filter.blur();
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: None");
    });

    test("Given an SNES game after an untouched NES game, then it keeps the NES game's NTSC", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(filter).toHaveText("Filter: NTSC");

        await loadSnesRom(page);
        await expect(filter).toHaveText("Filter: NTSC");
    });

    test("Given an SNES game after a Game Boy game on its own look, then it starts on None", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await page.locator("#rom").setInputFiles({
            name: "dmg-acid2.gb",
            mimeType: "application/octet-stream",
            buffer: readFileSync(path.join(process.cwd(), "roms", "gb", "automated_tests", "acid", "dmg-acid2.gb"))
        });
        await waitForRunningState(page);
        await expect(filter).toHaveText("Filter: Game Boy");

        await loadSnesRom(page);
        await expect(filter).toHaveText("Filter: None");
    });
});
