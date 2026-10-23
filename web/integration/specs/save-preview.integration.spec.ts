import { readFileSync } from "node:fs";

import { expect, test } from "@playwright/test";
import { loadGbaRomFromFileInput, openApp, startFromBundledRom, waitForRunningState } from "../helpers/lifecycle.helpers";

test.describe("Save-state preview", () => {
    test("Given a running game, when its state is saved, then a message confirms it and Load shows the saved moment and when", async ({ page }) => {
        await startFromBundledRom(page);
        await page.waitForTimeout(300);
        await page.locator("#save-state").click();
        await expect(page.locator(".neser-toast").filter({ hasText: "State saved" })).toBeVisible();
        await expect(page.locator("#save-state-section")).toHaveAttribute("data-save-state", "saved");

        const preview = page.locator("#load-preview");
        await expect(preview).toBeHidden();
        await page.locator("#load-state").hover();
        await expect(preview).toBeVisible();
        await expect(preview.locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);
        await expect(preview.locator(".load-preview-ago")).toHaveText("Saved just now");
        await expect(preview.locator(".load-preview-at")).toContainText(/^Today \d\d:\d\d · F7 to load$/);

        await page.mouse.move(5, 5);
        await expect(preview).toBeHidden();
        // From the keyboard it shows on focus; after a click, focus stays but the preview goes.
        await page.locator("#save-state").focus();
        await page.keyboard.press("Tab");
        await expect(page.locator("#load-state")).toBeFocused();
        await expect(preview).toBeVisible();
        await page.evaluate(() => (document.activeElement as HTMLElement).blur());
        await page.locator("#load-state").click();
        await page.mouse.move(5, 5);
        await expect(preview).toBeHidden();
        // Playing on with the keyboard does not bring it back over the game.
        await page.keyboard.press("ArrowRight");
        await expect(preview).toBeHidden();
    });

    test("Given a reload, when Load is hovered for the state saved before it, then the preview is still there", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#save-state").click();
        await expect(page.locator("#save-state-section")).toHaveAttribute("data-save-state", "saved");
        await page.reload();
        await startFromBundledRom(page);
        await page.locator("#load-state").hover();
        await expect(page.locator("#load-preview img")).toBeVisible();
        await expect(page.locator("#load-preview .load-preview-ago")).toHaveText("Saved just now");
    });

    test("Given a Game Boy Advance game, then Save and Load show but cannot be pressed, and say why", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        for (const id of ["#save-state", "#load-state"]) {
            const button = page.locator(id);
            await expect(button).toBeVisible();
            await expect(button).toHaveAttribute("aria-disabled", "true");
            await expect(button).toHaveAttribute("data-tip", "Save states are not available for Game Boy Advance games yet");
            await expect(button).not.toHaveAttribute("data-keys");
            // The reason shows on hover, which a disabled button would never get, and at full strength.
            await button.hover();
            await expect
                .poll(() => button.evaluate((el) => getComputedStyle(el, "::after").content))
                .toBe('"Save states are not available for Game Boy Advance games yet"');
            expect(await button.evaluate((el) => getComputedStyle(el).opacity)).toBe("1");
            expect(await button.evaluate((el) => getComputedStyle(el).boxShadow)).not.toContain("rgba(");
        }
        await page.locator("#save-state").click({ force: true });
        await expect(page.locator(".neser-toast").filter({ hasText: "State saved" })).toHaveCount(0);

        // With the game ejected, the reason about Game Boy Advance games goes too.
        await page.locator("#stop").click();
        await expect(page.locator("#save-state")).not.toHaveAttribute("aria-disabled");
        await expect(page.locator("#save-state")).toHaveAttribute("data-tip", "Save state");
    });

    test("Given two different games under one file name, then each keeps its own preview, and a game with no preview describes none", async ({ page }) => {
        const asGame = (path: string) => ({ name: "game.nes", mimeType: "application/octet-stream", buffer: readFileSync(path) });
        await openApp(page);
        await page.locator("#rom").setInputFiles(asGame("roms/nes/automated_tests/blargg_nes_cpu_test5/cpu.nes"));
        await waitForRunningState(page);
        await page.locator("#save-state").click();
        await expect(page.locator("#save-state-section")).toHaveAttribute("data-save-state", "saved");
        await expect(page.locator("#load-preview .load-preview-ago")).toHaveText("Saved just now");

        // Another ROM of the same name has no save and so no preview: nothing is described.
        await page.locator("#rom").setInputFiles(asGame("roms/nes/automated_tests/blargg_nes_cpu_test5/official.nes"));
        await waitForRunningState(page);
        await expect(page.locator("#load-preview")).not.toHaveAttribute("data-ready", "");
        await expect(page.locator("#load-preview .load-preview-ago")).toHaveText("");
        await expect(page.locator("#load-state")).not.toHaveAttribute("aria-describedby", /.+/);
    });
});
