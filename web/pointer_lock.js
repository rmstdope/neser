export function shouldKeepPointerLocked({
    arkanoidActive,
    windowFocused,
    releasedByEscape,
}) {
    return arkanoidActive && windowFocused && !releasedByEscape;
}

export function shouldForwardArkanoidMouseInput({ pointerLocked }) {
    return pointerLocked;
}
