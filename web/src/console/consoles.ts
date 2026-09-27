/**
 * The one list of consoles the web frontend runs, and what the web layer does differently for each.
 *
 * Adding a console to the web is one row here plus its wasm binding (its ROM extensions are a row
 * in Rust's `platform::rom_extensions`, which reaches the page through that binding) (and that binding's arm of
 * `ActiveEmulator` in app.ts). Facts the core reports at runtime — screen size, frame rate, the
 * audio sample rate it is configured to — stay with the binding and are not copied here.
 */

export const CONSOLE_KINDS = ["nes", "gb", "gba", "snes"] as const;

export type ConsoleKind = (typeof CONSOLE_KINDS)[number];

export interface ConsoleProfile {
    /** Pixel layout of the frame the binding hands to the display. */
    frameFormat: "rgb" | "rgba";
    audio: {
        /** Whether the binding's interleaved stereo samples are played when it offers them. */
        stereo: boolean;
        /** Which normalizer in audio/audio_normalizer.ts brings its samples into Web Audio's range. */
        sampleScale: "nes" | "gb" | "gba";
    };
    /** Which filters F4 cycles through: the NES set, the Game Boy set, or the stock filter alone. */
    filterFamily: "nes" | "gb" | "stock";
    /** The filter a game of this console starts on when the current one does not apply. */
    defaultFilter: string;
    /** Keyboard bindings shown in the help overlay, one entry per player shown. */
    playerKeyBindings: readonly string[];
    /** Whether the browser can save and load its state. */
    saveState: boolean;
    /** Whether every Start builds a new emulator instance, even for a game of the same console. */
    freshInstanceOnStart: boolean;
    /** Whether the Palette button (F8) is offered. */
    paletteButton: boolean;
    /** When the LCD colour-correction button is offered: never, only for colour games, or always. */
    colorCorrection: "never" | "color-only" | "always";
}

const PLAYER_1_KEYS = "W/A/S/D: D-Pad\nR: A\nT: B\n4: Select\n5: Start";
const PLAYER_2_KEYS = "I/J/K/L: D-Pad\nO: A\nP: B\n9: Select\n0: Start";
const AGB_KEYS = "W/A/S/D: D-Pad\nR: Y\nT: X\nF: B\nG: A\nV: L\nB: R\n4: Select\n5: Start";
const SNES_KEYS = "W/A/S/D: D-Pad\nR: B\nT: A\nY: X\nG: Y\nQ: L\nE: R\n4: Select\n5: Start";

export const CONSOLES: Readonly<Record<ConsoleKind, ConsoleProfile>> = {
    nes: {
        frameFormat: "rgba",
        audio: { stereo: false, sampleScale: "nes" },
        filterFamily: "nes",
        defaultFilter: "ntsc",
        playerKeyBindings: [PLAYER_1_KEYS, PLAYER_2_KEYS],
        saveState: true,
        freshInstanceOnStart: false,
        paletteButton: true,
        colorCorrection: "never",
    },
    gb: {
        frameFormat: "rgba",
        audio: { stereo: false, sampleScale: "gb" },
        filterFamily: "gb",
        defaultFilter: "gameboy",
        playerKeyBindings: [PLAYER_1_KEYS],
        saveState: true,
        freshInstanceOnStart: false,
        paletteButton: true,
        colorCorrection: "color-only",
    },
    gba: {
        frameFormat: "rgb",
        audio: { stereo: true, sampleScale: "gba" },
        filterFamily: "stock",
        defaultFilter: "stock",
        playerKeyBindings: [AGB_KEYS],
        saveState: false,
        freshInstanceOnStart: true,
        paletteButton: false,
        colorCorrection: "always",
    },
    snes: {
        frameFormat: "rgba",
        audio: { stereo: true, sampleScale: "gba" },
        filterFamily: "nes",
        defaultFilter: "stock",
        playerKeyBindings: [SNES_KEYS],
        saveState: true,
        freshInstanceOnStart: true,
        paletteButton: false,
        colorCorrection: "never",
    },
};
