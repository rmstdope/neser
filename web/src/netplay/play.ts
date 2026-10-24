import type { PlayData, PlayMessage } from "./session";

/**
 * Playing together (nr-ayk5.5, docs/ui/nr-ayk5.5-playing-together.html), without the page: one
 * game on two computers, the host's pad on port 1 and the joiner's on port 2 of both. Each side
 * owns one of these. A frame runs only once the lockstep rules (Rust, `WasmLockstep`) give both
 * pads for it, so the two games run the same frames on the same pads.
 *
 * Pause and reset ride inside the pad, in bits 12-15 (no NES or SNES button uses them), so both
 * games act on them after the same frame: a pause stops both after frame F, a reset resets both
 * after it. A command waits in the pad until a frame carries it, and the frame runs with those
 * bits masked off. Once both are stopped at the same frame, resuming, and resetting while paused,
 * are plain messages over the link, each naming the frame it is for: the other game may still be
 * a few frames from it, and then holds the message until it gets there. Presses of the same kind
 * that land within the input delay of each other were made before either side saw the first, so
 * they are one press. Nothing here touches the page: the core, the link, the count
 * and the clock are handed in, and `view()` is what the screens draw.
 */

/** The lockstep rules as `WasmLockstep` offers them. */
export interface LockstepRules {
    /** `[frame, pad1, pad2]` for the frame to run now, or empty while it holds. */
    advance(localPad: number): ArrayLike<number>;
    packet(): Uint8Array;
    receive(bytes: Uint8Array): boolean;
    next_frame(): number;
    remote_frames_known(): number;
    input_delay(): number;
}

/** The running game: plays one frame with both pads, or resets. */
export interface PlayCore {
    runFrame(pad1: number, pad2: number): void;
    reset(soft: boolean): void;
}

export interface PlayLink {
    sendPacket(bytes: Uint8Array): void;
    sendPlay(message: PlayMessage): void;
}

/** A window that pauses both games while it is open. */
export type PlayWindow = "settings" | "keys" | "together";
export type PauseReason = "pause" | PlayWindow;
/** What the count says above its numbers. */
export type CountLabel = "resume" | "reset-me" | "reset-them";
/** How playing together ended: this player ended it, the other did, or the other left. */
export type EndedHow = "me" | "them" | "left";

export interface PlayView {
    phase: "starting" | "playing" | "paused" | "counting" | "ended";
    pausedBy: "me" | "them" | null;
    pauseReason: PauseReason | null;
    count: CountLabel | null;
    /** The other player's game has held this one for half a second. */
    waiting: boolean;
    /** This game has been behind the other player's for half a second. */
    catchingUp: boolean;
    ended: EndedHow | null;
}

export interface Play {
    view(): PlayView;
    subscribe(listener: (view: PlayView) => void): void;
    /** The starting count is over: frames may run. */
    begin(): void;
    /** A local button: every keyboard, gamepad and touch control on this computer drives this player. */
    press(controller: number, button: number, pressed: boolean): void;
    /** One animation frame: runs what frames it may and sends the other side a packet. Returns how many ran. */
    tick(): number;
    /** Pause while playing, resume while paused (the Pause button and Space). */
    pause(): void;
    resume(): void;
    reset(soft: boolean): void;
    windowOpened(window: PlayWindow): void;
    windowClosed(window: PlayWindow): void;
    receive(bytes: Uint8Array): void;
    message(message: PlayMessage): void;
    /** Hands this side what the other sent; the session's `onPlay` listener. */
    hear(data: PlayData): void;
    ended(how: EndedHow): void;
}

/** How long a held or lagging game goes before the player is told. */
export const NOTICE_AFTER_MS = 500;
/** The most frames a game that is behind runs in one tick, to catch up. */
const CATCH_UP_FRAMES = 2;

const COMMAND_SHIFT = 12;
const BUTTONS = (1 << COMMAND_SHIFT) - 1;
const COMMANDS = {
    pause: 1,
    settings: 2,
    keys: 3,
    together: 4,
    reset: 5,
    hardReset: 6,
} as const;
const PAUSES: Record<number, PauseReason> = { 1: "pause", 2: "settings", 3: "keys", 4: "together" };

