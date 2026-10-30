import type { ConsoleKind } from "../console/consoles";
import { buildLabel, type BuildId } from "../netplay/build_id";
import { responseTimeRating } from "../netplay/response_time";
import { NETPLAY_KINDS, type GamePackage } from "../netplay/game_package";
import { ROOM_CODE_LENGTH, showRoomCode, tidyRoomCode } from "../netplay/room_code";
import type { JoinProblem, NetplaySession, NetplayView, Role } from "../netplay/session";

/**
 * The Play together window and its top-bar button (nr-ayk5.3; the agreed design is
 * docs/ui/nr-ayk5.3-play-together.html). It draws the session's view one step at a time and turns
 * the button into a status chip while connected. Closing the window keeps a connection; before
 * one, closing cancels the attempt. Once connected the host sends its game from here
 * (nr-ayk5.4, docs/ui/nr-ayk5.4-sending-the-game.html); closing the window while it is on its way
 * does not stop it, and the chip shows how far it is. While the game is played together
 * (nr-ayk5.5) the connected step offers only Disconnect, which the page asks about first, and the
 * chip says "Waiting…" while the other player's game holds this one.
 *
 * The two meet by one short room code (nr-z4nw, docs/ui/nr-z4nw-room-code.html): the host's step
 * shows it, with Copy code and Copy invite message; the joiner's step is one field that takes only
 * code characters, puts the dash in itself and keeps what was typed through every error.
 */

export type CurrentGame = { kind: ConsoleKind; title: string };

const HOSTABLE: readonly ConsoleKind[] = NETPLAY_KINDS;
const COPIED_FOR_MS = 1500;

const STEPS: Record<Role, readonly string[]> = {
    host: ["1. Share the code", "2. Connected"],
    join: ["1. Type the host's code", "2. Connected"],
};

/** Where the invite sends the other player. */
const NESER_ADDRESS = "https://neser.rmstdope.eu";

export function inviteMessage(gameTitle: string, room: string): string {
    return (
        `Play ${gameTitle} with me in NESER! Open ${NESER_ADDRESS}, press Play together, ` +
        `then Join a game, and type the code ${room}`
    );
}

/**
 * Always beneath the two cards (nr-ri8n, docs/ui/nr-ri8n-internet-play.html; worded for the relay
 * in nr-1wbx): plain information, nothing to accept.
 */
const ADDRESS_LINE =
    "ⓘ Playing together connects your computer to the other player's — directly, or through NESER's relay when it has to — so each of you can see the other's internet address.";

/**
 * A game relayed through outpost (nr-1wbx, docs/ui/nr-1wbx-relay.html): this line beneath the
 * response time, "Relayed" on the chip, and its own advice when it is too slow.
 */
const RELAY_LINE =
    "↪ Connected through NESER's relay in Sweden, because your networks don't allow a direct connection. This can add a little delay.";
const TOO_SLOW_TO_START = {
    direct: "The connection between the two computers is too slow for the game to keep up. Try a wired connection or being closer to the router — or play on the same network.",
    relayed:
        "The connection through NESER's relay is too slow for the game to keep up. A direct connection is usually faster — try again with both of you on a home network, ideally wired.",
};
const BECAME_TOO_SLOW = {
    direct: "The connection has become too slow to play. It may recover by itself — if not, try a wired connection or being closer to the router.",
    relayed:
        "The connection through NESER's relay has become too slow to play. It may recover by itself — if not, try again with both of you on a home network, ideally wired.",
};
/** Couldn't connect, by whether liaison offered a relay for the game. */
const COULD_NOT_CONNECT = {
    noRelay:
        "Your networks don't allow a direct connection, and NESER's relay can't be reached right now. Try again in a while, or with both of you on a home network.",
    relayTried:
        "Your networks blocked every way of connecting them, even through NESER's relay. This can happen on strict work or school networks — try again from another network.",
};

const relayWay = (view: NetplayView) => (view.relayed ? "relayed" : "direct");

const OTHER: Record<Role, string> = { host: "the other player", join: "the host" };

const kb = (bytes: number) => Math.floor(bytes / 1024);
const percent = (done: number, total: number) => (total > 0 ? Math.floor((done * 100) / total) : 0);

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text) node.textContent = text;
    return node;
}

