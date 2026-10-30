/**
 * Room codes (nr-z4nw): the six characters liaison names a room by, shown as `ABC-DEF`. The
 * alphabet leaves out I, O, 0 and 1, which read alike, so a code read out loud or typed from a
 * chat message cannot be mistaken.
 */

export const ROOM_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
export const ROOM_CODE_LENGTH = 6;

/** What is typed or pasted, as code characters: in capitals, anything else dropped, at most six. */
export function tidyRoomCode(text: string): string {
    return [...text.toUpperCase()]
        .filter((char) => ROOM_ALPHABET.includes(char))
        .join("")
        .slice(0, ROOM_CODE_LENGTH);
}

/** Tidied code characters as a player reads them: a dash after the third. */
export function showRoomCode(chars: string): string {
    return chars.length > 3 ? `${chars.slice(0, 3)}-${chars.slice(3)}` : chars;
}
