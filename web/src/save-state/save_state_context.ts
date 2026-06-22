import type { SaveStateRuntime } from "./save_state_runtime";

export async function createSaveStateContext({
    runtime,
    romMetadata,
    openDb,
    createRomSaveKey,
    createSaveStateController,
    saveStateFn,
    loadStateFn,
    setStatus
}: {
    runtime: SaveStateRuntime | null;
    romMetadata: { name: string; size: number; bytes: Uint8Array } | null;
    openDb: () => Promise<IDBDatabase>;
    createRomSaveKey: (meta: { name: string; size: number; bytes: Uint8Array }) => Promise<string>;
    createSaveStateController: (opts: {
        runtime: SaveStateRuntime;
        db: IDBDatabase;
        key: string;
        saveStateFn: (db: IDBDatabase, key: string, bytes: Uint8Array) => Promise<void>;
        loadStateFn: (db: IDBDatabase, key: string) => Promise<Uint8Array | null>;
        setStatus: (message: string, isError: boolean) => void;
    }) => { save(): Promise<boolean>; load(): Promise<boolean> };
    saveStateFn: (db: IDBDatabase, key: string, bytes: Uint8Array) => Promise<void>;
    loadStateFn: (db: IDBDatabase, key: string) => Promise<Uint8Array | null>;
    setStatus: (message: string, isError: boolean) => void;
}) {
    if (!runtime || !romMetadata) {
        return null;
    }

    const db = await openDb();
    const key = await createRomSaveKey({
        name: romMetadata.name,
        size: romMetadata.size,
        bytes: romMetadata.bytes
    });

    return createSaveStateController({
        runtime,
        db,
        key,
        saveStateFn,
        loadStateFn,
        setStatus
    });
}
