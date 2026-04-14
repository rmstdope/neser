export interface EmulationControlState {
    romLoaded: boolean;
    running: boolean;
    paused: boolean;
    isRecording: boolean;
}

export interface ButtonStates {
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
    return {
        startEnabled: romLoaded && !running,
        pauseEnabled: emulationActive,
        resetEnabled: emulationActive,
        stopEnabled: emulationActive,
        startLabel: isRecording ? "Start Recording" : "Start",
        pauseLabel: paused ? "Resume" : "Pause",
        stopLabel: isRecording ? "Stop Recording" : "Stop",
    };
}
