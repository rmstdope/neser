/** Where an error is said: by the file in the library, on the stopped picture, or as a brief message. */
export type ErrorPlace = "library" | "game" | "message";

/** Errors that mean the running game itself stopped, said on its picture. */
const GAME_STOPPED = [/^Emulation error/, /^Rendering error/, /^Failed to reset/];

export function errorPlace(view: "library" | "game", message: string): ErrorPlace {
    if (GAME_STOPPED.some((pattern) => pattern.test(message))) return "game";
    return view === "library" ? "library" : "message";
}

/** The library alert's heading: the file it was about, or what failed when there is none. */
export function libraryErrorTitle(fileName: string | null): string {
    if (!fileName) return "Couldn’t start a game";
    return `Couldn’t open “${fileName.slice(fileName.lastIndexOf("/") + 1)}”`;
}
