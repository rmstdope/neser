import { describe, expect, it } from "vitest";
import { computeButtonStates, emulationLifecycle } from "./emulation_controls";

describe("computeButtonStates", () => {
    // ── Stopped, no ROM ──────────────────────────────────────────────────
    it("disables all buttons when stopped with no ROM", () => {
        const s = computeButtonStates({ romLoaded: false, running: false, paused: false, isRecording: false });
        expect(s.startEnabled).toBe(false);
        expect(s.pauseEnabled).toBe(false);
        expect(s.resetEnabled).toBe(false);
        expect(s.stopEnabled).toBe(false);
    });

    // ── Stopped, ROM loaded ──────────────────────────────────────────────
    it("enables only Start when stopped with a ROM loaded", () => {
        const s = computeButtonStates({ romLoaded: true, running: false, paused: false, isRecording: false });
        expect(s.startEnabled).toBe(true);
        expect(s.pauseEnabled).toBe(false);
        expect(s.resetEnabled).toBe(false);
        expect(s.stopEnabled).toBe(false);
    });

    // ── Running ──────────────────────────────────────────────────────────
    it("disables Start and enables Pause/Reset/Stop when running", () => {
        const s = computeButtonStates({ romLoaded: true, running: true, paused: false, isRecording: false });
        expect(s.startEnabled).toBe(false);
        expect(s.pauseEnabled).toBe(true);
        expect(s.resetEnabled).toBe(true);
        expect(s.stopEnabled).toBe(true);
    });

    // ── Paused ───────────────────────────────────────────────────────────
    it("keeps Pause/Reset/Stop enabled when paused", () => {
        const s = computeButtonStates({ romLoaded: true, running: true, paused: true, isRecording: false });
        expect(s.startEnabled).toBe(false);
        expect(s.pauseEnabled).toBe(true);
        expect(s.resetEnabled).toBe(true);
        expect(s.stopEnabled).toBe(true);
    });

    // ── Pause label ──────────────────────────────────────────────────────
    it("shows 'Pause' label when running and not paused", () => {
        const s = computeButtonStates({ romLoaded: true, running: true, paused: false, isRecording: false });
        expect(s.pauseLabel).toBe("Pause");
    });

    it("shows 'Resume' label when paused", () => {
        const s = computeButtonStates({ romLoaded: true, running: true, paused: true, isRecording: false });
        expect(s.pauseLabel).toBe("Resume");
    });

    // ── Recording mode labels ────────────────────────────────────────────
    it("shows 'Start Recording' and 'Stop Recording' when isRecording is true and stopped", () => {
        const s = computeButtonStates({ romLoaded: true, running: false, paused: false, isRecording: true });
        expect(s.startLabel).toBe("Start Recording");
        expect(s.stopLabel).toBe("Stop Recording");
    });

    it("shows 'Stop Recording' when recording and running", () => {
        const s = computeButtonStates({ romLoaded: true, running: true, paused: false, isRecording: true });
        expect(s.stopLabel).toBe("Stop Recording");
    });

    // ── Default labels ───────────────────────────────────────────────────
    it("shows default labels when not recording", () => {
        const s = computeButtonStates({ romLoaded: true, running: false, paused: false, isRecording: false });
        expect(s.startLabel).toBe("Start");
        expect(s.stopLabel).toBe("Stop");
        expect(s.pauseLabel).toBe("Pause");
    });
});

describe("emulationLifecycle", () => {
    it("is idle when not running, whatever the paused flag says", () => {
        expect(emulationLifecycle(false, false)).toBe("idle");
        expect(emulationLifecycle(false, true)).toBe("idle");
    });

    it("is running when running and not paused", () => {
        expect(emulationLifecycle(true, false)).toBe("running");
    });

    it("is paused when running and paused", () => {
        expect(emulationLifecycle(true, true)).toBe("paused");
    });

    it("is carried on the button states, so the rendered state and the buttons agree", () => {
        const idle = computeButtonStates({ romLoaded: true, running: false, paused: false, isRecording: false });
        const running = computeButtonStates({ romLoaded: true, running: true, paused: false, isRecording: false });
        const paused = computeButtonStates({ romLoaded: true, running: true, paused: true, isRecording: false });
        expect([idle.lifecycle, running.lifecycle, paused.lifecycle]).toEqual(["idle", "running", "paused"]);
        expect(paused.pauseLabel).toBe("Resume");
        expect(running.pauseLabel).toBe("Pause");
    });
});
