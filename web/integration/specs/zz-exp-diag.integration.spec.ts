import { test } from "@playwright/test";
import { openApp, waitForRunningState } from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";

test("EXP: pointer lock after a file load, and zoom click timing", async ({ page, browser }) => {
    console.log(`EXP browser ${browser.version()}`);
    await openApp(page);
    const t0 = Date.now();
    await page.locator("#screen-plus").click({ timeout: 30_000 });
    const plusMs = Date.now() - t0;
    await page.waitForTimeout(100);
    const t1 = Date.now();
    await page.locator("#screen-minus").click({ timeout: 30_000 });
    console.log(`EXP zoom plus=${plusMs}ms minus=${Date.now() - t1}ms`);
    await page.locator("#rom").setInputFiles({ name: "suite.sfc", mimeType: "application/octet-stream", buffer: makeMinimalSnesRomBytes() });
    await waitForRunningState(page);
    await page.waitForTimeout(500);
    const lock = await page.evaluate(() => document.pointerLockElement?.id ?? null);
    console.log(`EXP lock-after-file-load=${lock}`);
    const t2 = Date.now();
    const ok = await page.locator("#pause").click({ timeout: 8000 }).then(() => true).catch(() => false);
    console.log(`EXP pause-click ok=${ok} ms=${Date.now() - t2}`);
});
