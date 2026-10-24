/**
 * @vitest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GamePackage } from "../netplay/game_package";
import type { NetplaySession, NetplayView } from "../netplay/session";
import { createPlayTogether, type CurrentGame } from "./play_together";

const IDLE: NetplayView = {
    role: null,
    step: 1,
    ownCode: null,
    pasteProblem: null,
    pasteReady: false,
    phase: "idle",
    failure: null,
    theirs: null,
    rtt: null,
    sending: null,
    notice: null,
};

class FakeSession implements NetplaySession {
    state: NetplayView = { ...IDLE };
    private listener: (view: NetplayView) => void = () => {};
    host = vi.fn(() => this.emit({ role: "host", step: 1, ownCode: "NSR1-hostcode" }));
    join = vi.fn(() => this.emit({ role: "join", step: 1 }));
    next = vi.fn();
    back = vi.fn();
    paste = vi.fn(async () => {});
    connect = vi.fn();
    disconnect = vi.fn(() => this.emit({ ...IDLE }));
    cancel = vi.fn(() => this.emit({ ...IDLE }));
    startAgain = vi.fn();
    link = vi.fn(() => null);
    startGame = vi.fn();
    cancelSending = vi.fn();
    view = () => this.state;
    subscribe(listener: (view: NetplayView) => void) {
        this.listener = listener;
    }
    emit(changes: Partial<NetplayView>) {
        this.state = { ...this.state, ...changes };
        this.listener(this.state);
    }
}

const BUILD = { version: "1.3.0", commit: "4f2a9c1" };
let session: FakeSession;
let game: CurrentGame | null;
const PACKAGE: GamePackage = { kind: "nes", name: "smb3.nes", rom: new Uint8Array([1]), firmware: null, state: new Uint8Array([2]) };
let onOpen: ReturnType<typeof vi.fn<() => void>>;
let onClose: ReturnType<typeof vi.fn<() => void>>;
let writeText: ReturnType<typeof vi.fn>;

function q(selector: string) {
    return document.querySelector<HTMLElement>(selector);
}
function body() {
    return q("#netplay-body")!;
}
function text() {
    return body().textContent!.replace(/\s+/g, " ");
}
function button(name: string) {
    return [...body().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent!.trim() === name);
}
function buttonNames() {
    return [...body().querySelectorAll<HTMLButtonElement>(".netplay-actions button")].map((b) => b.textContent!.trim());
}
function chip() {
    return q("#netplay-open")!;
}
function title() {
    return q("#netplay-title")!.textContent;
}
function open() {
    chip().click();
}

beforeEach(() => {
    document.body.innerHTML = `
        <button id="netplay-open" data-icon="people">Play together</button>
        <dialog id="netplay-panel"><h2 id="netplay-title">Play together</h2>
          <button id="netplay-close">Close</button><div id="netplay-body"></div></dialog>`;
    session = new FakeSession();
    game = { kind: "nes", title: "Super Mario Bros. 3" };
    onOpen = vi.fn();
    onClose = vi.fn();
    writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    createPlayTogether({
        dialog: q("#netplay-panel") as HTMLDialogElement,
        title: q("#netplay-title")!,
        body: body(),
        closeButton: q("#netplay-close") as HTMLButtonElement,
        openButton: chip() as HTMLButtonElement,
        session,
        build: BUILD,
        currentGame: () => game,
        hostGame: () => (game ? { game: PACKAGE, title: game.title } : null),
        onOpen,
        onClose,
    });
});

afterEach(() => {
    vi.useRealTimers();
});

describe("the choice", () => {
    it("offers hosting the running NES game and joining", () => {
        open();
        expect(title()).toBe("Play together");
        expect(text()).toContain("Two browsers on the same network, one game, two pads.");
        expect(text()).toContain("Host this game");
        expect(text()).toContain("Share Super Mario Bros. 3 with another player. You are player 1.");
        expect(text()).toContain("Join a game");
        expect(text()).toContain("Connect to a game someone else is hosting. You are player 2 — you don't need the game file.");
        expect(document.activeElement?.textContent).toContain("Host this game");
        expect(onOpen).toHaveBeenCalledOnce();
    });

    it("greys hosting out with no game, and puts focus on joining", () => {
        game = null;
        open();
        const host = body().querySelector<HTMLButtonElement>("[data-choice=host]")!;
        expect(host.disabled).toBe(true);
        expect(host.textContent).toContain("Host a game");
        expect(host.textContent).toContain("Load a NES or Super Nintendo game first, then come back here.");
        expect(document.activeElement?.textContent).toContain("Join a game");
    });

    it.each(["gb", "gba"] as const)("greys hosting out for a %s game", (kind) => {
        game = { kind, title: "Tetris" };
        open();
        const host = body().querySelector<HTMLButtonElement>("[data-choice=host]")!;
        expect(host.disabled).toBe(true);
        expect(host.textContent).toContain("Network play works with NES and Super Nintendo games.");
    });

    it("lets a SNES game be hosted", () => {
        game = { kind: "snes", title: "Super Mario World" };
        open();
        expect(body().querySelector<HTMLButtonElement>("[data-choice=host]")!.disabled).toBe(false);
    });
});

describe("hosting", () => {
    function hostStep1() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
    }

    it("shows step 1 with the code to send", () => {
        hostStep1();
        expect(session.host).toHaveBeenCalled();
        expect(title()).toBe("Host this game");
        const steps = [...body().querySelectorAll(".netplay-steps span")].map((s) => [s.textContent, s.className]);
        expect(steps).toEqual([
            ["1. Send your code", "on"],
            ["2. Paste their code", ""],
            ["3. Connected", ""],
        ]);
        expect(text()).toContain("Send this code to the other player — by chat, email, anything.");
        expect(body().querySelector(".netplay-code")!.textContent).toBe("NSR1-hostcode");
        expect(buttonNames()).toEqual(["Cancel", "Copy code", "Next"]);
    });

    it("copies the code, says so, and moves on only with Next", async () => {
        vi.useFakeTimers();
        hostStep1();
        button("Copy code")!.click();
        await vi.advanceTimersByTimeAsync(0);
        expect(writeText).toHaveBeenCalledWith("NSR1-hostcode");
        expect(buttonNames()).toContain("✓ Copied");
        expect(session.next).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(1600);
        expect(buttonNames()).toContain("Copy code");
        button("Next")!.click();
        expect(session.next).toHaveBeenCalled();
    });

    it("waits for its code before it can be copied", () => {
        hostStep1();
        session.emit({ ownCode: null });
        expect(button("Copy code")!.disabled).toBe(true);
        expect(button("Next")!.disabled).toBe(true);
    });

    it("asks for the reply at step 2, with Connect waiting for a valid one", () => {
        hostStep1();
        session.emit({ step: 2 });
        expect(text()).toContain("Paste the code the other player sends back.");
        const box = body().querySelector("textarea")!;
        expect(box.placeholder).toBe("Paste their code here");
        expect(document.activeElement).toBe(box);
        expect(buttonNames()).toEqual(["Back", "Connect"]);
        expect(button("Connect")!.disabled).toBe(true);
        box.value = "NSR1-reply";
        box.dispatchEvent(new Event("input"));
        expect(session.paste).toHaveBeenCalledWith("NSR1-reply");
        session.emit({ pasteReady: true });
        expect(button("Connect")!.disabled).toBe(false);
        expect(document.activeElement).toBe(box);
        expect(box.value).toBe("NSR1-reply");
        button("Connect")!.click();
        expect(session.connect).toHaveBeenCalled();
    });

    it("says what is wrong with a pasted reply, in the host's words", () => {
        hostStep1();
        session.emit({ step: 2, pasteProblem: "not-neser" });
        expect(text()).toContain("That isn't a NESER code. Copy the whole code from the other player's window and paste it again.");
        session.emit({ pasteProblem: "wrong-kind" });
        expect(text()).toContain("This is a host's code, not the other player's reply. Ask the other player for the code from their window.");
    });

    it("names both builds when the other player's NESER differs", () => {
        hostStep1();
        session.emit({ step: 2, phase: "failed", failure: "other-build", theirs: { version: "1.3.0", commit: "b81e07d" } });
        expect(text()).toContain(
            "Different versions of NESER. You have NESER 1.3.0 (4f2a9c1), the other player has 1.3.0 (b81e07d). You both need the same version to play together — reload the page on both computers."
        );
        expect(buttonNames()).toEqual(["Close"]);
    });

    it("shows Connecting with Cancel", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connecting" });
        expect(text()).toContain("⟳ Connecting…");
        expect(buttonNames()).toEqual(["Cancel"]);
    });

    it("says when the other player could not be reached, offering to start again", () => {
        hostStep1();
        session.emit({ step: 2, phase: "failed", failure: "unreachable" });
        expect(text()).toContain(
            "Couldn't reach the other player. Network play needs both computers on the same network (or the same VPN). Check that, then start again."
        );
        expect(buttonNames()).toEqual(["Close", "Start again"]);
        button("Start again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
    });

    it("is connected at step 3 with a Great response time", () => {
        hostStep1();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
        expect(text()).toContain("Connected to the other player.");
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 18 ms · Great");
        expect(body().querySelector(".netplay-pill")!.getAttribute("data-tone")).toBe("ok");
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        expect(text()).not.toContain("Waiting for the host");
    });

    it("warns when the response time is only Playable", () => {
        hostStep1();
        session.emit({ step: 3, phase: "connected", rtt: 84 });
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 84 ms · Playable");
        expect(text()).toContain("Your button presses will take a moment longer to show up. Fine for most games, harder for fast ones.");
    });

    it("turns red, still connected, when it becomes too slow, and back when it recovers", () => {
        hostStep1();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
        session.emit({ rtt: 190 });
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 190 ms · Too slow");
        expect(text()).toContain(
            "The connection has become too slow to play. It may recover by itself — if not, try both being on the same network, or closer to the router."
        );
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        session.emit({ rtt: 20 });
        expect(text()).not.toContain("too slow to play");
    });

    it("refuses a connection too slow at connect time", () => {
        hostStep1();
        session.emit({ step: 2, phase: "failed", failure: "too-slow", rtt: 210 });
        expect(text()).toContain("Too slow to play");
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 210 ms · Too slow");
        expect(text()).toContain(
            "The connection between the two browsers is too slow for the game to keep up. Try both being on the same network, or closer to the router."
        );
        expect(buttonNames()).toEqual(["Close", "Try again"]);
        button("Try again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
    });

    it("says when the other player leaves", () => {
        hostStep1();
        session.emit({ step: 3, phase: "failed", failure: "peer-left" });
        expect(text()).toContain("The other player disconnected.");
        expect(buttonNames()).toEqual(["Close", "Start again"]);
    });

    it("disconnects from step 3", () => {
        hostStep1();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
        button("Disconnect")!.click();
        expect(session.disconnect).toHaveBeenCalled();
        expect(title()).toBe("Play together");
    });
});

describe("joining", () => {
    function joinStep1() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
    }

    it("asks for the host's code at step 1", () => {
        joinStep1();
        expect(title()).toBe("Join a game");
        expect([...body().querySelectorAll(".netplay-steps span")].map((s) => s.textContent)).toEqual([
            "1. Paste the host's code",
            "2. Send your code",
            "3. Connected",
        ]);
        expect(text()).toContain("Paste the code the host sent you.");
        const box = body().querySelector("textarea")!;
        expect(box.placeholder).toBe("Paste the host's code here");
        expect(document.activeElement).toBe(box);
        expect(buttonNames()).toEqual(["Cancel", "Next"]);
        expect(button("Next")!.disabled).toBe(true);
    });

    it("says what is wrong with a pasted code, in the joiner's words", () => {
        joinStep1();
        session.emit({ pasteProblem: "not-neser" });
        expect(text()).toContain("That isn't a NESER code. Copy the whole code from the host's window and paste it again.");
        session.emit({ pasteProblem: "wrong-kind" });
        expect(text()).toContain("This is a reply code, not a host's code. Ask the host for the code from their window.");
    });

    it("names both builds when the host's NESER differs", () => {
        joinStep1();
        session.emit({ phase: "failed", failure: "other-build", theirs: { version: "1.3.0", commit: "b81e07d" } });
        expect(text()).toContain("You have NESER 1.3.0 (4f2a9c1), the host has 1.3.0 (b81e07d).");
        expect(buttonNames()).toEqual(["Close"]);
    });

    it("sends its code back at step 2 while waiting for the host", () => {
        joinStep1();
        session.emit({ step: 2, phase: "waiting", ownCode: "NSR1-reply" });
        expect(text()).toContain("Send this code back to the host.");
        expect(body().querySelector(".netplay-code")!.textContent).toBe("NSR1-reply");
        expect(text()).toContain("⟳ Waiting for the host to paste your code…");
        expect(buttonNames()).toEqual(["Cancel", "Copy code"]);
    });

    it("is connected to the host, waiting for the host to start the game", () => {
        joinStep1();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
        expect(text()).toContain("Connected to the host.");
        expect(text()).toContain("⟳ Waiting for the host to start the game…");
        expect(text()).not.toContain("The host will send the game next.");
        expect(buttonNames()).toEqual(["Disconnect"]);
    });
});

describe("closing the window", () => {
    it("before connecting cancels the attempt and resumes", () => {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        q("#netplay-close")!.click();
        expect(session.cancel).toHaveBeenCalled();
        expect(onClose).toHaveBeenCalledOnce();
        expect(chip().textContent).toBe("Play together");
    });

    it("from Cancel cancels the attempt and closes", () => {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        button("Cancel")!.click();
        expect(session.cancel).toHaveBeenCalled();
        expect(onClose).toHaveBeenCalledOnce();
    });

    it("once connected keeps the connection, and the top-bar button shows it", () => {
        open();
        session.emit({ role: "host", step: 3, phase: "connected", rtt: 18 });
        q("#netplay-close")!.click();
        expect(session.cancel).not.toHaveBeenCalled();
        expect(chip().querySelector(".netplay-chip-long")!.textContent).toBe("Connected · ");
        expect(chip().textContent).toBe("Connected · 18 ms");
        expect(chip().dataset.tone).toBe("ok");
        session.emit({ rtt: 84 });
        expect(chip().dataset.tone).toBe("warn");
        session.emit({ rtt: 190 });
        expect(chip().dataset.tone).toBe("err");
    });

    it("shows Disconnected on the button when the other player leaves, until it is pressed", () => {
        open();
        session.emit({ role: "host", step: 3, phase: "connected", rtt: 18 });
        q("#netplay-close")!.click();
        session.emit({ phase: "failed", failure: "peer-left" });
        expect(chip().textContent).toBe("Disconnected");
        expect(chip().dataset.tone).toBe("err");
        open();
        expect(text()).toContain("The other player disconnected.");
        q("#netplay-close")!.click();
        expect(session.cancel).toHaveBeenCalled();
        expect(chip().textContent).toBe("Play together");
    });

    it("gives focus back to the top-bar button", () => {
        open();
        q("#netplay-close")!.click();
        expect(document.activeElement).toBe(chip());
    });
});

describe("the pasted code", () => {
    it("stays in view while connecting", () => {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        session.emit({ step: 2 });
        const box = body().querySelector("textarea")!;
        box.value = "NSR1-reply";
        box.dispatchEvent(new Event("input"));
        session.emit({ phase: "connecting" });
        expect(body().querySelector("textarea")!.value).toBe("NSR1-reply");
    });
});

describe("a failed step", () => {
    it.each([
        ["host", { step: 2 as const, phase: "failed" as const, failure: "unreachable" as const }],
        ["join", { step: 1 as const, phase: "failed" as const, failure: "other-build" as const, theirs: BUILD }],
    ])("keeps the %s's pasted code but no longer takes another", (choice, failed) => {
        open();
        body().querySelector<HTMLButtonElement>(`[data-choice=${choice}]`)!.click();
        session.emit(failed);
        expect(body().querySelector("textarea")!.readOnly).toBe(true);
    });
});

describe("copying without the clipboard API", () => {
    it("says Copied only when the browser copied", async () => {
        Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
        const execCommand = vi.fn(() => false);
        Object.defineProperty(document, "execCommand", { value: execCommand, configurable: true });
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        button("Copy code")!.click();
        await new Promise((r) => setTimeout(r, 0));
        expect(execCommand).toHaveBeenCalledWith("copy");
        expect(buttonNames()).toContain("Copy code");
        execCommand.mockReturnValue(true);
        button("Copy code")!.click();
        await new Promise((r) => setTimeout(r, 0));
        expect(buttonNames()).toContain("✓ Copied");
    });
});

describe("a host whose connection cannot start", () => {
    it("says it could not reach the other player, offering to start again", () => {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        session.emit({ ownCode: null, phase: "failed", failure: "unreachable" });
        expect(text()).toContain("Couldn't reach the other player.");
        expect(buttonNames()).toEqual(["Close", "Start again"]);
    });
});

describe("sending the game (nr-ayk5.4)", () => {
    function hostConnected() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
    }
    function joinConnected() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        session.emit({ step: 3, phase: "connected", rtt: 18 });
    }
    const sending = (direction: "send" | "receive", done: number, total: number) =>
        session.emit({ sending: { direction, name: "Super Mario Bros. 3", done, total } });
    const alerts = () => [...body().querySelectorAll<HTMLElement>(".netplay-alert")].map((a) => [a.dataset.tone, a.textContent]);

    it("offers the host Start game, with focus, under the hint", () => {
        hostConnected();
        expect(text()).toContain("When you're both ready, send Super Mario Bros. 3 and start.");
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        expect(button("Start game")!.disabled).toBe(false);
        expect(button("Start game")!.className).toContain("btn-primary");
        expect(document.activeElement).toBe(button("Start game"));
        button("Start game")!.click();
        expect(session.startGame).toHaveBeenCalledWith(PACKAGE, "Super Mario Bros. 3");
    });

    it("greys Start game out, with the reason, when the host has no game", () => {
        hostConnected();
        game = null;
        q("#netplay-close")!.click();
        open();
        expect(button("Start game")!.disabled).toBe(true);
        expect(text()).toContain("Load a NES or Super Nintendo game first, then come back here.");
        expect(text()).not.toContain("When you're both ready");
    });

    it("greys Start game out for a game that cannot be played together", () => {
        hostConnected();
        game = { kind: "gb", title: "Tetris" };
        q("#netplay-close")!.click();
        open();
        expect(button("Start game")!.disabled).toBe(true);
        expect(text()).toContain("Network play works with NES and Super Nintendo games.");
    });

    it("shows the host's progress, in KB, with Cancel", () => {
        hostConnected();
        sending("send", 240 * 1024 + 500, 384 * 1024);
        expect(text()).toContain("Sending Super Mario Bros. 3…");
        expect(text()).toContain("240 KB of 384 KB");
        expect(body().querySelector<HTMLElement>(".netplay-progress i")!.style.width).toBe("62%");
        expect(buttonNames()).toEqual(["Cancel"]);
        expect(text()).not.toContain("You'll play with the host's game settings.");
        expect(document.activeElement).toBe(button("Cancel"));
        sending("send", 300 * 1024, 384 * 1024);
        expect(text()).toContain("300 KB of 384 KB");
        button("Cancel")!.click();
        expect(session.cancelSending).toHaveBeenCalled();
    });

    it("shows the joiner's progress, and that the host's settings are used", () => {
        joinConnected();
        sending("receive", 0, 384 * 1024);
        expect(text()).toContain("Receiving Super Mario Bros. 3…");
        expect(text()).toContain("0 KB of 384 KB");
        expect(text()).toContain("You'll play with the host's game settings. Your own settings come back when you stop playing together.");
        expect(text()).not.toContain("Waiting for the host");
        expect(buttonNames()).toEqual(["Cancel"]);
        button("Cancel")!.click();
        expect(session.cancelSending).toHaveBeenCalled();
    });

    it("keeps sending when the window is closed, and the chip shows how far it is", () => {
        hostConnected();
        sending("send", 62 * 1024, 100 * 1024);
        q("#netplay-close")!.click();
        expect(session.cancel).not.toHaveBeenCalled();
        expect(session.cancelSending).not.toHaveBeenCalled();
        expect(chip().textContent).toBe("Sending · 62%");
        expect(chip().querySelector(".netplay-chip-long")!.textContent).toBe("Sending · ");
        sending("send", 99 * 1024 + 1000, 100 * 1024);
        expect(chip().textContent).toBe("Sending · 99%");
        open();
        expect(text()).toContain("Sending Super Mario Bros. 3…");
    });

    it("shows Receiving on the joiner's chip", () => {
        joinConnected();
        sending("receive", 31, 50);
        q("#netplay-close")!.click();
        expect(chip().textContent).toBe("Receiving · 62%");
    });

    it("goes back to the connected chip once the game is under way", () => {
        hostConnected();
        sending("send", 10, 50);
        session.emit({ sending: null });
        expect(chip().textContent).toBe("Connected · 18 ms");
    });

    it("tells the joiner the host stopped, above the waiting line", () => {
        joinConnected();
        sending("receive", 10, 50);
        session.emit({ sending: null, notice: "host-stopped" });
        expect(alerts()).toEqual([["warn", "The host stopped sending the game."]]);
        expect(text()).toContain("⟳ Waiting for the host to start the game…");
    });

    it("tells the host the other player stopped it, with Start game again", () => {
        hostConnected();
        sending("send", 10, 50);
        session.emit({ sending: null, notice: "joiner-stopped" });
        expect(alerts()).toEqual([["warn", "The other player stopped the game from being sent."]]);
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        expect(text()).not.toContain("When you're both ready");
    });

    it("says the game didn't arrive, to each side in its words", () => {
        hostConnected();
        session.emit({ notice: "not-arrived" });
        expect(alerts()).toEqual([
            ["err", "The game didn't arrive. Something went wrong while sending it. You're still connected — try again."],
        ]);
        expect(body().querySelector(".netplay-alert b")!.textContent).toBe("The game didn't arrive.");
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        session.emit({ role: "join" });
        expect(alerts()).toEqual([
            ["err", "The game didn't arrive. Something went wrong while sending it. You're still connected — the host can try again."],
        ]);
        expect(text()).toContain("⟳ Waiting for the host to start the game…");
    });

    it("warns a joiner with a game of their own that it will be closed", () => {
        game = { kind: "gb", title: "The Legend of Zelda" };
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        expect(alerts()).toEqual([["warn", "The Legend of Zelda will be closed when the host's game arrives. Your saves are kept."]]);
    });

    it("does not warn a joiner with no game loaded", () => {
        game = null;
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        expect(alerts()).toEqual([]);
    });
});
