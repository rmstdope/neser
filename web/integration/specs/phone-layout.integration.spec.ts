import { devices, expect, test } from "@playwright/test";
import { loadGbaRomFromFileInput, loadRomFromFileInput, openApp, waitForIdleState, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";

// A device profile names its browser type, which a describe block may not set; the rest applies.
const { defaultBrowserType: _portraitBrowser, ...phonePortrait } = devices["Pixel 7"];
const { defaultBrowserType: _landscapeBrowser, ...phoneLandscape } = devices["Pixel 7 landscape"];

test.describe("Phone, held upright", () => {
    test.use(phonePortrait);

    test("Given a running game, then the picture spans the screen, the quick bar sits under it, and the controller below", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        const viewport = page.viewportSize()!;
        const picture = (await page.locator("#screen").boundingBox())!;
        const bar = (await page.locator("#handheld-bar").boundingBox())!;
        const pad = (await page.locator(".touch-joystick").boundingBox())!;
        expect(Math.round(picture.width)).toBeGreaterThanOrEqual(viewport.width - 2);
        expect(bar.y).toBeGreaterThanOrEqual(picture.y + picture.height - 1);
        expect(pad.y).toBeGreaterThanOrEqual(bar.y + bar.height - 1);
        expect(pad.y + pad.height).toBeLessThanOrEqual(viewport.height + 1);

        await page.locator("#hh-pause").tap();
        await waitForPausedState(page);
        await page.locator("#hh-pause").tap();
        await waitForRunningState(page);
    });

    test("Given a running game, when ⋯ is tapped, then the sheet opens with the game paused, and Eject returns to the library", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#hh-more").tap();
        const sheet = page.locator("#handheld-sheet");
        await expect(sheet).toBeVisible();
        await waitForPausedState(page);
        await sheet.getByText("Eject").tap();
        await waitForIdleState(page);
        await expect(sheet).toBeHidden();
        await expect(page.locator("#library")).toBeVisible();
    });

    const startAndOpenSheet = async (page: import("@playwright/test").Page) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#hh-more").tap();
        await expect(page.locator("#handheld-sheet")).toBeVisible();
        await waitForPausedState(page);
    };

    test("Given the open sheet, then the top bar stays clear and says Paused, and a tap on the picture closes it and resumes", async ({ page }) => {
        await startAndOpenSheet(page);
        await expect(page.locator("#fps-counter")).toHaveText("Paused");
        const backdropTop = await page.locator("#handheld-sheet").evaluate((el) => parseFloat(getComputedStyle(el, "::backdrop").top));
        const header = (await page.locator("header").boundingBox())!;
        expect(backdropTop).toBeGreaterThanOrEqual(header.y + header.height - 1);
        const picture = (await page.locator("#screen").boundingBox())!;
        await page.touchscreen.tap(picture.x + picture.width / 2, picture.y + picture.height / 2);
        await expect(page.locator("#handheld-sheet")).toBeHidden();
        await waitForRunningState(page);
        // Playing again, the chip shows the frame rate as a whole number.
        await expect(page.locator("#fps-counter")).toHaveText(/^\d+$/, { timeout: 10_000 });
    });

    test("Given the open sheet, when it is dragged down, then it closes and the game resumes; a short drag springs back", async ({ page }) => {
        await startAndOpenSheet(page);
        const grab = (await page.locator(".hh-grab").boundingBox())!;
        const drag = async (dy: number) => {
            await page.mouse.move(grab.x + grab.width / 2, grab.y + 2);
            await page.mouse.down();
            await page.mouse.move(grab.x + grab.width / 2, grab.y + 2 + dy, { steps: 5 });
            await page.mouse.up();
        };
        await drag(20);
        await expect(page.locator("#handheld-sheet")).toBeVisible();
        await drag(160);
        await expect(page.locator("#handheld-sheet")).toBeHidden();
        await waitForRunningState(page);
    });

    test("Given the open sheet, when a tap lands on its own edge, then it stays open", async ({ page }) => {
        await startAndOpenSheet(page);
        const sheet = (await page.locator("#handheld-sheet").boundingBox())!;
        const row = (await page.locator("#hh-settings").boundingBox())!;
        await page.touchscreen.tap(sheet.x + 6, row.y + row.height / 2);
        await page.touchscreen.tap(sheet.x + sheet.width / 2, sheet.y + sheet.height - 6);
        await expect(page.locator("#handheld-sheet")).toBeVisible();
        await waitForPausedState(page);
    });

    test("Given a game the player had paused, when the sheet opens and closes, then it stays paused", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#hh-pause").tap();
        await waitForPausedState(page);
        await page.locator("#hh-more").tap();
        await page.keyboard.press("Escape");
        await expect(page.locator("#handheld-sheet")).toBeHidden();
        await waitForPausedState(page);
    });

    test("Given the sheet, when Settings is opened from it, then the game stays paused until Settings closes", async ({ page }) => {
        await startAndOpenSheet(page);
        await page.locator("#hh-settings").tap();
        await expect(page.locator("#settings-panel")).toBeVisible();
        await page.waitForTimeout(300);
        await waitForPausedState(page);
        await page.locator("#settings-close").tap();
        await waitForRunningState(page);
    });

    test("Given the sheet, when Keys and controls is opened from it, then the game stays paused until it closes", async ({ page }) => {
        await startAndOpenSheet(page);
        await page.locator("#hh-help").tap();
        await expect(page.locator("#help-panel")).toBeVisible();
        await page.waitForTimeout(300);
        await waitForPausedState(page);
        await page.locator("#help-close").tap();
        await waitForRunningState(page);
    });

    test("Given the sheet, when Palette is tapped, then the palettes' short list opens, and a pick names the new one", async ({ page }) => {
        await startAndOpenSheet(page);
        await page.locator("#hh-palette").tap();
        const items = page.locator(".choice-menu [role='menuitemradio']");
        await expect(items.first()).toBeVisible();
        // Every entry lies on the sheet, where a finger can reach it.
        const sheet = (await page.locator("#handheld-sheet").boundingBox())!;
        for (const box of await items.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().toJSON()))) {
            expect(box.top).toBeGreaterThanOrEqual(sheet.y - 1);
            expect(box.bottom).toBeLessThanOrEqual(sheet.y + sheet.height + 1);
        }
        // The last entry, then the first: both are picked by a tap, and the sheet stays.
        const names = await items.locator(".choice-menu-label").allTextContents();
        await items.last().tap();
        await expect(page.locator("#hh-palette-name")).toHaveText(names[names.length - 1]);
        await page.locator("#hh-palette").tap();
        await items.first().tap();
        await expect(page.locator("#hh-palette-name")).toHaveText(names[0]);
        await expect(page.locator("#handheld-sheet")).toBeVisible();
    });

    test("Given a Game Boy Advance game, when Save is tapped, then it says why save states are not there yet", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        const save = page.locator("#hh-save");
        await expect(save).toHaveAttribute("aria-disabled", "true");
        // aria-disabled keeps it tappable for a finger; Playwright treats it as disabled, so force it.
        await save.tap({ force: true });
        await expect(page.locator(".neser-toast", { hasText: "Save states are not available for Game Boy Advance games yet" })).toBeVisible();
    });

    test("Given the Game Boy Advance's five looks, when the sheet shows them, then each name stays on one line", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#hh-more").tap();
        const heights = await page.locator("#hh-filter button").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height));
        expect(heights.length).toBe(5);
        expect(Math.max(...heights)).toBeLessThan(40);
        const fits = await page.locator("#handheld-sheet").evaluate((el) => el.scrollWidth <= el.clientWidth);
        expect(fits).toBe(true);
    });
});

