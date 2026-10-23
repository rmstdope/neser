import type { ConsoleKind } from "../console/consoles";
import type { RomListEntry } from "../rom/rom_list";
import { gameTitle } from "./game_title";

/** One bundled game in the library. */
export type LibraryRom = { title: string; file: string; path: string; url: string; system: string };
/** The bundled games of one folder, in name order. */
export type LibraryGroup = { folder: string; roms: LibraryRom[] };
/** The library's system chips: everything, or one family (GB includes Game Boy Color). */
export type LibrarySystem = "all" | "NES" | "GB" | "GBA" | "SNES";

/** Groups the bundled ROMs by folder; a file whose system is unknown is left out. */
export function groupLibrary(entries: RomListEntry[], kindFor: (name: string) => ConsoleKind | null): LibraryGroup[] {
    const byFolder = new Map<string, LibraryRom[]>();
    for (const { path, url } of entries) {
        const kind = kindFor(path);
        if (!kind) continue;
        const slash = path.lastIndexOf("/");
        const folder = slash >= 0 ? path.slice(0, slash) : "";
        const { system, title, file } = gameTitle(path, kind);
        const roms = byFolder.get(folder) ?? [];
        roms.push({ title, file, path, url, system });
        byFolder.set(folder, roms);
    }
    return [...byFolder.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([folder, roms]) => ({ folder, roms: roms.sort((a, b) => a.file.localeCompare(b.file)) }));
}

/** The groups that still hold a game matching the search (name or folder) and the system chip. */
export function filterLibrary(groups: LibraryGroup[], { query, system }: { query: string; system: LibrarySystem }): LibraryGroup[] {
    const needle = query.trim().toLowerCase();
    const inSystem = (rom: LibraryRom) => system === "all" || rom.system === system || (system === "GB" && rom.system === "GBC");
    return groups
        .map((group) => ({
            folder: group.folder,
            roms: group.roms.filter((rom) => inSystem(rom) && (needle === "" || rom.path.toLowerCase().includes(needle))),
        }))
        .filter((group) => group.roms.length > 0);
}

/** Draws the groups into `list`: a heading per folder over one button per game. */
export function renderLibrary(list: HTMLElement, groups: LibraryGroup[], query: string, onPick: (rom: LibraryRom) => void): void {
    if (groups.length === 0) {
        const empty = document.createElement("p");
        empty.className = "library-empty";
        empty.textContent = query.trim() ? `No test ROMs match “${query.trim()}”.` : "No test ROMs are bundled with this build.";
        list.replaceChildren(empty);
        return;
    }
    list.replaceChildren(
        ...groups.map((group) => {
            const section = document.createElement("div");
            section.className = "library-group";
            if (group.folder) {
                const title = document.createElement("div");
                title.className = "library-group-title";
                title.textContent = group.folder;
                section.append(title);
            }
            for (const rom of group.roms) {
                const item = document.createElement("button");
                item.type = "button";
                item.className = "library-item";
                item.dataset.path = rom.path;
                const badge = document.createElement("span");
                badge.className = "sys-badge";
                badge.dataset.system = rom.system;
                badge.textContent = rom.system;
                const name = document.createElement("span");
                name.className = "library-item-name";
                name.textContent = rom.title;
                const extension = document.createElement("span");
                extension.className = "library-item-ext";
                extension.textContent = rom.file.slice(rom.title.length);
                item.append(badge, name, extension);
                item.addEventListener("click", () => onPick(rom));
                section.append(item);
            }
            return section;
        })
    );
}

/** The release news as a wave: one span per letter, numbered for the CSS animation. */
export function renderNewsWave(el: HTMLElement, text: string): void {
    el.setAttribute("aria-label", text);
    el.replaceChildren(
        ...[...text].map((letter, index) => {
            const span = document.createElement("span");
            span.textContent = letter === " " ? " " : letter;
            span.style.setProperty("--i", String(index));
            span.setAttribute("aria-hidden", "true");
            return span;
        })
    );
}
