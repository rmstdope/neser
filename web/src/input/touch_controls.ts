/** NES button indices matching the keyboard mapping convention. */
export const NES_BUTTON = {
    A: 0,
    B: 1,
    SELECT: 2,
    START: 3,
    UP: 4,
    DOWN: 5,
    LEFT: 6,
    RIGHT: 7,
} as const;

const BUTTON_NAME_MAP: Record<string, number> = {
    a: NES_BUTTON.A,
    b: NES_BUTTON.B,
    select: NES_BUTTON.SELECT,
    start: NES_BUTTON.START,
    up: NES_BUTTON.UP,
    down: NES_BUTTON.DOWN,
    left: NES_BUTTON.LEFT,
    right: NES_BUTTON.RIGHT,
};

export type ButtonChangeCallback = (button: number, pressed: boolean) => void;

/**
 * Detect whether the current device supports touch input.
 */
export function isTouchDevice(): boolean {
    return (
        "ontouchstart" in window ||
        navigator.maxTouchPoints > 0
    );
}

/**
 * Detect whether the current device is a small-screen handheld (phone).
 * Uses `(pointer: coarse)` combined with a max-dimension check so that
 * both portrait and landscape orientations are correctly detected, and
 * large tablets / desktop touch screens are excluded.
 */
export function isHandheldDevice(): boolean {
    if (typeof window.matchMedia !== "function") return false;
    return (
        window.matchMedia("(pointer: coarse)").matches &&
        Math.min(window.innerWidth, window.innerHeight) <= 768
    );
}

/**
 * Resolve which NES button a DOM element represents, based on its
 * `data-button` attribute.  Returns `undefined` if the element is not
 * a touch-control button.
 */
export function resolveButtonFromElement(element: Element | null): number | undefined {
    if (!element) return undefined;
    const name = element.getAttribute("data-button");
    if (!name) return undefined;
    return BUTTON_NAME_MAP[name.toLowerCase()];
}

/**
 * Manages active touch state and fires callbacks on button changes.
 */
export class TouchInputManager {
    private activeTouches: Map<number, number> = new Map();
    private callback: ButtonChangeCallback;

    constructor(callback: ButtonChangeCallback) {
        this.callback = callback;
    }

    /** Handle a new touch starting on a button element. */
    handleTouchStart(touch: Touch): void {
        const button = resolveButtonFromElement(touch.target as Element);
        if (button === undefined) return;
        this.activeTouches.set(touch.identifier, button);
        this.callback(button, true);
    }

    /** Handle a touch moving (possibly sliding between buttons). */
    handleTouchMove(touch: Touch): void {
        const currentButton = this.activeTouches.get(touch.identifier);
        const newButton = resolveButtonFromElement(touch.target as Element);

        if (currentButton !== undefined && newButton !== currentButton) {
            // Finger slid off the current button
            this.activeTouches.delete(touch.identifier);
            this.callback(currentButton, false);
        }

        if (newButton !== undefined && newButton !== currentButton) {
            // Finger slid onto a new button
            this.activeTouches.set(touch.identifier, newButton);
            this.callback(newButton, true);
        }
    }

    /** Handle a touch ending. */
    handleTouchEnd(touch: Touch): void {
        const button = this.activeTouches.get(touch.identifier);
        if (button === undefined) return;
        this.activeTouches.delete(touch.identifier);
        this.callback(button, false);
    }

    /** Handle a touch being cancelled. */
    handleTouchCancel(touch: Touch): void {
        this.handleTouchEnd(touch);
    }

    /** Get the currently pressed button for a touch identifier, if any. */
    getButtonForTouch(identifier: number): number | undefined {
        return this.activeTouches.get(identifier);
    }

    /** Get count of active touches. */
    get activeCount(): number {
        return this.activeTouches.size;
    }
}

/**
 * Initialise touch control event listeners on the given container element.
 * Returns a cleanup function that removes all listeners.
 */
export function initTouchControls(
    container: Element,
    callback: ButtonChangeCallback,
): () => void {
    const manager = new TouchInputManager(callback);

    function forEachChanged(e: Event, handler: (t: Touch) => void) {
        e.preventDefault();
        const te = e as TouchEvent;
        for (const touch of Array.from(te.changedTouches)) {
            handler(touch);
        }
    }

    function onTouchStart(e: Event) { forEachChanged(e, (t) => manager.handleTouchStart(t)); }
    function onTouchMove(e: Event) { forEachChanged(e, (t) => manager.handleTouchMove(t)); }
    function onTouchEnd(e: Event) { forEachChanged(e, (t) => manager.handleTouchEnd(t)); }
    function onTouchCancel(e: Event) { forEachChanged(e, (t) => manager.handleTouchCancel(t)); }

    container.addEventListener("touchstart", onTouchStart, { passive: false });
    container.addEventListener("touchmove", onTouchMove, { passive: false });
    container.addEventListener("touchend", onTouchEnd, { passive: false });
    container.addEventListener("touchcancel", onTouchCancel, { passive: false });

    return () => {
        container.removeEventListener("touchstart", onTouchStart);
        container.removeEventListener("touchmove", onTouchMove);
        container.removeEventListener("touchend", onTouchEnd);
        container.removeEventListener("touchcancel", onTouchCancel);
    };
}
