export interface SaveStateRuntime {
    save_state_bytes(): Uint8Array;
    load_state_bytes(bytes: Uint8Array): void;
}
