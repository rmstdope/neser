import type { PauseReason, PlayView } from "../netplay/play";

/**
 * The screens over the game while playing together (nr-ayk5.5,
 * docs/ui/nr-ayk5.5-playing-together.html): paused, waiting for the other player, catching up and
 * ended, each dimming the picture with its heading, its line and its buttons; and the one question
 * before ending, "End playing together?". It draws the play controller's view and nothing else;
 * the 3-2-1 counts are `countdown.ts`.
 */

export interface PlayScreen {
    draw(view: PlayView): void;
    /** Asks before ending playing together; End playing together calls `onEnd`. */
    askToEnd(): void;
    remove(): void;
}

const OTHER_IS_IN: Record<PauseReason, string> = {
    pause: "The other player paused the game.",
    settings: "The other player is in Settings.",
    keys: "The other player is in Keys and controls.",
    together: "The other player is in the Play together window.",
};

const ENDED_BY = { me: "You ended it.", them: "The other player ended it.", left: "The other player left." } as const;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text) node.textContent = text;
    return node;
}

function btn(label: string, onClick: () => void, tone: "primary" | "plain" | "danger" = "plain"): HTMLButtonElement {
    const classes = { primary: "btn btn-sm btn-primary", plain: "btn btn-sm", danger: "btn btn-sm btn-error" };
    const button = el("button", classes[tone], label);
    button.type = "button";
    button.addEventListener("click", onClick);
    return button;
}

export function createPlayScreen({
    screen,
    role,
    onResume,
    onEnd,
    onBackToLibrary,
}: {
    screen: HTMLElement;
    role: "host" | "join";
    /** Resume: both games, while paused; on its own, once ended (host). */
    onResume: () => void;
    onEnd: () => void;
    onBackToLibrary: () => void;
}): PlayScreen {
    let overlay: HTMLElement | null = null;
    /** What is drawn, so a view that changes nothing on the screen leaves focus where it is. */
    let drawnKey = "";

    const dialog = el("dialog", "netplay-sheet netplay-confirm");
    dialog.setAttribute("aria-labelledby", "netplay-confirm-title");
    const title = el("h2", "", "End playing together?");
    title.id = "netplay-confirm-title";
    const keep = btn("Keep playing", () => closeQuestion());
    const end = btn(
        "End playing together",
        () => {
            closeQuestion();
            onEnd();
        },
        "danger"
    );
    const choices = el("div", "netplay-actions");
    choices.append(keep, end);
    dialog.append(title, el("p", "netplay-muted", "The game stops for both of you. You'll need new codes to play together again."), choices);
    // Esc closes the question as Keep playing does.
    dialog.addEventListener("cancel", (event) => {
        event.preventDefault();
        closeQuestion();
    });
    document.body.append(dialog);

    function closeQuestion() {
        if (typeof dialog.close === "function" && dialog.open) dialog.close();
        dialog.removeAttribute("open");
    }

    function askToEnd() {
        if (typeof dialog.showModal === "function") dialog.showModal();
        else dialog.setAttribute("open", "");
        keep.focus();
    }

    function show(key: string, heading: string, lines: string[], actions: HTMLButtonElement[], focus: HTMLButtonElement | null) {
        if (key === drawnKey) return;
        drawnKey = key;
        overlay?.remove();
        overlay = el("div", "netplay-screen");
        overlay.setAttribute("role", "group");
        overlay.setAttribute("aria-label", heading);
        overlay.append(el("h3", "", heading), ...lines.map((line) => el("p", "", line)));
        const row = el("div", "netplay-actions");
        row.append(...actions);
        overlay.append(row);
        screen.append(overlay);
        focus?.focus();
    }

    function clear() {
        drawnKey = "";
        overlay?.remove();
        overlay = null;
    }

    const endButton = () => btn("End playing together", askToEnd);

    function draw(view: PlayView) {
        if (view.phase === "ended" && view.ended) {
            closeQuestion();
            const action = role === "host" ? btn("Resume", onResume, "primary") : btn("Back to library", onBackToLibrary, "primary");
            const next = role === "host" ? "You can keep playing on your own from here." : "The host's game isn't kept on this computer.";
            return show(`ended|${view.ended}`, "Playing together has ended", [ENDED_BY[view.ended], next], [action], action);
        }
        if (view.phase === "paused" && view.pausedBy && view.pauseReason) {
            const line = view.pausedBy === "me" ? "You paused the game." : OTHER_IS_IN[view.pauseReason];
            const resume = btn("Resume", onResume, "primary");
            return show(`paused|${line}`, "Paused", [line], [endButton(), resume], resume);
        }
        if (view.phase === "playing" && view.waiting) {
            return show(
                "waiting",
                "⟳ Waiting for the other player…",
                ["The game carries on as soon as their game catches up."],
                [endButton()],
                null
            );
        }
        if (view.phase === "playing" && view.catchingUp) {
            return show("catching-up", "⟳ Catching up…", ["Your game is catching up with the other player's."], [], null);
        }
        clear();
    }

    return {
        draw,
        askToEnd,
        remove: () => {
            clear();
            closeQuestion();
            dialog.remove();
        },
    };
}
