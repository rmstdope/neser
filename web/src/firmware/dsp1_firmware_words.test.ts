import { describe, expect, it } from "vitest";
import {
    DIALOG_TEXT,
    DIALOG_TITLE,
    DSP1_FIRMWARE_SIZE,
    SIDEBAR_STORED,
    SIDEBAR_TITLE,
    STORED_MESSAGE,
    WRONG_SIZE_TITLE,
    formatFileSize,
    notStartedMessage,
    notStartedStatus,
    romDisplayName,
    wrongSizeDetail
} from "./dsp1_firmware_words";

describe("DSP-1 firmware words", () => {
    it("are the agreed strings", () => {
        expect(DSP1_FIRMWARE_SIZE).toBe(8192);
        expect(DIALOG_TITLE).toBe("This game needs the DSP-1 firmware");
        expect(DIALOG_TEXT).toBe("Choose your dsp1b.rom file (8 KB). It stays in this browser, so you only do this once.");
        expect(WRONG_SIZE_TITLE).toBe("That isn't a DSP-1 firmware file");
        expect(STORED_MESSAGE).toBe("DSP-1 firmware stored");
        expect(SIDEBAR_TITLE).toBe("SNES firmware");
        expect(SIDEBAR_STORED).toBe("DSP-1: stored ✓");
        expect(notStartedStatus("Super Mario Kart (USA)")).toBe(
            "Failed to load ROM: Super Mario Kart (USA) needs the DSP-1 firmware"
        );
        expect(notStartedMessage("Super Mario Kart (USA)")).toBe(
            "Not started: Super Mario Kart (USA) needs the DSP-1 firmware"
        );
        expect(wrongSizeDetail("Super Mario Kart (USA).sfc", 1258291)).toBe(
            "\"Super Mario Kart (USA).sfc\" is 1.2 MB; the firmware is exactly 8 KB."
        );
    });

    it("name a game by its file name without the extension", () => {
        expect(romDisplayName("Super Mario Kart (USA).sfc")).toBe("Super Mario Kart (USA)");
        expect(romDisplayName("pilotwings.v1.smc")).toBe("pilotwings.v1");
        expect(romDisplayName("noext")).toBe("noext");
    });

    it("format sizes in bytes, KB and MB", () => {
        expect(formatFileSize(812)).toBe("812 bytes");
        expect(formatFileSize(8192)).toBe("8 KB");
        expect(formatFileSize(12800)).toBe("12.5 KB");
        expect(formatFileSize(1258291)).toBe("1.2 MB");
        expect(formatFileSize(2 * 1024 * 1024)).toBe("2 MB");
    });
});
