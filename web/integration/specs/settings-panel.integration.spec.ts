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
    });

    test("Given an NES game, when Open debugger is pressed, then the panel closes and the debugger shows; with no game it cannot be pressed", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await expect(page.locator("#settings-debugger")).toBeDisabled();
        await page.keyboard.press("Escape");

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await openSettings(page, "Developer");
        await page.locator("#settings-debugger").click();
        await expect(page.locator("#settings-panel")).toBeHidden();
        await expect(page.locator("#debugger-panel")).toBeVisible();
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