function btn(label: string, onClick: () => void, primary = false): HTMLButtonElement {
    const button = el("button", primary ? "btn btn-sm btn-primary" : "btn btn-sm btn-ghost", label);
    button.type = "button";
    button.addEventListener("click", onClick);
    return button;
}

/** A red or amber box: a bold lead sentence, then the rest. */
function alert(tone: "warn" | "err", lead: string, rest = ""): HTMLElement {
    const box = el("div", "netplay-alert");
    box.dataset.tone = tone;
    box.setAttribute("role", "alert");
    if (lead) box.append(el("b", "", lead));
    if (lead && rest) box.append(" ");
    if (rest) box.append(rest);
    return box;
}

const JOIN_PROBLEMS: Record<JoinProblem, [string, string]> = {
    "room-not-found": ["There's no game with that code.", "Check the code with the host and try again."],
    "room-full": ["That game already has two players.", "Ask the host to start a new game and tell you the new code."],
    service: [
        "Network play can't start right now.",
        "NESER couldn't reach its meeting service on the internet. Check that this computer is online, then try again.",
    ],
    busy: ["Too many games are being started right now.", "Wait a minute, then try again."],
    "host-left": ["The host closed this game before you were connected.", "Ask them to start a new one and tell you the new code."],
};

const ROOM_NOTES = {
    expired: "The old code ran out, so here is a new one.",
    left: "The other player left before you were connected. Here is a new code.",
} as const;

function pill(rtt: number): HTMLElement {
    const rating = responseTimeRating(rtt);
    const node = el("span", "netplay-pill", `Response time ${rtt} ms · ${rating.word}`);
    node.dataset.tone = rating.tone;
    return node;
}

function dot(tone: string): HTMLElement {
    const node = el("span", "netplay-dot");
    node.dataset.tone = tone;
    node.setAttribute("aria-hidden", "true");
    return node;
}

