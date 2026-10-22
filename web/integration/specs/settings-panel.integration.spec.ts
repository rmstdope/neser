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
        await expect(keys).toContainText("X");
    });
});
