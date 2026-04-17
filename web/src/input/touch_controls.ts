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

type StickOffset = {
    x: number;
    y: number;
};

type ResolvedTouchState = {
    captured: boolean;
    buttons: number[];
    visualRoot: Element | null;
    stickOffset: StickOffset | null;
};

type ActiveTouchState = {
    buttons: number[];
    visualRoot: Element | null;
    stickOffset: StickOffset | null;
};

const JOYSTICK_DEADZONE_RATIO = 0.22;
const JOYSTICK_KNOB_OFFSET_RATIO = 0.45;

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

function sameButtons(left: number[], right: number[]): boolean {
    return left.length === right.length && left.every((button, index) => button === right[index]);
}

function sameStickOffset(left: StickOffset | null, right: StickOffset | null): boolean {
    return left?.x === right?.x && left?.y === right?.y;
}

function emitReleasedButtons(callback: ButtonChangeCallback, currentButtons: number[], nextButtons: number[]) {
    const next = new Set(nextButtons);
    for (const button of currentButtons) {
        if (!next.has(button)) {
            callback(button, false);
        }
    }
}

function emitPressedButtons(callback: ButtonChangeCallback, currentButtons: number[], nextButtons: number[]) {
    const current = new Set(currentButtons);
    for (const button of nextButtons) {
        if (!current.has(button)) {
            callback(button, true);
        }
    }
}

function collectPressedButtons(states: Iterable<ActiveTouchState>): number[] {
    const buttons: number[] = [];

    for (const state of states) {
        for (const button of state.buttons) {
            if (!buttons.includes(button)) {
                buttons.push(button);
            }
        }
    }

    return buttons;
}

function findTouchBindingElement(element: Element | null): Element | null {
    return element?.closest("[data-button], [data-touch-zone]") ?? null;
}

function togglePressedClass(element: Element | null, pressed: boolean) {
    element?.classList.toggle("pressed", pressed);
}

function applyVisualState(element: Element, buttons: number[], stickOffset: StickOffset | null) {
    const zone = element.getAttribute("data-touch-zone")?.toLowerCase();

    switch (zone) {
    case "joystick": {
        togglePressedClass(element, buttons.length > 0);
        if (element instanceof HTMLElement) {
            element.style.setProperty("--touch-stick-x", `${Math.round(stickOffset?.x ?? 0)}px`);
            element.style.setProperty("--touch-stick-y", `${Math.round(stickOffset?.y ?? 0)}px`);
        }
        return;
    }
    default:
        if (resolveButtonFromElement(element) !== undefined) {
            togglePressedClass(element, buttons.length > 0);
        }
    }
}

function resolveJoystickState(element: Element, touch: Touch): { buttons: number[]; stickOffset: StickOffset } {
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
        return { buttons: [], stickOffset: { x: 0, y: 0 } };
    }

    const centerX = rect.left + (rect.width / 2);
    const centerY = rect.top + (rect.height / 2);
    const distanceX = touch.clientX - centerX;
    const distanceY = touch.clientY - centerY;
    const radius = Math.min(rect.width, rect.height) / 2;
    const distance = Math.hypot(distanceX, distanceY);
    const clampedDistance = Math.min(distance, radius);
    const offsetScale = distance === 0 ? 0 : (clampedDistance / distance) * (JOYSTICK_KNOB_OFFSET_RATIO);
    const stickOffset = {
        x: distanceX * offsetScale,
        y: distanceY * offsetScale,
    };

    if (distance <= radius * JOYSTICK_DEADZONE_RATIO) {
        return { buttons: [], stickOffset: { x: 0, y: 0 } };
    }

    const sector = ((Math.round(Math.atan2(distanceY, distanceX) / (Math.PI / 4)) % 8) + 8) % 8;

    switch (sector) {
    case 0:
        return { buttons: [NES_BUTTON.RIGHT], stickOffset };
    case 1:
        return { buttons: [NES_BUTTON.DOWN, NES_BUTTON.RIGHT], stickOffset };
    case 2:
        return { buttons: [NES_BUTTON.DOWN], stickOffset };
    case 3:
        return { buttons: [NES_BUTTON.DOWN, NES_BUTTON.LEFT], stickOffset };
    case 4:
        return { buttons: [NES_BUTTON.LEFT], stickOffset };
    case 5:
        return { buttons: [NES_BUTTON.UP, NES_BUTTON.LEFT], stickOffset };
    case 6:
        return { buttons: [NES_BUTTON.UP], stickOffset };
    case 7:
        return { buttons: [NES_BUTTON.UP, NES_BUTTON.RIGHT], stickOffset };
    default:
        return { buttons: [], stickOffset };
    }
}

