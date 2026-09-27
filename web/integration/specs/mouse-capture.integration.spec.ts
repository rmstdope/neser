import { expect, test } from "@playwright/test";
import { openApp, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";

// The web frontend captures the mouse for the game (pointer lock on the canvas) when a ROM is
// chosen and when the game is clicked, wherever the browser grants it: CI's Linux Chromium does,
// the macOS headless shell does not. While it is captured, no click can reach the sidebar: the
// browser routes every mouse event to the canvas. A player gets the pointer back with Escape.
// This spec pins that a player who clicked the game can still pause it (nr-dv5).
test.describe("Mouse capture", () => {
    test("Given the game captured the mouse, when the player pauses, then the pause reaches the sidebar", async ({ page }) => {
        await openApp(page);
        await page.locator("#rom").setInputFiles({
            name: "suite.sfc",
            mimeType: "application/octet-stream",
            buffer: makeMinimalSnesRomBytes()
        });
        await waitForRunningState(page);

        // A player clicks the game to play it; that captures the mouse where the browser allows.
        await page.locator("#screen").click();

        await page.locator("#pause").click({ timeout: 5_000 });
        await waitForPausedState(page);
    });
});
