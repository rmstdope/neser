// The dialog that asks for the DSP-1 firmware once, and the sidebar block that shows, replaces
// and forgets the stored one (nr-auv).

import type { FirmwareStore } from "./dsp1_firmware_store";
import {
    DIALOG_TEXT,
    DIALOG_TITLE,
    DSP1_FIRMWARE_SIZE,
    STORED_MESSAGE,
    WRONG_SIZE_TITLE,
    wrongSizeDetail
} from "./dsp1_firmware_words";

export type Dsp1DialogElements = {
    dialog: HTMLDialogElement;
    title: HTMLElement;
    text: HTMLElement;
    chooseButton: HTMLButtonElement;
    cancelButton: HTMLButtonElement;
    fileInput: HTMLInputElement;
};

async function readFile(file: File): Promise<Uint8Array> {
    return new Uint8Array(await file.arrayBuffer());
}

/**
 * Opens the dialog and resolves with the chosen firmware, or `null` when the player cancels
 * (Cancel, Esc or the backdrop). A file of the wrong size keeps the dialog open and says so.
 */
export function requestDsp1Firmware(el: Dsp1DialogElements): Promise<Uint8Array | null> {
    el.title.textContent = DIALOG_TITLE;
    el.text.textContent = DIALOG_TEXT;
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
            if (file.size !== DSP1_FIRMWARE_SIZE) {
                el.title.textContent = WRONG_SIZE_TITLE;
                el.text.textContent = wrongSizeDetail(file.name, file.size);
                el.chooseButton.focus();
                return;
            }
            finish(await readFile(file));
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
 * The firmware for a DSP-1 game: the stored one, or the one the player chooses now, which is
 * stored at once. "Stored" is only said once storing succeeded; if it fails (private window,
 * quota) the game still starts with the chosen file and the next DSP-1 game asks again.
 */
export async function obtainDsp1Firmware({
    store,
    request,
    showMessage,
    onStored
}: {
    store: FirmwareStore;
    request: () => Promise<Uint8Array | null>;
    showMessage: (message: string) => void;
    onStored: () => Promise<void>;
}): Promise<Uint8Array | null> {
    const stored = await store.load().catch(() => null);
    if (stored) return stored;
    const chosen = await request();
    if (!chosen) return null;
    try {
        await store.store(chosen);
    } catch (err) {
        console.error("Failed to store DSP-1 firmware", err);
        return chosen;
    }
    showMessage(STORED_MESSAGE);
    await onStored();
    return chosen;
}

export type FirmwareSidebarElements = {
    section: HTMLElement;
    replaceButton: HTMLButtonElement;
    forgetButton: HTMLButtonElement;
    fileInput: HTMLInputElement;
};

/**
 * Wires the sidebar's "SNES firmware" block: shown only while a firmware is stored; Replace…
 * validates like the dialog (messages at the bottom, since no dialog is open); Forget removes
 * it at once, without asking.
 */
export function createFirmwareSidebar({
    elements,
    store,
    showMessage
}: {
    elements: FirmwareSidebarElements;
    store: FirmwareStore;
    showMessage: (message: string) => void;
}) {
    const { section, replaceButton, forgetButton, fileInput } = elements;

    async function refresh() {
        const stored = await store.load().catch(() => null);
        section.classList.toggle("hidden", !stored);
    }

    replaceButton.addEventListener("click", () => {
        fileInput.value = "";
        fileInput.click();
    });

    fileInput.addEventListener("change", async () => {
        const file = fileInput.files?.[0];
        fileInput.value = "";
        if (!file) return;
        if (file.size !== DSP1_FIRMWARE_SIZE) {
            showMessage(`${WRONG_SIZE_TITLE}\n${wrongSizeDetail(file.name, file.size)}`);
            return;
        }
        try {
            await store.store(await readFile(file));
        } catch (err) {
            console.error("Failed to store DSP-1 firmware", err);
            return;
        }
        showMessage(STORED_MESSAGE);
        await refresh();
    });

    forgetButton.addEventListener("click", async () => {
        await store.forget();
        section.classList.add("hidden");
    });

    return { refresh };
}
