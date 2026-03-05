import { test, expect } from "@playwright/test";
import {
    openApp,
    startFromBundledRom,
    waitForRunningState
} from "../helpers/lifecycle.helpers.mjs";

const GAMEPAD_TOGGLE_SELECTOR = "#gamepad-toggle";
const MUTE_BUTTON_SELECTOR = "#mute";
const FILTER_TOGGLE_SELECTOR = "#filter-toggle";
const SCREEN_PLUS_SELECTOR = "#screen-plus";
const SCREEN_MINUS_SELECTOR = "#screen-minus";
const SCREEN_SELECTOR = "#screen";
const STOP_BUTTON_SELECTOR = "#stop";

test.describe("Phase 2 runtime controls", () => {
    test("Given emulator is running, when keyboard input is sent, then no error state appears and input path remains active", async ({ page }) => {
        // Collect console errors and page errors
        const consoleErrors = [];
        const pageErrors = [];

        page.on("console", (msg) => {
            if (msg.type() === "error") {
                consoleErrors.push(msg.text());
            }
        });

        page.on("pageerror", (error) => {
            pageErrors.push(error.message);
        });

        await startFromBundledRom(page);

        // Test that keyboard input reaches the emulator by sending key events
        // Input keys: W/A/S/D/F/G/R/T
        const inputKeys = ["w", "a", "s", "d", "f", "g", "r", "t"];

        for (const key of inputKeys) {
            await page.locator(SCREEN_SELECTOR).press(key);
        }

        // Verify emulator is still running without errors
        await waitForRunningState(page);

        // Stop the emulator
        await page.locator(STOP_BUTTON_SELECTOR).click();

        // When stopped, input should be ignored safely (no crashes)
        for (const key of inputKeys) {
            await page.locator(SCREEN_SELECTOR).press(key);
        }

        // Wait a brief moment for any async errors to appear
        await page.waitForTimeout(100);

        // Verify no errors occurred during input
        expect(consoleErrors).toHaveLength(0);
        expect(pageErrors).toHaveLength(0);
    });

    test("Given gamepad toggle exists, when toggled, then state and aria-pressed update correctly", async ({ page }) => {
        await openApp(page);

        const gamepadToggle = page.locator(GAMEPAD_TOGGLE_SELECTOR);

        // Initial state should be "on" (default)
        await expect(gamepadToggle).toHaveAttribute("aria-pressed", "true");
        await expect(gamepadToggle).toContainText(/Gamepad.*On/i);

        // Click to toggle off
        await gamepadToggle.click();
        await expect(gamepadToggle).toHaveAttribute("aria-pressed", "false");
        await expect(gamepadToggle).toContainText(/Gamepad.*Off/i);

        // Click to toggle back on
        await gamepadToggle.click();
        await expect(gamepadToggle).toHaveAttribute("aria-pressed", "true");
        await expect(gamepadToggle).toContainText(/Gamepad.*On/i);
    });

    test("Given mute button exists, when toggled, then state and aria-pressed update correctly", async ({ page }) => {
        await openApp(page);

        const muteButton = page.locator(MUTE_BUTTON_SELECTOR);

        // Initial state should be unmuted (default)
        await expect(muteButton).toHaveAttribute("aria-pressed", "false");
        await expect(muteButton).toContainText(/Audio.*On/i);

        // Click to mute
        await muteButton.click();
        await expect(muteButton).toHaveAttribute("aria-pressed", "true");
        await expect(muteButton).toContainText(/Audio.*Off/i);

        // Click to unmute
        await muteButton.click();
        await expect(muteButton).toHaveAttribute("aria-pressed", "false");
        await expect(muteButton).toContainText(/Audio.*On/i);
    });

    test("Given filter toggle exists, when toggled repeatedly, then filter cycles without crashes", async ({ page }) => {
        await openApp(page);

        const filterToggle = page.locator(FILTER_TOGGLE_SELECTOR);

        // Get initial filter text
        const initialText = await filterToggle.textContent();
        expect(initialText).toContain("Filter:");

        // Click multiple times to cycle through filters
        const clickCount = 5;

        for (let i = 0; i < clickCount; i++) {
            await filterToggle.click();

            // Wait for text to update
            await page.waitForTimeout(50);

            const currentText = await filterToggle.textContent();
            expect(currentText).toContain("Filter:");

            // Text may change or stay the same depending on filter count
            // The important thing is no crash occurs
        }

        // Start emulation to test filter toggle doesn't crash rendering loop
        await startFromBundledRom(page);

        // Toggle filter while running
        await filterToggle.click();

        // Verify emulator still running
        await waitForRunningState(page);

        // Toggle again
        await filterToggle.click();

        // Verify emulator still running
        await waitForRunningState(page);
    });

    test("Given zoom controls exist, when clicked, then canvas presentation bounds change safely", async ({ page }) => {
        await openApp(page);

        const screenPlus = page.locator(SCREEN_PLUS_SELECTOR);
        const screenMinus = page.locator(SCREEN_MINUS_SELECTOR);
        const screen = page.locator(SCREEN_SELECTOR);

        // Get initial canvas height
        const initialBox = await screen.boundingBox();
        expect(initialBox).not.toBeNull();
        const initialHeight = initialBox.height;

        // Click zoom in
        await screenPlus.click();
        await page.waitForTimeout(100);

        const zoomedInBox = await screen.boundingBox();
        expect(zoomedInBox).not.toBeNull();
        const zoomedInHeight = zoomedInBox.height;

        // Height should increase or stay the same (if at max)
        expect(zoomedInHeight).toBeGreaterThanOrEqual(initialHeight);

        // Click zoom out
        await screenMinus.click();
        await page.waitForTimeout(100);

        const zoomedOutBox = await screen.boundingBox();
        expect(zoomedOutBox).not.toBeNull();
        const zoomedOutHeight = zoomedOutBox.height;

        // Height should decrease or stay the same (depending on state)
        expect(zoomedOutHeight).toBeLessThanOrEqual(zoomedInHeight);

        // Verify controls are still functional (not disabled unexpectedly)
        // Note: buttons may be disabled if at min/max zoom, but not both at once
        const plusDisabled = await screenPlus.isDisabled();
        const minusDisabled = await screenMinus.isDisabled();

        // Both zoom controls should never be disabled simultaneously
        expect(plusDisabled && minusDisabled).toBeFalsy();
    });
});
