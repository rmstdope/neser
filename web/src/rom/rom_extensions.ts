import { CONSOLE_KINDS, CONSOLES, consoleKindForExtension, type ConsoleKind } from "../console/consoles";

export function webRomExtensionForName(name: string): string {
    const dotIndex = name.lastIndexOf(".");
    if (dotIndex < 0 || dotIndex === name.length - 1) {
        return "";
    }
    return name.slice(dotIndex + 1).toLowerCase();
}

export function webRomConsoleKindForName(name: string): ConsoleKind | null {
    const extension = webRomExtensionForName(name);
    return extension === "" ? null : consoleKindForExtension(extension);
}

export function isSupportedWebRomName(name: string): boolean {
    return webRomConsoleKindForName(name) !== null;
}

export function supportedRomExtensionsText(): string {
    return CONSOLE_KINDS.flatMap((kind) => CONSOLES[kind].extensions.map((ext) => `.${ext}`)).join(", ");
}
