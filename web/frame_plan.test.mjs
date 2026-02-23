import test from "node:test";
import assert from "node:assert/strict";
import { planFrame } from "./frame_plan.js";

test("plans step and render when shouldRender is true", () => {
    assert.deepEqual(planFrame({ shouldRender: true }), {
        shouldStep: true,
        shouldRender: true
    });
});

test("skips step and render when shouldRender is false", () => {
    assert.deepEqual(planFrame({ shouldRender: false }), {
        shouldStep: false,
        shouldRender: false
    });
});
