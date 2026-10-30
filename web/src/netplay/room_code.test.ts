import { describe, expect, it } from "vitest";
import { ROOM_CODE_LENGTH, showRoomCode, tidyRoomCode } from "./room_code";

describe("tidying what is typed into a room code", () => {
    it("takes letters in capitals whatever is typed", () => {
        expect(tidyRoomCode("k7q")).toBe("K7Q");
    });

    it("drops the characters a code never uses: I, O, 0, 1 and symbols", () => {
        expect(tidyRoomCode("KI7O0Q1!?é")).toBe("K7Q");
    });

    it("tidies away spaces and dashes, so a pasted code with or without its dash works", () => {
        expect(tidyRoomCode(" k7q - m4x ")).toBe("K7QM4X");
        expect(tidyRoomCode("K7QM4X")).toBe("K7QM4X");
    });

    it("holds at most six code characters", () => {
        expect(tidyRoomCode("K7QM4XAB")).toBe("K7QM4X");
        expect(ROOM_CODE_LENGTH).toBe(6);
    });
});

describe("showing a room code", () => {
    it("puts the dash in by itself after the third character", () => {
        expect(showRoomCode("")).toBe("");
        expect(showRoomCode("K7")).toBe("K7");
        expect(showRoomCode("K7Q")).toBe("K7Q");
        expect(showRoomCode("K7QM")).toBe("K7Q-M");
        expect(showRoomCode("K7QM4X")).toBe("K7Q-M4X");
    });
});
