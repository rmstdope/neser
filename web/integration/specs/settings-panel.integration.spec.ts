import { test, expect, type Page } from "@playwright/test";
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
        await expect(page.locator("#help-panel")).toBeHidden();
    });

    test("Given an NES game, when Controls opens, then it lists the NES keys for both players, and the SNES keys for both of its players", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);

        await openSettings(page, "Controls");
        const keys = page.locator("#settings-keys");
        await expect(page.locator('#settings-keys-system [data-console="nes"]')).toHaveAttribute("aria-pressed", "true");
        await expect(keys.locator("h4")).toHaveText(["Player 1", "Player 2"]);
        await expect(keys.locator(".settings-key-action").first()).toHaveText("D-Pad");

        // The Super Nintendo has a second player too, on its own keys.
        await page.locator('#settings-keys-system [data-console="snes"]').click();
        await expect(keys.locator("h4")).toHaveText(["Player 1", "Player 2"]);
        const xRow = keys.locator(".settings-key-row").filter({ has: page.locator(".settings-key-action", { hasText: /^X$/ }) });
        await expect(xRow.locator("kbd")).toHaveText(["Y"]);
    });
});

test.describe("Settings panel, firmware and debugger", () => {
    test("Given SNES firmware not yet added, when a wrong file is chosen with Add…, then the panel says why, readable inside it", async ({ page }) => {
        await openApp(page);
        await openSettings(page);
        const add = page.getByRole("button", { name: "Add the DSP-2 firmware" });
        await expect(add).toBeVisible();
        const chooser = page.waitForEvent("filechooser");
        await add.click();
        await (await chooser).setFiles({ name: "x.rom", mimeType: "application/octet-stream", buffer: Buffer.alloc(100) });

        const message = page.locator("#snes-firmware-message");
        await expect(message).toBeVisible();
        await expect(message).toContainText("That isn't a DSP-2 firmware file");
        await expect(message).toContainText('"x.rom" is 100 bytes; the firmware is exactly 8 KB.');
        expect(await message.evaluate((el) => el.closest("#settings-panel") !== null)).toBe(true);

        // The words are about that file only: the next opening starts clear.
        await closeSettings(page);
        await openSettings(page);
        await expect(message).toBeHidden();
    });

    test("Given the open panel, then the top bar above it stays clear of the dimmed backdrop", async ({ page }) => {
        await openApp(page);
        await openSettings(page);
        const backdropTop = await page.locator("#settings-panel").evaluate((el) => getComputedStyle(el, "::backdrop").top);
        const header = (await page.locator("header").boundingBox())!;
        expect(parseFloat(backdropTop)).toBeGreaterThanOrEqual(header.y + header.height - 1);
    });

    test("Given the debugger button switched on, then the bug button shows with an NES game, docks the debugger beside the picture, and is remembered", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        const bug = page.locator("#debugger-open");
        await expect(bug).toBeHidden();

        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(bug).toBeVisible();
        await bug.click();
        const panel = page.locator("#debugger-panel");
        await expect(panel).toBeVisible();
        await expect(bug).toHaveAttribute("aria-pressed", "true");
        const picture = (await page.locator("#screen").boundingBox())!;
        const dock = (await panel.boundingBox())!;
        expect(dock.x).toBeGreaterThanOrEqual(picture.x + picture.width);

        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(panel).toBeHidden();
        await expect(bug).toHaveAttribute("aria-pressed", "false");

        await page.reload();
        await openSettings(page, "Developer");
        await expect(page.getByLabel("Show debugger button")).toBeChecked();
    });

    test("Given the docked debugger, when the game is ejected and another starts, then the debugger is closed and the button agrees", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        const bug = page.locator("#debugger-open");
        const panel = page.locator("#debugger-panel");
        await bug.click();
        await expect(panel).toBeVisible();

        await page.locator("#stop").click();
        await expect(panel).toHaveClass(/\bhidden\b/);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(panel).toBeHidden();
        await expect(bug).toHaveAttribute("aria-pressed", "false");
        // One press opens it again.
        await bug.click();
        await expect(panel).toBeVisible();
    });

    test("Given the debugger button switched on, when a Game Boy game runs, then there is no bug button and the picture has the view to itself", async ({ page }) => {
        await openApp(page);
        await openSettings(page, "Developer");
        await page.getByLabel("Show debugger button").check();
        await closeSettings(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#debugger-open").click();
        await expect(page.locator("#debugger-panel")).toBeVisible();

        await page.locator("#rom").setInputFiles("roms/gb/automated_tests/acid/dmg-acid2.gb");
        await waitForRunningState(page);
        await expect(page.locator("#debugger-open")).toBeHidden();
        await expect(page.locator("#game-view")).toHaveCSS("flex-direction", "column");
    });

    test("Given a narrow window, when the debugger opens, then it docks under the picture rather than squeezing beside it", async ({ page }) => {
        await page.setViewportSize({ width: 760, height: 900 });
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        const panel = page.locator("#debugger-panel");
        await expect(panel).toBeVisible();
        const picture = (await page.locator("#screen").boundingBox())!;
        const dock = (await panel.boundingBox())!;
        expect(dock.y).toBeGreaterThanOrEqual(picture.y + picture.height);
        expect(dock.width).toBeGreaterThan(600);
        // Its title, registers and current instruction are in the window, without scrolling to them.
        expect(dock.y + 200).toBeLessThanOrEqual(900);
        expect(await page.evaluate(() => window.scrollY)).toBe(0);
    });

    /** Starts the NES game in a window of the given size and opens the debugger with F5. */
    async function openDebuggerIn(page: Page, width: number, height: number) {
        await page.setViewportSize({ width, height });
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(page.locator("#debugger-panel #dbg-continue")).toBeVisible();
    }

    test("Given the debugger open in a 720px-tall window, when the window crosses 900px wide, then the picture keeps about its size and the debugger's top stays in view", async ({ page }) => {
        await openDebuggerIn(page, 901, 720);
        const panel = page.locator("#debugger-panel");
        const beside = (await page.locator("#screen").boundingBox())!;
        expect((await panel.boundingBox())!.x).toBeGreaterThanOrEqual(beside.x + beside.width);

        await page.setViewportSize({ width: 900, height: 720 });
        // The window's resize event, which caps the picture, runs before the next paint, not at once.
        await expect.poll(async () => Math.abs((await page.locator("#screen").boundingBox())!.width - beside.width) / beside.width).toBeLessThan(0.15);
        const under = (await page.locator("#screen").boundingBox())!;
        const dock = (await panel.boundingBox())!;
        expect(dock.y).toBeGreaterThanOrEqual(under.y + under.height);
        expect(dock.y + 200).toBeLessThanOrEqual(720);
        expect(await page.evaluate(() => window.scrollY)).toBe(0);

        // And back: only the debugger changes place.
        await page.setViewportSize({ width: 901, height: 720 });
        await expect.poll(async () => Math.abs((await page.locator("#screen").boundingBox())!.width - beside.width)).toBeLessThan(2);
    });

    test("Given a narrow window, when the debugger is closed, then the picture takes its full size again", async ({ page }) => {
        await page.setViewportSize({ width: 850, height: 720 });
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        const full = (await page.locator("#screen").boundingBox())!.width;
        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(page.locator("#debugger-panel")).toBeVisible();
        expect((await page.locator("#screen").boundingBox())!.width).toBeLessThan(full * 0.7);

        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(page.locator("#debugger-panel")).toBeHidden();
        expect((await page.locator("#screen").boundingBox())!.width).toBe(full);
    });

    test("Given a short narrow window, when the debugger opens, then the picture stays at the NES's own 256x240", async ({ page }) => {
        await openDebuggerIn(page, 850, 400);
        const picture = (await page.locator("#screen").boundingBox())!;
        expect(picture.height).toBeGreaterThanOrEqual(240 - 1);
        expect(picture.height).toBeLessThan(260);
        // The debugger starts below the bottom edge, so opening scrolls just enough to show its top.
        expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
        expect((await page.locator("#debugger-panel").boundingBox())!.y + 200).toBeLessThanOrEqual(400 + 1);
    });

    test("Given a narrow 720px-tall window, when the debugger opens, then its top is in view without scrolling", async ({ page }) => {
        await openDebuggerIn(page, 900, 720);
        expect(await page.evaluate(() => window.scrollY)).toBe(0);
        expect((await page.locator("#debugger-panel").boundingBox())!.y + 200).toBeLessThanOrEqual(720);
    });

    test("Given the debugger docked under a capped picture, when it closes, then zooming in is offered again", async ({ page }) => {
        await openDebuggerIn(page, 850, 720);
        const plus = page.locator("#screen-plus");
        await expect(plus).toBeDisabled();
        await page.locator("#screen").focus();
        await page.keyboard.press("F5");
        await expect(page.locator("#debugger-panel")).toBeHidden();
        await expect(plus).toBeEnabled();
    });

    test("Given the open panel over a running game, when the backdrop is clicked, then it closes and the game carries on", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await openSettings(page);
        await page.mouse.click(10, 400);
        await expect(page.locator("#settings-panel")).toBeHidden();
        await waitForRunningState(page);
    });

    test("Given the open panel over a running game, when Space is pressed in it, then the game stays paused", async ({ page }) => {
        await openApp(page);
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await openSettings(page, "Controls");
        await page.getByRole("tab", { name: "Controls" }).focus();
        await page.keyboard.press("Space");
        await waitForPausedState(page);
    });
});
