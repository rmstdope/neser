import { test, expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { closeSettings, loadRomFromFileInput, openApp, openSettings, waitForIdleState, waitForRunningState } from "../helpers/lifecycle.helpers";

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

/** Sets the choice in Settings › General and closes the panel again. */
async function chooseHardware(page: Page, value: "dmg" | "cgb") {
    await openSettings(page);
    await page.locator(CHOICE_SELECTOR).selectOption(value);
    await closeSettings(page);
}

/** Opens Settings to read the line under the choice, then closes it again. */
async function expectNote(page: Page, shown: boolean) {
    await openSettings(page);
    const note = page.locator(NOTE_SELECTOR);
    if (shown) {
        await expect(note).toBeVisible();
        await expect(note).toHaveText("Applies when you press Reset");
    } else {
        await expect(note).toBeHidden();
    }
    await closeSettings(page);
}

test.describe("Game Boy games run on", () => {
    test("Given a first visit, then the choice sits in Settings › General, reads Game Boy and shows no line", async ({ page }) => {
        await openApp(page);
        await expect(page.locator(CHOICE_SELECTOR)).toBeHidden();
        await openSettings(page);
        const choice = page.locator(CHOICE_SELECTOR);
        await expect(page.getByText("Game Boy games run on", { exact: true })).toBeVisible();
        await expect(page.getByLabel("Game Boy games run on")).toHaveId("gb-hardware");
        await expect(page.locator("#settings-tab-general").locator(CHOICE_SELECTOR)).toBeVisible();
        await expect(choice).toHaveValue("dmg");
        await expect(choice.locator("option")).toHaveText(["Game Boy", "Game Boy Color"]);
        await expect(page.locator(NOTE_SELECTOR)).toBeHidden();
    });

    test("Given Game Boy Color, an original Game Boy game runs coloured, a change waits for Reset, and the choice is remembered", async ({ page }) => {
        await openApp(page);
        const choice = page.locator(CHOICE_SELECTOR);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);
        const palette = page.locator(PALETTE_BUTTON_SELECTOR);

        await chooseHardware(page, "cgb");
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(colors).toBeVisible();
        await expect(colors).toHaveText("Colors: Raw");
        // Auto names the tint the Game Boy Color picks for this title.
        await expect(palette).toHaveText(/^Palette: Auto \(/);
        await expectNote(page, false);
        await page.keyboard.press("F8");
        await expect(palette).toHaveText("Palette: Brown");

        // Changed while it runs: the game carries on untouched, the line says when it applies.
        await openSettings(page);
        await choice.selectOption("dmg");
        await expect(page.locator(NOTE_SELECTOR)).toBeVisible();
        await expect(page.locator(NOTE_SELECTOR)).toHaveText("Applies when you press Reset");
        // Changed back to the console it runs on: the line goes.
        await choice.selectOption("cgb");
        await expect(page.locator(NOTE_SELECTOR)).toBeHidden();
        await closeSettings(page);
        await expect(colors).toBeVisible();
        await expect(palette).toHaveText("Palette: Brown");

        // Reset starts it over on the chosen console, and the line goes.
        await chooseHardware(page, "dmg");
        await expectNote(page, true);
        await page.locator("#reset").click();
        await expectNote(page, false);
        await expect(colors).toBeHidden();
        // Under the Game Boy screen filter (on for Game Boy games), as when the game starts on the Game Boy.
        await expect(page.locator("#filter-toggle")).toHaveText("Filter: Game Boy");
        await expect(palette).toHaveText("Palette: DMG Green");

        // The hard reset (Ctrl+Shift+R) applies the choice too.
        await chooseHardware(page, "cgb");
        await expectNote(page, true);
        await page.keyboard.press("Control+Shift+KeyR");
        await expectNote(page, false);
        await expect(colors).toBeVisible();
        await expect(palette).toHaveText(/^Palette: Auto \(/);
        await chooseHardware(page, "dmg");
        await page.locator("#reset").click();
        await expect(colors).toBeHidden();

        // Loading another original Game Boy game after a change starts it on the chosen console.
        await chooseHardware(page, "cgb");
        await expectNote(page, true);
        await loadGbRom(page, "dmg-acid2.gb");
        await expectNote(page, false);
        await expect(colors).toBeVisible();

        // Never shown with no game.
        await chooseHardware(page, "dmg");
        await expectNote(page, true);
        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expectNote(page, false);

        // Remembered across a reload.
        await chooseHardware(page, "cgb");
        await page.reload();
        await expect(page.locator("#start")).toBeVisible();
        await openSettings(page);
        await expect(choice).toHaveValue("cgb");
        await closeSettings(page);
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(colors).toBeVisible();
    });

    test("Given a state saved on the Game Boy Color, when it is loaded after a Reset onto the Game Boy, then it is restored on the Game Boy Color", async ({ page }) => {
        await openApp(page);
        const colors = page.locator(COLORS_BUTTON_SELECTOR);
        const section = page.locator("#save-state-section");

        await chooseHardware(page, "cgb");
        await loadGbRom(page, "dmg-acid2.gb");
        await expect(section).toBeVisible();
        await page.locator("#save-state").click();
        await expect(section).toHaveAttribute("data-save-state", "saved", { timeout: 5000 });

        await chooseHardware(page, "dmg");
        await page.locator("#reset").click();
        await expect(colors).toBeHidden();
        await expectNote(page, false);

        await page.locator("#load-state").click();
        await expect(section).toHaveAttribute("data-save-state", "loaded", { timeout: 5000 });
        await expect(colors).toBeVisible();
        await expect(page.locator(PALETTE_BUTTON_SELECTOR)).toHaveText(/^Palette: Auto \(/);
        // The choice still says Game Boy, so the line shows.
        await openSettings(page);
        await expect(page.locator(CHOICE_SELECTOR)).toHaveValue("dmg");
        await closeSettings(page);
        await expectNote(page, true);
    });

    test("Given a Game Boy Color or an NES game, changing the choice shows no line and leaves the game", async ({ page }) => {
        await openApp(page);

        await loadGbRom(page, "cgb-acid2.gbc");
        await chooseHardware(page, "cgb");
        await expectNote(page, false);
        await chooseHardware(page, "dmg");
        await expectNote(page, false);
        await expect(page.locator(COLORS_BUTTON_SELECTOR)).toBeVisible();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await chooseHardware(page, "cgb");
        await expectNote(page, false);
    });
});
