import { afterEach, beforeEach, expect, it } from "vitest";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { findRomFiles } from "./rom_manifest";

let root: string;

function touch(path: string) {
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, "");
}

beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "rom-manifest-"));
});

afterEach(() => {
    rmSync(root, { recursive: true, force: true });
});

it("findRomFiles follows the top-level symlinks the served ROM tree is made of", () => {
    // web/roms/automated_tests -> ../../roms/nes/automated_tests, as in the repository.
    touch(join(root, "roms", "nes", "automated_tests", "cpu", "instr.nes"));
    touch(join(root, "roms", "nes", "automated_tests", "gb", "blargg.gb"));
    touch(join(root, "roms", "nes", "manual_tests", "game.sfc"));
    const served = join(root, "web", "roms");
    mkdirSync(served, { recursive: true });
    symlinkSync("../../roms/nes/automated_tests", join(served, "automated_tests"));
    symlinkSync("../../roms/nes/manual_tests", join(served, "manual_tests"));

    expect(findRomFiles(served)).toEqual([
        "automated_tests/cpu/instr.nes",
        "automated_tests/gb/blargg.gb",
        "manual_tests/game.sfc"
    ]);
});

it("findRomFiles lists every extension the web picker supports and nothing else", () => {
    const served = join(root, "served");
    for (const name of ["a.nes", "b.gb", "c.gbc", "d.cgb", "e.gba", "f.sfc", "g.smc", "notes.txt", "roms.json"]) {
        touch(join(served, name));
    }

    expect(findRomFiles(served)).toEqual(["a.nes", "b.gb", "c.gbc", "d.cgb", "e.gba", "f.sfc", "g.smc"]);
});

it("findRomFiles does not follow a nested symlink out of its top-level tree, nor loop", () => {
    touch(join(root, "outside", "secret.nes"));
    touch(join(root, "tree", "inner", "kept.nes"));
    symlinkSync(join(root, "outside"), join(root, "tree", "inner", "escape"));
    symlinkSync(join(root, "tree"), join(root, "tree", "inner", "loop"));
    const served = join(root, "served");
    mkdirSync(served);
    symlinkSync(join(root, "tree"), join(served, "tree"));

    expect(findRomFiles(served)).toEqual(["tree/inner/kept.nes"]);
});
