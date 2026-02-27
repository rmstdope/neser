import assert from "node:assert/strict";
import test from "node:test";
import { formatWatchEntry, parseWatchAddressInput } from "./debugger_watch.js";

test("formatWatchEntry includes hex address, hex value, and binary bits", () => {
    const row = formatWatchEntry(0x0010, 0x7F);
    assert.ok(row.includes("$0010"));
    assert.ok(row.includes("$7F"));
    assert.ok(row.includes("0b01111111"));
});

test("parseWatchAddressInput accepts hex with and without prefix", () => {
    assert.strictEqual(parseWatchAddressInput("0010"), 0x0010);
    assert.strictEqual(parseWatchAddressInput("0x00FF"), 0x00FF);
});

test("parseWatchAddressInput rejects invalid values", () => {
    assert.strictEqual(parseWatchAddressInput("GG"), null);
    assert.strictEqual(parseWatchAddressInput(""), null);
});
