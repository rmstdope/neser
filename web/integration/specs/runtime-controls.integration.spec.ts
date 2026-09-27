import { test, expect } from "@playwright/test";
import {
    openApp,
    startFromBundledRom,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

const MUTE_BUTTON_SELECTOR = "#mute";
const FILTER_TOGGLE_SELECTOR = "#filter-toggle";
const SCREEN_PLUS_SELECTOR = "#screen-plus";
const SCREEN_MINUS_SELECTOR = "#screen-minus";
const SCREEN_SELECTOR = "#screen";
const STOP_BUTTON_SELECTOR = "#stop";

test.describe("Phase 2 runtime controls", () => {
    test("Given emulator is running, when keyboard input is sent, then no error state appears and input path remains active", async ({ page }) => {
        // Collect console errors and page errors
        const consoleErrors: string[] = [];
        const pageErrors: string[] = [];

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

    test("Given mute button exists, when toggled, then state and aria-pressed update correctly", async ({ page }) => {
        await openApp(page);

        const muteButton = page.locator(MUTE_BUTTON_SELECTOR);

        // Wait for element to be visible, enabled and stable
        await expect(muteButton).toBeVisible({ timeout: 10000 });
        await expect(muteButton).toBeEnabled({ timeout: 10000 });

        // Initial state should be unmuted (default)
        await expect(muteButton).toHaveAttribute("aria-pressed", "false");
        await expect(muteButton).toContainText(/Audio.*On/i);

        // Click to mute
        await muteButton.click({ timeout: 5000 });
        await expect(muteButton).toHaveAttribute("aria-pressed", "true");
        await expect(muteButton).toContainText(/Audio.*Off/i);

        // Click to unmute
        await muteButton.click({ timeout: 5000 });
        await expect(muteButton).toHaveAttribute("aria-pressed", "false");
        await expect(muteButton).toContainText(/Audio.*On/i);
    });

    test("Given filter toggle exists, when toggled repeatedly, then filter cycles without crashes", async ({ page }) => {
        // Start emulation first so we test filter toggling during active rendering
        // (avoids double page navigation which can exceed the 45s test timeout on CI)
        await startFromBundledRom(page);

        const filterToggle = page.locator(FILTER_TOGGLE_SELECTOR);

        // Wait for element to be visible, enabled and stable
        await expect(filterToggle).toBeVisible({ timeout: 10000 });
        await expect(filterToggle).toBeEnabled({ timeout: 10000 });

        // Get initial filter text
        const initialText = await filterToggle.textContent();
        expect(initialText).toContain("Filter:");

        // Click multiple times to cycle through filters while emulator is running
        const clickCount = 3;

        for (let i = 0; i < clickCount; i++) {
            await filterToggle.click({ timeout: 5000 });

            // Wait for text to update
            await page.waitForTimeout(50);

            const currentText = await filterToggle.textContent();
            expect(currentText).toContain("Filter:");

            // Text may change or stay the same depending on filter count
            // The important thing is no crash occurs
        }

        // Verify emulator still running after filter cycling
        await waitForRunningState(page);
    });

    // Every assignment to the screen's width or height reallocates the WebGL drawing buffer, and
    // Blink does it with synchronous GPU round trips that first wait for every frame in flight:
    // seconds on CI's software GL (a Zoom - once measured 4.97 s, nr-dv5). So a zoom click may
    // assign each at most once (nr-b5h; the count is kept on the page by an init script), and the
    // screen context has no multisampling, which tripled those round trips (nr-v5x).
    test("Given zoom controls exist, when clicked, then canvas presentation bounds change safely", async ({ page }) => {
        await page.addInitScript(() => {
            const counts = { width: 0, height: 0 };
            (window as unknown as { __screenBackingStoreWrites: typeof counts }).__screenBackingStoreWrites = counts;
            for (const dimension of ["width", "height"] as const) {
                const descriptor = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, dimension)!;
                Object.defineProperty(HTMLCanvasElement.prototype, dimension, {
                    ...descriptor,
                    set(this: HTMLCanvasElement, value: number) {
                        if (this.id === "screen") {
                            counts[dimension] += 1;
                        }
                        descriptor.set!.call(this, value);
                    },
                });
            }
            // A capture listener on window runs before the button's own handler and a bubble one
            // after it, so the gap is how long the click blocks the page. Playwright's click time
            // also waits for scrolling and for stable animation frames, which are slow on CI.
            const blocked: number[] = [];
            (window as unknown as { __clickBlockedMs: number[] }).__clickBlockedMs = blocked;
            let clickStartedAt = 0;
            window.addEventListener("click", () => { clickStartedAt = performance.now(); }, true);
            window.addEventListener("click", () => { blocked.push(performance.now() - clickStartedAt); });
        });
        const lastClickBlockedMs = () => page.evaluate(() => (window as unknown as { __clickBlockedMs: number[] }).__clickBlockedMs.slice(-1)[0] ?? NaN);
        const backingStoreWrites = () => page.evaluate(() => ({
            ...(window as unknown as { __screenBackingStoreWrites: { width: number; height: number } }).__screenBackingStoreWrites,
        }));
        const clickCountingWrites = async (button: ReturnType<typeof page.locator>, label: string) => {
            const before = await backingStoreWrites();
            const startedAt = Date.now();
            await button.click();
            const elapsedMs = Date.now() - startedAt;
            const after = await backingStoreWrites();
            const blockedMs = Math.round(await lastClickBlockedMs());
            console.log(`[nr-v5x] ${label} click took ${elapsedMs} ms, of which the page was blocked ${blockedMs} ms`);
            test.info().annotations.push({ type: "zoom-click-ms", description: `${label}: ${elapsedMs} (blocked ${blockedMs})` });
            expect(after.width - before.width, `${label}: canvas.width assignments`).toBeLessThanOrEqual(1);
            expect(after.height - before.height, `${label}: canvas.height assignments`).toBeLessThanOrEqual(1);
        };

        await openApp(page);

        // getContext with the type already created returns that context, so this reads the app's own.
        const antialias = await page.evaluate(
            (selector) => (document.querySelector(selector) as HTMLCanvasElement).getContext("webgl")?.getContextAttributes()?.antialias,
            SCREEN_SELECTOR,
        );
        expect(antialias, "the screen's WebGL context is multisampled").toBe(false);

        const screenPlus = page.locator(SCREEN_PLUS_SELECTOR);
        const screenMinus = page.locator(SCREEN_MINUS_SELECTOR);
        const screen = page.locator(SCREEN_SELECTOR);

        // Wait for elements to be visible, enabled and stable
        await expect(screenPlus).toBeVisible({ timeout: 10000 });
        await expect(screenPlus).toBeEnabled({ timeout: 10000 });
        await expect(screenMinus).toBeVisible({ timeout: 10000 });
        await expect(screenMinus).toBeEnabled({ timeout: 10000 });
        await expect(screen).toBeVisible({ timeout: 10000 });

        // Get initial canvas height
        const initialBox = await screen.boundingBox();
        expect(initialBox).not.toBeNull();
        const initialHeight = initialBox!.height;

        // Click zoom in
        await clickCountingWrites(screenPlus, "Zoom +");
        await page.waitForTimeout(100);

        const zoomedInBox = await screen.boundingBox();
        expect(zoomedInBox).not.toBeNull();
        const zoomedInHeight = zoomedInBox!.height;

        // Both buttons were enabled, which promises a visible change: the probe found one.
        expect(zoomedInHeight).toBeGreaterThan(initialHeight);

        // Click zoom out
        await clickCountingWrites(screenMinus, "Zoom -");
        await page.waitForTimeout(100);

        const zoomedOutBox = await screen.boundingBox();
        expect(zoomedOutBox).not.toBeNull();
        const zoomedOutHeight = zoomedOutBox!.height;

        expect(zoomedOutHeight).toBeLessThan(zoomedInHeight);

        // Verify controls are still functional (not disabled unexpectedly)
        // Note: buttons may be disabled if at min/max zoom, but not both at once
        const plusDisabled = await screenPlus.isDisabled();
        const minusDisabled = await screenMinus.isDisabled();

        // Both zoom controls should never be disabled simultaneously
        expect(plusDisabled && minusDisabled).toBeFalsy();
    });
});
