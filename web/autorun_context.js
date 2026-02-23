const SUPPORTED_AUTORUN_VERSION = 2;

/**
 * Parse an autorun file's bytes and return metadata about the recording.
 *
 * @param {Uint8Array} bytes - Raw bytes of a JSON-serialized AutorunFile.
 * @returns {{ version: number, frameCount: number, checkpointCount: number }}
 * @throws {Error} If the bytes are not valid JSON or the version is unsupported.
 */
export function parseAutorunFile(bytes) {
    let obj;
    try {
        const text = new TextDecoder().decode(bytes);
        obj = JSON.parse(text);
    } catch (e) {
        throw new Error(`Failed to parse autorun file: ${e.message}`);
    }
    if (obj.version !== SUPPORTED_AUTORUN_VERSION) {
        throw new Error(
            `Unsupported autorun version: ${obj.version} (expected ${SUPPORTED_AUTORUN_VERSION})`
        );
    }
    return {
        version: obj.version,
        frameCount: Array.isArray(obj.frames) ? obj.frames.length : 0,
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
    let loadedFile = null; // null | { bytes: Uint8Array, checkpointCount: number, frameCount: number }
    let selectedCheckpoint = null;
    let extend = false;

    return {
        // ── create-recording mode ─────────────────────────────────────────

        /** @returns {boolean} */
        isCreateRecording() {
            return createRecording;
        },

        /** @param {boolean} flag */
        setCreateRecording(flag) {
            createRecording = Boolean(flag);
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
        setLoadedFile(bytes, fileName = null) {
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
        setSelectedCheckpoint(idx) {
            selectedCheckpoint = idx ?? null;
        },

        /** @returns {boolean} */
        isExtend() {
            return extend;
        },

        /** @param {boolean} flag */
        setExtend(flag) {
            extend = Boolean(flag);
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
