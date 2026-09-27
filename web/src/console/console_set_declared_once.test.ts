import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { expect, it } from "vitest";

const WEB_SRC = join(__dirname, "..");
const TABLE = join("console", "consoles.ts");
/** Two console names joined into a string-literal union, e.g. `"nes" | "gb"`. */
const CONSOLE_UNION = /"(?:nes|gb|gba|snes)"\s*\|\s*"(?:nes|gb|gba|snes)"/;

function sourceFiles(dir: string): string[] {
    return readdirSync(dir).flatMap((name) => {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) return sourceFiles(path);
        return name.endsWith(".ts") && !name.endsWith(".test.ts") ? [path] : [];
    });
}

it("declares the set of consoles only in the console table", () => {
    const offenders = sourceFiles(WEB_SRC)
        .filter((path) => relative(WEB_SRC, path) !== TABLE)
        .filter((path) => CONSOLE_UNION.test(readFileSync(path, "utf8")))
        .map((path) => relative(WEB_SRC, path));
    expect(offenders).toEqual([]);
});
