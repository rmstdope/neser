import { expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";

const EXPECT_TIMEOUT_MS = 15_000;
const STATUS_SELECTOR = "#status";
const ROM_SELECT_ID = "rom-select";
const ROM_SELECT_SELECTOR = `#${ROM_SELECT_ID}`;
const BUNDLED_ROM_NAME = "cpu.nes";
const BUNDLED_ROM_PATH = path.join(
    "roms",
    "nes",
    "automated_tests",
    "blargg_nes_cpu_test5",
    BUNDLED_ROM_NAME
);

function readMockRomBytes() {
    return readFileSync(path.join(process.cwd(), BUNDLED_ROM_PATH));
}

function makeMinimalGbaRomBytes() {
    const rom = Buffer.alloc(0xC0);
    rom[0xB2] = 0x96;
    let check = 0;
    for (let offset = 0xA0; offset <= 0xBC; offset++) {
        check = (check - rom[offset]) & 0xFF;
    }
    rom[0xBD] = (check - 0x19) & 0xFF;
    return rom;
}

async function injectBundledRomOption(page: Page) {
    const romDataUrl = `data:application/octet-stream;base64,${readMockRomBytes().toString("base64")}`;
    await page.evaluate(({ value, romSelectId, bundledRomName }: { value: string; romSelectId: string; bundledRomName: string }) => {
        const romSelect = document.getElementById(romSelectId);
        if (!(romSelect instanceof HTMLSelectElement)) {
            throw new Error("Expected #rom-select to be an HTMLSelectElement");
        }

        const option = document.createElement("option");
        option.value = value;
        option.textContent = bundledRomName;
        romSelect.appendChild(option);
    }, {
        value: romDataUrl,
        romSelectId: ROM_SELECT_ID,
        bundledRomName: BUNDLED_ROM_NAME
    });
    return romDataUrl;
}

export async function openApp(page: Page) {
    await page.goto("/");
    await expect(page.locator("#start")).toBeVisible({ timeout: EXPECT_TIMEOUT_MS });
}

const EMULATION_CONTROLS_SELECTOR = "#emulation-controls";

/** Wait until the app itself reports this lifecycle on its controls block. */
async function waitForEmulationState(page: Page, state: "idle" | "running" | "paused") {
    await expect(page.locator(EMULATION_CONTROLS_SELECTOR)).toHaveAttribute("data-emulation-state", state, {
        timeout: EXPECT_TIMEOUT_MS
    });
}

/**
 * Give the pointer back to the player, as Escape does.
 *
 * Choosing a ROM and clicking the game capture the mouse for the game (pointer lock on the
 * canvas) wherever the browser grants it: CI's Linux Chromium does, the macOS headless shell
 * does not. While the mouse is captured the browser routes every mouse event to the canvas, so
 * no click reaches a control and Playwright reports the sidebar intercepting its own buttons
 * (nr-dv5). A player presses Escape before reaching for the sidebar; so does a spec.
 *
 * Escape is pressed on every poll, not once: the lock is granted a task or two after the load
 * requests it, and a single Escape that lands before the grant would leave the lock in place.
 */
export async function releaseCapturedMouse(page: Page) {
    await expect
        .poll(
            async () => {
                await page.keyboard.press("Escape");
                return page.evaluate(() => document.pointerLockElement === null);
            },
            {
                message: "Escape should release the mouse the game captured",
                timeout: EXPECT_TIMEOUT_MS
            }
        )
        .toBe(true);
}

/**
 * Wait until the game runs, then take the pointer back with Escape (see
 * `releaseCapturedMouse`): loading captures the mouse for the game, and every spec that
 * clicks a control afterwards is a player who has it back. A spec that needs the mouse
 * captured (a Zapper or Super Scope game) clicks the game after this, as
 * `mouse-capture.integration.spec.ts` does.
 */
export async function waitForRunningState(page: Page) {
    await waitForEmulationState(page, "running");
    await expect(page.locator("#stop")).toBeEnabled({ timeout: EXPECT_TIMEOUT_MS });
    await expect(page.locator("#pause")).toHaveText("Pause", { timeout: EXPECT_TIMEOUT_MS });
    await releaseCapturedMouse(page);
}

export async function waitForIdleState(page: Page) {
    await waitForEmulationState(page, "idle");
    await expect(page.locator("#stop")).toBeDisabled({ timeout: EXPECT_TIMEOUT_MS });
}

export async function waitForPausedState(page: Page) {
    await waitForEmulationState(page, "paused");
    await expect(page.locator("#pause")).toHaveText("Resume", { timeout: EXPECT_TIMEOUT_MS });
    await expect(page.locator("#stop")).toBeEnabled({ timeout: EXPECT_TIMEOUT_MS });
}

/** Load a NES ROM via the file input, setting romFromFile = true. */
export async function loadRomFromFileInput(page: Page) {
    const romBytes = readMockRomBytes();
    await page.locator("#rom").setInputFiles({
        name: BUNDLED_ROM_NAME,
        mimeType: "application/octet-stream",
        buffer: romBytes
    });
}

/** Load a minimal GBA ROM via the file input, setting romFromFile = true. */
export async function loadGbaRomFromFileInput(page: Page, name = "suite.gba") {
    await page.locator("#rom").setInputFiles({
        name,
        mimeType: "application/octet-stream",
        buffer: makeMinimalGbaRomBytes()
    });
}

export async function startFromBundledRom(page: Page) {
    await openApp(page);
    const romValue = await injectBundledRomOption(page);

    await page.evaluate(({ value, romSelectId }: { value: string; romSelectId: string }) => {
        const romSelect = document.getElementById(romSelectId);
        if (!(romSelect instanceof HTMLSelectElement)) {
            throw new Error("Expected #rom-select to be an HTMLSelectElement");
        }
        romSelect.value = value;
        romSelect.dispatchEvent(new Event("change", { bubbles: true }));
    }, {
        value: romValue,
        romSelectId: ROM_SELECT_ID
    });
    await waitForRunningState(page);
}