import { expect, Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";

const EXPECT_TIMEOUT_MS = 20_000;
const STATUS_SELECTOR = "#status";
const ROM_SELECT_ID = "rom-select";
const ROM_SELECT_SELECTOR = `#${ROM_SELECT_ID}`;
const BUNDLED_ROM_NAME = "cpu.nes";
const IDLE_STATUS_PATTERN = /Load a ROM to begin|Stopped\. You can restart or load a new ROM/;

function readMockRomBytes() {
    return readFileSync(path.join(process.cwd(), "roms", "nes", BUNDLED_ROM_NAME));
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
}

export async function openApp(page: Page) {
    await page.goto("/");
    await expect(page.locator(STATUS_SELECTOR)).toBeVisible();
    await expect(page.locator("#shortcut-reference")).toContainText("Shortcuts:", {
        timeout: EXPECT_TIMEOUT_MS
    });
}

export async function waitForRunningState(page: Page) {
    await expect(page.locator(STATUS_SELECTOR)).toContainText("Running...", {
        timeout: EXPECT_TIMEOUT_MS
    });
}

export async function waitForIdleState(page: Page) {
    await expect(page.locator(STATUS_SELECTOR)).toHaveText(IDLE_STATUS_PATTERN, {
        timeout: EXPECT_TIMEOUT_MS
    });
}

export async function startFromBundledRom(page: Page) {
    await openApp(page);
    await injectBundledRomOption(page);

    const bundledRomOption = page.locator(`${ROM_SELECT_SELECTOR} option`, {
        hasText: BUNDLED_ROM_NAME
    });
    await expect(bundledRomOption).toHaveCount(1, {
        timeout: EXPECT_TIMEOUT_MS
    });

    const romValue = await bundledRomOption.getAttribute("value");
    if (!romValue) {
        throw new Error("Expected bundled ROM option to have a value");
    }

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