/** One entry of a short list: a setting's value (ticked when current) or an action with its keys. */
export type Choice = {
    label: string;
    selected: boolean;
    /** A quiet word after the label, such as "default". */
    note?: string;
    /** The keys that do the same, drawn as key caps. */
    keys?: string[];
};

/** Every list's close, so whatever acts elsewhere (a shortcut, the game pausing) can close them all. */
const openMenuClosers = new Set<() => void>();

/** Closes every open list without picking, leaving focus where it is. */
export function closeOpenChoiceMenus(): void {
    for (const close of [...openMenuClosers]) close();
}

/**
 * A short list that opens above its button: every choice of a setting with the current one
 * ticked (`kind: "settings"`), or a few actions (`kind: "actions"`). Arrow keys move between the
 * entries, Enter or a click picks one, and Esc or a click anywhere else closes it. Focus returns
 * to the button whenever it closes. The list is built afresh on every opening from `choices()`.
 */
export function createChoiceMenu({
    anchor,
    choices,
    onPick,
    title,
    footer,
    footerKeys,
    kind = "settings",
}: {
    anchor: HTMLButtonElement;
    choices: () => Choice[];
    onPick: (index: number) => void;
    title?: string | (() => string);
    footer?: string;
    footerKeys?: string[];
    kind?: "settings" | "actions";
}) {
    let menu: HTMLElement | null = null;
    anchor.setAttribute("aria-haspopup", "menu");
    anchor.setAttribute("aria-expanded", "false");

    const keyCaps = (keys: string[]) =>
        keys.map((key) => {
            const cap = document.createElement("kbd");
            cap.className = "kbd kbd-sm";
            cap.textContent = key;
            return cap;
        });

    function items(): HTMLButtonElement[] {
        return menu ? [...menu.querySelectorAll<HTMLButtonElement>(".choice-menu-item")] : [];
    }

    const closeQuietly = () => close(false);

    function close(returnFocus = true) {
        if (!menu) return;
        openMenuClosers.delete(closeQuietly);
        menu.remove();
        menu = null;
        anchor.setAttribute("aria-expanded", "false");
        document.removeEventListener("pointerdown", onOutsidePointer, true);
        if (returnFocus) anchor.focus();
    }

    function onOutsidePointer(event: Event) {
        const target = event.target as Node | null;
        if (menu && target && !menu.contains(target) && !anchor.contains(target)) close(false);
    }

    function open() {
        const list = choices();
        menu = document.createElement("div");
        menu.className = "choice-menu";
        menu.setAttribute("role", "menu");
        const heading = typeof title === "function" ? title() : title;
        if (heading) {
            const titleEl = document.createElement("div");
            titleEl.className = "choice-menu-title";
            titleEl.textContent = heading;
            menu.append(titleEl);
            menu.setAttribute("aria-label", heading);
        }
        list.forEach((choice, index) => {
            const item = document.createElement("button");
            item.type = "button";
            item.className = "choice-menu-item";
            item.setAttribute("role", kind === "settings" ? "menuitemradio" : "menuitem");
            if (kind === "settings") item.setAttribute("aria-checked", choice.selected ? "true" : "false");
            item.tabIndex = -1;
            const tick = document.createElement("span");
            tick.className = "choice-menu-tick";
            tick.setAttribute("aria-hidden", "true");
            tick.textContent = kind === "settings" && choice.selected ? "✓" : "";
            const label = document.createElement("span");
            label.className = "choice-menu-label";
            label.textContent = choice.label;
            item.append(tick, label);
            if (choice.note) {
                const note = document.createElement("span");
                note.className = "choice-menu-note";
                note.textContent = choice.note;
                item.append(note);
            }
            if (choice.keys) {
                const keys = document.createElement("span");
                keys.className = "choice-menu-keys";
                keys.append(...keyCaps(choice.keys));
                item.append(keys);
            }
            item.addEventListener("click", () => {
                close();
                onPick(index);
            });
            menu!.append(item);
        });
        if (footer) {
            const footerEl = document.createElement("div");
            footerEl.className = "choice-menu-footer";
            footerEl.append(footerKeys?.length ? `${footer} ` : footer, ...keyCaps(footerKeys ?? []));
            menu.append(footerEl);
        }
        menu.addEventListener("keydown", (event) => {
            const all = items();
            const at = all.indexOf(document.activeElement as HTMLButtonElement);
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const step = event.key === "ArrowDown" ? 1 : -1;
                all[(at + step + all.length) % all.length]?.focus();
            } else if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                close();
            } else if (event.key === "Tab") {
                close(false);
            }
        });
        (anchor.parentElement ?? document.body).append(menu);
        // Opens above its own button, whichever group of the bar that sits in.
        menu.style.left = `${anchor.offsetLeft}px`;
        anchor.setAttribute("aria-expanded", "true");
        document.addEventListener("pointerdown", onOutsidePointer, true);
        openMenuClosers.add(closeQuietly);
        const current = list.findIndex((choice) => choice.selected);
        items()[current >= 0 ? current : 0]?.focus();
    }

    anchor.addEventListener("click", () => {
        if (menu) close();
        else open();
    });

    return { open, close: () => close(), isOpen: () => menu !== null };
}
