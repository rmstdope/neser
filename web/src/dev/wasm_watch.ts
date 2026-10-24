/**
 * Node-only: rebuilds the wasm when Rust changes during `pnpm run dev`, then reloads the page.
 * Vite's own HMR keeps handling TS and CSS under web/.
 */
import { spawn } from "child_process";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "path";
import { fileURLToPath } from "url";
import type { Plugin } from "vite";

/** Files at the repository root whose change means the wasm must be rebuilt. */
const ROOT_INPUTS = new Set(["Cargo.toml", "Cargo.lock", "build.rs"]);

/**
 * Editor and OS scratch files: dotfiles (`.DS_Store`, `.x.swp`), swap and temp files, backups,
 * and vim's all-digit write probe (`4913`). A save must not rebuild and reload for these.
 */
function isScratchFile(file: string): boolean {
    const name = basename(file);
    return (
        name.startsWith(".") ||
        name.endsWith("~") ||
        /\.(swp|swx|swo|tmp)$/.test(name) ||
        /^\d+$/.test(name)
    );
}

/** True when `file` is a Rust build input of the crate rooted at `repoRoot`. */
export function isRustInput(file: string, repoRoot: string): boolean {
    const rel = relative(repoRoot, file);
    if (rel === "" || rel.startsWith("..") || isAbsolute(rel)) return false;
    if (isScratchFile(rel)) return false;
    const parts = rel.split(sep);
    if (parts.length === 1) return ROOT_INPUTS.has(rel);
    return parts[0] === "src";
}

export interface RebuildScheduler {
    /** Asks for a rebuild; bursts are coalesced, and one is never run while another is. */
    schedule(): void;
}

/**
 * Debounced, single-flight rebuilds: changes made while a rebuild runs queue exactly one more,
 * and `onSuccess` runs after every rebuild that `run` reports as successful.
 */
export function createRebuildScheduler(
    run: () => Promise<boolean>,
    onSuccess: () => void,
    debounceMs: number,
): RebuildScheduler {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let running = false;
    let pending = false;

    async function start(): Promise<void> {
        timer = undefined;
        running = true;
        try {
            if (await run()) onSuccess();
        } catch (error) {
            // A dev server that dies on one bad rebuild loses the open page; log and keep serving.
            const message = error instanceof Error ? error.message : String(error);
            console.error(`[wasm-watch] rebuild or reload failed: ${message}`);
        } finally {
            running = false;
            if (pending) {
                pending = false;
                schedule();
            }
        }
    }

    function schedule(): void {
        if (running) {
            pending = true;
            return;
        }
        if (timer !== undefined) clearTimeout(timer);
        timer = setTimeout(() => void start(), debounceMs);
    }

    return { schedule };
}

export interface WasmWatchOptions {
    /** The crate's root; defaults to this repository's. */
    repoRoot?: string;
    /** Rebuilds web/pkg and resolves whether it succeeded; defaults to `pnpm run build:wasm:dev`. */
    rebuild?: () => Promise<boolean>;
    /** How long a burst of saves is collected before a rebuild starts. */
    debounceMs?: number;
}

const DEFAULT_REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

/** Runs the development wasm build with its output on this terminal. */
function buildWasmDev(repoRoot: string): Promise<boolean> {
    console.log("[wasm-watch] Rust changed; rebuilding the wasm");
    return new Promise((done) => {
        const child = spawn("pnpm", ["run", "build:wasm:dev"], { cwd: repoRoot, stdio: "inherit" });
        child.on("error", (error) => {
            console.error(`[wasm-watch] could not start the build: ${error.message}`);
            done(false);
        });
        child.on("exit", (code) => {
            if (code === 0) {
                console.log("[wasm-watch] wasm rebuilt; reloading the page");
            } else {
                console.error("[wasm-watch] the Rust build failed; the page keeps the last good wasm");
            }
            done(code === 0);
        });
    });
}

/**
 * During `vite` (the dev server) only: rebuilds the wasm when a Rust input changes and then
 * reloads the page. Vite stops watching web/pkg, so it never reloads onto a half-written build.
 */
export function wasmWatchPlugin(options: WasmWatchOptions = {}): Plugin {
    const repoRoot = options.repoRoot ?? DEFAULT_REPO_ROOT;
    const rebuild = options.rebuild ?? (() => buildWasmDev(repoRoot));
    const debounceMs = options.debounceMs ?? 200;

    return {
        name: "wasm-watch",
        apply: (_config, env) => env.command === "serve" && env.mode !== "test",
        config: () => ({ server: { watch: { ignored: ["**/pkg/**"] } } }),
        configureServer(server) {
            const client = server.environments.client;
            const scheduler = createRebuildScheduler(
                rebuild,
                () => {
                    client.moduleGraph.invalidateAll();
                    client.hot.send({ type: "full-reload" });
                },
                debounceMs,
            );
            server.watcher.add([
                join(repoRoot, "src"),
                ...[...ROOT_INPUTS].map((name) => join(repoRoot, name)),
            ]);
            const onFile = (file: string) => {
                if (isRustInput(file, repoRoot)) scheduler.schedule();
            };
            server.watcher.on("change", onFile);
            server.watcher.on("add", onFile);
            server.watcher.on("unlink", onFile);
        },
    };
}
