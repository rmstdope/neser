import { expect, it } from "vitest";
import { planFrame } from "./frame_plan.js";

it("plans step and render when shouldRender is true", () => {
    expect(planFrame({ shouldRender: true })).toEqual({
        shouldStep: true,
        shouldRender: true
    });
});

it("skips step and render when shouldRender is false", () => {
    expect(planFrame({ shouldRender: false })).toEqual({
        shouldStep: false,
        shouldRender: false
    });
});
