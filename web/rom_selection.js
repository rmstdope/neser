export async function handleRomSelection({
    bytes,
    name,
    running,
    stop,
    applyRomBytes,
    start
}) {
    if (running) {
        stop();
    }
    await applyRomBytes(bytes, name);
    if (start) {
        await start();
    }
}
