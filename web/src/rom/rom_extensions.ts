import { CONSOLE_KINDS, type ConsoleKind } from "../console/consoles";

/**
 * Which ROM extension means which console. The table is written once, in Rust
 * (`platform::rom_extensions::ROM_EXTENSIONS`), and reaches the page through the wasm binding's
 * `rom_extension_table()`; start-up installs it here. Until then no extension is known.
 */
let romExtensionTable: readonly (readonly [string, ConsoleKind])[] = [];

function isConsoleKind(value: unknown): value is ConsoleKind {
    return typeof value === "string" && (CONSOLE_KINDS as readonly string[]).includes(value);
}

/**
 * Replace the known ROM extensions with `entries`, the `[extension, consoleKind]` pairs the wasm
 * binding returns. An entry that is not a string extension paired with a known console is dropped.
 */
export function installRomExtensionTable(entries: unknown): void {
    const table: (readonly [string, ConsoleKind])[] = [];
    for (const entry of Array.isArray(entries) ? entries : []) {
        if (!Array.isArray(entry)) continue;
        const [extension, kind] = entry as unknown[];
        if (typeof extension === "string" && extension !== "" && isConsoleKind(kind)) {
            table.push([extension.toLowerCase(), kind]);
        }
    }
    romExtensionTable = table;
}

/** Install the table and set the ROM file input's accept list from it. */
export function applyRomExtensionTable(entries: unknown, romInput: HTMLInputElement | null): void {
    installRomExtensionTable(entries);
    if (romInput) romInput.accept = romPickerAccept();
}

export function webRomExtensionForName(name: string): string {
    const dotIndex = name.lastIndexOf(".");
    if (dotIndex < 0 || dotIndex === name.length - 1) {
        return "";
    }
    return name.slice(dotIndex + 1).toLowerCase();
}

export function webRomConsoleKindForName(name: string): ConsoleKind | null {
    const extension = webRomExtensionForName(name);
    if (extension === "") return null;
    return romExtensionTable.find(([known]) => known === extension)?.[1] ?? null;
}

export function isSupportedWebRomName(name: string): boolean {
    return webRomConsoleKindForName(name) !== null;
}

export function supportedRomExtensionsText(): string {
    return romExtensionTable.map(([extension]) => `.${extension}`).join(", ");
}

/** The ROM file input's accept list: every supported extension, then raw binaries. */
export function romPickerAccept(): string {
    return [...romExtensionTable.map(([extension]) => `.${extension}`), "application/octet-stream"].join(",");
}
