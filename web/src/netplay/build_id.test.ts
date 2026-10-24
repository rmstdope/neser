import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { THIS_BUILD, buildLabel, sameBuild } from "./build_id";

describe("the build a network game compares", () => {
    it("is this checkout's NESER version", () => {
        const cargo = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../../Cargo.toml"), "utf8");
        const version = /^version\s*=\s*"([^"]+)"/m.exec(cargo)![1];
        expect(THIS_BUILD.version).toBe(version);
    });

    it("carries a short commit hash", () => {
        expect(THIS_BUILD.commit).toMatch(/^([0-9a-f]{7,40}|unknown)$/);
    });

    it("is named as the version followed by the commit in brackets", () => {
        expect(buildLabel({ version: "1.3.0", commit: "4f2a9c1" })).toBe("1.3.0 (4f2a9c1)");
    });

    it("matches only when both the version and the commit match", () => {
        const a = { version: "1.3.0", commit: "4f2a9c1" };
        expect(sameBuild(a, { ...a })).toBe(true);
        expect(sameBuild(a, { ...a, commit: "b81e07d" })).toBe(false);
        expect(sameBuild(a, { ...a, version: "1.3.1" })).toBe(false);
    });
});
