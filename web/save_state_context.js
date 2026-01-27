export async function createSaveStateContext({
    nes,
    romMetadata,
    openDb,
    createRomSaveKey,
    createSaveStateController,
    saveStateFn,
    loadStateFn,
    setStatus
}) {
    if (!nes || !romMetadata) {
        return null;
    }

    const db = await openDb();
    const key = await createRomSaveKey({
        name: romMetadata.name,
        size: romMetadata.size,
        bytes: romMetadata.bytes
    });

    return createSaveStateController({
        nes,
        db,
        key,
        saveStateFn,
        loadStateFn,
        setStatus
    });
}
