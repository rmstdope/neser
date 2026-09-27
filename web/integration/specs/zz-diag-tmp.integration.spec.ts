import { expect, test } from "@playwright/test";
import { openApp, waitForRunningState } from "../helpers/lifecycle.helpers";
import { makeMinimalSnesRomBytes } from "../helpers/snes_rom.helpers";

async function diag(page: import("@playwright/test").Page, label: string) {
    const info = await page.evaluate(() => {
        const rect = (sel: string) => { const e = document.querySelector(sel); if (!e) return null; const r = e.getBoundingClientRect(); return [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)]; };
        const b = document.getElementById("save-state")!.getBoundingClientRect();
        const cx = b.x + b.width / 2, cy = b.y + b.height / 2;
        const stack = document.elementsFromPoint(cx, cy).map((e) => e.id || e.tagName + "." + String(e.className).slice(0, 30));
        const side = document.querySelector(".drawer-side") as HTMLElement;
        const aside = document.querySelector("aside") as HTMLElement;
        const cs = getComputedStyle(side);
        return {
            body: document.body.className, coarse: matchMedia("(pointer: coarse)").matches, inner: [innerWidth, innerHeight],
            point: [cx, cy], stack, save: rect("#save-state"), aside: rect("aside"), side: rect(".drawer-side"),
            sideStyle: [cs.position, cs.overflowY, cs.overflowX, cs.height, cs.pointerEvents], sideScroll: [side.scrollTop, side.scrollLeft, side.scrollHeight, side.clientHeight],
            asideStyle: [getComputedStyle(aside).pointerEvents, aside.scrollHeight, aside.clientHeight], pre: rect("pre"), preScroll: (document.querySelector("pre") as HTMLElement).scrollWidth,
            fullscreen: !!document.fullscreenElement, lock: document.pointerLockElement ? document.pointerLockElement.id : null, active: document.activeElement ? document.activeElement.id || document.activeElement.tagName : null, events: (window as any).__diagEvents, controls: rect("#emulation-controls"), firmware: rect("#snes-firmware-section")
        };
    });
    console.log(`DIAG ${label} ${JSON.stringify(info)}`);
}

test("diag: what covers Save State on CI", async ({ page }) => {
    await openApp(page);
    await page.evaluate(() => {
        (window as any).__diagEvents = [];
        for (const type of ["pointerdown", "mousedown", "mouseup", "click", "pointerlockchange"]) {
            window.addEventListener(type, (e: Event) => {
                const t = e.target as Element; const me = e as MouseEvent;
                (window as any).__diagEvents.push(`${type}@${me.clientX},${me.clientY}->${t && (t.id || t.tagName)}`);
            }, true);
        }
    });
    await diag(page, "before-rom");
    await page.locator("#rom").setInputFiles({ name: "suite.sfc", mimeType: "application/octet-stream", buffer: makeMinimalSnesRomBytes() });
    await waitForRunningState(page);
    for (let i = 0; i < 4; i++) { await diag(page, `running-${i}`); await page.waitForTimeout(250); }
    const clicked = await page.locator("#save-state").click({ timeout: 8000 }).then(() => true).catch((e) => { console.log("DIAG click failed: " + String(e).split("\n")[0]); return false; });
    await diag(page, `after-click-${clicked}`);
    await page.evaluate(() => document.exitPointerLock?.());
    await page.waitForTimeout(200);
    const clicked2 = await page.locator("#save-state").click({ timeout: 8000 }).then(() => true).catch((e) => { console.log("DIAG click2 failed: " + String(e).split("\n")[0]); return false; });
    await diag(page, `after-unlock-click-${clicked2}`);
    await page.evaluate(() => (document.getElementById("save-state") as HTMLButtonElement).click());
    await expect(page.locator("#save-state-section")).toHaveAttribute("data-save-state", "saved", { timeout: 10_000 });
    await diag(page, "after-js-click");
});
