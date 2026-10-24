/**
 * @vitest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { runCountdown } from "./countdown";

let screen: HTMLElement;

beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = `<div id="screen-wrap"><canvas></canvas></div>`;
    screen = document.getElementById("screen-wrap")!;
});
afterEach(() => vi.useRealTimers());

const overlay = () => screen.querySelector<HTMLElement>(".netplay-countdown");
const number = () => overlay()?.querySelector(".netplay-countdown-number")?.textContent;

describe("the count before playing together", () => {
    it.each([1, 2] as const)("tells player %i who they are above 3, 2, 1, one second each, then lets the game run", (player) => {
        const onDone = vi.fn();
        runCountdown({ screen, player, onDone });
        expect(overlay()!.querySelector(".netplay-countdown-player")!.textContent).toBe(`You are player ${player}`);
        expect(number()).toBe("3");
        vi.advanceTimersByTime(999);
        expect(number()).toBe("3");
        vi.advanceTimersByTime(1);
        expect(number()).toBe("2");
        vi.advanceTimersByTime(1000);
        expect(number()).toBe("1");
        expect(onDone).not.toHaveBeenCalled();
        vi.advanceTimersByTime(1000);
        expect(overlay()).toBeNull();
        expect(onDone).toHaveBeenCalledOnce();
    });

    it("says nothing after 1", () => {
        runCountdown({ screen, player: 1, onDone: () => {} });
        vi.advanceTimersByTime(3000);
        expect(screen.textContent).toBe("");
    });

    it("is announced to a screen reader", () => {
        runCountdown({ screen, player: 2, onDone: () => {} });
        expect(overlay()!.getAttribute("role")).toBe("status");
    });
});

describe("a count that is called off", () => {
    it("goes at once, without letting the game run", () => {
        const onDone = vi.fn();
        const cancel = runCountdown({ screen, player: 2, onDone });
        vi.advanceTimersByTime(1500);
        cancel();
        expect(overlay()).toBeNull();
        vi.advanceTimersByTime(5000);
        expect(onDone).not.toHaveBeenCalled();
    });
});
