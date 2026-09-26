export async function handleRomSelection({
    bytes,
    name,
    running,
    stop,
    applyRomBytes,
    start,
    focusCanvas
}: {
    bytes: Uint8Array;
    name: string;
    running: boolean;
    stop: () => void;
    applyRomBytes: (bytes: Uint8Array, name: string) => Promise<void>;
    /** Starts the game; resolving `false` means it did not start and focus is left alone. */
    start?: () => Promise<void | boolean>;
    focusCanvas?: () => void;
}) {
    if (running) {
        stop();
    }
    await applyRomBytes(bytes, name);
    const started = start ? await start() : undefined;
    if (focusCanvas && started !== false) {
        focusCanvas();
    }
}
