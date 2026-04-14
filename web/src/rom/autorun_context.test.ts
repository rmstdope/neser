import { expect, it } from "vitest";
import {
    parseAutorunFile,
    createAutorunContext
} from "./autorun_context";

// ── parseAutorunFile ──────────────────────────────────────────────────────────

it("parseAutorunFile returns checkpointCount and frameCount for valid file", () => {
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
    expect(info.checkpointCount).toBe(2);
    expect(info.frameCount).toBe(2);
    expect(info.version).toBe(2);
});

it("parseAutorunFile throws for invalid JSON", () => {
    const bytes = new TextEncoder().encode("not json");
    expect(() => parseAutorunFile(bytes)).toThrow(/parse|invalid|JSON/i);
});

it("parseAutorunFile throws for unsupported version", () => {
    const file = { version: 99, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    expect(() => parseAutorunFile(bytes)).toThrow(/version/i);
});

// ── createAutorunContext – create-recording mode ──────────────────────────────

it("createAutorunContext initial state: recording off, no loaded file", () => {
    const ctx = createAutorunContext();
    expect(ctx.isCreateRecording()).toBe(false);
    expect(ctx.getLoadedFile()).toBe(null);
    expect(ctx.isExtend()).toBe(false);
    expect(ctx.getSelectedCheckpoint()).toBe(null);
    expect(ctx.isActive()).toBe(false);
});

it("createAutorunContext setCreateRecording enables create-recording mode", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    expect(ctx.isCreateRecording()).toBe(true);
    expect(ctx.isActive()).toBe(true);
});

it("createAutorunContext setCreateRecording false deactivates", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    ctx.setCreateRecording(false);
    expect(ctx.isCreateRecording()).toBe(false);
    expect(ctx.isActive()).toBe(false);
});

it("getActiveConfig returns record config when create-recording is enabled", () => {
    const ctx = createAutorunContext();
    ctx.setCreateRecording(true);
    const config = ctx.getActiveConfig();
    expect(config?.mode).toBe("record");
});

// ── createAutorunContext – load file ─────────────────────────────────────────

it("createAutorunContext setLoadedFile stores file info", () => {
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
    expect(loaded !== null).toBeTruthy();
    expect(loaded!.checkpointCount).toBe(2);
    expect(loaded!.frameCount).toBe(600);
    expect(loaded!.bytes).toEqual(bytes);
});

it("createAutorunContext isActive returns true when file is loaded", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    expect(ctx.isActive()).toBe(true);
});

it("createAutorunContext setLoadedFile throws for invalid file", () => {
    const ctx = createAutorunContext();
    expect(
        () => ctx.setLoadedFile(new TextEncoder().encode("bad"))
    ).toThrow(/parse|invalid|JSON/i);
});

it("createAutorunContext clearLoadedFile removes file and deactivates (if no recording)", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    ctx.clearLoadedFile();
    expect(ctx.getLoadedFile()).toBe(null);
    expect(ctx.isActive()).toBe(false);
});

// ── createAutorunContext – checkpoint & extend ────────────────────────────────

it("createAutorunContext setSelectedCheckpoint stores index", () => {
    const ctx = createAutorunContext();
    ctx.setSelectedCheckpoint(3);
    expect(ctx.getSelectedCheckpoint()).toBe(3);
});

it("createAutorunContext setSelectedCheckpoint null means 'from beginning'", () => {
    const ctx = createAutorunContext();
    ctx.setSelectedCheckpoint(2);
    ctx.setSelectedCheckpoint(null);
    expect(ctx.getSelectedCheckpoint()).toBe(null);
});

it("createAutorunContext setExtend true enables extend mode", () => {
    const ctx = createAutorunContext();
    ctx.setExtend(true);
    expect(ctx.isExtend()).toBe(true);
});

// ── createAutorunContext – getActiveConfig for playback ───────────────────────

it("getActiveConfig returns playback config when file is loaded", () => {
    const file = {
        version: 2,
        frames: [{ player1: 0, player2: 0 }],
        checkpoints: [{ frame_index: 0, screen_crc: 0, state_bytes: [] }]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    const config = ctx.getActiveConfig();
    expect(config?.mode).toBe("playback");
    expect(config?.bytes).toEqual(bytes);
    expect(config?.checkpointIdx).toBe(null);
    expect(config?.extend).toBe(false);
});

it("getActiveConfig includes checkpointIdx and extend when set", () => {
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
    expect(config?.mode).toBe("playback");
    expect(config?.checkpointIdx).toBe(0);
    expect(config?.extend).toBe(true);
});

it("getActiveConfig returns null when neither recording nor file is loaded", () => {
    const ctx = createAutorunContext();
    expect(ctx.getActiveConfig()).toBe(null);
});

// ── createAutorunContext – record takes precedence over loaded file ─────────

it("getActiveConfig returns record mode when both recording and file are set", () => {
    const file = { version: 2, frames: [], checkpoints: [] };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    ctx.setCreateRecording(true);
    const config = ctx.getActiveConfig();
    expect(config?.mode).toBe("record");
});

// ── parseAutorunFile – v3 RLE format ─────────────────────────────────────────

it("parseAutorunFile returns correct frameCount for v3 RLE file", () => {
    const file = {
        version: 3,
        frames: [
            { player1: 0, player2: 0, repeat: 3 },
            { player1: 1, player2: 0, repeat: 1 }
        ],
        checkpoints: [{ frame_index: 3, screen_crc: 0, state_bytes: [] }]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const info = parseAutorunFile(bytes);
    expect(info.version).toBe(3);
    expect(info.frameCount).toBe(4); // 3 + 1 = 4 total frames
    expect(info.checkpointCount).toBe(1);
});

it("parseAutorunFile accepts both v2 and v3 formats", () => {
    // v2 file
    const v2 = {
        version: 2,
        frames: [{ player1: 0, player2: 0 }, { player1: 1, player2: 0 }],
        checkpoints: []
    };
    const v2bytes = new TextEncoder().encode(JSON.stringify(v2));
    const v2info = parseAutorunFile(v2bytes);
    expect(v2info.frameCount).toBe(2);

    // v3 file
    const v3 = {
        version: 3,
        frames: [{ player1: 0, player2: 0, repeat: 2 }, { player1: 1, player2: 0, repeat: 1 }],
        checkpoints: []
    };
    const v3bytes = new TextEncoder().encode(JSON.stringify(v3));
    const v3info = parseAutorunFile(v3bytes);
    expect(v3info.frameCount).toBe(3);
});

it("createAutorunContext setLoadedFile accepts v3 RLE file", () => {
    const file = {
        version: 3,
        frames: [
            { player1: 0, player2: 0, repeat: 600 }
        ],
        checkpoints: [
            { frame_index: 299, screen_crc: 1, state_bytes: [] },
            { frame_index: 599, screen_crc: 2, state_bytes: [] }
        ]
    };
    const bytes = new TextEncoder().encode(JSON.stringify(file));
    const ctx = createAutorunContext();
    ctx.setLoadedFile(bytes);
    const loaded = ctx.getLoadedFile();
    expect(loaded !== null).toBeTruthy();
    expect(loaded!.checkpointCount).toBe(2);
    expect(loaded!.frameCount).toBe(600);
});
