/** Where an error is said: by the file in the library, on the stopped picture, or as a brief message. */
export type ErrorPlace = "library" | "game" | "message";

/** Errors that mean the running game itself stopped, said on its picture. */
const GAME_STOPPED = [/^Emulation error/, /^Rendering error/, /^Failed to reset/];

export function errorPlace(view: "library" | "game", message: string): ErrorPlace {
    if (GAME_STOPPED.some((pattern) => pattern.test(message))) return "game";
    return view === "library" ? "library" : "message";
}

/** What the page says when the browser cannot draw games at all (no WebGL). */
export const BROWSER_CANNOT_DRAW = "WebGL is turned off or not supported here. Try another browser, or turn on hardware acceleration.";

export type LibraryAlert = { title: string; text: string; chooseAnother: boolean };

/**
 * The library alert for an error: headed by the file it was about (or what failed, when there is
 * none) and offering another file, except when the browser itself cannot draw, where no file helps.
 */
export function libraryAlert(message: string, fileName: string | null): LibraryAlert {
    if (message === BROWSER_CANNOT_DRAW) return { title: "This browser can’t show games", text: message, chooseAnother: false };
    const title = fileName ? `Couldn’t open “${fileName.slice(fileName.lastIndexOf("/") + 1)}”` : "Couldn’t start a game";
    return { title, text: message, chooseAnother: true };
}

/**
 * Shows a library alert in the page's own markup. It needs nothing but the document, so it works
 * even when the rest of the page could not start (a browser without WebGL).
 */
export function showLibraryAlert(doc: Document, alert: LibraryAlert): void {
    const status = doc.getElementById("status");
    if (status) {
        status.textContent = alert.text;
        status.dataset.state = "error";
    }
    const title = doc.getElementById("library-error-title");
    if (title) title.textContent = alert.title;
    const choose = doc.getElementById("library-error-choose");
    if (choose) choose.hidden = !alert.chooseAnother;
    const box = doc.getElementById("library-error");
    if (box) box.hidden = false;
}

/** A chosen file of a kind NESER does not run, in the drawn words, naming the kinds to try. */
export function unsupportedFileMessage(extension: string, supported: readonly string[]): string {
    const kinds = supported.map((e) => `.${e}`);
    const list = kinds.length > 1 ? `${kinds.slice(0, -1).join(", ")} or ${kinds[kinds.length - 1]}` : kinds.join("");
    const what = extension ? `.${extension} isn’t` : "That isn’t";
    return `${what} a game file NESER knows. Try a ${list} file.`;
}
