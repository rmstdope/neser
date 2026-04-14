export function createFrameLimiter(targetFps = 60) {
    let targetFrameMs = 1000 / targetFps;
    let lastTime: number | null = null;
    let accumulator = 0;
    // Tolerance to prevent frame skips from minor rAF jitter.
    // Without this, a 60Hz display targeting ~60fps can alternate between
    // "just under" and "just over" the threshold, halving effective FPS.
    const JITTER_TOLERANCE_MS = 1.5;
    return {
        shouldRender(timestamp: number) {
            if (typeof timestamp !== "number") {
                return true;
            }

            if (lastTime === null) {
                lastTime = timestamp;
                return true;
            }

            let delta = timestamp - lastTime;
            if (delta < 0) {
                delta = 0;
            }

            lastTime = timestamp;
            accumulator += delta;

            if (accumulator < targetFrameMs - JITTER_TOLERANCE_MS) {
                return false;
            }

            // When within tolerance but below target, reset to 0
            // (frame arrived slightly early — consume the timing debt).
            // When at or above target, use modulo to handle multi-frame catch-up.
            if (accumulator >= targetFrameMs) {
                accumulator %= targetFrameMs;
            } else {
                accumulator = 0;
            }
            return true;
        },
        setTargetFps(nextFps: number) {
            if (!Number.isFinite(nextFps) || nextFps <= 0) {
                return;
            }
            const newTargetFrameMs = 1000 / nextFps;
            if (newTargetFrameMs === targetFrameMs) {
                return;
            }
            targetFrameMs = newTargetFrameMs;
            lastTime = null;
            accumulator = 0;
        },
        reset() {
            lastTime = null;
            accumulator = 0;
        }
    };
}
