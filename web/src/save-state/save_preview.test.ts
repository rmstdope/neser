/**
 * @vitest-environment jsdom
 */
import "fake-indexeddb/auto";

import { describe, expect, it } from "vitest";

import { createSavePreviewStore, savedAgo, savedAtLine } from "./save_preview";

describe("savedAgo", () => {
    const now = new Date(2026, 9, 23, 14, 30, 0).getTime();
    it("Given when a state was saved, When said in words, Then it reads from just now through minutes and hours to days", () => {
        expect(savedAgo(now, now - 20_000)).toBe("Saved just now");
        expect(savedAgo(now, now - 60_000)).toBe("Saved 1 minute ago");
        expect(savedAgo(now, now - 2 * 60_000)).toBe("Saved 2 minutes ago");
        expect(savedAgo(now, now - 3 * 3_600_000)).toBe("Saved 3 hours ago");
        expect(savedAgo(now, new Date(2026, 9, 22, 20, 0).getTime())).toBe("Saved yesterday");
        expect(savedAgo(now, new Date(2026, 9, 18, 20, 0).getTime())).toBe("Saved 5 days ago");
        expect(savedAgo(now, new Date(2026, 9, 14, 20, 0).getTime())).toBe("Saved last week");
        expect(savedAgo(now, new Date(2026, 8, 1, 20, 0).getTime())).toBe(`Saved on ${new Date(2026, 8, 1).toLocaleDateString()}`);
    });
});

describe("savedAtLine", () => {
    it("Given a save time, When written out, Then it names the day and the time", () => {
        const now = new Date(2026, 9, 23, 14, 30);
        expect(savedAtLine(now, new Date(2026, 9, 23, 14, 2).getTime())).toBe("Today 14:02");
        expect(savedAtLine(now, new Date(2026, 9, 22, 9, 5).getTime())).toBe("Yesterday 09:05");
        // Further back, the date itself: "3 days ago 14:02" would not read as a time.
        expect(savedAtLine(now, new Date(2026, 9, 20, 14, 2).getTime())).toBe(`${new Date(2026, 9, 20).toLocaleDateString()} 14:02`);
        expect(savedAtLine(now, new Date(2026, 9, 14, 8, 0).getTime())).toBe(`${new Date(2026, 9, 14).toLocaleDateString()} 08:00`);
    });
});

describe("the save preview store", () => {
    let n = 0;
    const fresh = () => createSavePreviewStore(`neser-save-previews-test-${n++}`);

    it("Given a game's state was saved with a picture, When its preview is read, Then the picture and the time come back", async () => {
        const store = fresh();
        await store.put("cpu.nes", { thumbnail: "data:image/png;base64,AAA", savedAt: 123 });
        expect(await store.get("cpu.nes")).toEqual({ thumbnail: "data:image/png;base64,AAA", savedAt: 123 });
    });

    it("Given a game never saved here, When its preview is read, Then there is none", async () => {
        expect(await fresh().get("never.nes")).toBeNull();
    });

    it("Given a newer save, When stored, Then it replaces the older preview", async () => {
        const store = fresh();
        await store.put("a.nes", { thumbnail: null, savedAt: 1 });
        await store.put("a.nes", { thumbnail: "data:x", savedAt: 2 });
        expect(await store.get("a.nes")).toEqual({ thumbnail: "data:x", savedAt: 2 });
    });
});
