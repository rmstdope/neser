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
