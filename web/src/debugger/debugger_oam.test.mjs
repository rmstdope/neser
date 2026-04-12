import { expect, it } from "vitest";
import { formatOamEntry, buildOamHtml } from "./debugger_oam.js";

it("formatOamEntry includes index and all fields in hex", () => {
    const entry = formatOamEntry(5, 0x20, 0xAB, 0x03, 0x40);
    expect(entry.includes("05"), "should contain sprite index 05").toBeTruthy();
    expect(entry.includes("20"), "should contain Y=20").toBeTruthy();
    expect(entry.includes("AB"), "should contain tile=AB").toBeTruthy();
    expect(entry.includes("03"), "should contain attr=03").toBeTruthy();
    expect(entry.includes("40"), "should contain X=40").toBeTruthy();
});

it("buildOamHtml returns a string containing an oam title", () => {
    const oam = new Array(256).fill(0);
    const html = buildOamHtml(oam);
    expect(typeof html === "string").toBeTruthy();
    expect(html.length > 0, "should produce non-empty HTML").toBeTruthy();
    expect(html.toLowerCase().includes("oam"), "should mention OAM in the output").toBeTruthy();
});

it("buildOamHtml renders 64 sprite entries", () => {
    const oam = new Array(256).fill(0);
    // Sprite 3 at offset 12: Y=0xBB, tile=0xCC, attr=0x01, X=0xDD
    oam[12] = 0xBB;
    oam[13] = 0xCC;
    oam[14] = 0x01;
    oam[15] = 0xDD;
    const html = buildOamHtml(oam);
    expect(html.includes("BB"), "should render sprite Y field").toBeTruthy();
    expect(html.includes("CC"), "should render sprite tile field").toBeTruthy();
    expect(html.includes("DD"), "should render sprite X field").toBeTruthy();
});

it("buildOamHtml returns empty string for missing oam data", () => {
    const html = buildOamHtml(null);
    expect(html).toBe("");
});
