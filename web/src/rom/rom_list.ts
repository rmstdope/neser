import { isSupportedWebRomName } from "./rom_extensions";

export function parseDirectoryListing(html: string) {
    const dirs = [];
    const roms = [];
    const hrefRegex = /href\s*=\s*["']([^"']+)["']/gi;
    let match;
    while ((match = hrefRegex.exec(html)) !== null) {
        const href = match[1] || "";
        if (!href || href === "../") continue;
        if (href.endsWith("/")) {
            dirs.push(href);
        } else if (isSupportedWebRomName(href)) {
            roms.push(href);
        }
    }

    return {
        dirs: Array.from(new Set(dirs)).sort(),
        roms: Array.from(new Set(roms)).sort()
    };
}

export async function fetchRomList(baseUrl: string, fetchFn: typeof fetch = fetch, maxDepth = 4) {
    const baseRoot = new URL(baseUrl);
    const basePath = baseRoot.pathname.endsWith("/") ? baseRoot.pathname : `${baseRoot.pathname}/`;
    const basePathNoSlash = basePath.replace(/^\/+/, "");
    const queue = [{ url: baseRoot.toString(), depth: 0 }];
    const results = [];
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

    const sorted = results.sort((a, b) => a.path.localeCompare(b.path));
    if (sorted.length > 0) {
        return sorted;
    }

    const manifestUrl = new URL("roms.json", baseRoot).toString();
    const manifestResponse = await fetchFn(manifestUrl);
    if (!manifestResponse.ok) {
        return [];
    }
    const manifest = await manifestResponse.json();
    const roms = Array.isArray(manifest?.roms) ? manifest.roms : [];
    return roms
        .filter((rom: string) => typeof rom === "string" && isSupportedWebRomName(rom))
        .map((rom: string) => ({
            path: rom.replace(/^\//, ""),
            url: new URL(rom.replace(/^\//, ""), baseRoot).toString()
        }))
        .sort((a: { path: string }, b: { path: string }) => a.path.localeCompare(b.path));
}
