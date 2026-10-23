import { test, expect } from "@playwright/test";
import {
    closeSettings,
    loadRomFromFileInput,
    openApp,
    openSettings,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

test.describe("Settings panel", () => {
    test("Given a running game, when Settings opens, then the game pauses, and closing the panel carries on", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);

        await openSettings(page);
        await waitForPausedState(page);
        await closeSettings(page);
        await waitForRunningState(page);
    });

    test("Given the open panel, when Esc is pressed, then it closes and the game carries on", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);

        await openSettings(page);
        await page.keyboard.press("Escape");
        await expect(page.locator("#settings-panel")).toBeHidden();
        await waitForRunningState(page);
    });

    test("Given a game the player paused, when Settings opens and closes, then it stays paused", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#pause").click();
        await waitForPausedState(page);

        await openSettings(page);
        await closeSettings(page);
        await waitForPausedState(page);
    });

    test("Given the open panel, when a shortcut key is pressed, then the key stays with the panel", async ({ page }) => {
        await openApp(page);
        await openSettings(page);
        await page.keyboard.press("KeyH");
        await expect(page.locator("#shortcut-help-overlay")).toBeHidden();
    });

    test("Given an NES game, when Controls opens, then it lists the NES keys for both players", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);

        await openSettings(page, "Controls");
        const keys = page.locator("#settings-keys");
        await expect(page.locator('#settings-keys-system [data-console="nes"]')).toHaveAttribute("aria-pressed", "true");
        await expect(keys.locator("h4")).toHaveText(["Player 1", "Player 2"]);
        await expect(keys.locator(".settings-key-action").first()).toHaveText("D-Pad");

        await page.locator('#settings-keys-system [data-console="snes"]').click();
        await expect(keys.locator("h4")).toHaveCount(0);
        const xRow = keys.locator(".settings-key-row").filter({ has: page.locator(".settings-key-action", { hasText: /^X$/ }) });
        await expect(xRow.locator("kbd")).toHaveText(["Y"]);
    });
});

test.describe("Settings panel, firmware and debugger", () => {
    test("Given SNES firmware not yet added, when a wrong file is chosen with Add…, then the panel says why, readable inside it", async ({ page }) => {
        await openApp(page);
        await openSettings(page);
        const add = page.getByRole("button", { name: "Add the DSP-2 firmware" });
        await expect(add).toBeVisible();
        const chooser = page.waitForEvent("filechooser");
        await add.click();
        await (await chooser).setFiles({ name: "x.rom", mimeType: "application/octet-stream", buffer: Buffer.alloc(100) });

        const message = page.locator("#snes-firmware-message");
        await expect(message).toBeVisible();
        await expect(message).toContainText("That isn't a DSP-2 firmware file");
        await expect(message).toContainText('"x.rom" is 100 bytes; the firmware is exactly 8 KB.');
        expect(await message.evaluate((el) => el.closest("#settings-panel") !== null)).toBe(true);

        // The words are about that file only: the next opening starts clear.
        await closeSettings(page);
        await openSettings(page);
        await expect(message).toBeHidden();
    });

    test("Given the open panel, then the top bar above it stays clear of the dimmed backdrop", async ({ page }) => {
        await openApp(page);
        await openSettings(page);
        const backdropTop = await page.locator("#settings-panel").evaluate((el) => getComputedStyle(el, "::backdrop").top);
        const header = (await page.locator("header").boundingBox())!;
        expect(parseFloat(backdropTop)).toBeGreaterThanOrEqual(header.y + header.height - 1);
    });

    test("Given the debugger button switched on, then the bug button shows with an NES game, docks the debugger beside the picture, and is remembered", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        const bug = page.locator("#debugger-open");
        await expect(bug).toBeHidden();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(bug).toBeVisible();
        await bug.click();
        const panel = page.locator("#debugger-panel");
        await expect(panel).toBeVisible();
        await expect(bug).toHaveAttribute("aria-pressed", "true");
        const picture = (await page.locator("#screen").boundingBox())!;
        const dock = (await panel.boundingBox())!;
        expect(dock.x).toBeGreaterThanOrEqual(picture.x + picture.width);

        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(panel).toBeHidden();
        await expect(bug).toHaveAttribute("aria-pressed", "false");

        await page.reload();
        await openSettings(page, "Developer");
        await expect(page.getByLabel("Show debugger button")).toBeChecked();
    });

    test("Given the docked debugger, when the game is ejected and another starts, then the debugger is closed and the button agrees", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        const bug = page.locator("#debugger-open");
        const panel = page.locator("#debugger-panel");
        await bug.click();
        await expect(panel).toBeVisible();

        await page.locator("#stop").click();
        await expect(panel).toHaveClass(/\bhidden\b/);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(panel).toBeHidden();
        await expect(bug).toHaveAttribute("aria-pressed", "false");
        // One press opens it again.
        await bug.click();
        await expect(panel).toBeVisible();
    });

    test("Given the debugger button switched on, when a Game Boy game runs, then there is no bug button and the picture has the view to itself", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#debugger-open").click();
        await expect(page.locator("#debugger-panel")).toBeVisible();

        await page.locator("#rom").setInputFiles("roms/gb/automated_tests/acid/dmg-acid2.gb");
        await waitForRunningState(page);
        await expect(page.locator("#debugger-open")).toBeHidden();
        await expect(page.locator("#game-view")).toHaveCSS("flex-direction", "column");
    });

    test("Given a narrow window, when the debugger opens, then it docks under the picture rather than squeezing beside it", async ({ page }) => {
        await page.setViewportSize({ width: 760, height: 900 });
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        const panel = page.locator("#debugger-panel");
        await expect(panel).toBeVisible();
        const picture = (await page.locator("#screen").boundingBox())!;
        const dock = (await panel.boundingBox())!;
        expect(dock.y).toBeGreaterThanOrEqual(picture.y + picture.height);
        expect(dock.width).toBeGreaterThan(600);
    });

    test("Given the open panel over a running game, when the backdrop is clicked, then it closes and the game carries on", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await openSettings(page);
        await page.mouse.click(10, 400);
        await expect(page.locator("#settings-panel")).toBeHidden();
        await waitForRunningState(page);
    });

    test("Given the open panel over a running game, when Space is pressed in it, then the game stays paused", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await openSettings(page, "Controls");
        await page.getByRole("tab", { name: "Controls" }).focus();
        await page.keyboard.press("Space");
        await waitForPausedState(page);
    });
});
