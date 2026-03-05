import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(join(__dirname, "styles.css"), "utf8");

function extractRule(css, selector) {
    const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const match = css.match(new RegExp(escapedSelector + "\\s*\\{([^}]*)\\}"));
    return match ? match[1] : null;
}

test("debugger left panel (.debugger-disasm) uses flex: 35 for 35% proportional width", () => {
    const rule = extractRule(css, ".debugger-disasm");
    assert.ok(rule, ".debugger-disasm rule not found in styles.css");
    // Should use flex: 35 (not flex: 1) so the left panel occupies 35% of space
    assert.ok(rule.includes("flex: 35"), `.debugger-disasm should have flex: 35 but got: ${rule.trim()}`);
});

test("debugger right panel (.debugger-regs) uses flex: 65 for 65% proportional width", () => {
    const rule = extractRule(css, ".debugger-regs");
    assert.ok(rule, ".debugger-regs rule not found in styles.css");
    // Should use flex: 65 (not width: 300px) so the right panel occupies 65% of space
    assert.ok(rule.includes("flex: 65"), `.debugger-regs should have flex: 65 but got: ${rule.trim()}`);
    // Should not have a fixed width that overrides the proportional split
    assert.ok(!rule.includes("width: 300px"), `.debugger-regs should not have fixed width: 300px`);
});
