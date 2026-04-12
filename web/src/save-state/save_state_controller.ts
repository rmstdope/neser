export function createSaveStateController({
    nes,
    db,
    key,
    saveStateFn,
    loadStateFn,
    setStatus
}: {
    nes: { save_state_bytes(): Uint8Array; load_state_bytes(bytes: Uint8Array): void };
    db: IDBDatabase;
    key: string;
    saveStateFn: (db: IDBDatabase, key: string, bytes: Uint8Array) => Promise<void>;
    loadStateFn: (db: IDBDatabase, key: string) => Promise<Uint8Array | null>;
    setStatus: (message: string, isError: boolean) => void;
}) {
    async function save() {
        try {
            const bytes = nes.save_state_bytes();
            if (!bytes || bytes.length === 0) {
                setStatus("Failed to save state", true);
                return false;
            }
            await saveStateFn(db, key, bytes);
            setStatus("State saved", false);
            return true;
        } catch (error) {
            console.error("Failed to save state", error);
            setStatus("Failed to save state", true);
            return false;
        }
    }

    async function load() {
        try {
            const bytes = await loadStateFn(db, key);
            if (!bytes || bytes.length === 0) {
                setStatus("No save state found", true);
                return false;
            }
            nes.load_state_bytes(bytes);
            setStatus("State loaded", false);
            return true;
        } catch (error) {
            console.error("Failed to load state", error);
            setStatus("Failed to load state", true);
            return false;
        }
    }

    return {
        save,
        load
    };
}
