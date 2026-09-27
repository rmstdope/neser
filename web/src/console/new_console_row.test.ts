/**
 * Adding a console to the web is one row in the console table (its ROM extensions are a row in
 * Rust's table, which reaches the page through the wasm binding): every module that behaves
 * differently per console must pick a new row up without being edited. The table is replaced
 * here by the real one plus a made-up console, "next", whose every column is distinctive.
 */
import { describe, expect, it, vi } from "vitest";
import type { ConsoleKind, ConsoleProfile } from "./consoles";

vi.mock("./consoles", async (importOriginal) => {
    const real = await importOriginal<typeof import("./consoles")>();
    const next: ConsoleProfile = {
        frameFormat: "rgb",
        audio: { stereo: true, sampleScale: "gba" },
        filterFamily: "gb",
        defaultFilter: "gameboy",
        playerKeyBindings: ["NEXT KEYS"],
        saveState: true,
        freshInstanceOnStart: true,
        paletteButton: true,
        colorCorrection: "always",
    };
    const kinds = [...real.CONSOLE_KINDS, "next"];
    const consoles = { ...real.CONSOLES, next };
    return {
        ...real,
        CONSOLE_KINDS: kinds,
        CONSOLES: consoles,
    };
});

const NEXT = "next" as unknown as ConsoleKind;

describe("a console added as one table row", () => {
    it("has its ROMs recognised and named in the supported list once the table names it", async () => {
        const { installRomExtensionTable, webRomConsoleKindForName, supportedRomExtensionsText } = await import(
            "../rom/rom_extensions"
        );
        const { ROM_EXTENSION_TABLE } = await import("../rom/rom_extension_table.fixture");
        installRomExtensionTable([...ROM_EXTENSION_TABLE, ["nxt", "next"]]);
        expect(webRomConsoleKindForName("GAME.NXT")).toBe("next");
        expect(supportedRomExtensionsText()).toBe(".nes, .gb, .gbc, .cgb, .gba, .sfc, .smc, .nxt");
    });

    it("gets a fresh instance on every Start when its row asks for one", async () => {
        const { shouldCreateFreshEmulatorForRomStart } = await import("../rom/emulator_lifecycle");
        expect(shouldCreateFreshEmulatorForRomStart(NEXT, NEXT)).toBe(true);
    });

    it("offers save states when its row says so", async () => {
        const { supportsWebSaveState } = await import("../save-state/save_state_support");
        expect(supportsWebSaveState(NEXT)).toBe(true);
    });

    it("cycles its row's filter family and starts on its default filter", async () => {
        const { filterKeysForConsole, defaultFilterForConsole } = await import("../display/filters");
        const filters = {
            stock: { name: "None", type: "single" },
            ntsc: { name: "NTSC", type: "ntsc" },
            gameboy: { name: "Game Boy", type: "gb" },
        };
        expect(filterKeysForConsole(Object.keys(filters), filters, NEXT)).toEqual(["stock", "gameboy"]);
        expect(defaultFilterForConsole(NEXT)).toBe("gameboy");
    });

    it("shows the Palette and Colors buttons its row asks for", async () => {
        const { paletteButtonVisible } = await import("../display/palette_button");
        const { cgbColorButtonVisible } = await import("../display/cgb_color_correction");
        expect(paletteButtonVisible({ kind: NEXT, label: "Palette", running: true, paused: false })).toBe(true);
        expect(cgbColorButtonVisible({ kind: NEXT, isColor: false, running: true, paused: false })).toBe(true);
    });

    it("shows its row's keys in the help overlay", async () => {
        const { buildControllerOverlayText } = await import("../shortcuts/shortcut_help");
        expect(buildControllerOverlayText(0, NEXT)).toBe("Controller (Player 1)\nNEXT KEYS");
        expect(buildControllerOverlayText(1, NEXT)).toBe("Controller (Player 1)\nGamepad");
    });

    it("plays stereo when its row says so", async () => {
        const { getPlaybackAudioSamples } = await import("../audio/playback_samples");
        const stereo = new Float32Array([0.1, -0.1]);
        const playback = getPlaybackAudioSamples(NEXT, {
            get_audio_samples: () => new Float32Array([0]),
            get_audio_samples_stereo: () => stereo,
        });
        expect(playback).toEqual({ channels: 2, samples: stereo });
    });

    it("scales its mono samples with its row's normalizer", async () => {
        const { monoSampleNormalizer } = await import("../audio/audio_normalizer");
        // "gba" scale: bipolar with the output gain (0.75), not the NES unipolar or the plain Game Boy clamp.
        expect(monoSampleNormalizer(NEXT, 1.177)(-0.5)).toBeCloseTo(-0.375);
    });
});
