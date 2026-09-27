import { test, expect, Page } from "@playwright/test";
import { collectBrowserErrors } from "../helpers/browser-errors.helpers";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const FILTER_SELECTOR = "#filter-toggle";
// The LCD Grid's console art; GBA SP's 1 KB look-up table is inlined into the bundle and never fetched.
const BORDER_REQUEST = "**/gba-border-square-4x*.png";

async function loadGbaRom(page: Page) {
    await loadGbaRomFromFileInput(page);
    await waitForRunningState(page);
}

test.describe("GBA screen filters on the web (nr-0pe)", () => {
    test("Given the first GBA game on the page, then it starts on None and Filter and F4 cycle the five looks, also while paused", async ({ page }) => {
        const errors = collectBrowserErrors(page);
        // A pass WebGL refuses to draw reports only through the console, as a warning.
        const glErrors: string[] = [];
        page.on("console", (msg) => {
            if (/INVALID_|too many errors/.test(msg.text())) glErrors.push(msg.text());
        });
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await loadGbaRom(page);
        await expect(filter).toHaveText("Filter: None");

        for (const name of ["AGB-001", "Switch Online", "GBA SP", "LCD Grid", "None"]) {
            await filter.click();
            await expect(filter).toHaveText(`Filter: ${name}`);
        }

        await filter.blur();
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: AGB-001");

        await page.locator("#pause").click();
        await waitForPausedState(page);
        await filter.click();
        await expect(filter).toHaveText("Filter: Switch Online");
        await filter.blur();
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: GBA SP");
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: LCD Grid");

        // Each look compiled, fetched and drew: nothing sent the button back or stopped the game.
        await page.locator("#pause").click();
        await waitForRunningState(page);
        await page.waitForTimeout(500);
        await expect(filter).toHaveText("Filter: LCD Grid");
        await expect(page.locator("#status")).not.toContainText("Rendering error");
        expect(errors.pageErrors).toEqual([]);
        expect(glErrors).toEqual([]);
        expect(errors.consoleErrors.filter((e) => /shader|framebuffer|filter/i.test(e))).toEqual([]);
    });

    test("Given a look being fetched the first time, then the button names it at once", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);
        let release: () => void = () => {};
        const held = new Promise<void>((resolve) => { release = resolve; });
        await page.route(BORDER_REQUEST, async (route) => {
            await held;
            await route.continue();
        });

        await loadGbaRom(page);
        for (let i = 0; i < 4; i++) await filter.click();
        await expect(filter).toHaveText("Filter: LCD Grid");
        await page.waitForTimeout(300);
        await expect(filter).toHaveText("Filter: LCD Grid");
        release();
        await page.waitForTimeout(300);
        await expect(filter).toHaveText("Filter: LCD Grid");
    });

    test("Given a look that cannot be fetched, then the button goes back and the next press moves on", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);
        await page.route(BORDER_REQUEST, (route) => route.abort());

        await loadGbaRom(page);
        for (let i = 0; i < 3; i++) await filter.click();
        await expect(filter).toHaveText("Filter: GBA SP");
        await filter.click();
        await expect(filter).toHaveText("Filter: GBA SP");

        // Back online: the next press moves on from GBA SP, and the look is fetched this time.
        await page.unroute(BORDER_REQUEST);
        await filter.click();
        await expect(filter).toHaveText("Filter: LCD Grid");
        await page.waitForTimeout(500);
        await expect(filter).toHaveText("Filter: LCD Grid");
    });

    test("Given a GBA game after an NES game, then it starts on None", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(filter).toHaveText("Filter: NTSC");

        await loadGbaRom(page);
        await expect(filter).toHaveText("Filter: None");
    });

    test("Given an NES game after a GBA game on a GBA look, then it starts on the NES game's own look", async ({ page }) => {
        await openApp(page);
        const filter = page.locator(FILTER_SELECTOR);

        await loadGbaRom(page);
        await filter.click();
        await expect(filter).toHaveText("Filter: AGB-001");

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(filter).toHaveText("Filter: NTSC");
    });
});
