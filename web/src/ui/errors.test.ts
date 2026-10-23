/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { errorPlace, libraryErrorTitle } from "./errors";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("where an error is shown", () => {
    it("Given the library shows, When something fails, Then it is said there, by the file", () => {
        expect(errorPlace("library", "Unsupported file type .txt")).toBe("library");
        expect(errorPlace("library", "Failed to load ROM: boom")).toBe("library");
    });

    it("Given the game itself fails, When said, Then it is said on the picture, even if the page had already turned to the library", () => {
        expect(errorPlace("game", "Emulation error: unreachable")).toBe("game");
        expect(errorPlace("library", "Emulation error: unreachable")).toBe("game");
        expect(errorPlace("game", "Rendering error occurred. Please restart.")).toBe("game");
        expect(errorPlace("game", "Failed to reset GBA: x")).toBe("game");
    });

    it("Given a game runs, When something around it fails, Then a brief message says so and the game carries on", () => {
        expect(errorPlace("game", "Failed to enter fullscreen mode")).toBe("message");
        expect(errorPlace("game", "Failed to initialize save state")).toBe("message");
    });
});

describe("libraryErrorTitle", () => {
    it("Given the file it was about, When titled, Then it names the file; otherwise it says what failed", () => {
        expect(libraryErrorTitle("holiday-photo.jpg")).toBe("Couldn’t open “holiday-photo.jpg”");
        expect(libraryErrorTitle("roms/x/cpu.nes")).toBe("Couldn’t open “cpu.nes”");
        expect(libraryErrorTitle(null)).toBe("Couldn’t start a game");
    });
});

describe("the error places' markup", () => {
    it("Given the library, When reading it, Then an alert under the drop target holds the message line and Choose another…", () => {
        const alert = page().getElementById("library-error")!;
        expect(alert.getAttribute("role")).toBe("alert");
        expect(alert.closest("#library")).not.toBeNull();
        expect(alert.querySelector("#status")).not.toBeNull();
        expect(alert.querySelector("#library-error-choose")!.textContent).toBe("Choose another…");
    });

    it("Given the picture, When reading it, Then a stopped-game message with Reset and Back to library waits on it", () => {
        const overlay = page().getElementById("game-error")!;
        expect(overlay.closest("#screen-wrap")).not.toBeNull();
        expect(overlay.querySelector(".game-error-title")!.textContent).toBe("The game stopped");
        expect(overlay.querySelector("#game-error-reset")!.textContent).toBe("Reset");
        expect(overlay.querySelector("#game-error-library")!.textContent).toBe("Back to library");
    });
});
