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

const COLORS_BUTTON_SELECTOR = "#cgb-color-toggle";
const STOP_BUTTON_SELECTOR = "#stop";
const PAUSE_BUTTON_SELECTOR = "#pause";
const ACID_DIR = path.join("roms", "gb", "automated_tests", "acid");
const TOAST_SELECTOR = ".neser-toast";

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

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
        await waitForPausedState(page);
        await expect(colors).toBeHidden();

        await page.locator(PAUSE_BUTTON_SELECTOR).click();
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: Raw");
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

test.describe("Colors button (Game Boy Advance LCD colour correction)", () => {
    test("Given a GBA game, when Colors or F8 is pressed, then the label and toast use the core's words and Reset keeps the choice", async ({ page }) => {
        await openApp(page);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);

        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: Raw");

        await colors.click();
        await expect(colors).toHaveText("Colors: GBA screen");
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "Colors: GBA screen" })).toBeVisible();

        await page.keyboard.press("F8");
        await expect(colors).toHaveText("Colors: Raw");
        await expect(page.locator(TOAST_SELECTOR).filter({ hasText: "Colors: Raw" })).toBeVisible();

        await page.keyboard.press("F8");
        await expect(colors).toHaveText("Colors: GBA screen");

        await page.locator("#reset").click();
        await waitForRunningState(page);
        await expect(colors).toHaveText("Colors: GBA screen");
    });
});
