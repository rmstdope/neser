export function computeMouseCursorStyle({
    arkanoidActive,
    windowFocused,
    releasedByEscape = false,
}) {
    return arkanoidActive && windowFocused && !releasedByEscape ? "none" : "default";
}
