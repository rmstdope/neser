import { expect, it } from "vitest";
import { shouldCreateFreshEmulatorForRomStart } from "./emulator_lifecycle";

it("creates an emulator when no emulator exists", () => {
    expect(shouldCreateFreshEmulatorForRomStart(null, "gba")).toBe(true);
});

it("reuses NES and GB emulators when starting another ROM of the same kind", () => {
    expect(shouldCreateFreshEmulatorForRomStart("nes", "nes")).toBe(false);
    expect(shouldCreateFreshEmulatorForRomStart("gb", "gb")).toBe(false);
});

it("creates a fresh emulator when switching console kind", () => {
    expect(shouldCreateFreshEmulatorForRomStart("nes", "gba")).toBe(true);
    expect(shouldCreateFreshEmulatorForRomStart("gba", "nes")).toBe(true);
});

it("creates a fresh GBA emulator when starting another GBA ROM", () => {
    // GBA web rendering uses the core's native RGB framebuffer, so a fresh
    // instance keeps lifecycle restarts away from partially reset core state.
    expect(shouldCreateFreshEmulatorForRomStart("gba", "gba")).toBe(true);
});
