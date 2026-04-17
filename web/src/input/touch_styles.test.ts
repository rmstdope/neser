import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const currentDir = dirname(fileURLToPath(import.meta.url));
const mainCss = readFileSync(resolve(currentDir, "../../main.css"), "utf8");

describe("touch controls stylesheet", () => {
    it("Given the touch stylesheet, When styling movement controls, Then it defines joystick visuals and no longer styles segmented d-pad buttons", () => {
        expect(mainCss).toContain(".touch-joystick");
        expect(mainCss).toContain(".touch-joystick-knob");
        expect(mainCss).toContain("var(--touch-stick-x)");
        expect(mainCss).toContain("var(--touch-stick-y)");
        expect(mainCss).not.toContain(".touch-dpad-btn");
    });
});