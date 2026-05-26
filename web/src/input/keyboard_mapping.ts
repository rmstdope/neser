export function gbaKeyboardButtonForEvent(event: Pick<KeyboardEvent, "key" | "code">): number | null {
    const key = event.key.toLowerCase();
    switch (key) {
        case "z":
            return 0;
        case "x":
            return 1;
        case "backspace":
            return 2;
        case "5":
        case "enter":
            return 3;
        case "arrowup":
            return 4;
        case "arrowdown":
            return 5;
        case "v":
        case "arrowleft":
            return 6;
        case "b":
        case "arrowright":
            return 7;
        case "a":
            return 8;
        case "s":
            return 9;
        default:
            if (event.code === "ShiftLeft" || event.code === "ShiftRight") {
                return 2;
            }
            return null;
    }
}
