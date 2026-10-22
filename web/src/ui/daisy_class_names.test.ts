import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

// DaisyUI styles any element carrying one of its class names, whatever we meant by it: our own
// `status` line became a square (DaisyUI's status dot), and then `status-error` painted the
// line's background the colour of its text. So every class the page or the code uses that
// DaisyUI also defines must belong to a component we use on purpose.
const DELIBERATE_DAISYUI_COMPONENTS = [
    "alert", "badge", "btn", "checkbox", "drawer", "file-input", "kbd", "label", "modal", "range", "select", "text",
];

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const daisyCss = readFileSync(resolve(webDir, "../node_modules/daisyui/daisyui.css"), "utf8");
const daisyClasses = new Set([...daisyCss.matchAll(/\.(-?[a-zA-Z_][\w-]*)/g)].map((m) => m[1]));

function sourceFiles(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) return sourceFiles(path);
        return entry.name.endsWith(".ts") && !entry.name.endsWith(".test.ts") ? [path] : [];
    });
}

function classesWeUse(): Set<string> {
    const used = new Set<string>();
    const html = readFileSync(resolve(webDir, "index.html"), "utf8");
    for (const [, list] of html.matchAll(/class="([^"]*)"/g)) list.split(/\s+/).filter(Boolean).forEach((c) => used.add(c));
    for (const file of sourceFiles(resolve(webDir, "src"))) {
        const code = readFileSync(file, "utf8");
        for (const [, name] of code.matchAll(/classList\.(?:add|remove|toggle|contains)\(\s*"([^"]+)"/g)) used.add(name);
        for (const [, list] of code.matchAll(/className\s*=\s*"([^"]+)"/g)) list.split(/\s+/).forEach((c) => used.add(c));
    }
    return used;
}

describe("class names shared with DaisyUI", () => {
    it("Given every class the page and the code use, When DaisyUI defines it too, Then it belongs to a component we use on purpose", () => {
        const accidental = [...classesWeUse()]
            .filter((name) => daisyClasses.has(name))
            .filter((name) => !DELIBERATE_DAISYUI_COMPONENTS.some((c) => name === c || name.startsWith(`${c}-`)));
        expect(accidental).toEqual([]);
    });
});