test.describe("Desktop", () => {
    test("Given a running game on a desktop, then neither the quick bar nor the sheet shows", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(page.locator("#handheld-bar")).toBeHidden();
        await expect(page.locator("#handheld-sheet")).toBeHidden();
        await expect(page.locator("#fps-counter")).toContainText("fps");
    });
});

test.describe("Phone, held sideways", () => {
    test.use(phoneLandscape);

    test("Given a running game, then the picture takes the full height, the top bar steps aside, and the pill holds Pause and ⋯", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        const viewport = page.viewportSize()!;
        await expect(page.locator("header")).toBeHidden();
        const picture = (await page.locator("#screen").boundingBox())!;
        expect(picture.y).toBeGreaterThanOrEqual(-1);
        expect(picture.height).toBeGreaterThanOrEqual(viewport.height - 2);
        await expect(page.locator("#hh-pause")).toBeVisible();
        await expect(page.locator("#hh-more")).toBeVisible();
        await expect(page.locator("#hh-save")).toBeHidden();
        // The frame rate stays, small, at the top right.
        const chip = page.locator("#fps-counter");
        await expect(chip).toBeVisible({ timeout: 10_000 });
        const box = (await chip.boundingBox())!;
        expect(box.y).toBeLessThan(40);
        expect(box.x + box.width).toBeGreaterThan(viewport.width - 60);
    });
});
