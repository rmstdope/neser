// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { afterEach, expect, it, vi } from "vitest";

import { createCrosshair } from "./crosshair";

const currentDir = dirname(fileURLToPath(import.meta.url));
const mainCss = readFileSync(resolve(currentDir, "../../main.css"), "utf8");

afterEach(() => {
    vi.restoreAllMocks();
    document.head.innerHTML = "";
    document.body.innerHTML = "";
});

// nr-use: the shell's stylesheet paints every canvas black, and the sight's overlay is a canvas
// laid over the picture, so a captured light gun hid the whole game behind a black sheet.
it("Given the shell's stylesheet, When the light-gun sight's overlay is created over the picture, Then the overlay is see-through", () => {
    // jsdom has no 2D context; creating the sight draws nothing, so an empty one is enough.
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
        {} as CanvasRenderingContext2D,
    );
    const style = document.createElement("style");
    style.textContent = mainCss;
    document.head.appendChild(style);
    const wrap = document.createElement("div");
    wrap.className = "screen-wrap";
    const screen = document.createElement("canvas");
    screen.id = "screen";
    wrap.appendChild(screen);
    document.body.appendChild(wrap);

    createCrosshair(screen);

    const overlay = wrap.querySelectorAll("canvas")[1];
    expect(overlay).toBeDefined();
    expect(getComputedStyle(screen).backgroundColor).toBe("rgb(0, 0, 0)");
    expect(getComputedStyle(overlay).backgroundColor).toBe("rgba(0, 0, 0, 0)");
});
