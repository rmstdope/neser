/**
 * The count before playing together (nr-ayk5.4, docs/ui/nr-ayk5.4-sending-the-game.html): over
 * the dimmed picture, "You are player N" above 3, 2 and 1, one second each; then it goes and the
 * game runs, with no extra word. It cannot be skipped: the overlay takes the screen's pointer, and
 * the page ignores keys while it is up.
 */

const STEP_MS = 1000;

export function runCountdown({ screen, player, onDone }: { screen: HTMLElement; player: 1 | 2; onDone: () => void }): void {
    const overlay = document.createElement("div");
    overlay.className = "netplay-countdown";
    overlay.setAttribute("role", "status");
    const who = document.createElement("span");
    who.className = "netplay-countdown-player";
    who.textContent = `You are player ${player}`;
    const number = document.createElement("b");
    number.className = "netplay-countdown-number";
    overlay.append(who, number);
    screen.append(overlay);
    const show = (left: number) => {
        if (left === 0) {
            overlay.remove();
            onDone();
            return;
        }
        number.textContent = String(left);
        setTimeout(() => show(left - 1), STEP_MS);
    };
    show(3);
}
