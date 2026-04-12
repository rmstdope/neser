import { expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(join(__dirname, "..", "..", "styles.css"), "utf8");

function extractRule(css: string, selector: string) {
    const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const match = css.match(new RegExp(escapedSelector + "\\s*\\{([^}]*)\\}"));
    return match ? match[1] : null;
}

it("debugger left panel (.debugger-disasm) uses flex: 35 for 35% proportional width", () => {
    const rule = extractRule(css, ".debugger-disasm");
    expect(rule).toBeTruthy();
    // Should use flex: 35 (not flex: 1) so the left panel occupies 35% of space
    expect(/\bflex\s*:\s*35\b\s*;?/.test(rule!)).toBeTruthy();
});

it("debugger right panel (.debugger-regs) uses flex: 65 for 65% proportional width", () => {
    const rule = extractRule(css, ".debugger-regs");
    expect(rule).toBeTruthy();
    // Should use flex: 65 (not width: 300px) so the right panel occupies 65% of space
    expect(/\bflex\s*:\s*65\b\s*;?/.test(rule!)).toBeTruthy();
    // Should not have a fixed width that overrides the proportional split
    expect(rule!.includes("width: 300px")).toBe(false);
});
