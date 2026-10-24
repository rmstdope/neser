import { test, expect, Page } from "@playwright/test";
import { collectBrowserErrors } from "../helpers/browser-errors.helpers";
import {
    openApp,
    openSettings,
    startFromBundledRom,
    waitForPausedState,
    waitForRunningState,
    loadRomFromFileInput
} from "../helpers/lifecycle.helpers";

const AUTORUN_MODAL_SELECTOR = "#autorun-modal";
const AUTORUN_LOAD_BUTTON_SELECTOR = "#autorun-load";
const AUTORUN_FILE_INPUT_SELECTOR = "#autorun-file-input";
const AUTORUN_FILE_INFO_SELECTOR = "#autorun-file-info";
const AUTORUN_SUMMARY_SELECTOR = "#autorun-file-summary";
const AUTORUN_CHECKPOINT_SELECTOR = "#autorun-checkpoint-select";
const AUTORUN_EXTEND_SELECTOR = "#autorun-extend-check";
const AUTORUN_USE_BUTTON_SELECTOR = "#autorun-use-btn";
const AUTORUN_STATUS_SELECTOR = "#autorun-status";
const AUTORUN_CANCEL_SELECTOR = "#autorun-cancel";
const DEBUGGER_PANEL_SELECTOR = "#debugger-panel";

function createValidAutorunBuffer() {
    const payload = {
        version: 2,
        frames: [
            { player1: 0, player2: 0 },
            { player1: 1, player2: 0 },
            { player1: 2, player2: 0 },
            { player1: 3, player2: 0 }
        ],
        checkpoints: [
            { frame_index: 1, screen_crc: 10, state_bytes: [] },
            { frame_index: 3, screen_crc: 20, state_bytes: [] }
        ]
    };

    return Buffer.from(JSON.stringify(payload), "utf8");
}

async function openAutorunModal(page: Page) {
    await loadRomFromFileInput(page);
    // Loading a ROM auto-starts emulation; stop it so Load Autorun becomes enabled
    await page.locator("#stop").click();
    await expect(page.locator("#stop")).toBeDisabled();
    // Autorun is a developer tool: it lives in Settings › Developer.
    await openSettings(page, "Developer");
    await page.locator(AUTORUN_LOAD_BUTTON_SELECTOR).click();
    await expect(page.locator(AUTORUN_MODAL_SELECTOR)).toBeVisible();
}

async function uploadValidAutorunFile(page: Page) {
    await page.locator(AUTORUN_FILE_INPUT_SELECTOR).setInputFiles({
        name: "cpu.autorun",
        mimeType: "application/json",
        buffer: createValidAutorunBuffer()
    });
}

