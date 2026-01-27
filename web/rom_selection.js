export async function handleRomSelection({
    bytes,
    name,
    running,
    stop,
    applyRomBytes
}) {
    if (running) {
        stop();
    }
    await applyRomBytes(bytes, name);
}
