/**
 * The web shell's keyboard bindings. They are not declared here: every row comes from the
 * wasm binding `key_binding_table()`, the web rows of Rust's `platform::key_bindings`, which
 * the desktop shell reads too. This module only applies the rows.
 *
 * For one key on one console the rows are tried in order, stopping at the first one the
 * plugged device takes: an SNES pad before the NES joypad, a Super Scope before Select/Start.
 */

export type KeyBindingInput = "pad" | "snesPad" | "vsCoin" | "scopeTurbo" | "scopePause";

export interface KeyBindingRow {
    /** The lower-cased `KeyboardEvent.key`. */
    key: string;
    /** The web console kind: "nes", "gb", "gba" or "snes". */
    console: string;
    input: KeyBindingInput;
    /** The keyboard player, routed to the first or second keyboard port. */
    player: 1 | 2;
    /** The id the wasm setter for `input` takes. */
    button: number;
}

/** The console calls one row can make; each optional one exists only where the console has it. */
export interface KeyBindingSink {
    pad(port: number, button: number, pressed: boolean): void;
    /** An SNES pad in an NES port; false when the port holds none. */
    snesPad?(port: number, button: number, pressed: boolean): boolean;
    vsCoin?(): void;
    /** A plugged Super Scope; false when none is. */
    scope?(action: "turbo" | "pause", pressed: boolean, repeat: boolean): boolean;
}

const INPUTS: readonly KeyBindingInput[] = ["pad", "snesPad", "vsCoin", "scopeTurbo", "scopePause"];

/**
 * Check the rows `key_binding_table()` returned. An input this page cannot apply throws
 * rather than being dropped: a dropped row is a binding missing on the web, which is what
 * this table exists to prevent.
 */
export function parseKeyBindingTable(raw: readonly unknown[]): KeyBindingRow[] {
    return raw.map((value) => {
        const row = value as KeyBindingRow;
        if (!INPUTS.includes(row.input)) {
            throw new Error(`The web shell cannot apply the key binding input "${String(row.input)}"`);
        }
        return { key: row.key, console: row.console, input: row.input, player: row.player, button: row.button };
    });
}

/** The rows bound to `key` (a `KeyboardEvent.key`) on `console`, in the order they are tried. */
export function keyBindingsFor(table: readonly KeyBindingRow[], console: string, key: string): KeyBindingRow[] {
    const typed = key.toLowerCase();
    return table.filter((row) => row.console === console && row.key === typed);
}

/**
 * Apply `rows` for one key event. `ports` are the keyboard's ports (player 1 first). Returns
 * true when the key was the console's, so the caller prevents the browser's default.
 */
export function applyKeyBindings(
    rows: readonly KeyBindingRow[],
    sink: KeyBindingSink,
    ports: readonly number[],
    event: Pick<KeyboardEvent, "repeat">,
    pressed: boolean,
): boolean {
    for (const row of rows) {
        switch (row.input) {
            case "vsCoin":
                // A press inserts one coin and the core times the pulse, so repeats and the
                // release insert nothing: a coin line held ten frames reads as jammed (nr-use).
                if (pressed && !event.repeat) {
                    sink.vsCoin?.();
                }
                return true;
            case "scopeTurbo":
            case "scopePause":
                if (sink.scope?.(row.input === "scopeTurbo" ? "turbo" : "pause", pressed, event.repeat)) {
                    return true;
                }
                break;
            case "snesPad": {
                const port = portOf(row, ports);
                if (port === null) {
                    return false;
                }
                if (sink.snesPad?.(port, row.button, pressed)) {
                    return true;
                }
                break;
            }
            case "pad": {
                const port = portOf(row, ports);
                if (port === null) {
                    return false;
                }
                sink.pad(port, row.button, pressed);
                return true;
            }
        }
    }
    return false;
}

function portOf(row: KeyBindingRow, ports: readonly number[]): number | null {
    const port = row.player === 1 ? (ports[0] ?? 1) : (ports[1] ?? 2);
    return ports.includes(port) ? port : null;
}
