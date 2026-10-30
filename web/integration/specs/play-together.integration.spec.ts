import { test, expect, type Browser, type BrowserContext, type Page, type WebSocketRoute } from "@playwright/test";
import config from "../../../playwright.config";
import {
    loadGbaRomFromFileInput,
    loadRomFromFileInput,
    openApp,
    waitForIdleState,
    waitForPausedState,
    waitForRunningState
} from "../helpers/lifecycle.helpers";

// Two browser contexts on one runner connect over WebRTC by their own addresses, as two browsers on
// one network do (the STUN address from outpost, nr-ri8n, is offered too, but not needed here). Chromium hides its local addresses behind multicast-DNS names by default; the
// runner may not resolve those, so this spec has the browser offer its addresses as they are.
// What is under test is the page's connecting, not the operating system's mDNS.
test.use({
    launchOptions: {
        ...config.use?.launchOptions,
        args: [...(config.use?.launchOptions?.args ?? []), "--disable-features=WebRtcHideLocalIpsWithMdns"]
    }
});

const CONNECT_TIMEOUT_MS = 20_000;

/**
 * Liaison, the meeting service (nr-z4nw), played by the test so CI never reaches the real one: it
 * opens rooms named K7Q-M4X, T9W-2HB, …, puts a joiner in by its code, passes signals between the
 * two, and tells the one left when the other's connection closes, as liaison's README describes.
 */
type Room = { code: string; host: WebSocketRoute; joiner: WebSocketRoute | null };
const ROOM_CODES = ["K7Q-M4X", "T9W-2HB", "P3R-8ZD", "H6N-5LQ"];

class FakeLiaison {
    private rooms = new Map<string, Room>();
    private opened = 0;

    async serve(page: Page) {
        await page.routeWebSocket(/liaison\.rmstdope\.eu/, (ws) => this.accept(ws));
    }

    private accept(ws: WebSocketRoute) {
        let room: Room | null = null;
        const tell = (to: WebSocketRoute, message: object) => to.send(JSON.stringify(message));
        ws.onMessage((text) => {
            const message = JSON.parse(String(text));
            if (message.type === "create") {
                room = { code: ROOM_CODES[this.opened++ % ROOM_CODES.length], host: ws, joiner: null };
                this.rooms.set(room.code, room);
                tell(ws, { type: "created", room: room.code });
            } else if (message.type === "join") {
                const wanted = this.rooms.get(message.room);
                if (!wanted) return tell(ws, { type: "error", reason: "room-not-found" });
                if (wanted.joiner) return tell(ws, { type: "error", reason: "room-full" });
                wanted.joiner = ws;
                room = wanted;
                tell(ws, { type: "joined" });
                tell(wanted.host, { type: "peer-joined" });
            } else if (message.type === "signal" && room) {
                const other = ws === room.host ? room.joiner : room.host;
                if (other) tell(other, { type: "signal", data: message.data });
            }
        });
        ws.onClose(() => {
            if (!room) return;
            this.rooms.delete(room.code);
            const other = ws === room.host ? room.joiner : room.host;
            room = null;
            if (!other) return;
            tell(other, { type: "peer-left" });
            void other.close();
        });
    }
}

let liaison: FakeLiaison;

test.beforeEach(async ({ page }) => {
    liaison = new FakeLiaison();
    await liaison.serve(page);
});

function panel(page: Page) {
    return page.locator("#netplay-panel");
}

async function openPlayTogether(page: Page) {
    await page.locator("#netplay-open").click();
    await expect(panel(page)).toBeVisible();
}

async function shownCode(page: Page): Promise<string> {
    const box = panel(page).locator(".netplay-room");
    await expect(box).toHaveText(/^[A-Z2-9]{3}-[A-Z2-9]{3}$/, { timeout: CONNECT_TIMEOUT_MS });
    return (await box.textContent())!;
}

/** The joiner's browser, closed after each test so its connection does not outlive it. */
let joinerContext: BrowserContext | null = null;

async function secondBrowser(browser: Browser): Promise<Page> {
    joinerContext = await browser.newContext({ baseURL: test.info().project.use.baseURL });
    const page = await joinerContext.newPage();
    await liaison.serve(page);
    return page;
}

test.afterEach(async () => {
    await joinerContext?.close();
    joinerContext = null;
});