function resolveTouchState(touch: Touch): ResolvedTouchState {
    const element = findTouchBindingElement(touch.target as Element | null);

    const button = resolveButtonFromElement(element);
    if (button !== undefined) {
        return {
            captured: true,
            buttons: [button],
            visualRoot: element,
            stickOffset: null,
        };
    }

    const zone = element?.getAttribute("data-touch-zone")?.toLowerCase();
    switch (zone) {
    case "joystick": {
        const joystickState = resolveJoystickState(element, touch);
        return {
            captured: true,
            buttons: joystickState.buttons,
            visualRoot: element,
            stickOffset: joystickState.stickOffset,
        };
    }
    default:
        return {
            captured: false,
            buttons: [],
            visualRoot: null,
            stickOffset: null,
        };
    }
}

/**
 * Manages active touch state and fires callbacks on button changes.
 */
export class TouchInputManager {
    private activeTouches: Map<number, ActiveTouchState> = new Map();
    private callback: ButtonChangeCallback;

    constructor(callback: ButtonChangeCallback) {
        this.callback = callback;
    }

    private emitGlobalButtonChanges(previousButtons: number[]) {
        const nextButtons = collectPressedButtons(this.activeTouches.values());
        emitReleasedButtons(this.callback, previousButtons, nextButtons);
        emitPressedButtons(this.callback, previousButtons, nextButtons);
    }

    private refreshVisualState(changedRoots: Array<Element | null>) {
        const roots = new Set<Element>();
        for (const root of changedRoots) {
            if (root) {
                roots.add(root);
            }
        }
        for (const state of this.activeTouches.values()) {
            if (state.visualRoot) {
                roots.add(state.visualRoot);
            }
        }

        for (const root of roots) {
            const buttons: number[] = [];
            let stickOffset: StickOffset | null = null;

            for (const state of this.activeTouches.values()) {
                if (state.visualRoot !== root) continue;

                for (const button of state.buttons) {
                    if (!buttons.includes(button)) {
                        buttons.push(button);
                    }
                }

                if (state.stickOffset) {
                    stickOffset = state.stickOffset;
                }
            }

            applyVisualState(root, buttons, stickOffset);
        }
    }

    /** Handle a new touch starting on a button element. */
    handleTouchStart(touch: Touch): void {
        const resolved = resolveTouchState(touch);
        if (!resolved.captured) return;

        const previousButtons = collectPressedButtons(this.activeTouches.values());

        this.activeTouches.set(touch.identifier, {
            buttons: resolved.buttons,
            visualRoot: resolved.visualRoot,
            stickOffset: resolved.stickOffset,
        });
        this.emitGlobalButtonChanges(previousButtons);
        this.refreshVisualState([resolved.visualRoot]);
    }

    /** Handle a touch moving (possibly sliding between buttons). */
    handleTouchMove(touch: Touch): void {
        const currentState = this.activeTouches.get(touch.identifier);
        const resolved = resolveTouchState(touch);

        const previousButtons = collectPressedButtons(this.activeTouches.values());

        if (currentState === undefined) {
            if (!resolved.captured) return;

            this.activeTouches.set(touch.identifier, {
                buttons: resolved.buttons,
                visualRoot: resolved.visualRoot,
                stickOffset: resolved.stickOffset,
            });
            this.emitGlobalButtonChanges(previousButtons);
            this.refreshVisualState([resolved.visualRoot]);
            return;
        }

        if (!resolved.captured) {
            this.activeTouches.delete(touch.identifier);
            this.emitGlobalButtonChanges(previousButtons);
            this.refreshVisualState([currentState.visualRoot]);
            return;
        }

        const buttonsChanged = !sameButtons(currentState.buttons, resolved.buttons);
        const visualRootChanged = currentState.visualRoot !== resolved.visualRoot;
        const stickOffsetChanged = !sameStickOffset(currentState.stickOffset, resolved.stickOffset);

        if (!buttonsChanged && !visualRootChanged && !stickOffsetChanged) return;

        this.activeTouches.set(touch.identifier, {
            buttons: resolved.buttons,
            visualRoot: resolved.visualRoot,
            stickOffset: resolved.stickOffset,
        });

        if (buttonsChanged) {
            this.emitGlobalButtonChanges(previousButtons);
        }

        this.refreshVisualState([currentState.visualRoot, resolved.visualRoot]);
    }

    /** Handle a touch ending. */
    handleTouchEnd(touch: Touch): void {
        const state = this.activeTouches.get(touch.identifier);
        if (state === undefined) return;

        const previousButtons = collectPressedButtons(this.activeTouches.values());

        this.activeTouches.delete(touch.identifier);
        this.emitGlobalButtonChanges(previousButtons);
        this.refreshVisualState([state.visualRoot]);
    }

    /** Handle a touch being cancelled. */
    handleTouchCancel(touch: Touch): void {
        this.handleTouchEnd(touch);
    }

    /** Get the currently pressed buttons for a touch identifier, if any. */
    getButtonsForTouch(identifier: number): number[] {
        return [...(this.activeTouches.get(identifier)?.buttons ?? [])];
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
