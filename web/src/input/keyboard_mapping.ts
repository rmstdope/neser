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
