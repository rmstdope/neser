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

    test("Given a running game, then the zoom level names the shown picture's height in the console's own lines", async ({ page }) => {
        await startFromBundledRom(page);
        const level = page.locator("#zoom-level");
        const ratio = await page.evaluate(() => document.getElementById("screen")!.clientHeight / 224);
        await expect(level).toHaveText(`${Math.round(ratio * 10) / 10}×`);
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

test.describe("Short lists in the control bar", () => {
    test("Given an NES game, when Filter is pressed, then it lists the NES looks with the current one ticked, and picking one shows it", async ({ page }) => {
        await startFromBundledRom(page);
        const filter = page.locator("#filter-toggle");
        await expect(filter).toHaveText("Filter: NTSC");

        await filter.click();
        const items = page.locator(".choice-menu [role='menuitemradio']");
        await expect(page.locator(".choice-menu-title")).toHaveText("Filter · NES");
        await expect(items.locator(".choice-menu-label")).toHaveText(["None", "NTSC", "CRT"]);
        await expect(items.nth(1)).toHaveAttribute("aria-checked", "true");
        await expect(items.nth(1).locator(".choice-menu-note")).toHaveText("default");
        await expect(page.locator(".choice-menu-footer")).toHaveText("Cycle with F4");

        await page.keyboard.press("Escape");
        await expect(page.locator(".choice-menu")).toHaveCount(0);
        await expect(filter).toBeFocused();
        await expect(filter).toHaveText("Filter: NTSC");

        await filter.click();
        await items.filter({ hasText: "CRT" }).click();
        await expect(filter).toHaveText("Filter: CRT");
        await expect(page.locator(".choice-menu")).toHaveCount(0);

        await page.locator("#screen").focus();
        await page.keyboard.press("F4");
        await expect(filter).toHaveText("Filter: None");
    });

    test("Given an NES game, when Palette is pressed, then every palette is listed and picking one switches to it", async ({ page }) => {
        await startFromBundledRom(page);
        const palette = page.locator("#palette-cycle");
        await palette.click();
        const items = page.locator(".choice-menu [role='menuitemradio'] .choice-menu-label");
        await expect(items).toHaveText(["Default", "NesDev", "Smooth", "Classic", "Composite Direct", "Mesen"]);
        await items.filter({ hasText: "Classic" }).click();
        await expect(palette).toHaveText("Palette: Classic");
        await expect(page.locator(".neser-toast").filter({ hasText: "Palette: Classic" })).toBeVisible();

        // The list keeps its order, with the new choice ticked in its place.
        await palette.click();
        await expect(items).toHaveText(["Default", "NesDev", "Smooth", "Classic", "Composite Direct", "Mesen"]);
        await expect(page.locator(".choice-menu [aria-checked='true'] .choice-menu-label")).toHaveText("Classic");
        // A shortcut closes the open list.
        await page.keyboard.press("F4");
        await expect(page.locator(".choice-menu")).toHaveCount(0);
    });

    test("Given a running game, when Reset's arrow is pressed, then soft and hard reset are listed with their keys, and hard reset keeps the game running", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#reset-menu").click();
        const actions = page.locator(".choice-menu [role='menuitem']");
        await expect(actions.locator(".choice-menu-label")).toHaveText(["Soft reset", "Hard reset"]);
        await expect(actions.nth(1).locator("kbd")).toHaveText(["⇧", "Ctrl", "R"]);
        await expect(page.locator(".choice-menu-footer")).toHaveText("Hard reset is the same as power off and on.");
        await actions.nth(1).click();
        await expect(page.locator(".neser-toast").filter({ hasText: "Hard reset" })).toBeVisible();
        await waitForRunningState(page);

        await page.locator("#reset-menu").click();
        await page.locator(".choice-menu [role='menuitem']").nth(0).click();
        await expect(page.locator(".neser-toast").filter({ hasText: "Soft reset" })).toBeVisible();
    });
});
