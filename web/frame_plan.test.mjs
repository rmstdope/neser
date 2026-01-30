import test from "node:test";
import assert from "node:assert/strict";
import { planFrame } from "./frame_plan.js";

test("frame plan always steps", () => {
    assert.deepEqual(planFrame({ shouldRender: true }), {
        shouldStep: true,
        shouldRender: true
    });
    assert.deepEqual(planFrame({ shouldRender: false }), {
        shouldStep: true,
        shouldRender: false
    });
});
