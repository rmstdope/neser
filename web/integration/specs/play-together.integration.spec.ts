import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test";
import config from "../../../playwright.config";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

// Two browser contexts on one runner connect over WebRTC with no STUN server, as two browsers on
// one network do. Chromium hides its local addresses behind multicast-DNS names by default; the
// runner may not resolve those, so this spec has the browser offer its addresses as they are.
// What is under test is the page's connecting, not the operating system's mDNS.
test.use({
    launchOptions: {
        ...config.use?.launchOptions,
        args: [...(config.use?.launchOptions?.args ?? []), "--disable-features=WebRtcHideLocalIpsWithMdns"]
    }
});

const CONNECT_TIMEOUT_MS = 20_000;

function panel(page: Page) {
    return page.locator("#netplay-panel");
}

async function openPlayTogether(page: Page) {
    await page.locator("#netplay-open").click();
    await expect(panel(page)).toBeVisible();
}

async function shownCode(page: Page): Promise<string> {
    const box = panel(page).locator(".netplay-code");
    await expect(box).toHaveText(/^NSR1-/, { timeout: CONNECT_TIMEOUT_MS });
    return (await box.textContent())!;
}

/** The joiner's browser, closed after each test so its connection does not outlive it. */
let joinerContext: BrowserContext | null = null;

async function secondBrowser(browser: Browser): Promise<Page> {
    joinerContext = await browser.newContext({ baseURL: test.info().project.use.baseURL });
    return joinerContext.newPage();
}

test.afterEach(async () => {
    await joinerContext?.close();
    joinerContext = null;
});

test.describe("Play together", () => {
    test("Given a host running a NES game and a joiner in the library, when they swap codes, then both are connected with a Great response time, and the host's chip shows when the joiner leaves", async ({
        page: host,
        browser
    }) => {
        await openApp(host);
        await loadRomFromFileInput(host);
        await waitForRunningState(host);
        const joiner = await secondBrowser(browser);
        await openApp(joiner);

        await openPlayTogether(host);
        await waitForPausedState(host);
        await panel(host).getByRole("button", { name: /Host this game/ }).click();
        const hostCode = await shownCode(host);
        await panel(host).getByRole("button", { name: "Next" }).click();

        await openPlayTogether(joiner);
        await panel(joiner).getByRole("button", { name: /Join a game/ }).click();
        await panel(joiner).getByPlaceholder("Paste the host's code here").fill(hostCode);
        await panel(joiner).getByRole("button", { name: "Next" }).click();
        const reply = await shownCode(joiner);
        await expect(panel(joiner)).toContainText("⟳ Waiting for the host to paste your code…");

        await panel(host).getByPlaceholder("Paste their code here").fill(reply);
        await panel(host).getByRole("button", { name: "Connect" }).click();
        await expect(panel(host)).toContainText("Connected to the other player.", { timeout: CONNECT_TIMEOUT_MS });
        await expect(panel(host).locator(".netplay-pill")).toHaveText(/^Response time \d+ ms · Great$/);
        await expect(panel(joiner)).toContainText("Connected to the host.", { timeout: CONNECT_TIMEOUT_MS });
        await expect(panel(joiner)).toContainText("The host will send the game next.");

        // Closing the window keeps the connection; the game carries on behind the chip.
        await host.locator("#netplay-close").click();
        await expect(panel(host)).toBeHidden();
        await expect(host.locator("#netplay-open")).toHaveText(/^Connected · \d+ ms$/);
        await waitForRunningState(host);

        await panel(joiner).getByRole("button", { name: "Disconnect" }).click();
        await expect(host.locator("#netplay-open")).toHaveText("Disconnected", { timeout: CONNECT_TIMEOUT_MS });
        await host.locator("#netplay-open").click();
        await expect(panel(host)).toContainText("The other player disconnected.");
    });

    test("Given a Game Boy Advance game, when Play together opens, then hosting is greyed out with its reason", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForRunningState(page);

        await openPlayTogether(page);
        const host = panel(page).locator("[data-choice=host]");
        await expect(host).toBeDisabled();
        await expect(host).toContainText("Network play works with NES and Super Nintendo games.");
        await expect(panel(page).locator("[data-choice=join]")).toBeFocused();
    });

    test("Given a joiner, when the pasted text is not a code, then it says so and Next stays disabled", async ({ page }) => {
        await openApp(page);
        await openPlayTogether(page);
        await expect(panel(page).locator("[data-choice=host]")).toContainText("Host a game");
        await panel(page).getByRole("button", { name: /Join a game/ }).click();
        await panel(page).getByPlaceholder("Paste the host's code here").fill("hello can u paste the thing");
        await expect(panel(page)).toContainText("That isn't a NESER code.");
        await expect(panel(page).getByRole("button", { name: "Next" })).toBeDisabled();
        await page.keyboard.press("Escape");
        await expect(panel(page)).toBeHidden();
        await expect(page.locator("#netplay-open")).toBeFocused();
    });
});
