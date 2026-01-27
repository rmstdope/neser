export function parseDirectoryListing(html) {
    const doc = new DOMParser().parseFromString(html, "text/html");
    const anchors = Array.from(doc.querySelectorAll("a"));
    const dirs = [];
    const roms = [];

    for (const anchor of anchors) {
        const href = anchor.getAttribute("href") || "";
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
    const queue = [{ path: "", depth: 0 }];
    const results = [];
    const visited = new Set();

    while (queue.length > 0) {
        const { path, depth } = queue.shift();
        const url = new URL(path, baseUrl).toString();
        if (visited.has(url)) continue;
        visited.add(url);

        const response = await fetchFn(url);
        if (!response.ok) continue;
        const html = await response.text();
        const { dirs, roms } = parseDirectoryListing(html);

        for (const rom of roms) {
            results.push({ path: `${path}${rom}`, url: new URL(`${path}${rom}`, baseUrl).toString() });
        }

        if (depth < maxDepth) {
            for (const dir of dirs) {
                queue.push({ path: `${path}${dir}`, depth: depth + 1 });
            }
        }
    }

    return results.sort((a, b) => a.path.localeCompare(b.path));
}
