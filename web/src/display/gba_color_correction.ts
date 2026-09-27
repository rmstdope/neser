/** Label of the top-bar Colors button in a Game Boy Advance game: it names the current colour state. */
export function gbaColorButtonLabel(enabled: boolean): string {
    return enabled ? "Colors: GBA screen" : "Colors: Raw";
}
