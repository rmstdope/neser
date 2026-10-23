import { devices, expect, test } from "@playwright/test";
import { loadRomFromFileInput, openApp, waitForIdleState, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";

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
    });
});
