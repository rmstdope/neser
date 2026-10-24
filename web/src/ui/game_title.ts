import type { ConsoleKind } from "../console/consoles";

const SYSTEM_BADGE: Record<ConsoleKind, string> = { nes: "NES", gb: "GB", gba: "GBA", snes: "SNES" };

/**
 * What the top bar shows for the running game: its system badge, its name, and the file it came from.
 * `isColor` is the core's word on whether a Game Boy game runs in colour; without it the file's
 * extension decides, as it must for a game that is not running.
 */
export function gameTitle(romName: string, kind: ConsoleKind, isColor?: boolean): { system: string; title: string; file: string } {
    const file = romName.slice(romName.lastIndexOf("/") + 1);
    const dot = file.lastIndexOf(".");
    const title = dot > 0 ? file.slice(0, dot) : file;
    const extension = dot > 0 ? file.slice(dot + 1).toLowerCase() : "";
    const color = isColor ?? (extension === "gbc" || extension === "cgb");
    const system = kind === "gb" && color ? "GBC" : SYSTEM_BADGE[kind];
    return { system, title, file };
}
