export interface EmulationControlState {
    romLoaded: boolean;
    running: boolean;
    paused: boolean;
    isRecording: boolean;
}

/** Where the emulation is in its life: rendered as `data-emulation-state` on `#emulation-controls`. */
export type EmulationLifecycle = "idle" | "running" | "paused";

export function emulationLifecycle(running: boolean, paused: boolean): EmulationLifecycle {
    if (!running) return "idle";
    return paused ? "paused" : "running";
}

export interface ButtonStates {
    lifecycle: EmulationLifecycle;
    startEnabled: boolean;
    pauseEnabled: boolean;
    resetEnabled: boolean;
    stopEnabled: boolean;
    startLabel: string;
    pauseLabel: string;
    stopLabel: string;
}

export function computeButtonStates(state: EmulationControlState): ButtonStates {
    const { romLoaded, running, paused, isRecording } = state;
    const emulationActive = running;
    const lifecycle = emulationLifecycle(running, paused);
    return {
        lifecycle,
        startEnabled: romLoaded && !running,
        pauseEnabled: emulationActive,
        resetEnabled: emulationActive,
        stopEnabled: emulationActive,
        startLabel: isRecording ? "Start Recording" : "Start",
        pauseLabel: lifecycle === "paused" ? "Resume" : "Pause",
        stopLabel: isRecording ? "Stop Recording" : "Stop",
    };
}
