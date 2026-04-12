export function computeMouseCursorStyle({
    arkanoidActive,
    windowFocused,
    releasedByEscape = false,
}: {
    arkanoidActive: boolean;
    windowFocused: boolean;
    releasedByEscape?: boolean;
}) {
    return arkanoidActive && windowFocused && !releasedByEscape ? "none" : "default";
}
