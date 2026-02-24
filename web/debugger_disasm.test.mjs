import assert from "node:assert/strict";
import test from "node:test";
import { renderDisasmLines } from "./debugger_disasm.js";

test("renderDisasmLines returns contiguous block rows without separator newlines", () => {
    const html = renderDisasmLines([
        { addr: 0x8000, bytes: [0xA9, 0x01], text: "LDA #$01", is_current: true },
        { addr: 0x8002, bytes: [0x8D, 0x00, 0x02], text: "STA $0200", is_current: false }
    ]);

    assert.ok(html.includes("</span><span"));
    assert.equal(html.includes("\n"), false);
});