import test from "node:test";
import assert from "node:assert/strict";
import { clampScrollTop, sanitizeScrollTop } from "./ppu_viewer_scroll.js";

test("sanitizeScrollTop normalizes invalid values to zero", () => {
    assert.equal(sanitizeScrollTop(undefined), 0);
    assert.equal(sanitizeScrollTop(Number.NaN), 0);
    assert.equal(sanitizeScrollTop(-5), 0);
});

test("sanitizeScrollTop preserves finite non-negative values", () => {
    assert.equal(sanitizeScrollTop(0), 0);
    assert.equal(sanitizeScrollTop(42), 42);
});

test("clampScrollTop keeps value within scrollable range", () => {
    assert.equal(clampScrollTop(50, { scrollHeight: 300, clientHeight: 100 }), 50);
    assert.equal(clampScrollTop(500, { scrollHeight: 300, clientHeight: 100 }), 200);
    assert.equal(clampScrollTop(-10, { scrollHeight: 300, clientHeight: 100 }), 0);
});

test("clampScrollTop handles non-scrollable containers", () => {
    assert.equal(clampScrollTop(50, { scrollHeight: 100, clientHeight: 100 }), 0);
});
