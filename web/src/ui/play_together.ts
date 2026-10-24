import type { ConsoleKind } from "../console/consoles";
import { buildLabel, type BuildId } from "../netplay/build_id";
import { responseTimeRating } from "../netplay/response_time";
import { NETPLAY_KINDS, type GamePackage } from "../netplay/game_package";
import type { NetplaySession, NetplayView, Role } from "../netplay/session";

/**
 * The Play together window and its top-bar button (nr-ayk5.3; the agreed design is
 * docs/ui/nr-ayk5.3-play-together.html). It draws the session's view one step at a time and turns
 * the button into a status chip while connected. Closing the window keeps a connection; before
 * one, closing cancels the attempt. Once connected the host sends its game from here
 * (nr-ayk5.4, docs/ui/nr-ayk5.4-sending-the-game.html); closing the window while it is on its way
 * does not stop it, and the chip shows how far it is. While the game is played together
 * (nr-ayk5.5) the connected step offers only Disconnect, which the page asks about first, and the
 * chip says "Waiting…" while the other player's game holds this one.
 */

export type CurrentGame = { kind: ConsoleKind; title: string };

const HOSTABLE: readonly ConsoleKind[] = NETPLAY_KINDS;
const COPIED_FOR_MS = 1500;

const STEPS: Record<Role, readonly string[]> = {
    host: ["1. Send your code", "2. Paste their code", "3. Connected"],
    join: ["1. Paste the host's code", "2. Send your code", "3. Connected"],
};

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

