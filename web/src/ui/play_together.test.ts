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
    room: null,
    roomNote: null,
    joinProblem: null,
    phase: "idle",
    failure: null,
    theirs: null,
    rtt: null,
    sending: null,
    notice: null,
    leaving: null,
    relayTried: false,
    relayed: false,
};

class FakeSession implements NetplaySession {
    state: NetplayView = { ...IDLE };
    private listener: (view: NetplayView) => void = () => {};
    host = vi.fn(() => this.emit({ role: "host", step: 1, phase: "waiting", room: "K7Q-M4X" }));
    join = vi.fn(() => this.emit({ role: "join", step: 1 }));
    joinRoom = vi.fn();
    disconnect = vi.fn(() => this.emit({ ...IDLE }));
    cancel = vi.fn(() => this.emit({ ...IDLE }));
    startAgain = vi.fn();
    link = vi.fn(() => null);
    startGame = vi.fn();
    cancelSending = vi.fn(() => true);
    end = vi.fn(() => this.emit({ ...IDLE }));
    sendPacket = vi.fn();
    sendPlay = vi.fn();
    sendState = vi.fn();
    endUnsynced = vi.fn();
    onPlay = vi.fn();
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
const NO_RELAY =
    "Couldn't connect your two computers. Your networks don't allow a direct connection, and NESER's relay can't be reached right now. Try again in a while, or with both of you on a home network.";
const RELAY_TRIED =
    "Couldn't connect your two computers. Your networks blocked every way of connecting them, even through NESER's relay. This can happen on strict work or school networks — try again from another network.";
let session: FakeSession;
let game: CurrentGame | null;
const PACKAGE: GamePackage = { kind: "nes", name: "smb3.nes", rom: new Uint8Array([1]), firmware: null, state: new Uint8Array([2]) };
let onOpen: ReturnType<typeof vi.fn<() => void>>;
let onClose: ReturnType<typeof vi.fn<() => void>>;
let writeText: ReturnType<typeof vi.fn>;
let playing: boolean;
let waiting: boolean;
let resyncing: boolean;
let askedToEnd: ReturnType<typeof vi.fn<() => void>>;
let window_: ReturnType<typeof createPlayTogether>;

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
function alerts() {
    return [...body().querySelectorAll<HTMLElement>(".netplay-alert")].map((a) => [a.dataset.tone, a.textContent]);
}
function hostStep1() {
    open();
    body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
}
function joinStep1() {
    game = null;
    open();
    body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
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
    playing = false;
    waiting = false;
    resyncing = false;
    askedToEnd = vi.fn();
    window_ = createPlayTogether({
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
        playing: () => playing,
        waiting: () => waiting,
        resyncing: () => resyncing,
        // As the page does: while playing, Disconnect asks first.
        onDisconnect: () => (playing ? askedToEnd() : session.disconnect()),
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

    const ADDRESS_LINE =
        "ⓘ Playing together connects your computer to the other player's — directly, or through NESER's relay when it has to — so each of you can see the other's internet address.";

    it.each([
        ["a NES game", { kind: "nes", title: "Super Mario Bros. 3" }],
        ["no game", null],
        ["a Game Boy game", { kind: "gb", title: "Tetris" }],
    ] as const)("says beneath the cards that each player sees the other's internet address, with %s (nr-ri8n)", (_, current) => {
        game = current;
        open();
        const lines = body().querySelectorAll<HTMLElement>(".netplay-address");
        expect(lines).toHaveLength(1);
        expect(lines[0].textContent).toBe(ADDRESS_LINE);
        expect(lines[0].tagName).toBe("P");
        expect(lines[0].hasAttribute("tabindex")).toBe(false);
        const join = body().querySelector("[data-choice=join]")!;
        expect(join.compareDocumentPosition(lines[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it("shows the address line only in the first window", () => {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        expect(body().querySelector(".netplay-address")).toBeNull();
    });

    it("lets a SNES game be hosted", () => {
        game = { kind: "snes", title: "Super Mario World" };
        open();
        expect(body().querySelector<HTMLButtonElement>("[data-choice=host]")!.disabled).toBe(false);
    });
});

describe("hosting", () => {
    function room() {
        return body().querySelector(".netplay-room")?.textContent;
    }
    const INVITE =
        "Play Super Mario Bros. 3 with me in NESER! Open https://neser.rmstdope.eu, press Play together, then Join a game, and type the code K7Q-M4X";

    it("shows the room code straight away, with Copy code in focus", () => {
        hostStep1();
        expect(session.host).toHaveBeenCalled();
        expect(title()).toBe("Host this game");
        const steps = [...body().querySelectorAll(".netplay-steps span")].map((s) => [s.textContent, s.className]);
        expect(steps).toEqual([
            ["1. Share the code", "on"],
            ["2. Connected", ""],
        ]);
        expect(text()).toContain("Tell the other player this code:");
        expect(room()).toBe("K7Q-M4X");
        expect(text()).toContain("⟳ Waiting for the other player to type it…");
        expect(buttonNames()).toEqual(["Cancel", "Copy invite message", "Copy code"]);
        expect(document.activeElement).toBe(button("Copy code"));
    });

    it("cannot copy while the code is on its way, and gives Copy code focus once it is there", () => {
        open();
        session.host.mockImplementationOnce(() => session.emit({ role: "host", step: 1, phase: "opening", room: null }));
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        expect(room()).toBe("…");
        expect(button("Copy code")!.disabled).toBe(true);
        expect(button("Copy invite message")!.disabled).toBe(true);
        session.emit({ phase: "waiting", room: "K7Q-M4X" });
        expect(room()).toBe("K7Q-M4X");
        expect(button("Copy code")!.disabled).toBe(false);
        expect(document.activeElement).toBe(button("Copy code"));
    });

    it("copies only the code, and says Copied for a moment", async () => {
        vi.useFakeTimers();
        hostStep1();
        button("Copy code")!.click();
        await vi.advanceTimersByTimeAsync(0);
        expect(writeText).toHaveBeenCalledWith("K7Q-M4X");
        expect(buttonNames()).toEqual(["Cancel", "Copy invite message", "✓ Copied"]);
        await vi.advanceTimersByTimeAsync(1600);
        expect(buttonNames()).toEqual(["Cancel", "Copy invite message", "Copy code"]);
    });

    it("copies the invite message in one press, and shows what was copied", async () => {
        vi.useFakeTimers();
        hostStep1();
        button("Copy invite message")!.click();
        await vi.advanceTimersByTimeAsync(0);
        expect(writeText).toHaveBeenCalledWith(INVITE);
        expect(buttonNames()).toEqual(["Cancel", "✓ Invite copied", "Copy code"]);
        expect(body().querySelector(".netplay-copied")!.textContent).toBe(`Copied: “${INVITE}”`);
        await vi.advanceTimersByTimeAsync(1600);
        expect(buttonNames()).toEqual(["Cancel", "Copy invite message", "Copy code"]);
        expect(body().querySelector(".netplay-copied")!.textContent).toBe(`Copied: “${INVITE}”`);
    });

    it("shows a new code by itself when the old one ran out, clearing what was copied", async () => {
        hostStep1();
        button("Copy invite message")!.click();
        await new Promise((r) => setTimeout(r, 0));
        session.emit({ phase: "opening", room: null, roomNote: "expired" });
        session.emit({ phase: "waiting", room: "T9W-2HB" });
        expect(room()).toBe("T9W-2HB");
        expect(alerts()).toEqual([["warn", "The old code ran out, so here is a new one."]]);
        expect(text()).toContain("⟳ Waiting for the other player to type it…");
        expect(body().querySelector(".netplay-copied")).toBeNull();
        button("Copy code")!.click();
        await new Promise((r) => setTimeout(r, 0));
        expect(writeText).toHaveBeenLastCalledWith("T9W-2HB");
    });

    it("shows a new code by itself when the other player left before connecting", () => {
        hostStep1();
        session.emit({ phase: "connecting" });
        session.emit({ phase: "opening", room: null, roomNote: "left" });
        session.emit({ phase: "waiting", room: "T9W-2HB" });
        expect(room()).toBe("T9W-2HB");
        expect(alerts()).toEqual([["warn", "The other player left before you were connected. Here is a new code."]]);
        expect(buttonNames()).toEqual(["Cancel", "Copy invite message", "Copy code"]);
    });

    it("moves on by itself once the other player typed the code, offering only Cancel", () => {
        hostStep1();
        session.emit({ phase: "connecting" });
        expect(room()).toBe("K7Q-M4X");
        expect(text()).toContain("⟳ The other player is here — connecting…");
        expect(text()).not.toContain("Waiting for the other player");
        expect(buttonNames()).toEqual(["Cancel"]);
    });

    it("says when the meeting service cannot be reached, offering to try again", () => {
        hostStep1();
        session.emit({ phase: "failed", failure: "service", room: null });
        expect(alerts()).toEqual([
            [
                "err",
                "Network play can't start right now. NESER couldn't reach its meeting service on the internet. Check that this computer is online, then try again.",
            ],
        ]);
        expect(room()).toBeUndefined();
        expect(buttonNames()).toEqual(["Close", "Try again"]);
        button("Try again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
    });

    it("says when too many games are being started, offering to try again", () => {
        hostStep1();
        session.emit({ phase: "failed", failure: "busy", room: null });
        expect(alerts()).toEqual([["err", "Too many games are being started right now. Wait a minute, then try again."]]);
        expect(buttonNames()).toEqual(["Close", "Try again"]);
    });

    it("says when the two computers could not be connected and no relay was available, offering to start again (nr-1wbx)", () => {
        hostStep1();
        session.emit({ phase: "failed", failure: "unreachable", relayTried: false });
        expect(alerts()).toEqual([["err", NO_RELAY]]);
        expect(buttonNames()).toEqual(["Close", "Start again"]);
        expect(document.activeElement).toBe(button("Start again"));
        button("Start again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
    });

    it("is connected at step 2 with a Great response time", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
        const steps = [...body().querySelectorAll(".netplay-steps span")].map((s) => [s.textContent, s.className]);
        expect(steps).toEqual([
            ["1. Share the code", "done"],
            ["2. Connected", "on"],
        ]);
        expect(text()).toContain("Connected to the other player.");
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 18 ms · Great");
        expect(body().querySelector(".netplay-pill")!.getAttribute("data-tone")).toBe("ok");
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        expect(text()).not.toContain("Waiting for the host");
    });

    it("warns when the response time is only Playable", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 84 });
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 84 ms · Playable");
        expect(text()).toContain("Your button presses will take a moment longer to show up. Fine for most games, harder for fast ones.");
    });

    it("turns red, still connected, when it becomes too slow, and back when it recovers", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
        session.emit({ rtt: 190 });
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 190 ms · Too slow");
        expect(text()).toContain(
            "The connection has become too slow to play. It may recover by itself — if not, try a wired connection or being closer to the router."
        );
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
        session.emit({ rtt: 20 });
        expect(text()).not.toContain("too slow to play");
    });

    it("refuses a connection too slow at connect time", () => {
        hostStep1();
        session.emit({ phase: "failed", failure: "too-slow", rtt: 210 });
        expect(text()).toContain("Too slow to play");
        expect(body().querySelector(".netplay-pill")!.textContent).toBe("Response time 210 ms · Too slow");
        expect(text()).toContain(
            "The connection between the two computers is too slow for the game to keep up. Try a wired connection or being closer to the router — or play on the same network."
        );
        expect(buttonNames()).toEqual(["Close", "Try again"]);
        button("Try again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
    });

    it("says when the other player leaves", () => {
        hostStep1();
        session.emit({ step: 2, phase: "failed", failure: "peer-left" });
        expect(text()).toContain("The other player disconnected.");
        expect(buttonNames()).toEqual(["Close", "Start again"]);
    });

    it("disconnects from step 2", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
        button("Disconnect")!.click();
        expect(session.disconnect).toHaveBeenCalled();
        expect(title()).toBe("Play together");
    });
});

describe("joining", () => {
    function field() {
        return body().querySelector<HTMLInputElement>("input.netplay-room-field")!;
    }
    function type(value: string) {
        field().value = value;
        field().dispatchEvent(new Event("input"));
    }
    function enter() {
        field().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    }

    it("asks for the host's code in one field, with the field in focus", () => {
        joinStep1();
        expect(title()).toBe("Join a game");
        expect([...body().querySelectorAll(".netplay-steps span")].map((s) => [s.textContent, s.className])).toEqual([
            ["1. Type the host's code", "on"],
            ["2. Connected", ""],
        ]);
        expect(text()).toContain("Type the code the host tells you.");
        expect(field().placeholder).toBe("ABC-DEF");
        expect(field().getAttribute("autocapitalize")).toBe("characters");
        expect(text()).toContain("Codes never use I, O, 0 or 1.");
        expect(document.activeElement).toBe(field());
        expect(buttonNames()).toEqual(["Cancel", "Join"]);
        expect(button("Join")!.disabled).toBe(true);
    });

    it("shows capitals, puts in the dash, and leaves out what a code never contains", () => {
        joinStep1();
        type("k7");
        expect(field().value).toBe("K7");
        type("k7qi");
        expect(field().value).toBe("K7Q");
        type("K7Q0m");
        expect(field().value).toBe("K7Q-M");
        type("K7Q-M1!4 x9");
        expect(field().value).toBe("K7Q-M4X");
    });

    it("takes a pasted code with or without its dash, spaces or lower case", () => {
        joinStep1();
        type("  k7q m4x ");
        expect(field().value).toBe("K7Q-M4X");
        type("k7qm4x");
        expect(field().value).toBe("K7Q-M4X");
    });

    it("takes a pasted code whatever its length before tidying, even into a full field", () => {
        joinStep1();
        expect(field().hasAttribute("maxlength")).toBe(false);
        type("K7QM4X");
        type("K7Q-M4X  k7q - m4y ");
        expect(field().value).toBe("K7Q-M4X");
    });

    it("keeps the caret where a typo is being fixed", () => {
        joinStep1();
        type("K7QM4X");
        // The 7 is deleted: the dash moves, and the caret stays after the K.
        field().value = "KQ-M4X";
        field().setSelectionRange(1, 1);
        field().dispatchEvent(new Event("input"));
        expect(field().value).toBe("KQM-4X");
        expect(field().selectionStart).toBe(1);
        // A 7 typed there lands where the old one was.
        field().value = "K7QM-4X";
        field().setSelectionRange(2, 2);
        field().dispatchEvent(new Event("input"));
        expect(field().value).toBe("K7Q-M4X");
        expect(field().selectionStart).toBe(2);
        // Past the dash, the caret keeps its place after the code character before it.
        field().value = "K7Q-MZ4X";
        field().setSelectionRange(6, 6);
        field().dispatchEvent(new Event("input"));
        expect(field().value).toBe("K7Q-MZ4");
        expect(field().selectionStart).toBe(6);
    });

    it("enables Join once six characters are in, and Enter presses it", () => {
        joinStep1();
        type("K7QM4");
        expect(button("Join")!.disabled).toBe(true);
        enter();
        expect(session.joinRoom).not.toHaveBeenCalled();
        type("K7QM4X");
        expect(button("Join")!.disabled).toBe(false);
        enter();
        expect(session.joinRoom).toHaveBeenCalledWith("K7Q-M4X");
        button("Join")!.click();
        expect(session.joinRoom).toHaveBeenCalledTimes(2);
    });

    it("shows Connecting beneath the code, with only Cancel", () => {
        joinStep1();
        type("K7QM4X");
        session.emit({ phase: "connecting" });
        expect(field().value).toBe("K7Q-M4X");
        expect(field().readOnly).toBe(true);
        expect(text()).toContain("⟳ Connecting…");
        expect(buttonNames()).toEqual(["Cancel"]);
        session.emit({ phase: "waiting" });
        expect(text()).toContain("⟳ Connecting…");
        expect(buttonNames()).toEqual(["Cancel"]);
    });

    it.each([
        ["room-not-found", "There's no game with that code. Check the code with the host and try again."],
        ["room-full", "That game already has two players. Ask the host to start a new game and tell you the new code."],
        [
            "service",
            "Network play can't start right now. NESER couldn't reach its meeting service on the internet. Check that this computer is online, then try again.",
        ],
        ["busy", "Too many games are being started right now. Wait a minute, then try again."],
        ["host-left", "The host closed this game before you were connected. Ask them to start a new one and tell you the new code."],
    ] as const)("says %s beneath the field, keeping the code to fix and Join again", (problem, words) => {
        joinStep1();
        type("K7QM4Y");
        session.emit({ phase: "connecting" });
        session.emit({ phase: "idle", joinProblem: problem });
        expect(alerts()).toEqual([["err", words]]);
        expect(field().value).toBe("K7Q-M4Y");
        expect(field().readOnly).toBe(false);
        expect(document.activeElement).toBe(field());
        expect(buttonNames()).toEqual(["Cancel", "Join"]);
        expect(button("Join")!.disabled).toBe(false);
        type("K7QM4X");
        button("Join")!.click();
        expect(session.joinRoom).toHaveBeenLastCalledWith("K7Q-M4X");
    });

    it("names both builds when the host's NESER differs, offering only Close", () => {
        joinStep1();
        type("K7QM4X");
        session.emit({ phase: "failed", failure: "other-build", theirs: { version: "1.3.0", commit: "b81e07d" } });
        expect(alerts()).toEqual([
            [
                "err",
                "Different versions of NESER. You have NESER 1.3.0 (4f2a9c1), the host has 1.3.0 (b81e07d). You both need the same version to play together — reload the page on both computers.",
            ],
        ]);
        expect(field().value).toBe("K7Q-M4X");
        expect(buttonNames()).toEqual(["Close"]);
    });

    it("stops after too many wrong codes, and Start again keeps the code to edit", () => {
        joinStep1();
        type("K7QM4Y");
        session.emit({ phase: "failed", failure: "too-many-attempts" });
        expect(alerts()).toEqual([["err", "Too many wrong codes. Check the code with the host, then press Start again."]]);
        expect(buttonNames()).toEqual(["Close", "Start again"]);
        button("Start again")!.click();
        expect(session.startAgain).toHaveBeenCalled();
        session.emit({ phase: "idle", failure: null });
        expect(field().value).toBe("K7Q-M4Y");
        expect(field().readOnly).toBe(false);
        expect(buttonNames()).toEqual(["Cancel", "Join"]);
    });

    it("says when the two computers could not be connected and no relay was available, offering to start again (nr-1wbx)", () => {
        joinStep1();
        type("K7QM4X");
        session.emit({ phase: "failed", failure: "unreachable", relayTried: false });
        expect(alerts()).toEqual([["err", NO_RELAY]]);
        expect(buttonNames()).toEqual(["Close", "Start again"]);
        expect(document.activeElement).toBe(button("Start again"));
    });

    it("starts with an empty field after the window is closed and opened again", () => {
        joinStep1();
        type("K7QM4X");
        q("#netplay-close")!.click();
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        expect(field().value).toBe("");
    });

    it("is connected to the host, waiting for the host to start the game", () => {
        joinStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
        expect(text()).toContain("Connected to the host.");
        expect(text()).toContain("⟳ Waiting for the host to start the game…");
        expect(buttonNames()).toEqual(["Disconnect"]);
    });
});

describe("playing through the relay (nr-1wbx)", () => {
    const RELAY_LINE =
        "↪ Connected through NESER's relay in Sweden, because your networks don't allow a direct connection. This can add a little delay.";
    const relayLines = () => [...body().querySelectorAll<HTMLElement>(".netplay-relay")].map((line) => line.textContent);

    it.each([
        ["host", hostStep1],
        ["joiner", joinStep1],
    ] as const)("says the %s is connected through the relay, beneath the response time", (_, toStep1) => {
        toStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18, relayed: true });
        expect(relayLines()).toEqual([RELAY_LINE]);
        const live = body().querySelector(".netplay-live")!;
        expect([...live.children].map((child) => child.className)).toEqual(["netplay-pill", "netplay-relay"]);
        expect(body().querySelector(".netplay-relay")!.matches("button, a, [tabindex]")).toBe(false);
    });

    it("puts the line above the joiner's wait for the host", () => {
        joinStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18, relayed: true });
        expect(text().indexOf(RELAY_LINE)).toBeLessThan(text().indexOf("⟳ Waiting for the host to start the game…"));
    });

    it("says nothing of a relay on a direct connection", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18, relayed: false });
        expect(relayLines()).toEqual([]);
        expect(text()).not.toContain("relay");
    });

    it("keeps the line above the alert when the relayed connection becomes too slow, with the relay's advice", () => {
        hostStep1();
        session.emit({ step: 2, phase: "connected", rtt: 18, relayed: true });
        session.emit({ rtt: 190 });
        const live = body().querySelector(".netplay-live")!;
        expect([...live.children].map((child) => child.className)).toEqual(["netplay-pill", "netplay-relay", "netplay-alert"]);
        expect(alerts()).toEqual([
            [
                "err",
                "The connection through NESER's relay has become too slow to play. It may recover by itself — if not, try again with both of you on a home network, ideally wired.",
            ],
        ]);
        expect(buttonNames()).toEqual(["Disconnect", "Start game"]);
    });

    it("gives the relay's advice when a relayed connection is too slow at connect time", () => {
        hostStep1();
        session.emit({ phase: "failed", failure: "too-slow", rtt: 210, relayed: true });
        expect(text()).toContain("Too slow to play");
        expect(alerts()).toEqual([
            [
                "err",
                "The connection through NESER's relay is too slow for the game to keep up. A direct connection is usually faster — try again with both of you on a home network, ideally wired.",
            ],
        ]);
        expect(buttonNames()).toEqual(["Close", "Try again"]);
    });

    it.each([
        ["host", hostStep1],
        ["joiner", joinStep1],
    ] as const)("tells the %s the relay was tried and still could not connect them", (_, toStep1) => {
        toStep1();
        session.emit({ phase: "failed", failure: "unreachable", relayTried: true });
        expect(alerts()).toEqual([["err", RELAY_TRIED]]);
        expect(buttonNames()).toEqual(["Close", "Start again"]);
        expect(document.activeElement).toBe(button("Start again"));
    });

    it("says Relayed on the chip, and only the arrow on a narrow window, while the screen reader hears Relayed", () => {
        open();
        session.emit({ role: "host", step: 2, phase: "connected", rtt: 18, relayed: true });
        q("#netplay-close")!.click();
        expect(chip().querySelector(".netplay-chip-long")!.textContent).toBe("Connected · Relayed · ");
        expect(chip().querySelector(".netplay-chip-short")!.textContent).toBe("↪ ");
        expect(chip().querySelector(".netplay-chip-short")!.getAttribute("aria-hidden")).toBe("true");
        expect(chip().getAttribute("aria-label")).toBe("Connected · Relayed · 18 ms");
        expect(chip().dataset.tone).toBe("ok");
        session.emit({ rtt: 110 });
        expect(chip().dataset.tone).toBe("warn");
        expect(chip().getAttribute("aria-label")).toBe("Connected · Relayed · 110 ms");
        session.emit({ rtt: 160 });
        expect(chip().dataset.tone).toBe("err");
    });

    it("says neither Relayed nor the arrow while waiting or once disconnected", () => {
        open();
        session.emit({ role: "host", step: 2, phase: "connected", rtt: 84, relayed: true });
        q("#netplay-close")!.click();
        waiting = true;
        window_.refresh();
        expect(chip().textContent).toBe("Waiting…");
        expect(chip().hasAttribute("aria-label")).toBe(false);
        waiting = false;
        session.emit({ phase: "failed", failure: "peer-left" });
        expect(chip().textContent).toBe("Disconnected");
        expect(chip().hasAttribute("aria-label")).toBe(false);
    });

    it("leaves a direct game's chip as it was", () => {
        open();
        session.emit({ role: "host", step: 2, phase: "connected", rtt: 18, relayed: false });
        q("#netplay-close")!.click();
        expect(chip().textContent).toBe("Connected · 18 ms");
        expect(chip().querySelector(".netplay-chip-short")).toBeNull();
        expect(chip().hasAttribute("aria-label")).toBe(false);
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
        session.emit({ role: "host", step: 2, phase: "connected", rtt: 18 });
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
        session.emit({ role: "host", step: 2, phase: "connected", rtt: 18 });
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

describe("sending the game (nr-ayk5.4)", () => {
    function hostConnected() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=host]")!.click();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
    }
    function joinConnected() {
        open();
        body().querySelector<HTMLButtonElement>("[data-choice=join]")!.click();
        session.emit({ step: 2, phase: "connected", rtt: 18 });
    }
    const sending = (direction: "send" | "receive", done: number, total: number) =>
        session.emit({ sending: { direction, name: "Super Mario Bros. 3", done, total } });

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
        sending("send", 384 * 1024, 384 * 1024);
        expect(button("Cancel")!.disabled).toBe(true);
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

describe("playing together (nr-ayk5.5)", () => {
    function playingTogether(role: "host" | "join") {
        playing = true;
        session.emit({ role, step: 2, phase: "connected", rtt: 18 });
    }

    it.each(["host", "join"] as const)("offers the %s only Disconnect while the game is played, which asks first", (role) => {
        playingTogether(role);
        open();
        expect(buttonNames()).toEqual(["Disconnect"]);
        expect(text()).not.toContain("Waiting for the host to start the game");
        expect(text()).not.toContain("When you're both ready");
        expect(text()).toContain("Connected");
        button("Disconnect")!.click();
        expect(askedToEnd).toHaveBeenCalledOnce();
        expect(session.disconnect).not.toHaveBeenCalled();
    });

    it("shows Waiting… on an amber chip while the other player's game holds this one", () => {
        playingTogether("host");
        waiting = true;
        window_.refresh();
        expect(chip().textContent).toBe("Waiting…");
        expect(chip().dataset.tone).toBe("wait");
        waiting = false;
        window_.refresh();
        expect(chip().textContent).toBe("Connected · 18 ms");
    });

    it("shows Resyncing… on an amber chip while the games are resynced, and the response time alone when narrow", () => {
        playingTogether("join");
        resyncing = true;
        waiting = true;
        window_.refresh();
        expect(chip().dataset.tone).toBe("wait");
        expect(chip().querySelector(".netplay-chip-long")!.textContent).toBe("Resyncing…");
        expect(chip().querySelector(".netplay-chip-short")!.textContent).toBe("18 ms");
        resyncing = false;
        waiting = false;
        window_.refresh();
        expect(chip().textContent).toBe("Connected · 18 ms");
    });

    it("shows Disconnected to the player who ended it too, until pressed, and then the first choice", () => {
        playingTogether("join");
        session.emit({ ...IDLE });
        window_.markEnded();
        expect(chip().textContent).toBe("Disconnected");
        expect(chip().dataset.tone).toBe("err");
        open();
        expect(title()).toBe("Play together");
        expect(text()).toContain("Join a game");
        q("#netplay-close")!.click();
        expect(chip().textContent).toBe("Play together");
    });
});
