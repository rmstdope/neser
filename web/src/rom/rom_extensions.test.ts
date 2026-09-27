/**
 * @vitest-environment jsdom
 */
import { beforeEach, expect, it } from "vitest";
import {
    applyRomExtensionTable,
    installRomExtensionTable,
    isSupportedWebRomName,
    romPickerAccept,
    supportedRomExtensionsText,
    webRomConsoleKindForName,
    webRomExtensionForName
} from "./rom_extensions";
import { ROM_EXTENSION_TABLE } from "./rom_extension_table.fixture";

beforeEach(() => {
    installRomExtensionTable(ROM_EXTENSION_TABLE);
});

it("knows no ROM extension until the table is installed", () => {
    installRomExtensionTable([]);
    expect(webRomConsoleKindForName("mario.nes")).toBeNull();
    expect(supportedRomExtensionsText()).toBe("");
});

it("drops table entries that are not an extension paired with a known console", () => {
    installRomExtensionTable([["nes", "nes"], ["abc", "dreamcast"], ["x"], "gb", [7, "gb"], ["SFC", "snes"]]);
    expect(supportedRomExtensionsText()).toBe(".nes, .sfc");
    expect(webRomConsoleKindForName("game.sfc")).toBe("snes");
    expect(webRomConsoleKindForName("game.abc")).toBeNull();
});

it("treats a table that is not a list as empty", () => {
    installRomExtensionTable(undefined);
    expect(supportedRomExtensionsText()).toBe("");
});

it("gives the file picker every supported extension, then raw binaries", () => {
    expect(romPickerAccept()).toBe(".nes,.gb,.gbc,.cgb,.gba,.sfc,.smc,application/octet-stream");
});

it("sets the ROM input's accept list from the installed table at start-up", () => {
    const input = document.createElement("input");
    input.type = "file";
    applyRomExtensionTable([["nes", "nes"], ["sfc", "snes"]], input);
    expect(input.accept).toBe(".nes,.sfc,application/octet-stream");
    expect(webRomConsoleKindForName("game.sfc")).toBe("snes");
});

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
    expect(isSupportedWebRomName("GAME.SFC")).toBe(true);
    expect(isSupportedWebRomName("GAME.SMC")).toBe(true);
    expect(isSupportedWebRomName("notes.txt")).toBe(false);
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
