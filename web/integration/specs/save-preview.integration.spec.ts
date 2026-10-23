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
        await page.locator("#load-state").focus();
        await expect(preview).toBeVisible();
    });

    test("Given a reload, when Load is hovered for the state saved before it, then the preview is still there", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#save-state").click();
        await expect(page.locator("#save-state-section")).toHaveAttribute("data-save-state", "saved");
        await page.reload();
        await startFromBundledRom(page);
        await page.locator("#load-state").hover();
        await expect(page.locator("#load-preview img")).toBeVisible();
    });

    test("Given a Game Boy Advance game, then Save and Load show but cannot be pressed, and say why", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);
        for (const id of ["#save-state", "#load-state"]) {
            await expect(page.locator(id)).toBeVisible();
            await expect(page.locator(id)).toBeDisabled();
            await expect(page.locator(id)).toHaveAttribute("data-tip", "Save states are not available for Game Boy Advance games yet");
        }
    });
});
