/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const indexHtml = readFileSync(resolve(webDir, "index.html"), "utf8");
const mainCss = readFileSync(resolve(webDir, "main.css"), "utf8");
const page = new DOMParser().parseFromString(indexHtml, "text/html");

describe("Steel Glow theme", () => {
    it("Given the page, When it loads, Then it uses the steel theme and no other DaisyUI theme", () => {
        expect(page.documentElement.dataset.theme).toBe("steel");
        expect(mainCss).toMatch(/@plugin "daisyui" \{\s*themes: false;\s*\}/);
        expect(mainCss).toMatch(/@plugin "daisyui\/theme" \{[^}]*name: "steel";/);
    });

    it("Given the top bar, When reading the logo, Then it is the steel mark and the wordmark", () => {
        const logo = page.querySelector("header .neser-logo");
        expect(logo?.querySelector(".neser-logo-mark")?.textContent).toBe("N");
        expect(logo?.querySelector(".neser-logo-word")?.textContent).toBe("NESER");
        expect(page.querySelector("pre")).toBeNull();
    });

    it("Given the emulation controls, When reading them, Then Start is the one primary button and none use the old traffic-light colours", () => {
        expect(page.getElementById("start")?.classList.contains("btn-primary")).toBe(true);
        expect(page.getElementById("start")!.closest("#library")).not.toBeNull();
        expect(page.querySelectorAll("#emulation-controls .btn-primary")).toHaveLength(0);
        for (const id of ["pause", "reset", "stop", "save-state", "load-state", "autorun-load"]) {
            const classes = page.getElementById(id)!.classList;
            expect([...classes].filter((c) => /^btn-(success|warning|error|info)$/.test(c)), id).toEqual([]);
        }
    });
});
