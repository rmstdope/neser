/**
 * @vitest-environment jsdom
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PlayView } from "../netplay/play";
import { createPlayScreen, type PlayScreen } from "./play_screen";

const PLAYING: PlayView = {
    phase: "playing",
    pausedBy: null,
    pauseReason: null,
    count: null,
    waiting: false,
    catchingUp: false,
    ended: null,
};

let screen: HTMLElement;
let onResume: ReturnType<typeof vi.fn<() => void>>;
let onEnd: ReturnType<typeof vi.fn<() => void>>;
let onBackToLibrary: ReturnType<typeof vi.fn<() => void>>;

function make(role: "host" | "join"): PlayScreen {
    return createPlayScreen({ screen, role, onResume, onEnd, onBackToLibrary });
}
const overlay = () => screen.querySelector<HTMLElement>(".netplay-screen");
const heading = () => overlay()?.querySelector("h3")?.textContent;
const lines = () => [...(overlay()?.querySelectorAll("p") ?? [])].map((p) => p.textContent);
const buttons = () => [...(overlay()?.querySelectorAll("button") ?? [])].map((b) => b.textContent);
const button = (name: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name)!;
const confirm = () => document.querySelector<HTMLDialogElement>(".netplay-confirm")!;

beforeEach(() => {
    document.body.innerHTML = `<div id="screen-wrap"><canvas></canvas></div>`;
    screen = document.getElementById("screen-wrap")!;
    onResume = vi.fn();
    onEnd = vi.fn();
    onBackToLibrary = vi.fn();
});

describe("the screens over the game while playing together", () => {
    it("shows nothing while playing or counting", () => {
        const s = make("host");
        s.draw(PLAYING);
        expect(overlay()).toBeNull();
        s.draw({ ...PLAYING, phase: "counting", count: "resume" });
        expect(overlay()).toBeNull();
    });

    it.each([
        ["me", "pause", "You paused the game."],
        ["me", "settings", "You paused the game."],
        ["them", "pause", "The other player paused the game."],
        ["them", "settings", "The other player is in Settings."],
        ["them", "keys", "The other player is in Keys and controls."],
        ["them", "together", "The other player is in the Play together window."],
    ] as const)("pauses, by %s for %s: %s", (pausedBy, pauseReason, line) => {
        make("join").draw({ ...PLAYING, phase: "paused", pausedBy, pauseReason });
        expect(heading()).toBe("Paused");
        expect(lines()).toEqual([line]);
        expect(buttons()).toEqual(["End playing together", "Resume"]);
        expect(document.activeElement).toBe(button("Resume"));
    });

    it("resumes from its Resume button", () => {
        make("host").draw({ ...PLAYING, phase: "paused", pausedBy: "them", pauseReason: "pause" });
        button("Resume").click();
        expect(onResume).toHaveBeenCalledOnce();
    });

    it("says it is waiting for the other player, with End playing together at once", () => {
        const s = make("host");
        s.draw({ ...PLAYING, waiting: true });
        expect(heading()).toBe("⟳ Waiting for the other player…");
        expect(lines()).toEqual(["The game carries on as soon as their game catches up."]);
        expect(buttons()).toEqual(["End playing together"]);
        s.draw(PLAYING);
        expect(overlay()).toBeNull();
    });

    it("says it is catching up, with no buttons", () => {
        make("join").draw({ ...PLAYING, catchingUp: true });
        expect(heading()).toBe("⟳ Catching up…");
        expect(lines()).toEqual(["Your game is catching up with the other player's."]);
        expect(buttons()).toEqual([]);
    });

    it.each([
        ["me", "You ended it."],
        ["them", "The other player ended it."],
        ["left", "The other player left."],
    ] as const)("tells the host it has ended (%s) and offers to play on alone", (ended, first) => {
        make("host").draw({ ...PLAYING, phase: "ended", ended });
        expect(heading()).toBe("Playing together has ended");
        expect(lines()).toEqual([first, "You can keep playing on your own from here."]);
        expect(buttons()).toEqual(["Resume"]);
        expect(document.activeElement).toBe(button("Resume"));
        button("Resume").click();
        expect(onResume).toHaveBeenCalledOnce();
    });

    it("tells the joiner the host's game is not kept, and offers the library", () => {
        make("join").draw({ ...PLAYING, phase: "ended", ended: "left" });
        expect(lines()).toEqual(["The other player left.", "The host's game isn't kept on this computer."]);
        expect(buttons()).toEqual(["Back to library"]);
        expect(document.activeElement).toBe(button("Back to library"));
        button("Back to library").click();
        expect(onBackToLibrary).toHaveBeenCalledOnce();
    });

    it("goes when it is removed", () => {
        const s = make("host");
        s.draw({ ...PLAYING, waiting: true });
        s.remove();
        expect(overlay()).toBeNull();
    });
});

describe("ending playing together", () => {
    it("asks once from End playing together, with focus on Keep playing", () => {
        make("host").draw({ ...PLAYING, waiting: true });
        button("End playing together").click();
        expect(confirm().open).toBe(true);
        expect(confirm().querySelector("h2")!.textContent).toBe("End playing together?");
        expect(confirm().querySelector("p")!.textContent).toBe("The game stops for both of you. You'll need new codes to play together again.");
        const choices = [...confirm().querySelectorAll("button")].map((b) => b.textContent);
        expect(choices).toEqual(["Keep playing", "End playing together"]);
        expect(document.activeElement!.textContent).toBe("Keep playing");
    });

    it("keeps playing from Keep playing, and ends from End playing together", () => {
        const s = make("join");
        s.askToEnd();
        [...confirm().querySelectorAll("button")].find((b) => b.textContent === "Keep playing")!.click();
        expect(confirm().open).toBe(false);
        expect(onEnd).not.toHaveBeenCalled();
        s.askToEnd();
        [...confirm().querySelectorAll("button")].find((b) => b.textContent === "End playing together")!.click();
        expect(confirm().open).toBe(false);
        expect(onEnd).toHaveBeenCalledOnce();
    });

    it("keeps playing on Esc", () => {
        const s = make("host");
        s.askToEnd();
        confirm().dispatchEvent(new Event("cancel", { cancelable: true }));
        expect(confirm().open).toBe(false);
        expect(onEnd).not.toHaveBeenCalled();
    });

    it("stops asking when playing together ends under the question", () => {
        const s = make("host");
        s.askToEnd();
        s.draw({ ...PLAYING, phase: "ended", ended: "them" });
        expect(confirm().open).toBe(false);
    });
});
