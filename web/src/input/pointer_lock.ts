export function shouldKeepPointerLocked({
    arkanoidActive,
    windowFocused,
    releasedByEscape,
}: {
    arkanoidActive: boolean;
    windowFocused: boolean;
    releasedByEscape: boolean;
}) {
    return arkanoidActive && windowFocused && !releasedByEscape;
}

export function shouldForwardArkanoidMouseInput({ pointerLocked }: { pointerLocked: boolean }) {
    return pointerLocked;
}
