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

export async function waitForRunningState(page: Page) {
    await expect(page.locator("#stop")).toBeEnabled({ timeout: EXPECT_TIMEOUT_MS });
}

export async function waitForIdleState(page: Page) {
    await expect(page.locator("#stop")).toBeDisabled({ timeout: EXPECT_TIMEOUT_MS });
}

export async function waitForPausedState(page: Page) {
    // Wait for both the button text and the enabled state to ensure deterministic paused state
    await expect(page.locator("#pause")).toHaveText("Resume", { timeout: EXPECT_TIMEOUT_MS });
    // Also verify stop button is still enabled (emulation is paused, not stopped)
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