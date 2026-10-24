import { expect, test } from "@playwright/test";
import { loadRomFromFileInput, openApp, waitForIdleState, waitForRunningState } from "../helpers/lifecycle.helpers";

test.describe("Continue playing", () => {
    test("Given a game was played, then the library keeps it with a picture across a reload, plays it again, and Clear forgets it", async ({ page }) => {
        await openApp(page);
        await expect(page.locator("#recent")).toBeHidden();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        // Frames are drawn once the frame counter shows.
        await expect(page.locator("#fps-counter")).toBeVisible();
        await page.locator("#stop").click();
        await waitForIdleState(page);

        const card = page.getByRole("button", { name: "Play cpu again" });
        await expect(page.locator("#recent")).toBeVisible();
        await expect(card).toBeVisible();
        await expect(card.locator(".recent-card-when")).toHaveText("Today");
        await expect(card.locator(".sys-badge")).toHaveText("NES");
        await expect(card.locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);
        // The picture keeps to its frame, whatever the console's shape.
        const frame = (await card.locator(".recent-card-picture").boundingBox())!;
        expect(Math.abs(frame.width / frame.height - 16 / 11)).toBeLessThan(0.02);

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

    // The page can be gone before the picture's database write commits, and Chromium then aborts
    // the write (nr-39ib): a starved CI runner did it once. A second tab keeping the database busy
    // holds the write back until after the reload, every time.
    test("Given a game left by hiding the page while the database is busy, then its card still has a picture", async ({ page, context }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(page.locator("#fps-counter")).toBeVisible();
        const other = await context.newPage();
        await other.goto("/roms/roms.json");
        await other.evaluate(
            () =>
                new Promise<void>((resolve) => {
                    const request = indexedDB.open("neser-recent");
                    request.onsuccess = () => {
                        const games = request.result.transaction("games", "readwrite").objectStore("games");
                        const end = Date.now() + 1500;
                        const keepBusy = () => {
                            if (Date.now() < end) games.count().onsuccess = keepBusy;
                        };
                        keepBusy();
                        resolve();
                    };
                })
        );
        await page.evaluate(() => {
            Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
            document.dispatchEvent(new Event("visibilitychange"));
        });
        await page.reload();
        await expect(page.getByRole("button", { name: "Play cpu again" }).locator("img")).toHaveAttribute("src", /^data:image\/png;base64,/);
    });
});
