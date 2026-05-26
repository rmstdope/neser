export type WebRomConsoleKind = "nes" | "gb";

const GAME_BOY_EXTENSIONS = new Set(["gb", "gbc", "cgb"]);

export function webRomExtensionForName(name: string): string {
    const dotIndex = name.lastIndexOf(".");
    if (dotIndex < 0 || dotIndex === name.length - 1) {
        return "";
    }
    return name.slice(dotIndex + 1).toLowerCase();
}

export function webRomConsoleKindForName(name: string): WebRomConsoleKind | null {
    const extension = webRomExtensionForName(name);
    if (extension === "nes") {
        return "nes";
    }
    if (GAME_BOY_EXTENSIONS.has(extension)) {
        return "gb";
    }
    return null;
}

export function isSupportedWebRomName(name: string): boolean {
    return webRomConsoleKindForName(name) !== null;
}

export function supportedRomExtensionsText(): string {
    return ".nes, .gb, .gbc, .cgb";
}
