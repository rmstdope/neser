// The dialog that asks once for a SNES chip's firmware, and the sidebar block that lists, replaces
// and forgets each stored one (nr-auv, nr-608).

import type { FirmwareStore } from "./snes_firmware_store";
import {
    SNES_FIRMWARE_CHIPS,
    SNES_FIRMWARE_SIZE,
    type SnesFirmwareChip,
    dialogText,
    dialogTitle,
    notGenuineDetail,
    sidebarStored,
    storedMessage,
    wrongFileTitle,
    wrongSizeDetail
} from "./snes_firmware_words";

/** Whether `bytes` is a genuine dump of the chip `key` (the wasm `snes_dsp_firmware_is_genuine`). */
export type IsGenuine = (key: string, bytes: Uint8Array) => boolean;

export type FirmwareDialogElements = {
    dialog: HTMLDialogElement;
    title: HTMLElement;
    text: HTMLElement;
    chooseButton: HTMLButtonElement;
    cancelButton: HTMLButtonElement;
    fileInput: HTMLInputElement;
};

/** A chosen file's bytes, or the second line saying why it is refused (size first, then genuineness). */
async function checkFile(
    chip: SnesFirmwareChip,
    file: File,
    isGenuine: IsGenuine
): Promise<{ bytes: Uint8Array } | { refused: string }> {
    if (file.size !== SNES_FIRMWARE_SIZE) {
        return { refused: wrongSizeDetail(file.name, file.size) };
    }
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (!isGenuine(chip.key, bytes)) {
        return { refused: notGenuineDetail(chip, file.name) };
    }
    return { bytes };
}

/**
 * Opens the dialog for `chip` and resolves with the chosen firmware, or `null` when the player
 * cancels (Cancel, Esc or the backdrop). A file of the wrong size, or one that is not the
 * chip's genuine firmware, keeps the dialog open and says so.
 */
export function requestSnesFirmware(
    el: FirmwareDialogElements,
    chip: SnesFirmwareChip,
    isGenuine: IsGenuine
): Promise<Uint8Array | null> {
    el.title.textContent = dialogTitle(chip);
    el.text.textContent = dialogText(chip);
    el.fileInput.value = "";

    return new Promise((resolve) => {
        let settled = false;
        const onChoose = () => el.fileInput.click();
        const onCancelButton = () => finish(null);
        const onClose = () => finish(null);
        const onFile = async () => {
            const file = el.fileInput.files?.[0];
            el.fileInput.value = "";
            if (!file || settled) return;
            const checked = await checkFile(chip, file, isGenuine);
            if (settled) return;
            if ("refused" in checked) {
                el.title.textContent = wrongFileTitle(chip);
                el.text.textContent = checked.refused;
                el.chooseButton.focus();
                return;
            }
            finish(checked.bytes);
        };

        function finish(value: Uint8Array | null) {
            if (settled) return;
            settled = true;
            el.chooseButton.removeEventListener("click", onChoose);
            el.cancelButton.removeEventListener("click", onCancelButton);
            el.dialog.removeEventListener("close", onClose);
            el.fileInput.removeEventListener("change", onFile);
            if (el.dialog.open) el.dialog.close();
            resolve(value);
        }

        el.chooseButton.addEventListener("click", onChoose);
        el.cancelButton.addEventListener("click", onCancelButton);
        // Esc fires `cancel` and then `close`; the backdrop form closes it too.
        el.dialog.addEventListener("close", onClose);
        el.fileInput.addEventListener("change", onFile);
        el.dialog.showModal();
        el.chooseButton.focus();
    });
}

/**
 * The firmware for a game using `chip`: the stored one, or the one the player chooses now,
 * which is stored at once. A stored file that is not genuine (stored when only the size was
 * checked) counts as not stored. "Stored" is only said once storing succeeded; if it fails
 * (private window, quota) the game still starts with the chosen file and the next game asks
 * again.
 */
