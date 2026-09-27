// Node-only: imported by vite.config.ts to write web/roms/roms.json, never by the browser app.
import { readdirSync, realpathSync, statSync, type Stats } from "fs";
import { join, relative, isAbsolute } from "path";
import { isSupportedWebRomName } from "./rom_extensions";

/**
 * Every ROM the web picker supports under `servedDir`, as sorted slash-separated paths relative to it.
 *
 * The served tree is made of symlinks into the repository's `roms/` (web/roms/automated_tests ->
 * ../../roms/nes/automated_tests), so each top-level entry is followed and its own resolved target
 * is the containment root for everything beneath it: a nested symlink leaving that tree is skipped,
 * and a directory already walked is never walked again.
 */
export function findRomFiles(servedDir: string): string[] {
    const results: string[] = [];
    const visited = new Set<string>();
    for (const entry of readdirSync(servedDir)) {
        const fullPath = join(servedDir, entry);
        const rootReal = tryRealpath(fullPath);
        if (rootReal) walk(fullPath, entry, rootReal, visited, results);
    }
    return results.sort();
}

function walk(path: string, relPath: string, rootReal: string, visited: Set<string>, results: string[]) {
    const real = tryRealpath(path);
    const stat = tryStat(path);
    if (!real || !stat || !isWithin(rootReal, real)) return;

    if (stat.isDirectory()) {
        if (visited.has(real)) return;
        visited.add(real);
        for (const entry of readdirSync(path)) {
            walk(join(path, entry), `${relPath}/${entry}`, rootReal, visited, results);
        }
    } else if (stat.isFile() && isSupportedWebRomName(relPath)) {
        results.push(relPath);
    }
}

function isWithin(rootReal: string, targetReal: string): boolean {
    const rel = relative(rootReal, targetReal);
    return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

function tryRealpath(path: string): string | undefined {
    try { return realpathSync(path); } catch { return undefined; }
}

function tryStat(path: string): Stats | undefined {
    try { return statSync(path); } catch { return undefined; }
}
