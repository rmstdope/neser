import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    waitForIdleState,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const PALETTE_BUTTON_SELECTOR = "#palette-cycle";
const TOAST_SELECTOR = ".neser-toast";
const ACID_DIR = path.join("roms", "gb", "automated_tests", "acid");

async function loadGbRom(page: Page, fileName: string) {
    await page.locator("#rom").setInputFiles({
        name: fileName,
        mimeType: "application/octet-stream",
        buffer: readFileSync(path.join(process.cwd(), ACID_DIR, fileName))
    });
    await waitForRunningState(page);
}

test.describe("Palette button (does what F8 does)", () => {
    test("Given an NES game, when Palette or F8 is pressed, then the label names the new palette and the toast shows", async ({ page }) => {
        await openApp(page);
        const palette = page.locator(PALETTE_BUTTON_SELECTOR);
        await expect(palette).toBeHidden();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(palette).toBeVisible();
        await expect(palette).toHaveText("Palette: Default");
        await expect(palette).toHaveAttribute("title", "Change palette (F8)");

        // Sits right after Filter and before Mute.
        const order = await page.locator("header button").evaluateAll((els) =>
            els.filter((el) => (el as HTMLElement).offsetParent !== null).map((el) => el.id)
        );
        expect(order.slice(order.indexOf("filter-toggle"), order.indexOf("mute") + 1)).toEqual([
            "filter-toggle",
            "palette-cycle",
            "mute"
        ]);

        await palette.click();
        await expect(palette).toHaveText("Palette: NesDev");
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "Palette: NesDev" })).toBeVisible();
        await expect(palette).toBeFocused();

        await page.keyboard.press("F8");
        await expect(palette).toHaveText("Palette: Smooth");

        for (const name of ["Classic", "Composite Direct", "Mesen", "Default"]) {
            await palette.click();
            await expect(palette).toHaveText(`Palette: ${name}`);
        }

        await page.locator("#pause").click();
        await waitForPausedState(page);
        await expect(palette).toBeHidden();
        await page.locator("#pause").click();
        await expect(palette).toBeVisible();

        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expect(palette).toBeHidden();
    });

    test("Given games of each system, then it shows only where F8 changes something, with each game's starting palette", async ({ page }) => {
        await openApp(page);
        const palette = page.locator(PALETTE_BUTTON_SELECTOR);
        const filter = page.locator("#filter-toggle");

        // A Game Boy game starts under the Game Boy screen filter, whose starting palette is DMG Green.
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(filter).toHaveText("Filter: Game Boy");
        await expect(palette).toHaveText("Palette: DMG Green");
        await palette.click();
        await expect(palette).toHaveText("Palette: Pocket");

        // With the filter off the palette stays, and presses go on cycling.
        while ((await filter.textContent()) !== "Filter: None") await filter.click();
        await expect(palette).toHaveText("Palette: Pocket");
        await palette.click();
        await expect(palette).toHaveText("Palette: Light");
        await palette.click();
        await expect(palette).toHaveText("Palette: Grey");
        await palette.click();
        await palette.click();
        await expect(palette).toHaveText("Palette: Pocket");

        // Another original Game Boy game straight after (the same core is reused): it starts
        // from its own starting palette, Grey with the filter off, not the last choice.
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(filter).toHaveText("Filter: None");
        await expect(palette).toHaveText("Palette: Grey");

        await loadGbRom(page, "cgb-acid2.gbc");
        await expect(palette).toBeHidden();

        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(palette).toBeHidden();
    });
});
