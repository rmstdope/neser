import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { loadRomFromFileInput, openApp, waitForIdleState, waitForRunningState } from "../helpers/lifecycle.helpers";

// "Game Boy games run on" (nr-zdy.4): which console original Game Boy games run on.
const CHOICE_SELECTOR = "#gb-hardware";
const NOTE_SELECTOR = "#gb-hardware-note";
const COLORS_BUTTON_SELECTOR = "#cgb-color-toggle";
const PALETTE_BUTTON_SELECTOR = "#palette-cycle";
const ACID_DIR = path.join("roms", "gb", "automated_tests", "acid");

async function loadGbRom(page: Page, fileName: string) {
    await page.locator("#rom").setInputFiles({
        name: fileName,
        mimeType: "application/octet-stream",
        buffer: readFileSync(path.join(process.cwd(), ACID_DIR, fileName))
    });
    await waitForRunningState(page);
}

test.describe("Game Boy games run on", () => {
    test("Given a first visit, then the choice sits under the game pickers, reads Game Boy and shows no line", async ({ page }) => {
        await openApp(page);
        const choice = page.locator(CHOICE_SELECTOR);
        await expect(page.getByText("Game Boy games run on", { exact: true })).toBeVisible();
        await expect(page.getByLabel("Game Boy games run on")).toHaveId("gb-hardware");
        await expect(choice).toHaveValue("dmg");
        await expect(choice.locator("option")).toHaveText(["Game Boy", "Game Boy Color"]);
        await expect(page.locator(NOTE_SELECTOR)).toBeHidden();

        // Reached with Tab straight after the bundled-game dropdown.
        await page.locator("#rom-select").focus();
        await page.keyboard.press("Tab");
        await expect(choice).toBeFocused();

        // Directly under the game pickers, above the Start button.
        const [pickerBox, choiceBox, startBox] = await Promise.all(
            ["#rom-select", CHOICE_SELECTOR, "#start"].map((sel) => page.locator(sel).boundingBox())
        );
        expect(choiceBox!.y).toBeGreaterThan(pickerBox!.y);
        expect(choiceBox!.y).toBeLessThan(startBox!.y);
        expect(Math.round(choiceBox!.width)).toBe(Math.round(pickerBox!.width));
    });

    test("Given Game Boy Color, an original Game Boy game runs coloured, a change waits for Reset, and the choice is remembered", async ({ page }) => {
        await openApp(page);
        const choice = page.locator(CHOICE_SELECTOR);
        const note = page.locator(NOTE_SELECTOR);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);
        const palette = page.locator(PALETTE_BUTTON_SELECTOR);

        await choice.selectOption("cgb");
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: Raw");
        // Auto names the tint the Game Boy Color picks for this title.
        await expect(palette).toHaveText(/^Palette: Auto \(/);
        await expect(note).toBeHidden();
        await page.keyboard.press("F8");
        await expect(palette).toHaveText("Palette: Brown");

        // Changed while it runs: the game carries on untouched, the line says when it applies.
        await choice.focus();
        await choice.selectOption("dmg");
        await expect(choice).toBeFocused();
        await expect(note).toBeVisible();
        await expect(note).toHaveText("Applies when you press Reset");
        await expect(colors).toBeVisible();
        await expect(palette).toHaveText("Palette: Brown");

        // Changed back to the console it runs on: the line goes.
        await choice.selectOption("cgb");
        await expect(note).toBeHidden();

        // Reset starts it over on the chosen console, and the line goes.
        await choice.selectOption("dmg");
        await expect(note).toBeVisible();
        await page.locator("#reset").click();
        await expect(note).toBeHidden();
        await expect(colors).toBeHidden();
        // Under the Game Boy screen filter (on for Game Boy games), as when the game starts on the Game Boy.
        await expect(page.locator("#filter-toggle")).toHaveText("Filter: Game Boy");
        await expect(palette).toHaveText("Palette: DMG Green");

        // The hard reset (Ctrl+Shift+R) applies the choice too.
        await choice.selectOption("cgb");
        await expect(note).toBeVisible();
        await choice.evaluate((el) => (el as HTMLElement).blur());
        await page.keyboard.press("Control+Shift+KeyR");
        await expect(note).toBeHidden();
        await expect(colors).toBeVisible();
        await expect(palette).toHaveText(/^Palette: Auto \(/);
        await choice.selectOption("dmg");
        await page.locator("#reset").click();
        await expect(colors).toBeHidden();

        // Loading another original Game Boy game after a change starts it on the chosen console.
        await choice.selectOption("cgb");
        await expect(note).toBeVisible();
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(note).toBeHidden();
        await expect(colors).toBeVisible();

        // Never shown with no game.
        await choice.selectOption("dmg");
        await expect(note).toBeVisible();
        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expect(note).toBeHidden();

        // Remembered across a reload.
        await choice.selectOption("cgb");
        await page.reload();
        await expect(page.locator("#start")).toBeVisible();
        await expect(choice).toHaveValue("cgb");
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(colors).toBeVisible();
    });

    test("Given a Game Boy Color or an NES game, changing the choice shows no line and leaves the game", async ({ page }) => {
        await openApp(page);
        const choice = page.locator(CHOICE_SELECTOR);
        const note = page.locator(NOTE_SELECTOR);

        await loadGbRom(page, "cgb-acid2.gbc");
        await choice.selectOption("cgb");
        await expect(note).toBeHidden();
        await choice.selectOption("dmg");
        await expect(note).toBeHidden();
        await expect(page.locator(COLORS_BUTTON_SELECTOR)).toBeVisible();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await choice.selectOption("cgb");
        await expect(note).toBeHidden();
    });
});
