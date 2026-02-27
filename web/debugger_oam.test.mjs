import assert from "node:assert/strict";
import test from "node:test";
import { formatOamEntry, buildOamHtml } from "./debugger_oam.js";

test("formatOamEntry includes index and all fields in hex", () => {
    const entry = formatOamEntry(5, 0x20, 0xAB, 0x03, 0x40);
    assert.ok(entry.includes("05"), "should contain sprite index 05");
    assert.ok(entry.includes("20"), "should contain Y=20");
    assert.ok(entry.includes("AB"), "should contain tile=AB");
    assert.ok(entry.includes("03"), "should contain attr=03");
    assert.ok(entry.includes("40"), "should contain X=40");
});

test("buildOamHtml returns a string containing an oam title", () => {
    const oam = new Array(256).fill(0);
    const html = buildOamHtml(oam);
    assert.ok(typeof html === "string");
    assert.ok(html.length > 0, "should produce non-empty HTML");
    assert.ok(html.toLowerCase().includes("oam"), "should mention OAM in the output");
});

test("buildOamHtml renders 64 sprite entries", () => {
    const oam = new Array(256).fill(0);
    // Sprite 3 at offset 12: Y=0xBB, tile=0xCC, attr=0x01, X=0xDD
    oam[12] = 0xBB;
    oam[13] = 0xCC;
    oam[14] = 0x01;
    oam[15] = 0xDD;
    const html = buildOamHtml(oam);
    assert.ok(html.includes("BB"), "should render sprite Y field");
    assert.ok(html.includes("CC"), "should render sprite tile field");
    assert.ok(html.includes("DD"), "should render sprite X field");
});

test("buildOamHtml returns empty string for missing oam data", () => {
    const html = buildOamHtml(null);
    assert.strictEqual(html, "");
});
