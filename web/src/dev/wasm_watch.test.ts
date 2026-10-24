import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { join } from "path";
import { createRebuildScheduler, isRustInput } from "./wasm_watch";

const repo = "/work/neser";

describe("isRustInput", () => {
    it.each([
        "src/nes/cpu.rs",
        "src/lib.rs",
        "src/gba/bios/bios.bin",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
    ])("rebuilds the wasm for %s", (rel) => {
        expect(isRustInput(join(repo, rel), repo)).toBe(true);
    });

    it.each([
        "web/src/app.ts",
        "web/main.css",
        "web/pkg/neser.js",
        "web/pkg/neser_bg.wasm",
        "target/wasm32-unknown-unknown/wasm-dev/neser.wasm",
        "node_modules/vite/index.js",
        "package.json",
        "../other/src/lib.rs",
    ])("leaves %s to Vite", (rel) => {
        expect(isRustInput(join(repo, rel), repo)).toBe(false);
    });
});

describe("createRebuildScheduler", () => {
    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    /** A rebuild whose completion the test controls. */
    function controllableRun() {
        const finishers: Array<(ok: boolean) => void> = [];
        const run = vi.fn(
            () => new Promise<boolean>((resolve) => finishers.push(resolve)),
        );
        return { run, finish: (ok: boolean) => finishers.shift()!(ok) };
    }

    it("runs one rebuild for a burst of changes", async () => {
        const { run, finish } = controllableRun();
        const onSuccess = vi.fn();
        const scheduler = createRebuildScheduler(run, onSuccess, 100);

        scheduler.schedule();
        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(50);
        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);

        expect(run).toHaveBeenCalledTimes(1);
        finish(true);
        await vi.advanceTimersByTimeAsync(0);
        expect(onSuccess).toHaveBeenCalledTimes(1);
    });

    it("runs exactly one more rebuild for changes made while one is running", async () => {
        const { run, finish } = controllableRun();
        const onSuccess = vi.fn();
        const scheduler = createRebuildScheduler(run, onSuccess, 100);

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        expect(run).toHaveBeenCalledTimes(1);

        scheduler.schedule();
        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(500);
        expect(run).toHaveBeenCalledTimes(1);

        finish(true);
        await vi.advanceTimersByTimeAsync(100);
        expect(run).toHaveBeenCalledTimes(2);
        finish(true);
        await vi.advanceTimersByTimeAsync(500);
        expect(run).toHaveBeenCalledTimes(2);
        expect(onSuccess).toHaveBeenCalledTimes(2);
    });

    it("does not reload after a failed rebuild", async () => {
        const { run, finish } = controllableRun();
        const onSuccess = vi.fn();
        const scheduler = createRebuildScheduler(run, onSuccess, 100);

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        finish(false);
        await vi.advanceTimersByTimeAsync(0);

        expect(onSuccess).not.toHaveBeenCalled();
    });
});