/** The host shows its room code and the joiner types it, until both windows say they are connected. */
async function connect(host: Page, joiner: Page) {
    await openPlayTogether(host);
    await panel(host).getByRole("button", { name: /Host this game/ }).click();
    const code = await shownCode(host);

    await openPlayTogether(joiner);
    await panel(joiner).getByRole("button", { name: /Join a game/ }).click();
    await panel(joiner).getByPlaceholder("ABC-DEF").fill(code);
    await panel(joiner).getByRole("button", { name: "Join", exact: true }).click();
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

/** Both games show their count at the same time, saying what each is told, and then both play on. */
async function bothCount(host: Page, joiner: Page, hostWords: string, joinerWords: string) {
    await Promise.all([
        expect(host.locator(".netplay-countdown")).toContainText(hostWords, { timeout: CONNECT_TIMEOUT_MS }),
        expect(joiner.locator(".netplay-countdown")).toContainText(joinerWords, { timeout: CONNECT_TIMEOUT_MS }),
    ]);
    for (const page of [host, joiner]) await waitForPlaying(page);
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

/**
 * Resyncing (nr-ayk5.6): two games in lockstep never drift on their own, so the host's page has
 * what it sends tampered with before it leaves. `window.__tamper.corruptHash` changes the state
 * hash in the next lockstep packet that carries a new one (bytes led by transfer id 0: version, ack,
 * first frame, count, count pads, hash flag, frame, hash), so the joiner finds the games differ
 * and the real resync follows. The pieces of the host's state (transfer id 0xFFFFFFFF) can be
 * held back a while, so the resync screen stays long enough to be looked at, or dropped.
 */
async function tamperWithHostLink(host: Page) {
    await host.addInitScript(() => {
        const tamper = { corruptHash: false, stateDelayMs: 0, dropState: false };
        (window as unknown as { __tamper: typeof tamper }).__tamper = tamper;
        const send = RTCDataChannel.prototype.send;
        let lastHashFrame = -1;
        RTCDataChannel.prototype.send = function (this: RTCDataChannel, data: string | ArrayBufferView<ArrayBuffer> | ArrayBuffer | Blob) {
            if (ArrayBuffer.isView(data)) {
                const bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
                const id = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, true);
                const flagAt = 4 + 10 + 2 * bytes[4 + 9];
                if (id === 0 && bytes[flagAt] === 1) {
                    // Every packet repeats the newest hash and the joiner keeps the first it hears,
                    // so only the first packet with a new one is changed.
                    const frame = new DataView(bytes.buffer, bytes.byteOffset + flagAt + 1, 4).getUint32(0, true);
                    const fresh = frame !== lastHashFrame;
                    lastHashFrame = frame;
                    if (tamper.corruptHash && fresh) {
                        tamper.corruptHash = false;
                        const changed = bytes.slice();
                        changed[flagAt + 5] ^= 0xff;
                        return send.call(this, changed);
                    }
                }
                if (id === 0xffffffff) {
                    if (tamper.dropState) return;
                    if (tamper.stateDelayMs > 0) {
                        const later = bytes.slice();
                        setTimeout(() => send.call(this, later), tamper.stateDelayMs);
                        return;
                    }
                }
            }
            return (send as (this: RTCDataChannel, d: unknown) => void).call(this, data);
        };
    });
}

async function tamper(host: Page, change: { corruptHash?: boolean; stateDelayMs?: number; dropState?: boolean }) {
    await host.evaluate((c) => Object.assign((window as unknown as { __tamper: object }).__tamper, c), change);
}

function resyncScreen(page: Page) {
    return page.locator(".netplay-screen", { hasText: "⟳ Resyncing the games…" });
}

test.describe("Play together", () => {
    test("Given a host running a NES game and a joiner in the library, when the joiner types the host's room code, then both are connected with a Great response time, and the host's chip shows when the joiner leaves", async ({
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
        const code = await shownCode(host);
        await expect(panel(host)).toContainText("⟳ Waiting for the other player to type it…");
        await expect(panel(host).getByRole("button", { name: "Copy code" })).toBeFocused();

        await openPlayTogether(joiner);
        await panel(joiner).getByRole("button", { name: /Join a game/ }).click();
        const field = panel(joiner).getByPlaceholder("ABC-DEF");
        await expect(field).toBeFocused();
        await field.pressSequentially(code.replace("-", "").toLowerCase());
        await expect(field).toHaveValue(code);
        await field.press("Enter");
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
        await bothCount(host, joiner, "Resuming", "Resuming");
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
        await bothCount(host, joiner, "Resuming", "Resuming");

        await host.locator("#settings-open").click();
        await expect(joiner.locator(".netplay-screen")).toContainText("The other player is in Settings.", { timeout: CONNECT_TIMEOUT_MS });
        await host.keyboard.press("Escape");
        await bothCount(host, joiner, "Resuming", "Resuming");

        await host.locator("#reset").click();
        await bothCount(host, joiner, "You reset the game", "The other player reset the game");

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
        await expect(endedScreen(joiner).locator("p")).toHaveText(["You ended it.", "The host's game isn't kept on this computer."]);
        await expect(endedScreen(host).locator("p")).toHaveText(["The other player ended it.", "You can keep playing on your own from here."], {
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

    test("Given two players playing together, when their games stop matching, then both stop under the resync screen, count Resynced and play on", async ({
        page: host,
        browser
    }) => {
        // Two browsers connect, count down and play through several shared steps.
        test.slow();
        await tamperWithHostLink(host);
        const joiner = await startPlaying(host, browser);
        // Two real games, compared once a second, play on unseen while they match.
        await host.waitForTimeout(4000);
        for (const page of [host, joiner]) await expect(resyncScreen(page)).toHaveCount(0);
        // The joiner's window is phone-narrow: its chip keeps the dot and the response time.
        await joiner.setViewportSize({ width: 700, height: 800 });
        await tamper(host, { stateDelayMs: 3000, corruptHash: true });
        for (const page of [host, joiner]) {
            await expect(resyncScreen(page)).toContainText("Your two games stopped matching, so the other player's game is being set to the host's.", {
                timeout: CONNECT_TIMEOUT_MS
            });
            await expect(resyncScreen(page).getByRole("button", { name: "End playing together" })).toBeVisible();
            await expect(page.locator("#netplay-open")).toHaveAttribute("data-tone", "wait");
        }
        await expect(host.locator("#netplay-open .netplay-chip-long")).toHaveText("Resyncing…");
        await expect(host.locator("#netplay-open .netplay-chip-long")).toBeVisible();
        await expect(host.locator("#netplay-open .netplay-chip-short")).toBeHidden();
        await expect(joiner.locator("#netplay-open .netplay-chip-long")).toBeHidden();
        await expect(joiner.locator("#netplay-open .netplay-chip-short")).toBeVisible();
        await expect(joiner.locator("#netplay-open .netplay-chip-short")).toHaveText(/^\d+ ms$/);
        // Pause does nothing while it is resyncing.
        await joiner.keyboard.press("Space");
        await expect(resyncScreen(joiner)).toBeVisible();
        await bothCount(host, joiner, "Resynced", "Resynced");
        for (const page of [host, joiner]) {
            await expect(page.locator(".netplay-screen")).toHaveCount(0);
            await expect(page.locator("#netplay-open")).toHaveText(/^Connected · \d+ ms$/);
        }
    });

    test("Given two players playing together, when the host's game never reaches the joiner during a resync, then after 15 seconds playing together ends on both because the games couldn't be resynced", async ({
        page: host,
        browser
    }) => {
        // Two browsers connect, play, and wait out the resync's 15 seconds.
        test.slow();
        await tamperWithHostLink(host);
        const joiner = await startPlaying(host, browser);
        await tamper(host, { dropState: true, corruptHash: true });
        await expect(resyncScreen(joiner)).toBeVisible({ timeout: CONNECT_TIMEOUT_MS });
        await expect(endedScreen(host).locator("p")).toHaveText(["The games couldn't be resynced.", "You can keep playing on your own from here."], {
            timeout: 25_000
        });
        await expect(endedScreen(joiner).locator("p")).toHaveText(["The games couldn't be resynced.", "The host's game isn't kept on this computer."], {
            timeout: CONNECT_TIMEOUT_MS
        });
        for (const page of [host, joiner]) await expect(page.locator("#netplay-open")).toHaveText("Disconnected");
        await expect(endedScreen(joiner).getByRole("button", { name: "Back to library" })).toBeFocused();
        await endedScreen(host).getByRole("button", { name: "Resume" }).click();
        await waitForRunningState(host);
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

    test("Given a joiner, when the code typed is no game's, then it says so beneath the field and keeps the code to fix", async ({ page }) => {
        await openApp(page);
        await openPlayTogether(page);
        await expect(panel(page).locator("[data-choice=host]")).toContainText("Host a game");
        await panel(page).getByRole("button", { name: /Join a game/ }).click();
        const field = panel(page).getByPlaceholder("ABC-DEF");
        await field.fill("abc-de");
        await expect(panel(page).getByRole("button", { name: "Join", exact: true })).toBeDisabled();
        await field.pressSequentially("f");
        await panel(page).getByRole("button", { name: "Join", exact: true }).click();
        await expect(panel(page)).toContainText("There's no game with that code. Check the code with the host and try again.");
        await expect(field).toHaveValue("ABC-DEF");
        await expect(field).toBeFocused();
        // A code pasted from a chat, with spaces round the dash, is taken whole.
        await field.selectText();
        await page.keyboard.insertText(" k7q - m4x ");
        await expect(field).toHaveValue("K7Q-M4X");
        await expect(panel(page).getByRole("button", { name: "Join", exact: true })).toBeEnabled();
        await page.keyboard.press("Escape");
        await expect(panel(page)).toBeHidden();
        await expect(page.locator("#netplay-open")).toBeFocused();
    });
});
