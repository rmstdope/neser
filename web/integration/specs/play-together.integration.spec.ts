import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test";
import config from "../../../playwright.config";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    waitForIdleState,
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

/** The two players swap codes until both windows say they are connected. */
async function connect(host: Page, joiner: Page) {
    await openPlayTogether(host);
    await panel(host).getByRole("button", { name: /Host this game/ }).click();
    const hostCode = await shownCode(host);
    await panel(host).getByRole("button", { name: "Next" }).click();

    await openPlayTogether(joiner);
    await panel(joiner).getByRole("button", { name: /Join a game/ }).click();
    await panel(joiner).getByPlaceholder("Paste the host's code here").fill(hostCode);
    await panel(joiner).getByRole("button", { name: "Next" }).click();
    const reply = await shownCode(joiner);

    await panel(host).getByPlaceholder("Paste their code here").fill(reply);
    await panel(host).getByRole("button", { name: "Connect" }).click();
    await expect(panel(host)).toContainText("Connected to the other player.", { timeout: CONNECT_TIMEOUT_MS });
    await expect(panel(joiner)).toContainText("Connected to the host.", { timeout: CONNECT_TIMEOUT_MS });
}

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
        await expect(panel(joiner)).toContainText("⟳ Waiting for the host to start the game…");

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

    test("Given a host running a NES game and a joiner running a game of their own, when the host presses Start game, then both count down as players 1 and 2 and run the host's game, which the joiner does not keep", async ({
        page: host,
        browser
    }) => {
        await openApp(host);
        await loadRomFromFileInput(host);
        await waitForRunningState(host);
        const joiner = await secondBrowser(browser);
        await openApp(joiner);
        await loadGbaRomFromFileInput(joiner);
        await waitForRunningState(joiner);

        await openPlayTogether(joiner);
        await panel(joiner).getByRole("button", { name: /Join a game/ }).click();
        await expect(panel(joiner)).toContainText("will be closed when the host's game arrives. Your saves are kept.");
        await joiner.keyboard.press("Escape");
        await connect(host, joiner);
        await expect(panel(joiner)).toContainText("⟳ Waiting for the host to start the game…");
        const hostName = await host.locator("#game-name").textContent();
        await expect(panel(host)).toContainText(`When you're both ready, send ${hostName} and start.`);
        await expect(panel(host).getByRole("button", { name: "Start game" })).toBeFocused();

        await panel(host).getByRole("button", { name: "Start game" }).click();
        for (const [page, player] of [[host, 1], [joiner, 2]] as const) {
            await expect(page.locator(".netplay-countdown")).toContainText(`You are player ${player}`, { timeout: CONNECT_TIMEOUT_MS });
            await expect(panel(page)).toBeHidden();
        }
        for (const page of [host, joiner]) {
            await expect(page.locator(".netplay-countdown")).toBeHidden({ timeout: 5_000 });
            await waitForRunningState(page);
        }
        await expect(joiner.locator("#game-name")).toHaveText(hostName!);
        await expect(joiner.locator("#netplay-open")).toHaveText(/^Connected · \d+ ms$/);
        await expect(joiner.locator("#save-state")).toBeDisabled();
        await expect(joiner.locator("#recent")).not.toContainText(hostName!);

        // Playing together ends: the received game goes with it.
        await openPlayTogether(host);
        await panel(host).getByRole("button", { name: "Disconnect" }).click();
        await expect(joiner.locator("#netplay-open")).toHaveText("Disconnected", { timeout: CONNECT_TIMEOUT_MS });
        await waitForIdleState(joiner);
        await expect(joiner.locator("#library")).toBeVisible();
        await host.keyboard.press("Escape");
        await waitForRunningState(host);
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
