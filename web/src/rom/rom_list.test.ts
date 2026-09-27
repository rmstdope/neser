import { beforeEach, expect, it } from "vitest";
import { installRomExtensionTable } from "./rom_extensions";
import { ROM_EXTENSION_TABLE } from "./rom_extension_table.fixture";
import { fetchRomList, parseDirectoryListing } from "./rom_list";

beforeEach(() => {
    installRomExtensionTable(ROM_EXTENSION_TABLE);
});

it("parseDirectoryListing extracts dirs and roms", () => {
    const html = `
        <html><body>
            <a href="../">../</a>
            <a href="cpu_reset/">cpu_reset/</a>
            <a href="ram_after_reset.nes">ram_after_reset.nes</a>
            <a href="notes.txt">notes.txt</a>
        </body></html>
    `;
    const { dirs, roms } = parseDirectoryListing(html);
    expect(dirs).toEqual(["cpu_reset/"]);
    expect(roms).toEqual(["ram_after_reset.nes"]);
});

it("fetchRomList walks directories and returns rom entries", async () => {
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

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toEqual(["cpu_reset/ram_after_reset.nes", "root.nes"]);
});

it("fetchRomList uses manifest when directory listing unavailable", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, ""],
        [`${base}roms.json`, JSON.stringify({
            roms: ["cpu_reset/ram_after_reset.nes", "root.nes"]
        })]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return {
            ok: true,
            text: async () => responses.get(key),
            json: async () => JSON.parse(responses.get(key)!)
        };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toEqual(["cpu_reset/ram_after_reset.nes", "root.nes"]);
});

it("fetchRomList normalizes base-relative hrefs", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, `
            <a href="roms/automated_tests/">automated_tests/</a>
        `],
        [`${base}automated_tests/`, `
            <a href="ram_after_reset.nes">ram_after_reset.nes</a>
        `]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toEqual(["automated_tests/ram_after_reset.nes"]);
});

it("fetchRomList avoids duplicating base paths", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, `
            <a href="automated_tests/">automated_tests/</a>
        `],
        [`${base}automated_tests/`, `
            <a href="instr_test-v5/">instr_test-v5/</a>
        `],
        [`${base}automated_tests/instr_test-v5/`, `
            <a href="roms/automated_tests/">automated_tests/</a>
            <a href="nestest.nes">nestest.nes</a>
        `]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (key.includes("/roms/automated_tests/instr_test-v5/roms/automated_tests/")) {
            throw new Error("duplicated base path detected");
        }
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toEqual(["automated_tests/instr_test-v5/nestest.nes"]);
});

// ── Game Boy (.gb) support ──────────────────────────────────────────────────

it("parseDirectoryListing includes .gb files alongside .nes", () => {
    const html = `
        <html><body>
            <a href="../">../</a>
            <a href="tetris.gb">tetris.gb</a>
            <a href="game.nes">game.nes</a>
            <a href="notes.txt">notes.txt</a>
        </body></html>
    `;
    const { roms } = parseDirectoryListing(html);
    expect(roms).toContain("tetris.gb");
    expect(roms).toContain("game.nes");
});

it("parseDirectoryListing includes .gbc and .cgb files", () => {
    const html = `
        <html><body>
            <a href="game.gbc">game.gbc</a>
            <a href="camera.cgb">camera.cgb</a>
            <a href="game.gb">game.gb</a>
        </body></html>
    `;
    const { roms } = parseDirectoryListing(html);
    expect(roms).toContain("game.gbc");
    expect(roms).toContain("camera.cgb");
    expect(roms).toContain("game.gb");
});

it("parseDirectoryListing includes .gba files", () => {
    const html = `
        <html><body>
            <a href="suite.gba">suite.gba</a>
            <a href="game.nes">game.nes</a>
            <a href="notes.txt">notes.txt</a>
        </body></html>
    `;
    const { roms } = parseDirectoryListing(html);
    expect(roms).toContain("suite.gba");
    expect(roms).toContain("game.nes");
    expect(roms).not.toContain("notes.txt");
});

it("fetchRomList includes .gb entries from directory listing", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, `
            <a href="../">../</a>
            <a href="gb/">gb/</a>
            <a href="root.nes">root.nes</a>
        `],
        [`${base}gb/`, `
            <a href="../">../</a>
            <a href="tetris.gb">tetris.gb</a>
        `]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toContain("gb/tetris.gb");
    expect(paths).toContain("root.nes");
});

it("fetchRomList includes .gba entries from directory listing", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, `
            <a href="../">../</a>
            <a href="gba/">gba/</a>
            <a href="root.nes">root.nes</a>
        `],
        [`${base}gba/`, `
            <a href="../">../</a>
            <a href="suite.gba">suite.gba</a>
        `]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return { ok: true, text: async () => responses.get(key) };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toContain("gba/suite.gba");
    expect(paths).toContain("root.nes");
});

