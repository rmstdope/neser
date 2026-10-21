import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium, expect } from "@playwright/test";

const baseURL = process.env.NESER_CONTAINER_URL || "http://127.0.0.1:18081";
const revision = process.env.REVISION;
assert.match(revision || "", /^[a-f0-9]{40}$/);
const browser = await chromium.launch({ headless: true });
try {
    const page = await browser.newPage({ baseURL });
    const errors = [];
    page.on("pageerror", error => errors.push(error.message));
    const identity = await page.request.get("/release.json");
    assert.equal(identity.status(), 200);
    assert.equal((await identity.json()).revision, revision);
    assert.equal(identity.headers()["cache-control"], "no-store");
    assert.equal((await page.request.get("/healthz")).status(), 200);
    assert.deepEqual(await (await page.request.get("/roms/roms.json")).json(), { roms: [] });
    for (const path of ["/missing.js", "/assets/missing.wasm", "/Cargo.toml", "/.git/config", "/roms/"]) {
        assert.equal((await page.request.get(path)).status(), 404, path);
    }
    await page.goto("/");
    await expect(page.locator("#screen")).toBeVisible();
    await expect(page.locator("#rom")).toHaveAttribute("accept", /\.nes.*\.gba.*\.sfc/, { timeout: 15_000 });
    const wasm = await page.request.get(await page.evaluate(() =>
        performance.getEntriesByType("resource").find(entry => entry.name.endsWith(".wasm")).name));
    assert.match(wasm.headers()["content-type"], /application\/wasm/);
    assert.match(wasm.headers()["cache-control"], /immutable/);
    const rom = await readFile("roms/nes/automated_tests/blargg_nes_cpu_test5/cpu.nes");
    await page.locator("#rom").setInputFiles({
        name: "cpu.nes", mimeType: "application/octet-stream", buffer: rom,
    });
    await expect(page.locator("#emulation-controls")).toHaveAttribute("data-emulation-state", "running", { timeout: 15_000 });
    await page.keyboard.press("Escape");
    await page.reload();
    await expect(page.locator("#rom")).toHaveAttribute("accept", /\.nes/);
    assert.deepEqual(errors, []);
    console.log("Production image: exact revision, WASM initialization, local ROM loading and no bundled ROMs verified");
} finally {
    await browser.close();
}