export async function obtainSnesFirmware({
    chip,
    store,
    isGenuine,
    request,
    showMessage,
    onStored
}: {
    chip: SnesFirmwareChip;
    store: FirmwareStore;
    isGenuine: IsGenuine;
    request: () => Promise<Uint8Array | null>;
    showMessage: (message: string) => void;
    onStored: () => Promise<void>;
}): Promise<Uint8Array | null> {
    const stored = await store.load(chip.key).catch(() => null);
    if (stored && isGenuine(chip.key, stored)) return stored;
    const chosen = await request();
    if (!chosen) return null;
    try {
        await store.store(chip.key, chosen);
    } catch (err) {
        console.error(`Failed to store ${chip.label} firmware`, err);
        return chosen;
    }
    showMessage(storedMessage(chip));
    await onStored();
    return chosen;
}

export type FirmwareSidebarElements = {
    section: HTMLElement;
    /** Where the rows go, one per stored chip. */
    rows: HTMLElement;
    /** The hidden file chooser every row's Replace… opens. */
    fileInput: HTMLInputElement;
};

/**
 * Wires the sidebar's "SNES firmware" block: one row per chip with a stored genuine file, in
 * `SNES_FIRMWARE_CHIPS` order, each with its own Replace… and Forget; the block is shown only
 * while a row exists. Replace… checks like the dialog (messages at the bottom, since no dialog
 * is open); Forget removes that chip's file at once, without asking.
 */
export function createFirmwareSidebar({
    elements,
    store,
    isGenuine,
    showMessage
}: {
    elements: FirmwareSidebarElements;
    store: FirmwareStore;
    isGenuine: IsGenuine;
    showMessage: (message: string) => void;
}) {
    const { section, rows, fileInput } = elements;
    let replacing: SnesFirmwareChip | null = null;

    function row(chip: SnesFirmwareChip): HTMLElement {
        const line = document.createElement("div");
        line.className = "flex items-center gap-2";
        line.dataset.chip = chip.key;
        const label = document.createElement("span");
        label.className = "text-xs flex-1";
        label.textContent = sidebarStored(chip);
        const replace = document.createElement("button");
        replace.type = "button";
        replace.className = "btn btn-outline btn-xs";
        replace.textContent = "Replace…";
        replace.setAttribute("aria-label", `Replace the ${chip.label} firmware`);
        replace.addEventListener("click", () => {
            replacing = chip;
            fileInput.value = "";
            fileInput.click();
        });
        const forget = document.createElement("button");
        forget.type = "button";
        forget.className = "btn btn-outline btn-xs";
        forget.textContent = "Forget";
        forget.setAttribute("aria-label", `Forget the ${chip.label} firmware`);
        forget.addEventListener("click", async () => {
            await store.forget(chip.key);
            await refresh();
        });
        line.append(label, replace, forget);
        return line;
    }

    async function refresh() {
        const keys = new Set(await store.storedKeys().catch(() => [] as string[]));
        // A file that is not genuine (stored when only the size was checked) counts as not
        // stored, as it does when a game asks for it, so it gets no row.
        const stored: SnesFirmwareChip[] = [];
        for (const chip of SNES_FIRMWARE_CHIPS) {
            if (!keys.has(chip.key)) continue;
            const bytes = await store.load(chip.key).catch(() => null);
            if (bytes && isGenuine(chip.key, bytes)) stored.push(chip);
        }
        rows.replaceChildren(...stored.map(row));
        section.classList.toggle("hidden", stored.length === 0);
    }

    fileInput.addEventListener("change", async () => {
        const file = fileInput.files?.[0];
        const chip = replacing;
        fileInput.value = "";
        if (!file || !chip) return;
        const checked = await checkFile(chip, file, isGenuine);
        if ("refused" in checked) {
            showMessage(`${wrongFileTitle(chip)}\n${checked.refused}`);
            return;
        }
        try {
            await store.store(chip.key, checked.bytes);
        } catch (err) {
            console.error(`Failed to store ${chip.label} firmware`, err);
            return;
        }
        showMessage(storedMessage(chip));
        await refresh();
    });

    return { refresh };
}
