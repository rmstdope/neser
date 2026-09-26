// Every word the browser version shows about SNES coprocessor firmware (nr-auv, nr-608, nr-tfq), as
// agreed with the navigator. Kept in one place so the tests can pin them byte for byte.

/** A chip whose firmware the player supplies. `key` matches the wasm chip key and the store key. */
export type SnesFirmwareChip = {
    readonly key: string;
    readonly label: string;
    /** The file the words ask for. */
    readonly file: string;
};

/**
 * Every chip NESER emulates, in the order the sidebar lists them. A new chip adds one entry
 * here and its row in the Rust firmware table (`src/snes/dsp`).
 */
export const SNES_FIRMWARE_CHIPS: readonly SnesFirmwareChip[] = [
    { key: "dsp1", label: "DSP-1", file: "dsp1b.rom" },
    { key: "dsp2", label: "DSP-2", file: "dsp2.rom" },
    { key: "dsp4", label: "DSP-4", file: "dsp4.rom" }
];

export function chipByKey(key: string): SnesFirmwareChip | undefined {
    return SNES_FIRMWARE_CHIPS.find((chip) => chip.key === key);
}

/** A DSP firmware image is exactly this many bytes: 2048 24-bit opcodes and 1024 data words. */
export const SNES_FIRMWARE_SIZE = 8192;

export const SIDEBAR_TITLE = "SNES firmware";

export function dialogTitle(chip: SnesFirmwareChip): string {
    return `This game needs the ${chip.label} firmware`;
}

export function dialogText(chip: SnesFirmwareChip): string {
    return `Choose your ${chip.file} file (8 KB). It stays in this browser, so you only do this once.`;
}

/** The heading when a chosen file is refused, whatever the reason. */
export function wrongFileTitle(chip: SnesFirmwareChip): string {
    return `That isn't a ${chip.label} firmware file`;
}

export function storedMessage(chip: SnesFirmwareChip): string {
    return `${chip.label} firmware stored`;
}

export function sidebarStored(chip: SnesFirmwareChip): string {
    return `${chip.label}: stored ✓`;
}

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

/** The second line when a chosen 8 KB file is not the chip's genuine firmware. */
export function notGenuineDetail(chip: SnesFirmwareChip, fileName: string): string {
    return `"${fileName}" is 8 KB but is not the ${chip.label} firmware.`;
}

/** The status line, in red, when the player cancels the dialog. */
export function notStartedStatus(chip: SnesFirmwareChip, game: string): string {
    return `Failed to load ROM: ${game} needs the ${chip.label} firmware`;
}

/** The 3-second message when the player cancels the dialog. */
export function notStartedMessage(chip: SnesFirmwareChip, game: string): string {
    return `Not started: ${game} needs the ${chip.label} firmware`;
}
