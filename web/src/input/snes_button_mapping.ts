export function remapLegacySnesButtonId(button: number) {
    switch (button) {
        case 0: return 1; // B
        case 1: return 11; // Y
        case 2: return 2; // Select
        case 3: return 3; // Start
        case 4: return 4; // Up
        case 5: return 5; // Down
        case 6: return 6; // Left
        case 7: return 7; // Right
        case 8: return 0; // A
        case 9: return 10; // X
        case 10: return 8; // L
        case 11: return 9; // R
        default: return button;
    }
}
