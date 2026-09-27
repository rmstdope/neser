import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
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

type Rgb = [number, number, number];

/** The screen's colour at each point, given as fractions of its width and height. */
async function pictureAt(page: Page, points: [number, number][]): Promise<Rgb[]> {
    const png = (await page.locator("#screen").screenshot()).toString("base64");
    return page.evaluate(async ({ png, points }) => {
        const img = new Image();
        img.src = `data:image/png;base64,${png}`;
        await img.decode();
        const c = document.createElement("canvas");
        c.width = img.width;
        c.height = img.height;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, 0, 0);
        return points.map(([x, y]) => {
            const d = ctx.getImageData(Math.floor(x * (img.width - 1)), Math.floor(y * (img.height - 1)), 1, 1).data;
            return [d[0], d[1], d[2]] as [number, number, number];
        });
    }, { png, points });
}

// ppu/shades.gba draws vertical bands from black (left) to full blue (right).
const TOP_MIDDLE: [number, number] = [0.5, 0.03];
const RIGHT_MIDDLE: [number, number] = [0.9, 0.5];
const isBlue = ([r, g, b]: Rgb) => b > 60 && b > r + 30;
const isBlack = ([r, g, b]: Rgb) => r < 16 && g < 16 && b < 16;

async function loadShades(page: Page) {
    await page.locator("#rom").setInputFiles({
        name: "shades.gba",
        mimeType: "application/octet-stream",
        buffer: readFileSync(path.join(process.cwd(), "roms", "gba", "automated_tests", "gba-tests", "ppu", "shades.gba"))
    });
    await waitForRunningState(page);
    // Past the boot logo: the top of the screen shows the game's blue bands.
    await expect.poll(async () => isBlue((await pictureAt(page, [TOP_MIDDLE]))[0]), { timeout: 20_000 }).toBe(true);
}

async function pressFilterUntil(page: Page, name: string) {
    const filter = page.locator(FILTER_SELECTOR);
    for (let i = 0; i < 5 && (await filter.textContent()) !== `Filter: ${name}`; i++) await filter.click();
    await expect(filter).toHaveText(`Filter: ${name}`);
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

    test("Given a GBA game, then every look changes the picture and LCD Grid draws the console around it", async ({ page }) => {
        await openApp(page);
        await loadShades(page);
        const [noneRight] = await pictureAt(page, [RIGHT_MIDDLE]);

        for (const name of ["AGB-001", "Switch Online", "GBA SP"]) {
            await pressFilterUntil(page, name);
            await expect.poll(async () => {
                const [rgb] = await pictureAt(page, [RIGHT_MIDDLE]);
                return rgb.some((v, i) => Math.abs(v - noneRight[i]) > 12);
            }, { message: `${name} changes the picture` }).toBe(true);
        }

        await pressFilterUntil(page, "LCD Grid");
        // The top of the screen is now above the console: black, where None shows the game.
        await expect.poll(async () => isBlack((await pictureAt(page, [TOP_MIDDLE]))[0])).toBe(true);
    });

    test("Given LCD Grid's console art still being fetched, then the picture stays on the previous look until it arrives", async ({ page }) => {
        await openApp(page);
        let release: () => void = () => {};
        const held = new Promise<void>((resolve) => { release = resolve; });
        await page.route(BORDER_REQUEST, async (route) => {
            await held;
            await route.continue();
        });
        await loadShades(page);
        await pressFilterUntil(page, "LCD Grid");
        await page.waitForTimeout(500);
        expect(isBlue((await pictureAt(page, [TOP_MIDDLE]))[0])).toBe(true);
        release();
        await expect.poll(async () => isBlack((await pictureAt(page, [TOP_MIDDLE]))[0])).toBe(true);
    });

    test("Given a paused GBA game, then a new look is drawn at once", async ({ page }) => {
        await openApp(page);
        await loadShades(page);
        await page.locator("#pause").click();
        await waitForPausedState(page);
        const [before] = await pictureAt(page, [RIGHT_MIDDLE]);

        await pressFilterUntil(page, "Switch Online");
        await expect.poll(async () => {
            const [rgb] = await pictureAt(page, [RIGHT_MIDDLE]);
            return rgb.some((v, i) => Math.abs(v - before[i]) > 12);
        }).toBe(true);
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
