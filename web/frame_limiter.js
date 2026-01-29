export function createFrameLimiter(targetFps = 60) {
    const targetFrameMs = 1000 / targetFps;
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
        reset() {
            lastTime = null;
            accumulator = 0;
        }
    };
}
