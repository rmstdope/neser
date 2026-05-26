import { test, expect } from "@playwright/test";

const ESSENTIAL_CONTROL_SELECTORS = ["#screen", "#start", "#pause", "#stop"];

test.describe("web app shell", () => {
    test("renders essential controls", async ({ page }) => {
        await page.goto("/");

        for (const selector of ESSENTIAL_CONTROL_SELECTORS) {
            await expect(page.locator(selector)).toBeVisible();
        }
    });

    test("accepts GBA ROM files in the file picker", async ({ page }) => {
        await page.goto("/");

        await expect(page.locator("#rom")).toHaveAttribute("accept", /(^|,)\.gba(,|$)/);
    });
});
