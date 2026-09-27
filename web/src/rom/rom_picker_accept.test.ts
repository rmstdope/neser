/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from "vitest";

import indexHtml from "../../index.html?raw";
import { CONSOLE_KINDS, CONSOLES } from "../console/consoles";

function romPickerAcceptedExtensions(): string[] {
    const documentRoot = new DOMParser().parseFromString(indexHtml, "text/html");
    const input = documentRoot.getElementById("rom");
    if (!(input instanceof HTMLInputElement)) {
        throw new Error("the ROM file input should exist");
    }
    return input.accept
        .split(",")
        .map((entry) => entry.trim().toLowerCase())
        .filter((entry) => entry.startsWith("."))
        .map((entry) => entry.slice(1));
}

describe("the ROM file picker", () => {
    it("accepts exactly the ROM extensions the consoles support", () => {
        const supported = CONSOLE_KINDS.flatMap((kind) => CONSOLES[kind].extensions);
        expect([...romPickerAcceptedExtensions()].sort()).toEqual([...supported].sort());
    });

    it("accepts SNES ROMs (.sfc and .smc)", () => {
        expect(romPickerAcceptedExtensions()).toEqual(expect.arrayContaining(["sfc", "smc"]));
    });
});
