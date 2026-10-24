/**
 * Node-only: rebuilds the wasm when Rust changes during `pnpm run dev`, then reloads the page.
 * Vite's own HMR keeps handling TS and CSS under web/.
 */
import { isAbsolute, relative, sep } from "path";

/** Files at the repository root whose change means the wasm must be rebuilt. */
const ROOT_INPUTS = new Set(["Cargo.toml", "Cargo.lock", "build.rs"]);

/** True when `file` is a Rust build input of the crate rooted at `repoRoot`. */
export function isRustInput(file: string, repoRoot: string): boolean {
    const rel = relative(repoRoot, file);
    if (rel === "" || rel.startsWith("..") || isAbsolute(rel)) return false;
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