test.describe("Phase 3 extended UX", () => {
    test("Given autorun modal is opened, when dialog is shown, then expected controls are present", async ({ page }) => {
        await openApp(page);
        await openAutorunModal(page);

        await expect(page.locator(AUTORUN_FILE_INPUT_SELECTOR)).toBeVisible();
        await expect(page.locator(AUTORUN_CHECKPOINT_SELECTOR)).toBeVisible();
        await expect(page.locator(AUTORUN_EXTEND_SELECTOR)).toBeVisible();
        await expect(page.locator(AUTORUN_USE_BUTTON_SELECTOR)).toBeVisible();
        await expect(page.locator(AUTORUN_USE_BUTTON_SELECTOR)).toBeDisabled();
    });

    test("Given autorun modal is open, when no valid file is provided, then use action stays disabled until valid file is loaded", async ({ page }) => {
        await openApp(page);
        await openAutorunModal(page);

        const autorunUseButton = page.locator(AUTORUN_USE_BUTTON_SELECTOR);
        const autorunSummary = page.locator(AUTORUN_SUMMARY_SELECTOR);

        await expect(autorunUseButton).toBeDisabled();

        await uploadValidAutorunFile(page);

        await expect(page.locator(AUTORUN_FILE_INFO_SELECTOR)).toBeVisible();
        await expect(autorunSummary).toContainText("cpu.autorun");
        await expect(autorunSummary).toContainText("4 frames");
        await expect(autorunSummary).toContainText("2 checkpoints");
        await expect(page.locator(AUTORUN_CHECKPOINT_SELECTOR).locator("option")).toHaveCount(3);
        await expect(autorunUseButton).toBeEnabled();
    });

    test("Given autorun has been configured, when autorun state changes, then cancel visibility transitions correctly", async ({ page }) => {
        await openApp(page);
        await expect(page.locator(AUTORUN_CANCEL_SELECTOR)).toBeHidden();

        await openAutorunModal(page);
        await uploadValidAutorunFile(page);
        await page.locator(AUTORUN_EXTEND_SELECTOR).check();
        await page.locator(AUTORUN_CHECKPOINT_SELECTOR).selectOption("1");
        await page.locator(AUTORUN_USE_BUTTON_SELECTOR).click();

        await expect(page.locator(AUTORUN_STATUS_SELECTOR)).toContainText("From checkpoint 2");
        await expect(page.locator(AUTORUN_STATUS_SELECTOR)).toContainText("Extending");
        await expect(page.locator(AUTORUN_CANCEL_SELECTOR)).toBeVisible();

        await page.locator(AUTORUN_CANCEL_SELECTOR).click();

        await expect(page.locator(AUTORUN_STATUS_SELECTOR)).toHaveText("");
        await expect(page.locator(AUTORUN_CANCEL_SELECTOR)).toBeHidden();
    });

    test("Given a game is running, when H is pressed, then the keys and controls panel opens with the game paused, and H closes it", async ({ page }) => {
        await startFromBundledRom(page);
        const panel = page.locator("#help-panel");
        await expect(panel).toBeHidden();

        await page.locator("#screen").focus();
        await page.keyboard.press("h");
        await expect(panel).toBeVisible();
        await expect(panel.locator("#help-title")).toHaveText("Keys and controls");
        await waitForPausedState(page);
        const debuggerRow = panel.locator(".help-shortcut").filter({ hasText: "Debugger" });
        await expect(debuggerRow.locator("kbd")).toHaveText(["F5"]);
        await expect(panel.locator("#help-chips button")).toHaveText(["NES · Player 1", "Player 2", "GB", "GBA", "SNES"]);
        await expect(panel.locator("#help-chips button").first()).toHaveAttribute("aria-pressed", "true");
        await expect(panel.locator("#help-gamepad-note")).toHaveText("Plug in a gamepad and press any button to use it instead.");
        await expect(panel.locator('#help-pad [data-button="A"] kbd')).toHaveText(["R"]);
        await panel.locator("#help-chips button", { hasText: "Player 2" }).click();
        await expect(panel.locator('#help-pad [data-button="A"] kbd')).toHaveText(["O"]);

        await page.keyboard.press("h");
        await expect(panel).toBeHidden();
        await waitForRunningState(page);
    });

    test("Given no game, when ? is pressed in the top bar, then the panel shows any system's controller, and Esc closes it", async ({ page }) => {
        await openApp(page);
        await page.locator("#help-open").click();
        const panel = page.locator("#help-panel");
        await expect(panel).toBeVisible();
        await panel.locator("#help-chips button", { hasText: /^GBA$/ }).click();
        await expect(panel.locator("#help-chips button")).toHaveText(["GBA · Player 1", "NES", "GB", "SNES"]);
        await expect(panel.locator('#help-pad [data-button="L"] kbd')).toHaveText(["V"]);
        await expect(panel.locator('#help-pad [data-button="X"]')).toHaveCount(0);

        // The Super Nintendo's second player has keys too.
        await panel.locator("#help-chips button", { hasText: /^SNES$/ }).click();
        await panel.locator("#help-chips button", { hasText: "Player 2" }).click();
        await expect(panel.locator('#help-pad [data-button="B"] kbd')).toHaveText(["P"]);
        await page.keyboard.press("Escape");
        await expect(panel).toBeHidden();
    });

    test("Given emulator is running, when debugger is toggled with shortcut, then panel visibility changes without browser errors", async ({ page }) => {
        const { consoleErrors, pageErrors } = collectBrowserErrors(page);

        await startFromBundledRom(page);

        const debuggerPanel = page.locator(DEBUGGER_PANEL_SELECTOR);

        await expect(debuggerPanel).toBeHidden();

        await page.keyboard.press("F5");

        await expect(debuggerPanel).toBeVisible();
        await expect(debuggerPanel.locator("#dbg-continue")).toBeVisible();

        await page.keyboard.press("F5");

        await expect(debuggerPanel).toBeHidden();
        await waitForRunningState(page);

        expect(consoleErrors).toHaveLength(0);
        expect(pageErrors).toHaveLength(0);
    });

    for (const { width, gamepad } of [
        { width: 390, gamepad: false },
        { width: 320, gamepad: false },
        { width: 780, gamepad: false },
        { width: 1280, gamepad: true },
        { width: 390, gamepad: true },
        { width: 320, gamepad: true },
    ]) {
        test(`Given a ${width}px window${gamepad ? " and a gamepad" : ""}, when the keys and controls panel shows each pad, then no two buttons overlap and the pad fits the panel`, async ({ page }) => {
            if (gamepad) {
                await page.addInitScript(() => {
                    const pad = { id: "Test pad", index: 0, connected: true, mapping: "standard", buttons: [], axes: [], timestamp: 0 };
                    Object.defineProperty(navigator, "getGamepads", { value: () => [pad, null, null, null] });
                });
            }
            await page.setViewportSize({ width, height: 844 });
            await openApp(page);
            await page.evaluate(() => (document.getElementById("help-open") as HTMLButtonElement).click());
            const panel = page.locator("#help-panel");
            await expect(panel).toBeVisible();
            if (gamepad) await expect(panel.locator("#help-pad .pad-gamepad")).toHaveText("Gamepad");
            for (const system of ["NES", "GBA", "SNES"]) {
                await panel.locator("#help-chips button", { hasText: new RegExp(`^${system}( · Player 1)?$`) }).click();
                const boxes = await panel.locator("#help-pad .pad-button, #help-pad .pad-gamepad").evaluateAll((els) =>
                    els.map((el) => {
                        const r = el.getBoundingClientRect();
                        return { name: (el as HTMLElement).dataset.button ?? "Gamepad", l: r.left, r: r.right, t: r.top, b: r.bottom };
                    })
                );
                const overlaps = boxes.flatMap((a, i) =>
                    boxes.slice(i + 1).filter((b) => a.l < b.r && b.l < a.r && a.t < b.b && b.t < a.b).map((b) => `${a.name}×${b.name}`)
                );
                expect(overlaps, system).toEqual([]);
                // A keyboard player's keys move to the list whenever the pad is shrunk, at any width (a
                // 780px window's two-column panel shrinks it too); a gamepad player never has one.
                const shrunk = await panel.locator("#help-pad").evaluate((el) => Number((el as HTMLElement).style.zoom) < 1);
                await expect(panel.locator("#help-pad-keys"), system).toBeVisible({ visible: shrunk && !gamepad });
                // Nothing in the panel runs past it, so it never scrolls sideways.
                const fits = await panel.evaluate((el) => el.scrollWidth <= el.clientWidth);
                expect(fits, system).toBe(true);
            }
        });
    }

    test("Given a 320px window, when the keys and controls panel opens, then the shrunk pad carries no keys and a list beneath it does, at full size", async ({ page }) => {
        await page.setViewportSize({ width: 320, height: 844 });
        await openApp(page);
        await page.evaluate(() => (document.getElementById("help-open") as HTMLButtonElement).click());
        const panel = page.locator("#help-panel");
        const list = panel.locator("#help-pad-keys");
        await expect(panel).toBeVisible();
        await panel.locator("#help-chips button", { hasText: /^NES( · Player 1)?$/ }).click();
        await expect(list).toBeVisible();
        await expect(panel.locator("#help-pad .pad-keys kbd").first()).toBeHidden();
        await expect(list.locator(".pad-key-label")).toHaveText(["D-Pad", "A", "B", "Select", "Start"]);
        await expect(list.locator(".pad-key-row").first().locator("kbd")).toHaveText(["W", "A", "S", "D"]);
        const height = (selector: string) => panel.locator(selector).first().evaluate((el) => el.getBoundingClientRect().height);
        expect(await height("#help-pad-keys kbd")).toBe(await height(".help-shortcut kbd"));
        const order = await panel.evaluate(() => {
            const bottom = (id: string) => document.getElementById(id)!.getBoundingClientRect();
            return bottom("help-pad").bottom <= bottom("help-pad-keys").top && bottom("help-pad-keys").bottom <= bottom("help-gamepad-note").top;
        });
        expect(order).toBe(true);
        expect(await panel.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);

        await panel.locator("#help-chips button", { hasText: /^SNES$/ }).click();
        await expect(list.locator(".pad-key-label")).toHaveText(["D-Pad", "B", "A", "X", "Y", "L", "R", "Select", "Start"]);

        // Widening the window past the pad's full size moves the keys back onto it, and back again.
        await page.setViewportSize({ width: 1280, height: 844 });
        await expect(list).toBeHidden();
        await expect(panel.locator('#help-pad [data-button="A"] kbd')).toBeVisible();
        await page.setViewportSize({ width: 320, height: 844 });
        await expect(list).toBeVisible();
        await expect(panel.locator('#help-pad [data-button="A"] kbd')).toBeHidden();
    });

    test("Given a 390px window and a gamepad, when the keys and controls panel opens, then there is no list and the Gamepad cap is full size", async ({ page }) => {
        await page.addInitScript(() => {
            const pad = { id: "Test pad", index: 0, connected: true, mapping: "standard", buttons: [], axes: [], timestamp: 0 };
            Object.defineProperty(navigator, "getGamepads", { value: () => [pad, null, null, null] });
        });
        await page.setViewportSize({ width: 390, height: 844 });
        await openApp(page);
        await page.evaluate(() => (document.getElementById("help-open") as HTMLButtonElement).click());
        const panel = page.locator("#help-panel");
        await expect(panel.locator("#help-pad .pad-gamepad")).toHaveText("Gamepad");
        await expect(panel.locator("#help-pad-keys")).toBeHidden();
        await expect(panel.locator("#help-gamepad-note")).toBeHidden();
        const height = (selector: string) => panel.locator(selector).first().evaluate((el) => el.getBoundingClientRect().height);
        expect(await height("#help-pad .pad-gamepad kbd")).toBeCloseTo(await height(".help-shortcut kbd"), 0);
    });
});
