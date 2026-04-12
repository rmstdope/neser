export function createFrameLimiter(targetFps = 60) {
    let targetFrameMs = 1000 / targetFps;
    let lastTime = null;
    let accumulator = 0;
    return {
        shouldRender(timestamp) {
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

            if (accumulator < targetFrameMs) {
                return false;
            }

            accumulator %= targetFrameMs;
            return true;
        },
        setTargetFps(nextFps) {
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
