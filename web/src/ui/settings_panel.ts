/** One row of a console's key list: the button and the keyboard keys that press it. */
export type KeyBindingRow = { action: string; keys: string[] };

/** Splits a console's key text ("W/A/S/D: D-Pad\nR: A") into one row per button. */
export function keyBindingRows(text: string): KeyBindingRow[] {
    return text
        .split("\n")
        .filter((line) => line.includes(": "))
        .map((line) => {
            const [keys, action] = line.split(": ");
            return { action, keys: keys.split("/") };
        });
}

/** Fills `list` with one row per button: its name, then a key cap per key. */
export function renderKeyList(list: HTMLElement, text: string): void {
    list.replaceChildren(
        ...keyBindingRows(text).map(({ action, keys }) => {
            const row = document.createElement("div");
            row.className = "settings-key-row";
            const name = document.createElement("span");
            name.className = "settings-key-action";
            name.textContent = action;
            const caps = document.createElement("span");
            caps.className = "settings-key-caps";
            caps.append(
                ...keys.map((key) => {
                    const cap = document.createElement("kbd");
                    cap.className = "kbd kbd-sm";
                    cap.textContent = key;
                    return cap;
                })
            );
            row.append(name, caps);
            return row;
        })
    );
}

/**
 * The settings panel: a modal sheet with tabs. It always opens on the first tab; `onOpen` and
 * `onClose` are told once per opening, however it closes (the close button, Esc, or code).
 */
export function createSettingsPanel({
    dialog,
    openButton,
    closeButton,
    onOpen,
    onClose,
}: {
    dialog: HTMLDialogElement;
    openButton: HTMLButtonElement;
    closeButton: HTMLButtonElement;
    onOpen: () => void;
    onClose: () => void;
}) {
    const tabs = [...dialog.querySelectorAll<HTMLElement>('[role="tab"]')];
    let isOpen = false;

    function select(tab: HTMLElement) {
        for (const other of tabs) {
            const selected = other === tab;
            other.setAttribute("aria-selected", selected ? "true" : "false");
            other.tabIndex = selected ? 0 : -1;
            const page = document.getElementById(other.getAttribute("aria-controls") ?? "");
            if (page) page.hidden = !selected;
        }
    }

    function closed() {
        if (!isOpen) return;
        isOpen = false;
        openButton.setAttribute("aria-expanded", "false");
        onClose();
    }

    function open() {
        if (isOpen) return;
        isOpen = true;
        select(tabs[0]);
        if (typeof dialog.showModal === "function") dialog.showModal();
        else dialog.setAttribute("open", "");
        openButton.setAttribute("aria-expanded", "true");
        onOpen();
    }

    function close() {
        if (typeof dialog.close === "function") dialog.close();
        else dialog.removeAttribute("open");
        closed();
    }

    for (const tab of tabs) tab.addEventListener("click", () => select(tab));
    openButton.addEventListener("click", open);
    closeButton.addEventListener("click", close);
    // Esc closes a modal dialog by itself; a click on the backdrop closes it too.
    dialog.addEventListener("close", closed);
    dialog.addEventListener("click", (event) => {
        if (event.target === dialog) close();
    });

    return { open, close, isOpen: () => isOpen };
}
