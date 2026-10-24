import { describe, expect, it } from "vitest";
import type { PlayView } from "../netplay/play";
import { createSlowNote, SLOW_NOTE_MS } from "./slow_note";

type Note = Pick<PlayView, "phase" | "slowNote">;

function page() {
    const shown: { message: string; ms: number; dismissed: boolean }[] = [];
    const note = createSlowNote((message, ms) => {
        const entry = { message, ms, dismissed: false };
        shown.push(entry);
        return () => {
            entry.dismissed = true;
        };
    });
    return { note, shown };
}

const playing = (slowNote: Note["slowNote"]): Note => ({ phase: "playing", slowNote });

describe("the slow-speed note (nr-ayk5.7)", () => {
    it("says it for 5 seconds, worded for this side", () => {
        expect(SLOW_NOTE_MS).toBe(5000);
        const me = page();
        me.note.draw(playing("me"));
        expect(me.shown).toEqual([
            { message: "Your computer can't keep full speed, so both games run a little slower.", ms: 5000, dismissed: false },
        ]);
        const them = page();
        them.note.draw(playing("them"));
        expect(them.shown.map((s) => s.message)).toEqual([
            "The other player's computer can't keep full speed, so both games run a little slower.",
        ]);
    });

    it("shows nothing before a slowdown", () => {
        const { note, shown } = page();
        note.draw(playing(null));
        expect(shown).toEqual([]);
    });

    it("shows once, however often the view is drawn", () => {
        const { note, shown } = page();
        note.draw(playing("me"));
        note.draw(playing("me"));
        note.draw(playing("me"));
        expect(shown).toHaveLength(1);
    });

    it("goes when the game stops, and does not come back", () => {
        for (const phase of ["paused", "counting", "resyncing", "ended"] as const) {
            const { note, shown } = page();
            note.draw(playing("me"));
            note.draw({ phase, slowNote: "me" });
            expect(shown[0].dismissed).toBe(true);
            note.draw(playing("me"));
            expect(shown).toHaveLength(1);
        }
    });

    it("waits for play to show a note first asked for while stopped", () => {
        const { note, shown } = page();
        note.draw({ phase: "paused", slowNote: "them" });
        expect(shown).toEqual([]);
        note.draw(playing("them"));
        expect(shown).toHaveLength(1);
        expect(shown[0].dismissed).toBe(false);
    });

    it("goes when playing together ends, and can show again the next time", () => {
        const { note, shown } = page();
        note.draw(playing("me"));
        note.end();
        expect(shown[0].dismissed).toBe(true);
        note.draw(playing("me"));
        expect(shown).toHaveLength(2);
    });
});
