import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
    loadRomFromFileInput,
    openApp,
    waitForIdleState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const COLORS_BUTTON_SELECTOR = "#cgb-color-toggle";
const STOP_BUTTON_SELECTOR = "#stop";
const ACID_DIR = path.join("roms", "gb", "automated_tests", "acid");

async function loadGbRom(page: Page, fileName: string) {
    await page.locator("#rom").setInputFiles({
        name: fileName,
        mimeType: "application/octet-stream",
        buffer: readFileSync(path.join(process.cwd(), ACID_DIR, fileName))
    });
    await waitForRunningState(page);
}

test.describe("Colors button (Game Boy Color LCD colour correction)", () => {
    test("Given a colour game, when Colors is pressed, then it names the new state and keeps focus", async ({ page }) => {
        await openApp(page);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);
        await expect(colors).toBeHidden();

        await loadGbRom(page, "cgb-acid2.gbc");
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: Raw");

        await colors.click();
        await expect(colors).toHaveText("Colors: GBC screen");
        await expect(colors).toBeFocused();

        await colors.click();
        await expect(colors).toHaveText("Colors: Raw");
        await waitForRunningState(page);
    });

    test("Given the state is on, when other games load, then it hides and comes back with the same state", async ({ page }) => {
        await openApp(page);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);

        await loadGbRom(page, "cgb-acid2.gbc");
        await colors.click();
        await expect(colors).toHaveText("Colors: GBC screen");

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(colors).toBeHidden();

        await loadGbRom(page, "dmg-acid2.gb");
        await expect(colors).toBeHidden();

        await loadGbRom(page, "cgb-acid2.gbc");
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: GBC screen");

        await page.locator(STOP_BUTTON_SELECTOR).click();
        await waitForIdleState(page);
        await expect(colors).toBeHidden();

        await page.reload();
        await loadGbRom(page, "cgb-acid2.gbc");
        await expect(colors).toHaveText("Colors: Raw");
    });
});
