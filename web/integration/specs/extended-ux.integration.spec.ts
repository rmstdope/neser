import { test, expect, Page } from "@playwright/test";
import { collectBrowserErrors } from "../helpers/browser-errors.helpers";
import {
    openApp,
    startFromBundledRom,
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
const SHORTCUT_HELP_OVERLAY_SELECTOR = "#shortcut-help-overlay";
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

    test("Given app shell is rendered, when shortcut help key is pressed, then overlay shows shortcuts", async ({ page }) => {
        await openApp(page);

        const shortcutHelpOverlay = page.locator(SHORTCUT_HELP_OVERLAY_SELECTOR);

        await expect(shortcutHelpOverlay).toHaveAttribute("aria-hidden", "true");
        await expect(shortcutHelpOverlay).toBeHidden();

        await page.keyboard.press("h");

        await expect(shortcutHelpOverlay).toBeVisible();
        await expect(shortcutHelpOverlay).toHaveAttribute("aria-hidden", "false");
        await expect(shortcutHelpOverlay).toContainText("Shortcuts");
        await expect(shortcutHelpOverlay).toContainText("F5: Debugger Toggle");

        await page.keyboard.press("h");

        await expect(shortcutHelpOverlay).toHaveAttribute("aria-hidden", "true");
        await expect(shortcutHelpOverlay).toBeHidden();
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
});