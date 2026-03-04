export async function handleRomSelection({
    bytes,
    name,
    running,
    stop,
    applyRomBytes,
    start,
    focusCanvas
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