export function createPlay({
    player,
    lockstep,
    link,
    core,
    count,
    now = () => performance.now(),
}: {
    player: 1 | 2;
    lockstep: LockstepRules;
    link: PlayLink;
    core: PlayCore;
    /** Shows a 3-2-1 count with `label` above it, calling `onDone` once it is over; returns its cancel. */
    count: (label: CountLabel, onDone: () => void) => () => void;
    now?: () => number;
}): Play {
    let state: PlayView = {
        phase: "starting",
        pausedBy: null,
        pauseReason: null,
        count: null,
        waiting: false,
        catchingUp: false,
        ended: null,
    };
    const listeners: ((view: PlayView) => void)[] = [];
    /** Each local controller's buttons, by controller number. */
    const held = new Map<number, number>();
    /** A command waiting for a frame to carry it, or 0. */
    let command = 0;
    /** The window that paused the game, while it is open and the game is still paused by it. */
    let pausingWindow: PlayWindow | null = null;
    /** A window closed after a frame carried its pause, before this game reached that frame. */
    let closedBeforeItsPause: PlayWindow | null = null;
    let lastPauseFrame: number | null = null;
    let lastResetFrame: number | null = null;
    /** A message for the frame this game is still on its way to, held until it gets there. */
    let early: PlayMessage | null = null;
    let heldSince: number | null = null;
    let behindSince: number | null = null;
    let cancelCount: (() => void) | null = null;
    const delay = lockstep.input_delay();

    function set(changes: Partial<PlayView>) {
        const next = { ...state, ...changes };
        const same = (Object.keys(next) as (keyof PlayView)[]).every((key) => next[key] === state[key]);
        state = next;
        if (!same) for (const listener of listeners) listener(state);
    }

    function localPad(): number {
        let pad = 0;
        for (const buttons of held.values()) pad |= buttons;
        return pad & BUTTONS;
    }

    function startCount(label: CountLabel) {
        cancelCount?.();
        heldSince = null;
        behindSince = null;
        set({ phase: "counting", count: label, pausedBy: null, pauseReason: null, waiting: false, catchingUp: false });
        cancelCount = count(label, () => {
            cancelCount = null;
            heldSince = null;
            set({ phase: "playing", count: null });
        });
    }

    const isPause = (c: number) => PAUSES[c] !== undefined;
    const isReset = (c: number) => c === COMMANDS.reset || c === COMMANDS.hardReset;

    /** `frame` is the last frame run before the reset. */
    function resetNow(soft: boolean, by: "me" | "them", frame: number) {
        core.reset(soft);
        lastResetFrame = frame;
        pausingWindow = null;
        closedBeforeItsPause = null;
        // A reset press of this player's not yet carried by a frame is answered by this one.
        if (isReset(command)) command = 0;
        startCount(by === "me" ? "reset-me" : "reset-them");
    }

    /** What a command carried by `frame` does, after the frame has run. */
    function act(frame: number, pad1: number, pad2: number) {
        const commands = [pad1 >> COMMAND_SHIFT, pad2 >> COMMAND_SHIFT];
        const mine = player - 1;
        // A reset wins over a pause carried by the same frame, on both sides alike.
        const resetter = commands.findIndex(isReset);
        // A press made before the last reset reached that player lands within the delay: the same reset.
        if (resetter >= 0 && (lastResetFrame === null || frame - lastResetFrame > delay)) {
            return resetNow(commands[resetter] === COMMANDS.reset, resetter === mine ? "me" : "them", frame);
        }
        const pauser = commands.findIndex(isPause);
        if (pauser < 0) return;
        // The other player's press made before this pause reached them is the same pause.
        if (lastPauseFrame !== null && frame - lastPauseFrame <= delay) {
            closedBeforeItsPause = null;
            return;
        }
        lastPauseFrame = frame;
        heldSince = null;
        behindSince = null;
        // A pause press of this player's not yet carried by a frame is answered by this one.
        if (isPause(command)) command = 0;
        const reason = PAUSES[commands[pauser]];
        set({ phase: "paused", pausedBy: pauser === mine ? "me" : "them", pauseReason: reason, waiting: false, catchingUp: false });
        const held = early;
        early = null;
        if (held && held.at === lockstep.next_frame()) message(held);
        // Its window has closed already: closing it resumes both, as it would have once paused.
        if (pauser === mine && closedBeforeItsPause === reason) resume();
        closedBeforeItsPause = null;
    }

    /**
     * Whether this game is behind the other's, judged before this tick catches up. Behind is more
     * than a frame past the input delay (two games ticking together are never that far apart), and
     * it lasts until the game is level again, so catching up a frame at a time still counts.
     */
    function judgeBehind(): boolean {
        const known = lockstep.remote_frames_known();
        if (known > delay + 1) return true;
        return behindSince !== null && known > delay;
    }

    function tick(): number {
        if (state.phase !== "playing") return 0;
        let ran = 0;
        const wasBehind = judgeBehind();
        const most = wasBehind ? CATCH_UP_FRAMES : 1;
        while (ran < most && state.phase === "playing") {
            const inputs = lockstep.advance(localPad() | (command << COMMAND_SHIFT));
            if (inputs.length < 3) break;
            // The frame recorded it: it now travels to the other side with this pad.
            command = 0;
            ran++;
            const [frame, pad1, pad2] = [inputs[0], inputs[1], inputs[2]];
            core.runFrame(pad1 & BUTTONS, pad2 & BUTTONS);
            act(frame, pad1, pad2);
        }
        link.sendPacket(lockstep.packet());
        if (state.phase !== "playing") return ran;
        const t = now();
        heldSince = ran > 0 ? null : (heldSince ?? t);
        behindSince = wasBehind ? (behindSince ?? t) : null;
        const waiting = heldSince !== null && t - heldSince >= NOTICE_AFTER_MS;
        const catchingUp = !waiting && behindSince !== null && t - behindSince >= NOTICE_AFTER_MS;
        set({ waiting, catchingUp });
        return ran;
    }

    function resume() {
        if (state.phase !== "paused") return;
        link.sendPlay({ t: "resume", at: lockstep.next_frame() });
        pausingWindow = null;
        startCount("resume");
    }

    function message(message: PlayMessage) {
        const at = lockstep.next_frame();
        // Sent once the other game stopped there; this one is still on its way.
        if (message.at > at) {
            if (state.phase === "playing") early = message;
            return;
        }
        // For a frame this game has passed: it cannot be played where it was meant.
        if (message.at < at) return;
        if (message.t === "resume") {
            if (state.phase === "paused") {
                pausingWindow = null;
                startCount("resume");
            }
            return;
        }
        if (state.phase === "paused" || state.phase === "counting") resetNow(message.soft, "them", at - 1);
    }

    return {
        view: () => state,
        subscribe: (listener) => {
            listeners.push(listener);
        },
        begin: () => {
            if (state.phase !== "starting") return;
            heldSince = null;
            set({ phase: "playing" });
        },
        press: (controller, button, pressed) => {
            const buttons = held.get(controller) ?? 0;
            held.set(controller, pressed ? buttons | (1 << button) : buttons & ~(1 << button));
        },
        tick,
        pause: () => {
            if (state.phase === "paused") return resume();
            if (state.phase === "playing" && command === 0) command = COMMANDS.pause;
        },
        resume,
        reset: (soft) => {
            if (state.phase === "playing") {
                command = soft ? COMMANDS.reset : COMMANDS.hardReset;
            } else if (state.phase === "paused") {
                link.sendPlay({ t: "reset", soft, at: lockstep.next_frame() });
                resetNow(soft, "me", lockstep.next_frame() - 1);
            }
        },
        windowOpened: (window) => {
            closedBeforeItsPause = null;
            // Opened during a count, its pause waits for the count to end and the first frame after.
            if (state.phase !== "playing" && state.phase !== "counting" && state.phase !== "starting") return;
            if (command === 0) command = COMMANDS[window];
            pausingWindow = window;
        },
        windowClosed: (window) => {
            if (pausingWindow !== window) return;
            pausingWindow = null;
            if (state.phase === "paused") resume();
            // Closed before a frame carried its pause: the pause is not needed.
            else if (command === COMMANDS[window]) command = 0;
            // A frame carries it already, so both games will pause there: then they resume.
            else if (state.phase === "playing") closedBeforeItsPause = window;
        },
        receive: (bytes) => {
            lockstep.receive(bytes);
        },
        message,
        hear: (data) => {
            if (data instanceof Uint8Array) lockstep.receive(data);
            else message(data);
        },
        ended: (how) => {
            if (state.phase === "ended") return;
            cancelCount?.();
            cancelCount = null;
            command = 0;
            set({ phase: "ended", ended: how, count: null, waiting: false, catchingUp: false, pausedBy: null, pauseReason: null });
        },
    };
}
