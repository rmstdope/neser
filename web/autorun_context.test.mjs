import assert from "node:assert/strict";
import test from "node:test";
import {
    parseAutorunFile,
    createAutorunContext
} from "./autorun_context.js";

// ── parseAutorunFile ──────────────────────────────────────────────────────────

test("parseAutorunFile returns checkpointCount and frameCount for valid file", () => {
    const file = {
        version: 2,
        frames: [{ player1: 0, player2: 0 }, { player1: 1, player2: 0 }],
        checkpoints: [
            { frame_index: 0, screen_crc: 0, state_bytes: [] },
            { frame_index: 1, screen_crc: 0, state_bytes: [] }
        ]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const info = parseAutorunFile(bytes);
    assert.equal(info.checkpointCount, 2);
    assert.equal(info.frameCount, 2);
    assert.equal(info.version, 2);
});

test("parseAutorunFile throws for invalid JSON", () => {
    const bytes = new TextEncoder().encode("not json");
    assert.throws(() => parseAutorunFile(bytes), /parse|invalid|JSON/i);
});

test("parseAutorunFile throws for unsupported version", () => {
    const file = { version: 99, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    assert.throws(() => parseAutorunFile(bytes), /version/i);
});

// ── createAutorunContext – create-recording mode ──────────────────────────────

test("createAutorunContext initial state: recording off, no loaded file", () => {
    const ctx = createAutorunContext();
    assert.equal(ctx.isCreateRecording(), false);
    assert.equal(ctx.getLoadedFile(), null);
    assert.equal(ctx.isExtend(), false);
    assert.equal(ctx.getSelectedCheckpoint(), null);
    assert.equal(ctx.isActive(), false);
});

test("createAutorunContext setCreateRecording enables create-recording mode", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    assert.equal(ctx.isCreateRecording(), true);
    assert.equal(ctx.isActive(), true);
});

test("createAutorunContext setCreateRecording false deactivates", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    ctx.setCreateRecording(false);
    assert.equal(ctx.isCreateRecording(), false);
    assert.equal(ctx.isActive(), false);
});

test("getActiveConfig returns record config when create-recording is enabled", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    const config = ctx.getActiveConfig();
    assert.equal(config?.mode, "record");
});

// ── createAutorunContext – load file ─────────────────────────────────────────

test("createAutorunContext setLoadedFile stores file info", () => {
    const file = {
        version: 2,
        frames: new Array(600).fill({ player1: 0, player2: 0 }),
        checkpoints: [
            { frame_index: 299, screen_crc: 1, state_bytes: [] },
            { frame_index: 599, screen_crc: 2, state_bytes: [] }
        ]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    const loaded = ctx.getLoadedFile();
    assert.ok(loaded !== null);
    assert.equal(loaded.checkpointCount, 2);
    assert.equal(loaded.frameCount, 600);
    assert.deepEqual(loaded.bytes, bytes);
});

test("createAutorunContext isActive returns true when file is loaded", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    assert.equal(ctx.isActive(), true);
});

test("createAutorunContext setLoadedFile throws for invalid file", () => {
    const ctx = createAutorunContext();
    assert.throws(
        () => ctx.setLoadedFile(new TextEncoder().encode("bad")),
        /parse|invalid|JSON/i
    );
});

test("createAutorunContext clearLoadedFile removes file and deactivates (if no recording)", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    ctx.clearLoadedFile();
    assert.equal(ctx.getLoadedFile(), null);
    assert.equal(ctx.isActive(), false);
});

// ── createAutorunContext – checkpoint & extend ────────────────────────────────

test("createAutorunContext setSelectedCheckpoint stores index", () => {
    const ctx = createAutorunContext();
    ctx.setSelectedCheckpoint(3);
    assert.equal(ctx.getSelectedCheckpoint(), 3);
});

test("createAutorunContext setSelectedCheckpoint null means 'from beginning'", () => {
    const ctx = createAutorunContext();
    ctx.setSelectedCheckpoint(2);
    ctx.setSelectedCheckpoint(null);
    assert.equal(ctx.getSelectedCheckpoint(), null);
});

test("createAutorunContext setExtend true enables extend mode", () => {
    const ctx = createAutorunContext();
    ctx.setExtend(true);
    assert.equal(ctx.isExtend(), true);
});

// ── createAutorunContext – getActiveConfig for playback ───────────────────────

test("getActiveConfig returns playback config when file is loaded", () => {
    const file = {
        version: 2,
        frames: [{ player1: 0, player2: 0 }],
        checkpoints: [{ frame_index: 0, screen_crc: 0, state_bytes: [] }]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    const config = ctx.getActiveConfig();
    assert.equal(config?.mode, "playback");
    assert.deepEqual(config?.bytes, bytes);
    assert.equal(config?.checkpointIdx, null);
    assert.equal(config?.extend, false);
});

test("getActiveConfig includes checkpointIdx and extend when set", () => {
    const file = {
        version: 2,
        frames: new Array(300).fill({ player1: 0, player2: 0 }),
        checkpoints: [{ frame_index: 299, screen_crc: 0, state_bytes: [] }]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    ctx.setSelectedCheckpoint(0);
    ctx.setExtend(true);
    const config = ctx.getActiveConfig();
    assert.equal(config?.mode, "playback");
    assert.equal(config?.checkpointIdx, 0);
    assert.equal(config?.extend, true);
});

test("getActiveConfig returns null when neither recording nor file is loaded", () => {
    const ctx = createAutorunContext();
    assert.equal(ctx.getActiveConfig(), null);
});

// ── createAutorunContext – record takes precedence over loaded file ─────────

test("getActiveConfig returns record mode when both recording and file are set", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    ctx.setCreateRecording(true);
    const config = ctx.getActiveConfig();
    assert.equal(config?.mode, "record");
});
