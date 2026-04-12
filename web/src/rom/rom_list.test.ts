import { expect, it } from "vitest";
import { fetchRomList, parseDirectoryListing } from "./rom_list";

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
