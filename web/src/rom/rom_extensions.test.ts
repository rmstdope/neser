import { expect, it } from "vitest";
import {
    isSupportedWebRomName,
    supportedRomExtensionsText,
    webRomConsoleKindForName,
    webRomExtensionForName
} from "./rom_extensions";

it("classifies NES ROM names as NES", () => {
    expect(webRomConsoleKindForName("mario.nes")).toBe("nes");
    expect(webRomConsoleKindForName("MARIO.NES")).toBe("nes");
});

it("classifies DMG and CGB Game Boy ROM extensions as Game Boy", () => {
    expect(webRomConsoleKindForName("tetris.gb")).toBe("gb");
    expect(webRomConsoleKindForName("zelda.gbc")).toBe("gb");
    expect(webRomConsoleKindForName("pocket-camera.cgb")).toBe("gb");
    expect(webRomConsoleKindForName("POKEMON.CGB")).toBe("gb");
});

it("classifies Game Boy Advance ROM names as GBA", () => {
    expect(webRomConsoleKindForName("metroid.gba")).toBe("gba");
    expect(webRomConsoleKindForName("METROID.GBA")).toBe("gba");
});

it("classifies SNES ROM names as SNES", () => {
    expect(webRomConsoleKindForName("zelda.sfc")).toBe("snes");
    expect(webRomConsoleKindForName("mario.smc")).toBe("snes");
    expect(webRomConsoleKindForName("GAME.SFC")).toBe("snes");
    expect(webRomConsoleKindForName("GAME.SMC")).toBe("snes");
});

it("supports SNES ROM names", () => {
    expect(isSupportedWebRomName("zelda.sfc")).toBe(true);
    expect(isSupportedWebRomName("mario.smc")).toBe(true);
});

it("rejects unsupported web ROM extensions", () => {
    expect(webRomConsoleKindForName("notes.txt")).toBeNull();
    expect(webRomConsoleKindForName("advance.agb")).toBeNull();
});

it("extracts lower-case extensions for messages", () => {
    expect(webRomExtensionForName("POKEMON.CGB")).toBe("cgb");
    expect(webRomExtensionForName("README")).toBe("");
    expect(webRomExtensionForName("game.")).toBe("");
});

it("lists all supported web ROM extensions for user-facing messages", () => {
    expect(supportedRomExtensionsText()).toBe(".nes, .gb, .gbc, .cgb, .gba, .sfc, .smc");
});

it("handles edge case: empty string", () => {
    expect(webRomConsoleKindForName("")).toBeNull();
});

it("handles edge case: no extension", () => {
    expect(webRomConsoleKindForName("file")).toBeNull();
});

it("handles edge case: multiple dots", () => {
    expect(webRomConsoleKindForName("my.game.nes")).toBe("nes");
});

it("handles edge case: dots at end with no extension name", () => {
    expect(webRomConsoleKindForName("file.")).toBeNull();
});

it("handles edge case: hidden file .nes", () => {
    expect(webRomConsoleKindForName(".nes")).toBe("nes");
});

it("handles edge case: path with slashes", () => {
    expect(webRomConsoleKindForName("/path/to/game.gb")).toBe("gb");
});

it("handles edge case: Windows path", () => {
    expect(webRomConsoleKindForName("C:\\games\\pokemon.gbc")).toBe("gb");
});
