import { expect, it } from "vitest";
import { createFrameLimiter } from "./frame_limiter";

const FRAME_MS = 1000 / 60;

it("frame limiter throttles above 60Hz", () => {
    const limiter = createFrameLimiter(60);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(8)).toBe(false);
    expect(limiter.shouldRender(16)).toBe(true);  // within jitter tolerance of 16.67ms
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

    // Simulate rAF jitter around 16.67ms target:
    // Frames arriving slightly early should still render (within tolerance)
    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(8)).toBe(false);    // 8ms, too early
    expect(limiter.shouldRender(16)).toBe(true);    // 8ms later = 16ms total, within tolerance
    expect(limiter.shouldRender(24)).toBe(false);   // 8ms since last render
    expect(limiter.shouldRender(33)).toBe(true);    // 17ms since last render = ~16ms
});

it("frame limiter does not render at 30Hz when display matches target", () => {
    // Regression: on a 60Hz display targeting NTSC (~60.1fps), minor jitter
    // should not cause the limiter to skip every other frame (producing 30fps).
    const limiter = createFrameLimiter(60.0988); // NTSC
    const RAF_INTERVAL = 16.667; // 60Hz display

    expect(limiter.shouldRender(0)).toBe(true);
    // Simulate 10 consecutive rAF calls: all should render
    let rendered = 0;
    for (let i = 1; i <= 10; i++) {
        if (limiter.shouldRender(i * RAF_INTERVAL)) rendered++;
    }
    expect(rendered).toBe(10);
});

it("frame limiter updates target fps", () => {
    const limiter = createFrameLimiter(60);

    limiter.setTargetFps(50);

    expect(limiter.shouldRender(0)).toBe(true);
    expect(limiter.shouldRender(10)).toBe(false);
    expect(limiter.shouldRender(20)).toBe(true);
});
