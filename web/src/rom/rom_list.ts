import { isSupportedWebRomName } from "./rom_extensions";

export function parseDirectoryListing(html: string) {
    const dirs = new Set<string>();
    const roms = new Set<string>();
    const hrefRegex = /href\s*=\s*["']([^"']+)["']/gi;
    let match;
    while ((match = hrefRegex.exec(html)) !== null) {
        const href = match[1];
        if (!href || href === "../") continue;
        if (href.endsWith("/")) {
            dirs.add(href);
        } else if (isSupportedWebRomName(href)) {
            roms.add(href);
        }
    }

    return {
        dirs: Array.from(dirs).sort(),
        roms: Array.from(roms).sort()
    };
}

export interface RomListEntry {
    path: string;
    url: string;
}

/**
 * The ROMs served under `baseUrl`: read from the build's roms.json, or, when that is missing or
 * empty, by crawling the server's directory listings (up to `maxDepth` levels).
 */
export async function fetchRomList(
    baseUrl: string,
    fetchFn: typeof fetch = fetch,
    maxDepth = 4
): Promise<RomListEntry[]> {
    const fromManifest = await fetchManifestRomList(baseUrl, fetchFn);
    if (fromManifest.length > 0) {
        return fromManifest;
    }
    return crawlRomList(baseUrl, fetchFn, maxDepth);
}

async function fetchManifestRomList(baseUrl: string, fetchFn: typeof fetch): Promise<RomListEntry[]> {
    const baseRoot = new URL(baseUrl);
    let manifest: unknown;
    try {
        const response = await fetchFn(new URL("roms.json", baseRoot).toString());
        if (!response.ok) return [];
        manifest = await response.json();
    } catch {
        return [];
    }
    const roms: unknown[] = Array.isArray((manifest as { roms?: unknown })?.roms)
        ? (manifest as { roms: unknown[] }).roms
        : [];
    return roms
        .filter((rom): rom is string => typeof rom === "string" && isSupportedWebRomName(rom))
        .map((rom) => {
            const path = rom.replace(/^\//, "");
            const encoded = path.split("/").map(encodeURIComponent).join("/");
            return { path, url: new URL(encoded, baseRoot).toString() };
        })
        .sort((a, b) => a.path.localeCompare(b.path));
}

async function crawlRomList(baseUrl: string, fetchFn: typeof fetch, maxDepth: number): Promise<RomListEntry[]> {
    const baseRoot = new URL(baseUrl);
    const basePath = baseRoot.pathname.endsWith("/") ? baseRoot.pathname : `${baseRoot.pathname}/`;
    const basePathNoSlash = basePath.replace(/^\/+/, "");
    const queue = [{ url: baseRoot.toString(), depth: 0 }];
    const results: RomListEntry[] = [];
    const visited = new Set();

    const normalizeHref = (href: string) => {
        if (!href) return href;
        if (href.startsWith("http://") || href.startsWith("https://") || href.startsWith("/")) {
            return href;
        }
        const trimmed = href.replace(/^\.\//, "");
        if (trimmed.startsWith(basePathNoSlash)) {
            return `/${trimmed}`;
        }
        if (trimmed.startsWith(`roms/${basePathNoSlash}`)) {
            return `/${trimmed.slice("roms/".length)}`;
        }
        if (trimmed.startsWith("roms/")) {
            return `/${trimmed.slice("roms/".length)}`;
        }
        return trimmed;
    };

    while (queue.length > 0) {
        const { url, depth } = queue.shift()!;
        if (visited.has(url)) continue;
        visited.add(url);

        const response = await fetchFn(url);
        if (!response.ok) continue;
        const html = await response.text();
        const { dirs, roms } = parseDirectoryListing(html);

        for (const rom of roms) {
            const normalizedRom = normalizeHref(rom);
            const resolved = new URL(normalizedRom, url);
            if (resolved.origin !== baseRoot.origin) continue;
            if (!resolved.pathname.startsWith(basePath)) continue;
            const relativePath = resolved.pathname.slice(basePath.length);
            if (!relativePath) continue;
            results.push({
                path: relativePath,
                url: resolved.toString()
            });
        }

        if (depth < maxDepth) {
            for (const dir of dirs) {
                const normalizedDir = normalizeHref(dir);
                const resolved = new URL(normalizedDir, url);
                if (resolved.origin !== baseRoot.origin) continue;
                if (!resolved.pathname.startsWith(basePath)) continue;
                if (resolved.pathname === basePath) continue;
                const normalized = resolved.toString();
                queue.push({ url: normalized, depth: depth + 1 });
            }
        }
    }

    return results.sort((a, b) => a.path.localeCompare(b.path));
}
