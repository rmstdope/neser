/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from "vitest";

import indexHtml from "../../index.html?raw";
import { installRomExtensionTable, romPickerAccept, supportedRomExtensionsText } from "./rom_extensions";
import { ROM_EXTENSION_TABLE } from "./rom_extension_table.fixture";

function romInput(): HTMLInputElement {
    const documentRoot = new DOMParser().parseFromString(indexHtml, "text/html");
    const input = documentRoot.getElementById("rom");
    if (!(input instanceof HTMLInputElement)) {
        throw new Error("the ROM file input should exist");
    }
    return input;
}

describe("the ROM file picker", () => {
    it("carries no extension list of its own in the page: start-up sets it from the wasm table", () => {
        expect(romInput().hasAttribute("accept")).toBe(false);
    });

    it("accepts exactly the ROM extensions the table supports, SNES included", () => {
        installRomExtensionTable(ROM_EXTENSION_TABLE);
        const accepted = romPickerAccept()
            .split(",")
            .filter((entry) => entry.startsWith("."));
        const supported = supportedRomExtensionsText().split(", ");
        expect(accepted).toEqual(supported);
        expect(accepted).toEqual(expect.arrayContaining([".sfc", ".smc"]));
    });
});
