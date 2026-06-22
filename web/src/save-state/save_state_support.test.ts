import { expect, it } from "vitest";
import { supportsWebSaveState } from "./save_state_support";

it("supports web save-state for NES and SNES only", () => {
    expect(supportsWebSaveState("nes")).toBe(true);
    expect(supportsWebSaveState("snes")).toBe(true);
    expect(supportsWebSaveState("gb")).toBe(false);
    expect(supportsWebSaveState("gba")).toBe(false);
    expect(supportsWebSaveState(null)).toBe(false);
});
