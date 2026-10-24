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

/**
 * The shared game runs. Eject is greyed out while playing together (aria-disabled, which
 * Playwright reads as disabled), so this does not wait for it as `waitForRunningState` does.
 */
async function waitForPlaying(page: Page) {
    await expect(page.locator("#emulation-controls")).toHaveAttribute("data-emulation-state", "running", { timeout: CONNECT_TIMEOUT_MS });
    await expect(page.locator("#pause")).toHaveText("Pause");
    await expect(page.locator("#pause")).toBeEnabled();
}

function endedScreen(page: Page) {
    return page.locator(".netplay-screen", { hasText: "Playing together has ended" });
}

/** Answers "End playing together?" with End playing together. */
async function confirmEnd(page: Page) {
    const question = page.locator(".netplay-confirm");
    await expect(question).toContainText("The game stops for both of you. You'll need new codes to play together again.");
    await expect(question.getByRole("button", { name: "Keep playing" })).toBeFocused();
    await question.getByRole("button", { name: "End playing together" }).click();
}

/** The host runs a NES game, the two connect, the host starts it, and both are playing after the count. */
async function startPlaying(host: Page, browser: Browser): Promise<Page> {
    await openApp(host);
    await loadRomFromFileInput(host);
    await waitForRunningState(host);
    const joiner = await secondBrowser(browser);
    await openApp(joiner);
    await connect(host, joiner);
    await panel(host).getByRole("button", { name: "Start game" }).click();
    for (const page of [host, joiner]) {
        await expect(page.locator(".netplay-countdown")).toBeVisible({ timeout: CONNECT_TIMEOUT_MS });
        await expect(page.locator(".netplay-countdown")).toBeHidden({ timeout: 5_000 });
        await waitForPlaying(page);
    }
    return joiner;
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
        // Two browsers connect, count down and play through several shared steps.
        test.slow();
        await openApp(host);
        await loadRomFromFileInput(host);
        await waitForRunningState(host);
        const joiner = await secondBrowser(browser);
        await openApp(joiner);
        await loadGbaRomFromFileInput(joiner);
        await waitForPlaying(joiner);

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
            await waitForPlaying(page);
        }
        await expect(joiner.locator("#game-name")).toHaveText(hostName!);
        await expect(joiner.locator("#netplay-open")).toHaveText(/^Connected · \d+ ms$/);
        await expect(joiner.locator("#save-state")).toBeDisabled();
        await expect(joiner.locator("#recent")).not.toContainText(hostName!);

        // Playing together ends (nr-ayk5.5): Disconnect asks first, both stop on the last picture,
        // and the joiner's Back to library lets the received game go.
        await openPlayTogether(host);
        await panel(host).getByRole("button", { name: "Disconnect" }).click();
        await confirmEnd(host);
        await expect(endedScreen(joiner)).toContainText("The other player ended it.", { timeout: CONNECT_TIMEOUT_MS });
        await expect(joiner.locator("#netplay-open")).toHaveText("Disconnected");
        await endedScreen(joiner).getByRole("button", { name: "Back to library" }).click();
        await waitForIdleState(joiner);
        await expect(joiner.locator("#library")).toBeVisible();
        await expect(endedScreen(host)).toContainText("You ended it.");
        await endedScreen(host).getByRole("button", { name: "Resume" }).click();
        await waitForRunningState(host);
    });

    test("Given two players playing together, then the controls that would make the games differ are greyed with their reason, and the Keys and controls panel says which player each is", async ({
        page: host,
        browser
    }) => {
        // Two browsers connect, count down and play through several shared steps.
        test.slow();
        const joiner = await startPlaying(host, browser);
        for (const page of [host, joiner]) {
            for (const id of ["#save-state", "#load-state", "#stop", "#library-open"]) {
                await expect(page.locator(id)).toHaveAttribute("aria-disabled", "true");
                await expect(page.locator(id)).toHaveAttribute("data-tip", "Not available while playing together");
            }
        }
        await host.locator("#screen").click();
        await host.keyboard.press("F6");
        await expect(host.locator(".neser-toast")).toHaveText("Not available while playing together");
        await joiner.locator("#stop").click({ force: true });
        await waitForPlaying(joiner);
        await joiner.keyboard.press("h");
        await expect(joiner.locator("#help-netplay")).toHaveText("Playing together — your keys and gamepads control player 2.");
        await expect(host.locator(".netplay-screen")).toContainText("The other player is in Keys and controls.", { timeout: CONNECT_TIMEOUT_MS });
        await joiner.keyboard.press("Escape");
        for (const page of [host, joiner]) {
            await expect(page.locator(".netplay-countdown")).toContainText("Resuming", { timeout: CONNECT_TIMEOUT_MS });
            await waitForPlaying(page);
        }
    });

    test("Given two players playing together, when either pauses, resets, opens Settings or ends it, then both games do so and each is told who did", async ({
        page: host,
        browser
    }) => {
        // Two browsers connect, count down and play through several shared steps.
        test.slow();
        const joiner = await startPlaying(host, browser);

        await joiner.locator("#pause").click();
        await expect(host.locator(".netplay-screen")).toContainText("The other player paused the game.", { timeout: CONNECT_TIMEOUT_MS });
        await expect(joiner.locator(".netplay-screen")).toContainText("You paused the game.");
        await expect(joiner.locator(".netplay-screen").getByRole("button", { name: "Resume" })).toBeFocused();
        for (const page of [host, joiner]) {
            await expect(page.locator("#emulation-controls")).toHaveAttribute("data-emulation-state", "paused");
            await expect(page.locator("#pause")).toHaveText("Resume");
        }
        await host.locator(".netplay-screen").getByRole("button", { name: "Resume" }).click();
        for (const page of [host, joiner]) {
            await expect(page.locator(".netplay-countdown")).toContainText("Resuming", { timeout: CONNECT_TIMEOUT_MS });
            await waitForPlaying(page);
        }

        await host.locator("#settings-open").click();
        await expect(joiner.locator(".netplay-screen")).toContainText("The other player is in Settings.", { timeout: CONNECT_TIMEOUT_MS });
        await host.keyboard.press("Escape");
        for (const page of [host, joiner]) {
            await expect(page.locator(".netplay-countdown")).toContainText("Resuming", { timeout: CONNECT_TIMEOUT_MS });
            await waitForPlaying(page);
        }

        await host.locator("#reset").click();
        await expect(host.locator(".netplay-countdown")).toContainText("You reset the game", { timeout: CONNECT_TIMEOUT_MS });
        await expect(joiner.locator(".netplay-countdown")).toContainText("The other player reset the game", { timeout: CONNECT_TIMEOUT_MS });
        for (const page of [host, joiner]) await waitForPlaying(page);

        await host.locator("#pause").click();
        await expect(joiner.locator(".netplay-screen")).toContainText("The other player paused the game.", { timeout: CONNECT_TIMEOUT_MS });
        await joiner.locator(".netplay-screen").getByRole("button", { name: "End playing together" }).click();
        const question = joiner.locator(".netplay-confirm");
        await expect(question).toContainText("End playing together?");
        await expect(question.getByRole("button", { name: "Keep playing" })).toBeFocused();
        await joiner.keyboard.press("Escape");
        await expect(question).toBeHidden();
        await expect(joiner.locator(".netplay-screen")).toContainText("The other player paused the game.");
        await joiner.locator(".netplay-screen").getByRole("button", { name: "End playing together" }).click();
        await confirmEnd(joiner);
        await expect(endedScreen(joiner)).toContainText("You ended it. The host's game isn't kept on this computer.");
        await expect(endedScreen(host)).toContainText("The other player ended it. You can keep playing on your own from here.", {
            timeout: CONNECT_TIMEOUT_MS
        });
        await expect(host.locator("#save-state")).not.toHaveAttribute("aria-disabled", "true");
        await host.keyboard.press("Space");
        await waitForRunningState(host);
        await expect(host.locator(".netplay-screen")).toHaveCount(0);
    });

    test("Given two players playing together, when one closes their page, then the other's game stops and says they left", async ({
        page: host,
        browser
    }) => {
        // Two browsers connect, count down and play through several shared steps.
        test.slow();
        const joiner = await startPlaying(host, browser);
        await joiner.close();
        await expect(endedScreen(host)).toContainText("The other player left.", { timeout: CONNECT_TIMEOUT_MS });
        await expect(host.locator("#netplay-open")).toHaveText("Disconnected");
    });

    test("Given a Game Boy Advance game, when Play together opens, then hosting is greyed out with its reason", async ({ page }) => {
        await openApp(page);
        await loadGbaRomFromFileInput(page);
        await waitForPlaying(page);

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
