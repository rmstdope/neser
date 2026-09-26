// Every word the browser version shows about the DSP-1 firmware (nr-auv), as agreed with the
// navigator. Kept in one place so the tests can pin them byte for byte.

/** A DSP-1 firmware image is exactly this many bytes: 2048 24-bit opcodes and 1024 data words. */
export const DSP1_FIRMWARE_SIZE = 8192;

export const DIALOG_TITLE = "This game needs the DSP-1 firmware";
export const DIALOG_TEXT = "Choose your dsp1b.rom file (8 KB). It stays in this browser, so you only do this once.";
export const WRONG_SIZE_TITLE = "That isn't a DSP-1 firmware file";
export const STORED_MESSAGE = "DSP-1 firmware stored";
export const SIDEBAR_TITLE = "SNES firmware";
export const SIDEBAR_STORED = "DSP-1: stored ✓";

/** A game's name in messages: its file name without the last extension. */
export function romDisplayName(fileName: string): string {
    const dot = fileName.lastIndexOf(".");
    return dot > 0 ? fileName.slice(0, dot) : fileName;
}

/** A file size as a person reads it: "812 bytes", "8 KB", "12.5 KB", "1.2 MB". */
export function formatFileSize(bytes: number): string {
    if (bytes < 1024) {
        return `${bytes} bytes`;
    }
    const oneDecimal = (value: number) => {
        const text = (Math.round(value * 10) / 10).toFixed(1);
        return text.endsWith(".0") ? text.slice(0, -2) : text;
    };
    if (bytes < 1024 * 1024) {
        return `${oneDecimal(bytes / 1024)} KB`;
    }
    return `${oneDecimal(bytes / (1024 * 1024))} MB`;
}

/** The second line when a chosen file is not 8192 bytes. */
export function wrongSizeDetail(fileName: string, size: number): string {
    return `"${fileName}" is ${formatFileSize(size)}; the firmware is exactly 8 KB.`;
}

/** The status line, in red, when the player cancels the dialog. */
export function notStartedStatus(game: string): string {
    return `Failed to load ROM: ${game} needs the DSP-1 firmware`;
}

/** The 3-second message when the player cancels the dialog. */
export function notStartedMessage(game: string): string {
    return `Not started: ${game} needs the DSP-1 firmware`;
}
