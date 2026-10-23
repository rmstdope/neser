/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { BROWSER_CANNOT_DRAW, errorPlace, libraryAlert, showLibraryAlert, unsupportedFileMessage } from "./errors";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("where an error is shown", () => {
    it("Given the library shows, When something fails, Then it is said there, by the file", () => {
        expect(errorPlace("library", ".txt isn’t a game file NESER knows. Try a .nes file.")).toBe("library");
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

describe("libraryAlert", () => {
    it("Given the file it was about, When titled, Then it names the file and offers another; otherwise it says what failed", () => {
        expect(libraryAlert("x", "holiday-photo.jpg")).toEqual({ title: "Couldn’t open “holiday-photo.jpg”", text: "x", chooseAnother: true });
        expect(libraryAlert("x", "roms/x/cpu.nes").title).toBe("Couldn’t open “cpu.nes”");
        expect(libraryAlert("x", null).title).toBe("Couldn’t start a game");
    });

    it("Given a browser that cannot draw games, When said, Then it blames the browser, says what to try, and offers no file", () => {
        expect(libraryAlert(BROWSER_CANNOT_DRAW, "cpu.nes")).toEqual({
            title: "This browser can’t show games",
            text: "WebGL is turned off or not supported here. Try another browser, or turn on hardware acceleration.",
            chooseAnother: false,
        });
    });
});

describe("showLibraryAlert", () => {
    it("Given the page's own markup, When an alert is shown, Then its heading, words and Choose another… follow it", () => {
        const doc = page();
        showLibraryAlert(doc, libraryAlert(BROWSER_CANNOT_DRAW, null));
        expect(doc.getElementById("library-error")!.hidden).toBe(false);
        expect(doc.getElementById("library-error-title")!.textContent).toBe("This browser can’t show games");
        expect(doc.getElementById("status")!.textContent).toBe(BROWSER_CANNOT_DRAW);
        expect(doc.getElementById("status")!.dataset.state).toBe("error");
        expect((doc.getElementById("library-error-choose") as HTMLElement).hidden).toBe(true);
    });
});

describe("unsupportedFileMessage", () => {
    it("Given a file of a kind NESER does not run, When said, Then it names the extension and the ones to try, as drawn", () => {
        expect(unsupportedFileMessage("jpg", ["nes", "gb", "gbc", "gba", "sfc", "smc"])).toBe(
            ".jpg isn’t a game file NESER knows. Try a .nes, .gb, .gbc, .gba, .sfc or .smc file."
        );
        expect(unsupportedFileMessage("", ["nes", "gb"])).toBe("That isn’t a game file NESER knows. Try a .nes or .gb file.");
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
