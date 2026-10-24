// The phone layout's small rules: what its frame-rate chip says, and when a drag closes the sheet.

/** The phone's frame-rate chip, as drawn: the whole number while playing, "Paused" while not. */
export function phoneFpsChip(fps: number, paused: boolean): string {
    return paused ? "Paused" : String(Math.round(fps));
}

/** How far down the ⋯ sheet must be pulled before letting go closes it. */
const SHEET_DRAG_CLOSE_PX = 60;

/** Whether a drag of `dy` pixels (down is positive) closes the sheet; a shorter one springs back. */
export function sheetDragCloses(dy: number): boolean {
    return dy >= SHEET_DRAG_CLOSE_PX;
}
