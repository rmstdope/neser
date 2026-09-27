import type { WebRomConsoleKind } from "../rom/rom_extensions";

/**
 * The Colors button is shown only while a game is running whose colours can
 * be corrected: on the Game Boy core, Game Boy Color games and black-and-white
 * games the Game Boy Color colourises; and every Game Boy Advance game. It is
 * hidden while paused, because no new frame would be drawn and a press must
 * change the picture at once.
 */
export function cgbColorButtonVisible(state: {
    kind: WebRomConsoleKind | null;
    isColor: boolean;
    running: boolean;
    paused: boolean;
}): boolean {
    const correctable = (state.kind === "gb" && state.isColor) || state.kind === "gba";
    return correctable && state.running && !state.paused;
}

/** The part of the Game Boy core the colour state talks to. */
export interface CgbColorCore {
    set_cgb_color_correction(enabled: boolean): void;
    cgb_color_correction(): boolean;
    /** The colour correction's state in the words of its corner message (the button label). */
    color_label(): string;
}

/**
 * The page's one colour-correction choice, shared by F8 and the Colors button.
 * It lives for the page (each load starts at Raw) and is handed to every new
 * Game Boy instance, so a choice carries on to the next game. The running
 * game is the truth; the words shown are always the core's.
 */
export function createCgbColorControl(view: { refreshButton(): void; showMessage(message: string): void }) {
    let enabled = false;
    return {
        enabled: (): boolean => enabled,
        /** The Colors button: switch the running game, relabel and show the core's message. */
        click(core: CgbColorCore): void {
            enabled = !core.cgb_color_correction();
            core.set_cgb_color_correction(enabled);
            view.refreshButton();
            view.showMessage(core.color_label());
        },
        /** Remember the running game's state (F8 may have changed it) for the next game. */
        follow(core: CgbColorCore): void {
            enabled = core.cgb_color_correction();
        }
    };
}
