import test from "node:test";
import assert from "node:assert/strict";
import { createFrameLimiter } from "./frame_limiter.js";

const FRAME_MS = 1000 / 60;

test("frame limiter throttles above 60Hz", () => {
    const limiter = createFrameLimiter(60);

    assert.equal(limiter.shouldRender(0), true);
    assert.equal(limiter.shouldRender(8), false);
    assert.equal(limiter.shouldRender(16), false);
    assert.equal(limiter.shouldRender(17), true);
});

test("frame limiter allows resume after reset", () => {
    const limiter = createFrameLimiter(60);

    assert.equal(limiter.shouldRender(0), true);
    assert.equal(limiter.shouldRender(8), false);

    limiter.reset();

    assert.equal(limiter.shouldRender(0), true);
});

test("frame limiter stays stable under jitter", () => {
    const limiter = createFrameLimiter(60);

    assert.equal(limiter.shouldRender(0), true);
    assert.equal(limiter.shouldRender(10), false);
    assert.equal(limiter.shouldRender(20), true);
    assert.equal(limiter.shouldRender(28), false);
    assert.equal(limiter.shouldRender(40), true);
});

test("frame limiter updates target fps", () => {
    const limiter = createFrameLimiter(60);

    limiter.setTargetFps(50);

    assert.equal(limiter.shouldRender(0), true);
    assert.equal(limiter.shouldRender(10), false);
    assert.equal(limiter.shouldRender(20), true);
});
