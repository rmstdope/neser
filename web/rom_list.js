export function parseDirectoryListing(html) {
    const dirs = [];
    const roms = [];
    const hrefRegex = /href\s*=\s*["']([^"']+)["']/gi;
    let match;
    while ((match = hrefRegex.exec(html)) !== null) {
        const href = match[1] || "";
        if (!href || href === "../") continue;
        if (href.endsWith("/")) {
            dirs.push(href);
        } else if (href.toLowerCase().endsWith(".nes")) {
            roms.push(href);
        }
    }

    return {
        dirs: Array.from(new Set(dirs)).sort(),
        roms: Array.from(new Set(roms)).sort()
    };
}

export async function fetchRomList(baseUrl, fetchFn = fetch, maxDepth = 4) {
    const baseRoot = new URL(baseUrl);
    const basePath = baseRoot.pathname.endsWith("/") ? baseRoot.pathname : `${baseRoot.pathname}/`;
    const queue = [{ url: baseRoot.toString(), depth: 0 }];
    const results = [];
    const visited = new Set();

    while (queue.length > 0) {
        const { url, depth } = queue.shift();
        if (visited.has(url)) continue;
        visited.add(url);

        const response = await fetchFn(url);
        if (!response.ok) continue;
        const html = await response.text();
        const { dirs, roms } = parseDirectoryListing(html);

        for (const rom of roms) {
            const resolved = new URL(rom, url);
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
                const resolved = new URL(dir, url);
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
        .filter((rom) => typeof rom === "string" && rom.toLowerCase().endsWith(".nes"))
        .map((rom) => ({
            path: rom.replace(/^\//, ""),
            url: new URL(rom.replace(/^\//, ""), baseRoot).toString()
        }))
        .sort((a, b) => a.path.localeCompare(b.path));
}
