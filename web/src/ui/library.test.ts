/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it, vi } from "vitest";

import type { ConsoleKind } from "../console/consoles";
import { filterLibrary, groupLibrary, renderLibrary, renderNewsWave } from "./library";

const kindFor = (name: string): ConsoleKind | null =>
    ({ nes: "nes", gb: "gb", gbc: "gb", gba: "gba", sfc: "snes" } as Record<string, ConsoleKind>)[name.split(".").pop()!] ?? null;

const entries = [
    { path: "automated_tests/blargg_apu/02.len_table.nes", url: "u/2" },
    { path: "automated_tests/blargg_apu/01.len_ctr.nes", url: "u/1" },
    { path: "gb/acid/cgb-acid2.gbc", url: "u/3" },
    { path: "gb/acid/dmg-acid2.gb", url: "u/4" },
    { path: "gba/arm.gba", url: "u/5" },
    { path: "top.sfc", url: "u/6" },
];

describe("groupLibrary", () => {
    it("Given the bundled ROMs, When grouped, Then each folder lists its games by name with their system", () => {
        const groups = groupLibrary(entries, kindFor);
        expect(groups.map((g) => g.folder)).toEqual(["", "automated_tests/blargg_apu", "gb/acid", "gba"]);
        expect(groups[1].roms).toEqual([
            { title: "01.len_ctr", file: "01.len_ctr.nes", path: "automated_tests/blargg_apu/01.len_ctr.nes", url: "u/1", system: "NES" },
            { title: "02.len_table", file: "02.len_table.nes", path: "automated_tests/blargg_apu/02.len_table.nes", url: "u/2", system: "NES" },
        ]);
        expect(groups[2].roms.map((r) => r.system)).toEqual(["GBC", "GB"]);
    });

    it("Given a file whose system is unknown, When grouped, Then it is left out", () => {
        expect(groupLibrary([{ path: "x/readme.txt", url: "u" }], kindFor)).toEqual([]);
    });
});

describe("filterLibrary", () => {
    const groups = groupLibrary(entries, kindFor);
    const titles = (gs: ReturnType<typeof filterLibrary>) => gs.flatMap((g) => g.roms.map((r) => r.title));

    it("Given a search, When filtering, Then games whose name or folder match it remain, ignoring case", () => {
        expect(titles(filterLibrary(groups, { query: "LEN_C", system: "all" }))).toEqual(["01.len_ctr"]);
        expect(titles(filterLibrary(groups, { query: "acid", system: "all" }))).toEqual(["cgb-acid2", "dmg-acid2"]);
    });

    it("Given a system, When filtering, Then only its games remain, and GB includes Game Boy Color games", () => {
        expect(titles(filterLibrary(groups, { query: "", system: "GB" }))).toEqual(["cgb-acid2", "dmg-acid2"]);
        expect(titles(filterLibrary(groups, { query: "", system: "SNES" }))).toEqual(["top"]);
    });

    it("Given nothing matches, When filtering, Then no empty folders remain", () => {
        expect(filterLibrary(groups, { query: "zzz", system: "all" })).toEqual([]);
    });
});

describe("renderLibrary", () => {
    it("Given groups, When rendered, Then each folder is a heading over one button per game, and pressing one picks it", () => {
        const list = document.createElement("div");
        const onPick = vi.fn();
        renderLibrary(list, groupLibrary(entries.slice(0, 2), kindFor), "", onPick);
        expect(list.querySelector(".library-group-title")!.textContent).toBe("automated_tests/blargg_apu");
        const items = [...list.querySelectorAll<HTMLButtonElement>("button.library-item")];
        expect(items.map((b) => b.querySelector(".library-item-name")!.textContent)).toEqual(["01.len_ctr", "02.len_table"]);
        expect(items[0].querySelector(".sys-badge")!.textContent).toBe("NES");
        items[1].click();
        expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ url: "u/2", file: "02.len_table.nes" }));
    });

    it("Given no match for a search, When rendered, Then it says so in words", () => {
        const list = document.createElement("div");
        renderLibrary(list, [], "zzz", () => {});
        expect(list.textContent).toBe("No test ROMs match “zzz”.");
    });
});

describe("renderNewsWave", () => {
    it("Given the release news, When drawn, Then each letter is its own span numbered for the wave, spaces kept", () => {
        const el = document.createElement("div");
        renderNewsWave(el, "Hi you");
        const spans = [...el.querySelectorAll("span")];
        expect(spans.map((s) => s.textContent)).toEqual(["H", "i", " ", "y", "o", "u"]);
        expect(spans[3].style.getPropertyValue("--i")).toBe("3");
        expect(el.getAttribute("aria-label")).toBe("Hi you");
    });
});

describe("the library's markup", () => {
    const page = () => {
        const html = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../index.html"), "utf8");
        return new DOMParser().parseFromString(html, "text/html");
    };

    it("Given the page, When it opens, Then the library shows and there is no sidebar", () => {
        const doc = page();
        expect(doc.querySelector("main")!.dataset.view).toBe("library");
        expect(doc.querySelector("aside, .drawer")).toBeNull();
        expect(doc.getElementById("library-title")!.textContent).toBe("Drop a game anywhere to play");
    });

    it("Given the library, When reading how a game is picked, Then a file chooser, a search, the system chips and the list are there", () => {
        const doc = page();
        const library = doc.getElementById("library")!;
        expect(library.querySelector("#library-choose")!.textContent).toBe("Choose a file…");
        expect(library.querySelector("#rom")).not.toBeNull();
        expect(library.querySelector("#library-search")!.getAttribute("aria-label")).toBe("Search the bundled test ROMs");
        expect([...library.querySelectorAll("#library-systems button")].map((b) => b.textContent)).toEqual(["All", "NES", "GB", "GBA", "SNES"]);
        expect(library.querySelector("#library-list")).not.toBeNull();
        expect(library.querySelector("#library-back")!.textContent).toBe("Back to the game");
    });

    it("Given the top bar, When reading it, Then a Library button opens the library", () => {
        const button = page().querySelector("header #library-open")!;
        expect(button.textContent).toBe("Library");
        expect(button.getAttribute("aria-controls")).toBe("library");
    });
});
