import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EventEmitter } from "events";
import { join } from "path";
import type { ConfigEnv, UserConfig, ViteDevServer } from "vite";
import { createRebuildScheduler, isRustInput, wasmWatchPlugin } from "./wasm_watch";

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
        "src/nes/.cpu.rs.swp",
        "src/nes/cpu.rs.swp",
        "src/nes/cpu.rs~",
        "src/nes/4913",
        "src/.DS_Store",
        "src/nes/cpu.rs.tmp",
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

    it("keeps serving when a rebuild throws", async () => {
        const run = vi
            .fn<() => Promise<boolean>>()
            .mockRejectedValueOnce(new Error("spawn exploded"))
            .mockResolvedValueOnce(true);
        const onSuccess = vi.fn();
        const error = vi.spyOn(console, "error").mockImplementation(() => {});
        const scheduler = createRebuildScheduler(run, onSuccess, 100);

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        expect(error).toHaveBeenCalledWith(expect.stringContaining("spawn exploded"));
        expect(onSuccess).not.toHaveBeenCalled();

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        expect(run).toHaveBeenCalledTimes(2);
        expect(onSuccess).toHaveBeenCalledTimes(1);
        error.mockRestore();
    });

    it("keeps serving when the reload throws", async () => {
        const run = vi.fn(async () => true);
        const onSuccess = vi.fn(() => {
            throw new Error("hot channel closed");
        });
        const error = vi.spyOn(console, "error").mockImplementation(() => {});
        const scheduler = createRebuildScheduler(run, onSuccess, 100);

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        expect(error).toHaveBeenCalledWith(expect.stringContaining("hot channel closed"));

        scheduler.schedule();
        await vi.advanceTimersByTimeAsync(100);
        expect(run).toHaveBeenCalledTimes(2);
        error.mockRestore();
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

describe("wasmWatchPlugin", () => {
    /** The parts of a Vite dev server the plugin touches. */
    function fakeServer() {
        const watcher = Object.assign(new EventEmitter(), { add: vi.fn() });
        const invalidateAll = vi.fn();
        const send = vi.fn();
        const server = {
            watcher,
            environments: { client: { moduleGraph: { invalidateAll }, hot: { send } } },
        };
        return { server: server as unknown as ViteDevServer, watcher, invalidateAll, send };
    }

    function start(rebuild: () => Promise<boolean>) {
        const plugin = wasmWatchPlugin({ repoRoot: repo, rebuild, debounceMs: 10 });
        const fake = fakeServer();
        (plugin.configureServer as (server: ViteDevServer) => void)(fake.server);
        return fake;
    }

    function applies(env: ConfigEnv): boolean {
        const plugin = wasmWatchPlugin({ repoRoot: repo });
        return (plugin.apply as (config: UserConfig, env: ConfigEnv) => boolean)({}, env);
    }

    it("runs only on the development server", () => {
        expect(applies({ command: "serve", mode: "development" })).toBe(true);
        expect(applies({ command: "build", mode: "production" })).toBe(false);
        expect(applies({ command: "serve", mode: "test" })).toBe(false);
    });

    it("keeps Vite from reacting to the wasm bindings while they are rewritten", () => {
        const plugin = wasmWatchPlugin({ repoRoot: repo });
        const config = (plugin.config as () => UserConfig)();
        expect(config.server?.watch?.ignored).toContain("**/pkg/**");
    });

    it("watches the Rust inputs", () => {
        const { watcher } = start(vi.fn(async () => true));
        const watched = watcher.add.mock.calls.flat(2);
        expect(watched).toEqual(
            expect.arrayContaining([
                join(repo, "src"),
                join(repo, "Cargo.toml"),
                join(repo, "Cargo.lock"),
                join(repo, "build.rs"),
            ]),
        );
    });

    describe("with real timers", () => {
        it("rebuilds and reloads the page when Rust changes", async () => {
            const rebuild = vi.fn(async () => true);
            const { watcher, invalidateAll, send } = start(rebuild);

            watcher.emit("change", join(repo, "src/nes/cpu.rs"));
            await vi.waitFor(() => expect(send).toHaveBeenCalled());

            expect(rebuild).toHaveBeenCalledTimes(1);
            expect(invalidateAll).toHaveBeenCalledTimes(1);
            expect(invalidateAll.mock.invocationCallOrder[0]).toBeLessThan(
                send.mock.invocationCallOrder[0],
            );
            expect(send).toHaveBeenCalledWith({ type: "full-reload" });
        });

        it("rebuilds when a Rust file is added or removed", async () => {
            const rebuild = vi.fn(async () => true);
            const { watcher, send } = start(rebuild);

            watcher.emit("add", join(repo, "src/new_module.rs"));
            await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
            watcher.emit("unlink", join(repo, "src/new_module.rs"));
            await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(2));
        });

        it("leaves web code to Vite", async () => {
            const rebuild = vi.fn(async () => true);
            const { watcher, send } = start(rebuild);

            watcher.emit("change", join(repo, "web/src/app.ts"));
            watcher.emit("change", join(repo, "web/main.css"));
            await new Promise((resolve) => setTimeout(resolve, 50));

            expect(rebuild).not.toHaveBeenCalled();
            expect(send).not.toHaveBeenCalled();
        });

        it("keeps the page as it is when the Rust build fails", async () => {
            const rebuild = vi.fn(async () => false);
            const { watcher, invalidateAll, send } = start(rebuild);

            watcher.emit("change", join(repo, "src/lib.rs"));
            await vi.waitFor(() => expect(rebuild).toHaveBeenCalled());
            await new Promise((resolve) => setTimeout(resolve, 20));

            expect(invalidateAll).not.toHaveBeenCalled();
            expect(send).not.toHaveBeenCalled();
        });
    });
});