function pasteAlert(role: Role, problem: NetplayView["pasteProblem"]): HTMLElement | null {
    if (problem === "not-neser") {
        const from = role === "join" ? "the host's window" : "the other player's window";
        return alert("err", "That isn't a NESER code.", `Copy the whole code from ${from} and paste it again.`);
    }
    if (problem === "wrong-kind") {
        return role === "join"
            ? alert("err", "This is a reply code, not a host's code.", "Ask the host for the code from their window.")
            : alert("err", "This is a host's code, not the other player's reply.", "Ask the other player for the code from their window.");
    }
    return null;
}

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
    /** Disconnect on the connected step. */
    onDisconnect?: () => void;
}) {
    let isOpen = false;
    /** Which screen is drawn; a change of it redraws the window, anything else updates it in place. */
    let screenKey = "";
    let update: (view: NetplayView) => void = () => {};
    /** The text last pasted, shown again while it is being connected or after it failed. */
    let lastPaste = "";
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
        } else if (view.phase === "connected" && waiting()) {
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = "wait";
            openButton.replaceChildren(dot("wait"), "Waiting…");
        } else if (view.phase === "connected" && view.rtt !== null) {
            const tone = responseTimeRating(view.rtt).tone;
            openButton.removeAttribute("data-icon");
            openButton.dataset.tone = tone;
            openButton.replaceChildren(dot(tone), el("span", "netplay-chip-long", "Connected · "), `${view.rtt} ms`);
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
            () => session.join()
        );
        body.append(el("p", "netplay-lede", "Two browsers on the same network, one game, two pads."), host, join);
        (hostable ? host : join).focus();
    }

    /** A code to copy, with Copy code turning to "✓ Copied" for a moment. */
    function codeBox(view: NetplayView) {
        const box = el("div", "netplay-code");
        box.tabIndex = 0;
        box.setAttribute("aria-label", "Your code");
        let copiedTimer: ReturnType<typeof setTimeout> | undefined;
        const copy = btn("Copy code", async () => {
            const code = session.view().ownCode;
            if (!code) return;
            if (!(await copyText(code))) return;
            copy.textContent = "✓ Copied";
            clearTimeout(copiedTimer);
            copiedTimer = setTimeout(() => (copy.textContent = "Copy code"), COPIED_FOR_MS);
        });
        const fill = (v: NetplayView) => {
            box.textContent = v.ownCode ?? "…";
            copy.disabled = v.ownCode === null;
        };
        fill(view);
        return { box, copy, fill };
    }

    function pasteBox(role: Role, placeholder: string, view: NetplayView) {
        const box = el("textarea", "netplay-paste");
        box.value = view.phase === "idle" ? "" : lastPaste;
        // Once connecting or failed, the code shown is the one that was tried; Start again takes a new one.
        box.readOnly = view.phase !== "idle";
        box.placeholder = placeholder;
        box.rows = 4;
        box.spellcheck = false;
        box.setAttribute("aria-label", placeholder);
        box.addEventListener("input", () => {
            lastPaste = box.value;
            void session.paste(box.value);
        });
        const problem = el("div");
        const fill = (v: NetplayView) => problem.replaceChildren(...[pasteAlert(role, v.pasteProblem)].filter((n) => n !== null));
        return { box, problem, fill };
    }

    function drawHost(view: NetplayView) {
        if (view.step === 1) {
            const code = codeBox(view);
            body.append(steps("host", 1), el("p", "", "Send this code to the other player — by chat, email, anything."), code.box);
            if (view.phase === "failed") {
                // The browser could not make a connection to offer at all.
                body.append(failureAlert(view), actions(btn("Close", close), btn("Start again", () => session.startAgain(), true)));
                body.querySelector<HTMLButtonElement>(".netplay-actions button")?.focus();
                return;
            }
            const next = btn("Next", () => session.next(), true);
            body.append(actions(btn("Cancel", close), code.copy, next));
            update = (v) => {
                code.fill(v);
                next.disabled = v.ownCode === null;
            };
            update(view);
            code.copy.focus();
            return;
        }
        if (view.phase === "failed" && view.failure === "too-slow") return drawRefused(view);
        const paste = pasteBox("host", "Paste their code here", view);
        body.append(steps("host", 2), el("p", "", "Paste the code the other player sends back."), paste.box, paste.problem);
        if (view.phase === "connecting") {
            body.append(el("p", "netplay-muted", "⟳ Connecting…"), actions(btn("Cancel", close)));
            return;
        }
        if (view.phase === "failed") {
            body.append(failureAlert(view));
            body.append(
                view.failure === "other-build"
                    ? actions(btn("Close", close))
                    : actions(btn("Close", close), btn("Start again", () => session.startAgain(), true))
            );
            return;
        }
        const connect = btn("Connect", () => session.connect(), true);
        body.append(actions(btn("Back", () => session.back()), connect));
        update = (v) => {
            paste.fill(v);
            connect.disabled = !v.pasteReady;
        };
        update(view);
        paste.box.focus();
    }

    function drawJoin(view: NetplayView) {
        if (view.phase === "failed" && view.failure === "too-slow") return drawRefused(view);
        if (view.step === 1) {
            const paste = pasteBox("join", "Paste the host's code here", view);
            body.append(steps("join", 1), el("p", "", "Paste the code the host sent you."), paste.box, paste.problem);
            if (view.phase === "failed") {
                body.append(failureAlert(view), actions(btn("Close", close)));
                return;
            }
            const own = currentGame();
            if (own) body.append(alert("warn", "", `${own.title} will be closed when the host's game arrives. Your saves are kept.`));
            const next = btn("Next", () => session.next(), true);
            body.append(actions(btn("Cancel", close), next));
            update = (v) => {
                paste.fill(v);
                next.disabled = !v.pasteReady;
            };
            update(view);
            paste.box.focus();
            return;
        }
        const code = codeBox(view);
        body.append(steps("join", 2), el("p", "", "Send this code back to the host."), code.box);
        if (view.phase === "failed") {
            body.append(failureAlert(view), actions(btn("Close", close), btn("Start again", () => session.startAgain(), true)));
            return;
        }
        body.append(el("p", "netplay-muted", "⟳ Waiting for the host to paste your code…"), actions(btn("Cancel", close), code.copy));
        update = code.fill;
        code.copy.focus();
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
        return alert(
            "err",
            "Couldn't reach the other player.",
            "Network play needs both computers on the same network (or the same VPN). Check that, then start again."
        );
    }

    function drawRefused(view: NetplayView) {
        const status = el("p", "netplay-status");
        status.append(dot("err"), " ", el("b", "", "Too slow to play"));
        body.append(status);
        if (view.rtt !== null) body.append(pill(view.rtt));
        body.append(
            alert(
                "err",
                "",
                "The connection between the two browsers is too slow for the game to keep up. Try both being on the same network, or closer to the router."
            ),
            actions(btn("Close", close), btn("Try again", () => session.startAgain(), true))
        );
        body.querySelector<HTMLButtonElement>(".netplay-actions button")?.focus();
    }

    function drawConnected(view: NetplayView) {
        const role = view.role!;
        body.append(steps(role, 3));
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
            if (rating.tone === "warn") {
                live.append(alert("warn", "", "Your button presses will take a moment longer to show up. Fine for most games, harder for fast ones."));
            } else if (rating.tone === "err") {
                live.append(
                    alert(
                        "err",
                        "",
                        "The connection has become too slow to play. It may recover by itself — if not, try both being on the same network, or closer to the router."
                    )
                );
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
        const key = `${view.role}|${view.step}|${view.phase}|${view.failure}|${view.sending?.direction ?? ""}|${view.notice}`;
        if (key === screenKey) return update(view);
        screenKey = key;
        update = () => {};
        body.replaceChildren();
        title.textContent = view.role === "host" ? "Host this game" : view.role === "join" ? "Join a game" : "Play together";
        if (view.role === null) drawChoice();
        else if (view.step === 3) drawConnected(view);
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