export function createPlayTogether({
    dialog,
    title,
    body,
    closeButton,
    openButton,
    session,
    build,
    currentGame,
    hostGame,
    onOpen,
    onClose,
    playing = () => false,
    waiting = () => false,
    resyncing = () => false,
    onDisconnect = () => session.disconnect(),
}: {
    dialog: HTMLDialogElement;
    title: HTMLElement;
    body: HTMLElement;
    closeButton: HTMLButtonElement;
    openButton: HTMLButtonElement;
    session: NetplaySession;
    build: BuildId;
    currentGame: () => CurrentGame | null;
    /** What Start game sends: the host's game as it is now, and its title; null without one to send. */
    hostGame: () => { game: GamePackage; title: string } | null;
    onOpen: () => void;
    onClose: () => void;
    /** Whether the game is being played together now. */
    playing?: () => boolean;
    /** Whether the other player's game is holding this one. */
    waiting?: () => boolean;
    /** Whether the two games are being resynced. */
    resyncing?: () => boolean;
    /** Disconnect on the connected step. */
    onDisconnect?: () => void;
}) {
    let isOpen = false;
    /** Which screen is drawn; a change of it redraws the window, anything else updates it in place. */
    let screenKey = "";
    let update: (view: NetplayView) => void = () => {};
    /** The code characters in the joiner's field, kept through every error and Start again. */
    let typedRoom = "";
    const buttonLabel = openButton.textContent ?? "Play together";
    /** This player ended playing together: the chip says Disconnected until it is pressed. */
    let endedHere = false;

    function close() {
        if (!isOpen) return;
        isOpen = false;
        if (typeof dialog.close === "function" && dialog.open) dialog.close();
        else dialog.removeAttribute("open");
        // Only a live connection outlives the window; anything else is abandoned with it.
        if (session.view().phase !== "connected") session.cancel();
        openButton.setAttribute("aria-expanded", "false");
        drawButton(session.view());
        openButton.focus();
        onClose();
    }

    function open() {
        if (isOpen) return;
        isOpen = true;
        if (endedHere) {
            endedHere = false;
            drawButton(session.view());
        }
        screenKey = "";
        if (typeof dialog.showModal === "function") dialog.showModal();
        else dialog.setAttribute("open", "");
        openButton.setAttribute("aria-expanded", "true");
        draw(session.view());
        onOpen();
    }

    function drawButton(view: NetplayView) {
        // Only the relayed chip names itself, so a screen reader hears "Relayed" on a narrow window too.
        openButton.removeAttribute("aria-label");
        const lost = view.phase === "failed" && view.failure === "peer-left";
        if (view.phase === "connected" && view.sending) {
            const { direction, done, total } = view.sending;
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = "busy";
            openButton.replaceChildren(
                dot("busy"),
                el("span", "netplay-chip-long", direction === "send" ? "Sending · " : "Receiving · "),
                `${percent(done, total)}%`
            );
        } else if (view.phase === "connected" && resyncing()) {
            // A narrow window keeps the dot and the response time only, as everywhere.
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = "wait";
            openButton.replaceChildren(
                dot("wait"),
                el("span", "netplay-chip-long", "Resyncing…"),
                el("span", "netplay-chip-short", `${view.rtt ?? 0} ms`)
            );
        } else if (view.phase === "connected" && waiting()) {
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = "wait";
            openButton.replaceChildren(dot("wait"), "Waiting…");
        } else if (view.phase === "connected" && view.rtt !== null) {
            const tone = responseTimeRating(view.rtt).tone;
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = tone;
            if (view.relayed) {
                const arrow = el("span", "netplay-chip-short", "↪ ");
                arrow.setAttribute("aria-hidden", "true");
                openButton.replaceChildren(dot(tone), el("span", "netplay-chip-long", "Connected · Relayed · "), arrow, `${view.rtt} ms`);
                openButton.setAttribute("aria-label", `Connected · Relayed · ${view.rtt} ms`);
            } else {
                openButton.replaceChildren(dot(tone), el("span", "netplay-chip-long", "Connected · "), `${view.rtt} ms`);
            }
        } else if (lost || (endedHere && view.phase === "idle")) {
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = "err";
            openButton.replaceChildren(dot("err"), "Disconnected");
        } else {
            openButton.dataset.icon = "people";
            delete openButton.dataset.tone;
            openButton.replaceChildren(buttonLabel);
        }
    }

    function steps(role: Role, step: number): HTMLElement {
        const strip = el("div", "netplay-steps");
        STEPS[role].forEach((label, index) => {
            const span = el("span", index + 1 < step ? "done" : index + 1 === step ? "on" : "", label);
            strip.append(span);
        });
        return strip;
    }

    function actions(...buttons: HTMLButtonElement[]): HTMLElement {
        const row = el("div", "netplay-actions");
        row.append(...buttons);
        return row;
    }

    function choiceCard(choice: Role, heading: string, line: string, enabled: boolean, onPick: () => void) {
        const card = el("button", "netplay-card");
        card.type = "button";
        card.dataset.choice = choice;
        card.disabled = !enabled;
        card.append(el("b", "", heading), el("small", "", line));
        card.addEventListener("click", onPick);
        return card;
    }

    function drawChoice() {
        const game = currentGame();
        const hostable = game !== null && HOSTABLE.includes(game.kind);
        const host = !game
            ? choiceCard("host", "Host a game", "Load a NES or Super Nintendo game first, then come back here.", false, () => {})
            : !hostable
              ? choiceCard("host", "Host this game", "Network play works with NES and Super Nintendo games.", false, () => {})
              : choiceCard("host", "Host this game", `Share ${game.title} with another player. You are player 1.`, true, () => session.host());
        const join = choiceCard(
            "join",
            "Join a game",
            "Connect to a game someone else is hosting. You are player 2 — you don't need the game file.",
            true,
            () => {
                typedRoom = "";
                session.join();
            }
        );
        body.append(
            el("p", "netplay-lede", "Two browsers on the same network, one game, two pads."),
            host,
            join,
            el("p", "netplay-address", ADDRESS_LINE)
        );
        (hostable ? host : join).focus();
    }

    /** A copy button that says `done` for a moment once `text()` is on the clipboard. */
    function copyButton(label: string, done: string, text: () => string | null, onCopied: (copied: string) => void = () => {}) {
        let timer: ReturnType<typeof setTimeout> | undefined;
        const button = btn(label, async () => {
            const copied = text();
            if (!copied || !(await copyText(copied))) return;
            button.textContent = done;
            onCopied(copied);
            clearTimeout(timer);
            timer = setTimeout(() => (button.textContent = label), COPIED_FOR_MS);
        });
        return button;
    }

    /** The first button of the step's row, for a step whose only way on is a button. */
    function focusFirstAction() {
        body.querySelector<HTMLButtonElement>(".netplay-actions button")?.focus();
    }

    /** Couldn't connect puts focus on Start again, as agreed for it (nr-ri8n); other failures on the first button. */
    function focusFailureAction(view: NetplayView) {
        if (view.failure === "unreachable") body.querySelector<HTMLButtonElement>(".netplay-actions .btn-primary")?.focus();
        else focusFirstAction();
    }

    function drawHost(view: NetplayView) {
        if (view.phase === "failed" && view.failure === "too-slow") return drawRefused(view);
        body.append(steps("host", 1));
        if (view.phase === "failed") {
            // Liaison out of reach or busy is tried again; anything else starts again.
            const again = view.failure === "service" || view.failure === "busy" ? "Try again" : "Start again";
            body.append(failureAlert(view), actions(btn("Close", close), btn(again, () => session.startAgain(), true)));
            return focusFailureAction(view);
        }
        const room = el("div", "netplay-room", view.room ?? "…");
        room.setAttribute("aria-label", "Room code");
        body.append(el("p", "", "Tell the other player this code:"), room);
        if (view.roomNote) body.append(alert("warn", "", ROOM_NOTES[view.roomNote]));
        if (view.phase === "connecting") {
            body.append(el("p", "netplay-muted", "⟳ The other player is here — connecting…"), actions(btn("Cancel", close)));
            return focusFirstAction();
        }
        body.append(el("p", "netplay-muted", "⟳ Waiting for the other player to type it…"));
        const shown = () => session.view().room;
        const copiedLine = el("p", "netplay-muted netplay-small netplay-copied");
        const invite = copyButton(
            "Copy invite message",
            "✓ Invite copied",
            () => {
                const code = shown();
                return code ? inviteMessage(currentGame()?.title ?? "a game", code) : null;
            },
            (message) => {
                copiedLine.textContent = `Copied: “${message}”`;
                if (!copiedLine.isConnected) body.append(copiedLine);
            }
        );
        const copy = copyButton("Copy code", "✓ Copied", shown);
        invite.disabled = copy.disabled = view.room === null;
        body.append(actions(btn("Cancel", close), invite, copy));
        if (view.room) copy.focus();
        else focusFirstAction();
    }

    function drawJoin(view: NetplayView) {
        if (view.phase === "failed" && view.failure === "too-slow") return drawRefused(view);
        const field = el("input", "netplay-room-field");
        field.type = "text";
        field.placeholder = "ABC-DEF";
        field.setAttribute("aria-label", "Room code");
        field.setAttribute("autocapitalize", "characters");
        field.autocomplete = "off";
        field.spellcheck = false;
        // No maxlength: the browser would cut a paste short before it is tidied (" k7q - m4x ").
        field.value = showRoomCode(typedRoom);
        // While a code is tried, and after a failure, the field shows the code that was tried.
        field.readOnly = view.phase !== "idle";
        body.append(
            steps("join", 1),
            el("p", "", "Type the code the host tells you."),
            field,
            el("p", "netplay-muted netplay-small", "Codes never use I, O, 0 or 1.")
        );
        if (view.phase === "failed") {
            body.append(
                failureAlert(view),
                view.failure === "other-build"
                    ? actions(btn("Close", close))
                    : actions(btn("Close", close), btn("Start again", () => session.startAgain(), true))
            );
            return focusFailureAction(view);
        }
        if (view.phase !== "idle") {
            body.append(el("p", "netplay-muted", "⟳ Connecting…"), actions(btn("Cancel", close)));
            return focusFirstAction();
        }
        if (view.joinProblem) body.append(alert("err", ...JOIN_PROBLEMS[view.joinProblem]));
        const own = currentGame();
        if (own) body.append(alert("warn", "", `${own.title} will be closed when the host's game arrives. Your saves are kept.`));
        const join = btn("Join", () => session.joinRoom(showRoomCode(typedRoom)), true);
        const ready = () => (join.disabled = typedRoom.length !== ROOM_CODE_LENGTH);
        field.addEventListener("input", () => {
            // The caret stays after the same code character, so a typo is fixed where it is.
            const before = tidyRoomCode(field.value.slice(0, field.selectionStart ?? field.value.length)).length;
            typedRoom = tidyRoomCode(field.value);
            field.value = showRoomCode(typedRoom);
            const at = showRoomCode(typedRoom.slice(0, before)).length;
            field.setSelectionRange(at, at);
            ready();
        });
        field.addEventListener("keydown", (event) => {
            if (event.key !== "Enter" || join.disabled) return;
            event.preventDefault();
            join.click();
        });
        ready();
        body.append(actions(btn("Cancel", close), join));
        field.focus();
    }

    function failureAlert(view: NetplayView): HTMLElement {
        const role = view.role!;
        if (view.failure === "other-build" && view.theirs) {
            return alert(
                "err",
                "Different versions of NESER.",
                `You have NESER ${buildLabel(build)}, ${OTHER[role]} has ${buildLabel(view.theirs)}. ` +
                    "You both need the same version to play together — reload the page on both computers."
            );
        }
        if (view.failure === "peer-left") return alert("err", "The other player disconnected.");
        if (view.failure === "service") return alert("err", ...JOIN_PROBLEMS.service);
        if (view.failure === "busy") return alert("err", ...JOIN_PROBLEMS.busy);
        if (view.failure === "too-many-attempts") {
            return alert("err", "Too many wrong codes.", "Check the code with the host, then press Start again.");
        }
        return alert("err", "Couldn't connect your two computers.", view.relayOffered ? COULD_NOT_CONNECT.relayTried : COULD_NOT_CONNECT.noRelay);
    }

    function drawRefused(view: NetplayView) {
        const status = el("p", "netplay-status");
        status.append(dot("err"), " ", el("b", "", "Too slow to play"));
        body.append(status);
        if (view.rtt !== null) body.append(pill(view.rtt));
        body.append(
            alert("err", "", TOO_SLOW_TO_START[relayWay(view)]),
            actions(btn("Close", close), btn("Try again", () => session.startAgain(), true))
        );
        body.querySelector<HTMLButtonElement>(".netplay-actions button")?.focus();
    }

    function drawConnected(view: NetplayView) {
        const role = view.role!;
        body.append(steps(role, 2));
        if (view.phase === "failed") {
            body.append(failureAlert(view), actions(btn("Close", close), btn("Start again", () => session.startAgain(), true)));
            body.querySelector<HTMLButtonElement>(".netplay-actions button")?.focus();
            return;
        }
        const status = el("p", "netplay-status");
        const live = el("div", "netplay-live");
        body.append(status, live);
        const fillSending = view.sending ? drawSending(view) : drawReady(view);
        update = (v) => {
            fillSending(v);
            if (v.rtt === null) return;
            const rating = responseTimeRating(v.rtt);
            status.replaceChildren(dot(rating.tone), " ", el("b", "", "Connected"), ` to ${OTHER[role]}.`);
            live.replaceChildren(pill(v.rtt));
            if (v.relayed) live.append(el("p", "netplay-relay", RELAY_LINE));
            if (rating.tone === "warn") {
                live.append(alert("warn", "", "Your button presses will take a moment longer to show up. Fine for most games, harder for fast ones."));
            } else if (rating.tone === "err") {
                live.append(alert("err", "", BECAME_TOO_SLOW[relayWay(v)]));
            }
        };
        update(view);
    }

    function noticeAlert(view: NetplayView): HTMLElement | null {
        if (view.notice === "host-stopped") return alert("warn", "", "The host stopped sending the game.");
        if (view.notice === "joiner-stopped") return alert("warn", "", "The other player stopped the game from being sent.");
        if (view.notice === "not-arrived") {
            const then = view.role === "host" ? "try again." : "the host can try again.";
            return alert("err", "The game didn't arrive.", `Something went wrong while sending it. You're still connected — ${then}`);
        }
        return null;
    }

    /** The connected step with nothing on its way: the host's Start game, the joiner's wait. */
    function drawReady(view: NetplayView): (v: NetplayView) => void {
        const notice = noticeAlert(view);
        if (notice) body.append(notice);
        const disconnect = btn("Disconnect", onDisconnect);
        // Mid-game, there is nothing to start nor wait for.
        if (playing()) {
            body.append(actions(disconnect));
            disconnect.focus();
            return () => {};
        }
        if (view.role === "join") {
            body.append(el("p", "netplay-muted", "⟳ Waiting for the host to start the game…"), actions(disconnect));
            disconnect.focus();
            return () => {};
        }
        const game = currentGame();
        const hostable = game !== null && HOSTABLE.includes(game.kind);
        const start = btn(
            "Start game",
            () => {
                const sent = hostGame();
                if (sent) session.startGame(sent.game, sent.title);
            },
            true
        );
        start.disabled = !hostable;
        const hint = !game
            ? "Load a NES or Super Nintendo game first, then come back here."
            : !hostable
              ? "Network play works with NES and Super Nintendo games."
              : notice
                ? null
                : `When you're both ready, send ${game.title} and start.`;
        if (hint) body.append(el("p", "netplay-muted", hint));
        body.append(actions(disconnect, start));
        (hostable ? start : disconnect).focus();
        return () => {};
    }

    /** The game on its way: a bar and a count on both sides, and Cancel. */
    function drawSending(view: NetplayView): (v: NetplayView) => void {
        const { direction, name } = view.sending!;
        const bar = el("i");
        const progress = el("div", "netplay-progress");
        progress.setAttribute("role", "progressbar");
        progress.setAttribute("aria-valuemin", "0");
        progress.setAttribute("aria-valuemax", "100");
        progress.append(bar);
        const count = el("p", "netplay-muted netplay-count");
        body.append(el("p", "netplay-sending", `${direction === "send" ? "Sending" : "Receiving"} ${name}…`), progress, count);
        if (direction === "receive") {
            body.append(
                el(
                    "p",
                    "netplay-muted netplay-small",
                    "You'll play with the host's game settings. Your own settings come back when you stop playing together."
                )
            );
        }
        const cancel = btn("Cancel", () => session.cancelSending());
        body.append(actions(cancel));
        cancel.focus();
        return (v) => {
            if (!v.sending) return;
            const share = percent(v.sending.done, v.sending.total);
            bar.style.width = `${share}%`;
            progress.setAttribute("aria-valuenow", String(share));
            count.textContent = `${kb(v.sending.done)} KB of ${kb(v.sending.total)} KB`;
            // Everything has arrived and both are about to start; there is nothing left to cancel.
            cancel.disabled = v.sending.done >= v.sending.total;
        };
    }

    function draw(view: NetplayView) {
        const key = [
            view.role,
            view.step,
            view.phase,
            view.failure,
            view.sending?.direction ?? "",
            view.notice,
            view.room,
            view.roomNote,
            view.joinProblem,
        ].join("|");
        if (key === screenKey) return update(view);
        screenKey = key;
        update = () => {};
        body.replaceChildren();
        title.textContent = view.role === "host" ? "Host this game" : view.role === "join" ? "Join a game" : "Play together";
        if (view.role === null) drawChoice();
        else if (view.step === 2) drawConnected(view);
        else if (view.role === "host") drawHost(view);
        else drawJoin(view);
    }

    session.subscribe((view) => {
        drawButton(view);
        if (isOpen) draw(view);
    });
    openButton.addEventListener("click", open);
    closeButton.addEventListener("click", close);
    // Esc closes a modal dialog by itself; this hears it as it hears the ✕.
    dialog.addEventListener("close", close);
    dialog.addEventListener("click", (event) => {
        if (event.target === dialog) close();
    });
    drawButton(session.view());

    return {
        open,
        close,
        isOpen: () => isOpen,
        /** Draws the chip again, for a change the session does not know of (the game waiting). */
        refresh: () => drawButton(session.view()),
        /** This player ended playing together. */
        markEnded: () => {
            endedHere = true;
            drawButton(session.view());
        },
    };
}

/**
 * Copies to the clipboard, saying whether it did; a page served over plain http on a home network
 * has no clipboard API, so the older copy command is tried there.
 */
async function copyText(text: string): Promise<boolean> {
    try {
        await navigator.clipboard.writeText(text);
        return true;
    } catch {
        // Fall through to the old way, which works without a secure context.
    }
    const scratch = document.createElement("textarea");
    scratch.value = text;
    scratch.style.position = "fixed";
    scratch.style.opacity = "0";
    document.body.append(scratch);
    scratch.select();
    const copied = document.execCommand("copy");
    scratch.remove();
    return copied;
}
