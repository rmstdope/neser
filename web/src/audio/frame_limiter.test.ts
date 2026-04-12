import { expect, it } from "vitest";
import { createFrameLimiter } from "./frame_limiter";

const FRAME_MS = 1000 / 60;

it("frame limiter throttles above 60Hz", () => {
    const limiter = createFrameLimiter(60);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(8)).toBe(false);
    expect(limiter.shouldRender(16)).toBe(false);
    expect(limiter.shouldRender(17)).toBe(true);
});

it("frame limiter allows resume after reset", () => {
    const limiter = createFrameLimiter(60);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(8)).toBe(false);

    limiter.reset();

    expect(limiter.shouldRender(0)).toBe(true);
});

it("frame limiter stays stable under jitter", () => {
    const limiter = createFrameLimiter(60);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(10)).toBe(false);
    expect(limiter.shouldRender(20)).toBe(true);
    expect(limiter.shouldRender(28)).toBe(false);
    expect(limiter.shouldRender(40)).toBe(true);
});

it("frame limiter updates target fps", () => {
    const limiter = createFrameLimiter(60);

    limiter.setTargetFps(50);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(10)).toBe(false);
    expect(limiter.shouldRender(20)).toBe(true);
});
