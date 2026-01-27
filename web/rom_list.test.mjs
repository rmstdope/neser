import assert from "node:assert/strict";
import test from "node:test";
import { fetchRomList, parseDirectoryListing } from "./rom_list.js";

test("parseDirectoryListing extracts dirs and roms", () => {
    const html = `
        <html><body>
            <a href="../">../</a>
            <a href="cpu_reset/">cpu_reset/</a>
            <a href="ram_after_reset.nes">ram_after_reset.nes</a>
            <a href="notes.txt">notes.txt</a>
        </body></html>
    `;
    const { dirs, roms } = parseDirectoryListing(html);
    assert.deepEqual(dirs, ["cpu_reset/"]);
    assert.deepEqual(roms, ["ram_after_reset.nes"]);
});

test("fetchRomList walks directories and returns rom entries", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, `
            <a href="../">../</a>
            <a href="cpu_reset/">cpu_reset/</a>
            <a href="root.nes">root.nes</a>
        `],
        [`${base}cpu_reset/`, `
            <a href="../">../</a>
            <a href="ram_after_reset.nes">ram_after_reset.nes</a>
            <a href="readme.txt">readme.txt</a>
        `]
    ]);

    const fetchFn = async (url) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn, 4);
    const paths = entries.map((entry) => entry.path);
    assert.deepEqual(paths, ["cpu_reset/ram_after_reset.nes", "root.nes"]);
});