it("fetchRomList manifest fallback includes .gb entries", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, ""],
        [`${base}roms.json`, JSON.stringify({
            roms: ["tetris.gb", "game.nes", "color.gbc", "camera.cgb"]
        })]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return {
            ok: true,
            text: async () => responses.get(key),
            json: async () => JSON.parse(responses.get(key)!)
        };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toContain("tetris.gb");
    expect(paths).toContain("game.nes");
    expect(paths).toContain("color.gbc");
    expect(paths).toContain("camera.cgb");
});

it("fetchRomList manifest fallback includes .gba entries", async () => {
    const base = "https://example.com/roms/";
    const responses = new Map([
        [base, ""],
        [`${base}roms.json`, JSON.stringify({
            roms: ["suite.gba", "game.nes", "notes.txt"]
        })]
    ]);

    const fetchFn = async (url: any) => {
        const key = url.toString();
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "" };
        }
        return {
            ok: true,
            text: async () => responses.get(key),
            json: async () => JSON.parse(responses.get(key)!)
        };
    };

    const entries = await fetchRomList(base, fetchFn as any, 4);
    const paths = entries.map((entry: any) => entry.path);
    expect(paths).toContain("suite.gba");
    expect(paths).toContain("game.nes");
    expect(paths).not.toContain("notes.txt");
});

// ── Manifest first (nr-4vr) ─────────────────────────────────────────────────

function recordingFetch(responses: Map<string, string>) {
    const requested: string[] = [];
    const fetchFn = async (url: any) => {
        const key = url.toString();
        requested.push(key);
        if (!responses.has(key)) {
            return { ok: false, status: 404, text: async () => "", json: async () => { throw new Error("404"); } };
        }
        return {
            ok: true,
            text: async () => responses.get(key),
            json: async () => JSON.parse(responses.get(key)!)
        };
    };
    return { fetchFn, requested };
}

it("fetchRomList reads the manifest and fetches no directory listing when it lists ROMs", async () => {
    const base = "https://example.com/roms/";
    const { fetchFn, requested } = recordingFetch(new Map([
        [base, `<a href="crawled/">crawled/</a>`],
        [`${base}crawled/`, `<a href="crawled.nes">crawled.nes</a>`],
        [`${base}roms.json`, JSON.stringify({ roms: ["b/game.sfc", "a/test.nes"] })]
    ]));

    const entries = await fetchRomList(base, fetchFn as any, 4);

    expect(requested).toEqual([`${base}roms.json`]);
    expect(entries).toEqual([
        { path: "a/test.nes", url: `${base}a/test.nes` },
        { path: "b/game.sfc", url: `${base}b/game.sfc` }
    ]);
});

it("fetchRomList crawls when the manifest lists no ROMs", async () => {
    const base = "https://example.com/roms/";
    const { fetchFn, requested } = recordingFetch(new Map([
        [base, `<a href="crawled/">crawled/</a>`],
        [`${base}crawled/`, `<a href="crawled.nes">crawled.nes</a>`],
        [`${base}roms.json`, JSON.stringify({ roms: [] })]
    ]));

    const entries = await fetchRomList(base, fetchFn as any, 4);

    expect(requested[0]).toBe(`${base}roms.json`);
    expect(entries.map((entry: any) => entry.path)).toEqual(["crawled/crawled.nes"]);
});

it("fetchRomList crawls when the manifest is missing or malformed", async () => {
    const base = "https://example.com/roms/";
    for (const manifest of [undefined, "not json"]) {
        const responses = new Map([
            [base, `<a href="root.nes">root.nes</a>`]
        ]);
        if (manifest !== undefined) responses.set(`${base}roms.json`, manifest);
        const { fetchFn } = recordingFetch(responses);

        const entries = await fetchRomList(base, fetchFn as any, 4);

        expect(entries.map((entry: any) => entry.path)).toEqual(["root.nes"]);
    }
});

it("fetchRomList encodes manifest file names into URLs and keeps them readable as paths", async () => {
    const base = "https://example.com/roms/";
    const { fetchFn } = recordingFetch(new Map([
        [`${base}roms.json`, JSON.stringify({ roms: ["dir #1/100% (a?b).nes"] })]
    ]));

    const entries = await fetchRomList(base, fetchFn as any, 4);

    expect(entries).toEqual([
        { path: "dir #1/100% (a?b).nes", url: `${base}dir%20%231/100%25%20(a%3Fb).nes` }
    ]);
});
