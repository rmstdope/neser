import { devices, expect, test } from "@playwright/test";
import { loadRomFromFileInput, openApp, startFromBundledRom, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";

test.describe("Control bar", () => {
    test("Given a running NES game, when Pause is pressed, then Resume sits under the same point and Palette keeps its place, disabled", async ({ page }) => {
        await startFromBundledRom(page);
        const pause = page.locator("#pause");
        const palette = page.locator("#palette-cycle");
        await expect(palette).toBeVisible();
        const box = (await pause.boundingBox())!;
        const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
        const paletteBefore = (await palette.boundingBox())!;

        await page.mouse.click(point.x, point.y);
        await waitForPausedState(page);
        const underPointer = await page.evaluate(({ x, y }) => document.elementFromPoint(x, y)?.closest("button")?.id ?? null, point);
        expect(underPointer).toBe("pause");
        await expect(palette).toBeVisible();
        await expect(palette).toBeDisabled();
        expect(Math.round((await palette.boundingBox())!.x)).toBe(Math.round(paletteBefore.x));

        await page.mouse.click(point.x, point.y);
        await waitForRunningState(page);
        await expect(palette).toBeEnabled();
    });
});

// A device profile names its browser type, which a describe block may not set; the rest applies.
const { defaultBrowserType: _browser, ...phoneLandscape } = devices["Pixel 7 landscape"];

test.describe("Control bar on a phone held sideways", () => {
    test.use(phoneLandscape);

    test("Given a running game, then the control bar stays out of the way and the whole picture is on screen", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);

        // The touch controls play the game here; the bar returns in the phone layout step.
        await expect(page.locator("#emulation-controls")).toBeHidden();
        const viewport = page.viewportSize()!;
        const picture = (await page.locator("#screen").boundingBox())!;
        // The top bar overlapping the picture's top few pixels predates the control bar, as on main.
        expect(picture.y).toBeGreaterThanOrEqual(0);
        expect(picture.y + picture.height).toBeLessThanOrEqual(viewport.height + 1);
    });
});
