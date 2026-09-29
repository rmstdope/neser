export function gbaKeyboardButtonForEvent(event: Pick<KeyboardEvent, "key" | "code">): number | null {
    const key = event.key.toLowerCase();
    switch (key) {
        case "g":
            return 0;
        case "f":
            return 1;
        case "4":
            return 2;
        case "5":
            return 3;
        case "w":
            return 4;
        case "s":
            return 5;
        case "a":
            return 6;
        case "d":
            return 7;
        case "v":
            return 8;
        case "b":
            return 9;
        default:
            return null;
    }
}

export function snesKeyboardButtonForEvent(event: Pick<KeyboardEvent, "key" | "code">): number | null {
    const key = event.key.toLowerCase();
    switch (key) {
        case "r":
            return 0; // B
        case "g":
            return 1; // Y
        case "4":
            return 2; // Select
        case "5":
            return 3; // Start
        case "w":
            return 4;
        case "s":
            return 5;
        case "a":
            return 6;
        case "d":
            return 7;
        case "t":
            return 8; // A
        case "y":
            return 9; // X
        case "q":
            return 10; // L
        case "e":
            return 11; // R
        default:
            return null;
    }
}

/**
 * Key 6 inserts a Vs. System coin into slot 1, as on desktop. A press inserts one coin and
 * the core times the coin pulse, so key repeats and the release insert nothing: a coin line
 * held ten frames or more reads as a jammed coin to Vs. Duck Hunt. Returns true when the
 * key was the coin key.
 */
export function applyVsCoinKey(
    nes: { insert_vs_coin(slot: number): void },
    event: Pick<KeyboardEvent, "key" | "repeat">,
    pressed: boolean
): boolean {
    if (event.key !== "6") {
        return false;
    }
    if (pressed && !event.repeat) {
        nes.insert_vs_coin(0);
    }
    return true;
}
