const SUPPORTED_AUTORUN_VERSIONS = [2, 3];
const MAX_LOGICAL_FRAMES = 10_000_000;

/**
 * Parse an autorun file's bytes and return metadata about the recording.
 *
 * Accepts both v2 (per-frame entries) and v3 (RLE-encoded frames with `repeat` field).
 * For v3, the logical frame count is the sum of all `repeat` values.
 *
 * @param {Uint8Array} bytes - Raw bytes of a JSON-serialized AutorunFile.
 * @returns {{ version: number, frameCount: number, checkpointCount: number }}
 * @throws {Error} If the bytes are not valid JSON or the version is unsupported.
 */
export function parseAutorunFile(bytes: Uint8Array) {
    let obj: Record<string, unknown>;
    try {
        const text = new TextDecoder().decode(bytes);
        obj = JSON.parse(text);
    } catch (e: unknown) {
        throw new Error(`Failed to parse autorun file: ${e instanceof Error ? e.message : String(e)}`);
    }
    if (!SUPPORTED_AUTORUN_VERSIONS.includes(obj.version as number)) {
        throw new Error(
            `Unsupported autorun version: ${obj.version} (expected one of ${SUPPORTED_AUTORUN_VERSIONS.join(", ")})`
        );
    }
    const version = obj.version as number;
    let frameCount = 0;
    if (Array.isArray(obj.frames)) {
        if (version === 3) {
            // v3 uses RLE: each entry has a `repeat` count
            for (const f of obj.frames) {
                const repeat = Number((f as { repeat?: unknown }).repeat ?? 1);
                if (!Number.isFinite(repeat) || repeat < 1 || repeat !== Math.floor(repeat)) {
                    throw new Error(`Invalid repeat value in v3 autorun frame: ${JSON.stringify(f)}`);
                }
                frameCount += repeat;
                if (frameCount > MAX_LOGICAL_FRAMES) {
                    throw new Error(`Autorun file exceeds maximum of ${MAX_LOGICAL_FRAMES} logical frames`);
                }
            }
        } else {
            // v2: one entry per frame
            frameCount = obj.frames.length;
        }
    }
    return {
        version,
        frameCount,
        checkpointCount: Array.isArray(obj.checkpoints) ? obj.checkpoints.length : 0
    };
}

/**
 * Create an autorun context that manages the web frontend's autorun UI state.
 *
 * Two mutually-exclusive modes:
 *  - **Create recording**: when `createRecording` is `true`, every ROM run is
 *    recorded and the recording is offered for download when the ROM is stopped.
 *  - **Playback/Extend**: when an autorun file is loaded via `setLoadedFile`, the
 *    recording is played back (optionally from a specific checkpoint, optionally
 *    extended after playback ends).
 *
 * `getActiveConfig()` is the single query point used when starting a ROM:
 *  - Returns `{ mode: 'record' }` when create-recording is on.
 *  - Returns `{ mode: 'playback', bytes, checkpointIdx, extend }` when a file is loaded.
 *  - Returns `null` when neither mode is active.
 *
 * Create-recording takes precedence over a loaded file when both are set.
 */
export function createAutorunContext() {
    let createRecording = false;
    let loadedFile: { bytes: Uint8Array; checkpointCount: number; frameCount: number; fileName: string | null } | null = null;
    let selectedCheckpoint: number | null = null;
    let extend = false;

    return {
        // ── create-recording mode ─────────────────────────────────────────

        /** @returns {boolean} */
        isCreateRecording() {
            return createRecording;
        },

        /** @param {boolean} flag */
        setCreateRecording(flag: boolean) {
            createRecording = flag;
        },

        // ── loaded file ───────────────────────────────────────────────────

        /**
         * @returns {{ bytes: Uint8Array, checkpointCount: number, frameCount: number } | null}
         */
        getLoadedFile() {
            return loadedFile;
        },

        /**
         * Parse and store an autorun file.
         * @param {Uint8Array} bytes
         * @param {string|null} [fileName] - Original filename (e.g. "mario.autorun"), used to derive expected ROM name.
         * @throws {Error} If the bytes are invalid.
         */
        setLoadedFile(bytes: Uint8Array, fileName: string | null = null) {
            const info = parseAutorunFile(bytes);
            loadedFile = {
                bytes,
                checkpointCount: info.checkpointCount,
                frameCount: info.frameCount,
                fileName
            };
        },

        /** Clear the loaded autorun file. */
        clearLoadedFile() {
            loadedFile = null;
            selectedCheckpoint = null;
            extend = false;
        },

        /**
         * Returns the expected .nes ROM filename derived from the loaded autorun filename.
         * E.g. "mario.autorun" → "mario.nes". Returns null if no file is loaded or filename is unknown.
         * @returns {string|null}
         */
        getExpectedRomName() {
            if (!loadedFile?.fileName) return null;
            return loadedFile.fileName.replace(/\.autorun$/i, ".nes");
        },

        // ── checkpoint & extend ───────────────────────────────────────────

        /** @returns {number | null} 0-based checkpoint index, or null for "from beginning". */
        getSelectedCheckpoint() {
            return selectedCheckpoint;
        },

        /** @param {number | null} idx */
        setSelectedCheckpoint(idx: number | null) {
            selectedCheckpoint = idx ?? null;
        },

        /** @returns {boolean} */
        isExtend() {
            return extend;
        },

        /** @param {boolean} flag */
        setExtend(flag: boolean) {
            extend = flag;
        },

        // ── combined queries ──────────────────────────────────────────────

        /**
         * Returns true when any autorun mode is active (recording or playback).
         * @returns {boolean}
         */
        isActive() {
            return createRecording || loadedFile !== null;
        },

        /**
         * Returns the active autorun configuration for the next ROM run,
         * or `null` if no autorun is configured.
         *
         * Create-recording takes precedence over a loaded file.
         *
         * @returns {{ mode: 'record' } |
         *           { mode: 'playback', bytes: Uint8Array, checkpointIdx: number | null, extend: boolean } |
         *           null}
         */
        getActiveConfig() {
            if (createRecording) {
                return { mode: "record" };
            }
            if (loadedFile !== null) {
                return {
                    mode: "playback",
                    bytes: loadedFile.bytes,
                    checkpointIdx: selectedCheckpoint,
                    extend
                };
            }
            return null;
        }
    };
}
