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
    start?: () => Promise<void>;
    focusCanvas?: () => void;
}) {
    if (running) {
        stop();
    }
    await applyRomBytes(bytes, name);
    if (start) {
        await start();
    }
    if (focusCanvas) {
        focusCanvas();
    }
}
