import { expect, test } from "@playwright/test";
import { loadRomFromFileInput, openApp, waitForIdleState, waitForRunningState } from "../helpers/lifecycle.helpers";

test.describe("Continue playing", () => {
    test("Given a game was played, then the library keeps it with a picture across a reload, plays it again, and Clear forgets it", async ({ page }) => {
        await openApp(page);
        await expect(page.locator("#recent")).toBeHidden();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.waitForTimeout(500);
        await page.locator("#stop").click();
        await waitForIdleState(page);

        const card = page.getByRole("button", { name: "Play cpu again" });
        await expect(page.locator("#recent")).toBeVisible();
        await expect(card).toBeVisible();
        await expect(card.locator(".recent-card-when")).toHaveText("Today");
        await expect(card.locator(".sys-badge")).toHaveText("NES");
        await expect(card.locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);

        await page.reload();
        await expect(card).toBeVisible();
        await card.click();
        await waitForRunningState(page);
        await expect(page.locator("#game-name")).toHaveText("cpu");

        await page.locator("#stop").click();
        await waitForIdleState(page);
        await page.locator("#recent-clear").click();
        await expect(page.locator("#recent")).toBeHidden();
        await page.reload();
        await expect(page.locator("#recent")).toBeHidden();
    });
});
