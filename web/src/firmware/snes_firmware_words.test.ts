import { describe, expect, it } from "vitest";
import {
    SIDEBAR_TITLE,
    SNES_FIRMWARE_CHIPS,
    SNES_FIRMWARE_SIZE,
    chipByKey,
    dialogText,
    dialogTitle,
    formatFileSize,
    notGenuineDetail,
    notStartedMessage,
    notStartedStatus,
    romDisplayName,
    sidebarStored,
    storedMessage,
    wrongFileTitle,
    wrongSizeDetail
} from "./snes_firmware_words";

const dsp1 = chipByKey("dsp1")!;
const dsp2 = chipByKey("dsp2")!;
const dsp4 = chipByKey("dsp4")!;

describe("SNES firmware chips", () => {
    it("list the chips in chip order, with the file each asks for", () => {
        expect(SNES_FIRMWARE_CHIPS.map((c) => [c.key, c.label, c.file])).toEqual([
            ["dsp1", "DSP-1", "dsp1b.rom"],
            ["dsp2", "DSP-2", "dsp2.rom"],
            ["dsp4", "DSP-4", "dsp4.rom"]
        ]);
        expect(chipByKey("dsp9")).toBeUndefined();
    });
});

describe("SNES firmware words", () => {
    it("are the agreed DSP-1 strings", () => {
        expect(SNES_FIRMWARE_SIZE).toBe(8192);
        expect(dialogTitle(dsp1)).toBe("This game needs the DSP-1 firmware");
        expect(dialogText(dsp1)).toBe(
            "Choose your dsp1b.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(wrongFileTitle(dsp1)).toBe("That isn't a DSP-1 firmware file");
        expect(storedMessage(dsp1)).toBe("DSP-1 firmware stored");
        expect(SIDEBAR_TITLE).toBe("SNES firmware");
        expect(sidebarStored(dsp1)).toBe("DSP-1: stored ✓");
        expect(notStartedStatus(dsp1, "Super Mario Kart (USA)")).toBe(
            "Failed to load ROM: Super Mario Kart (USA) needs the DSP-1 firmware"
        );
        expect(notStartedMessage(dsp1, "Super Mario Kart (USA)")).toBe(
            "Not started: Super Mario Kart (USA) needs the DSP-1 firmware"
        );
        expect(wrongSizeDetail("Super Mario Kart (USA).sfc", 1258291)).toBe(
            "\"Super Mario Kart (USA).sfc\" is 1.2 MB; the firmware is exactly 8 KB."
        );
        expect(notGenuineDetail(dsp1, "dsp2.rom")).toBe("\"dsp2.rom\" is 8 KB but is not the DSP-1 firmware.");
    });

    it("are the agreed DSP-2 strings", () => {
        expect(dialogTitle(dsp2)).toBe("This game needs the DSP-2 firmware");
        expect(dialogText(dsp2)).toBe(
            "Choose your dsp2.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(wrongFileTitle(dsp2)).toBe("That isn't a DSP-2 firmware file");
        expect(wrongSizeDetail("mario.zip", 1258291)).toBe("\"mario.zip\" is 1.2 MB; the firmware is exactly 8 KB.");
        expect(notGenuineDetail(dsp2, "dsp1b.rom")).toBe("\"dsp1b.rom\" is 8 KB but is not the DSP-2 firmware.");
        expect(storedMessage(dsp2)).toBe("DSP-2 firmware stored");
        expect(sidebarStored(dsp2)).toBe("DSP-2: stored ✓");
        expect(notStartedStatus(dsp2, "Dungeon Master (Japan)")).toBe(
            "Failed to load ROM: Dungeon Master (Japan) needs the DSP-2 firmware"
        );
        expect(notStartedMessage(dsp2, "Dungeon Master (Japan)")).toBe(
            "Not started: Dungeon Master (Japan) needs the DSP-2 firmware"
        );
    });

    it("are the agreed DSP-4 strings", () => {
        expect(dialogTitle(dsp4)).toBe("This game needs the DSP-4 firmware");
        expect(dialogText(dsp4)).toBe(
            "Choose your dsp4.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(wrongFileTitle(dsp4)).toBe("That isn't a DSP-4 firmware file");
        expect(wrongSizeDetail("mario.zip", 1258291)).toBe("\"mario.zip\" is 1.2 MB; the firmware is exactly 8 KB.");
        expect(notGenuineDetail(dsp4, "dsp3.rom")).toBe("\"dsp3.rom\" is 8 KB but is not the DSP-4 firmware.");
        expect(storedMessage(dsp4)).toBe("DSP-4 firmware stored");
        expect(sidebarStored(dsp4)).toBe("DSP-4: stored ✓");
        expect(notStartedStatus(dsp4, "Top Gear 3000 (USA)")).toBe(
            "Failed to load ROM: Top Gear 3000 (USA) needs the DSP-4 firmware"
        );
        expect(notStartedMessage(dsp4, "Top Gear 3000 (USA)")).toBe(
            "Not started: Top Gear 3000 (USA) needs the DSP-4 firmware"
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
