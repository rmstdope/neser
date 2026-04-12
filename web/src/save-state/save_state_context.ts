export async function createSaveStateContext({
    nes,
    romMetadata,
    openDb,
    createRomSaveKey,
    createSaveStateController,
    saveStateFn,
    loadStateFn,
    setStatus
}: {
    nes: { save_state_bytes(): Uint8Array; load_state_bytes(bytes: Uint8Array): void } | null;
    romMetadata: { name: string; size: number; bytes: Uint8Array } | null;
    openDb: () => Promise<IDBDatabase>;
    createRomSaveKey: (meta: { name: string; size: number; bytes: Uint8Array }) => Promise<string>;
    createSaveStateController: (opts: any) => { save(): Promise<boolean>; load(): Promise<boolean> };
    saveStateFn: (db: IDBDatabase, key: string, bytes: Uint8Array) => Promise<void>;
    loadStateFn: (db: IDBDatabase, key: string) => Promise<Uint8Array | null>;
    setStatus: (message: string, isError: boolean) => void;
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
