import { expect, it } from "vitest";
import { formatWatchEntry, parseWatchAddressInput } from "./debugger_watch.js";

it("formatWatchEntry includes hex address, hex value, and binary bits", () => {
    const row = formatWatchEntry(0x0010, 0x7F);
    expect(row.includes("$0010")).toBeTruthy();
    expect(row.includes("$7F")).toBeTruthy();
    expect(row.includes("0b01111111")).toBeTruthy();
});

it("parseWatchAddressInput accepts hex with and without prefix", () => {
    expect(parseWatchAddressInput("0010")).toBe(0x0010);
    expect(parseWatchAddressInput("0x00FF")).toBe(0x00FF);
});

it("parseWatchAddressInput rejects invalid values", () => {
    expect(parseWatchAddressInput("GG")).toBe(null);
    expect(parseWatchAddressInput("")).toBe(null);
});
