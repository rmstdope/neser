import type { WebRomConsoleKind } from "../rom/rom_extensions";

/** Label of the top-bar button: it names the current colour state. */
export function cgbColorButtonLabel(enabled: boolean): string {
    return enabled ? "Colors: GBC screen" : "Colors: Raw";
}

/**
 * Corner message when the correction is switched, by F8 or the Colors button.
 * The words match the button label and the desktop message; kept separate so
 * a later label change does not silently change the message.
 */
export function cgbColorToastMessage(enabled: boolean): string {
    return enabled ? "Colors: GBC screen" : "Colors: Raw";
}

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
}

/**
 * The page's one colour-correction state, shared by F8 and the Colors button.
 * It lives for the page (each load starts at Raw) and is handed to every new
 * Game Boy instance, so a choice carries on to the next game.
 */
export function createCgbColorControl(view: { refreshButton(): void; showMessage(message: string): void }) {
    let enabled = false;
    return {
        enabled: (): boolean => enabled,
        /** The Colors button: switch, apply to the running game, relabel and show the message. */
        click(core: CgbColorCore | null): void {
            enabled = !enabled;
            core?.set_cgb_color_correction(enabled);
            view.refreshButton();
            view.showMessage(cgbColorToastMessage(enabled));
        },
        /** After F8, which switched the game and queued its own message: follow it and relabel. */
        afterF8(core: CgbColorCore): void {
            enabled = core.cgb_color_correction();
            view.refreshButton();
        }
    };
}
