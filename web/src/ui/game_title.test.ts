import { describe, expect, it } from "vitest";

import { gameTitle } from "./game_title";

describe("gameTitle", () => {
    it("Given a bundled ROM path, When naming the game, Then the title drops folders and extension and the file keeps its extension", () => {
        expect(gameTitle("automated_tests/allpads-r9/allpads.nes", "nes")).toEqual({ system: "NES", title: "allpads", file: "allpads.nes" });
    });

    it("Given each console, When naming its system, Then it uses the short badge name", () => {
        expect(gameTitle("a.sfc", "snes").system).toBe("SNES");
        expect(gameTitle("a.gba", "gba").system).toBe("GBA");
        expect(gameTitle("a.gb", "gb").system).toBe("GB");
    });

    it("Given a Game Boy Color file, When naming its system, Then the badge says GBC", () => {
        expect(gameTitle("Some Game.gbc", "gb").system).toBe("GBC");
        expect(gameTitle("Some Game.CGB", "gb").system).toBe("GBC");
    });

    it("Given a file with no extension, When naming the game, Then the whole name is the title", () => {
        expect(gameTitle("README", "nes").title).toBe("README");
    });
});
