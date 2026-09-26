/**
 * @vitest-environment jsdom
 */
import { beforeEach, describe, expect, it } from "vitest";

import indexHtml from "../../index.html?raw";
import type { FirmwareStore } from "./dsp1_firmware_store";
import {
    createFirmwareSidebar,
    obtainDsp1Firmware,
    requestDsp1Firmware,
    type Dsp1DialogElements
} from "./dsp1_firmware_dialog";

function loadMarkup() {
    document.documentElement.innerHTML = new DOMParser()
        .parseFromString(indexHtml, "text/html")
        .documentElement.innerHTML;
    const dialog = document.getElementById("dsp1-firmware-modal") as HTMLDialogElement;
    // jsdom's <dialog> may lack the modal API; give it the minimal behaviour the code uses.
    if (typeof dialog.showModal !== "function" || typeof dialog.close !== "function") {
        dialog.showModal = function () {
            this.setAttribute("open", "");
        };
        dialog.close = function () {
            if (!this.hasAttribute("open")) return;
            this.removeAttribute("open");
            this.dispatchEvent(new Event("close"));
        };
    }
}

function dialogElements(): Dsp1DialogElements {
    return {
        dialog: document.getElementById("dsp1-firmware-modal") as HTMLDialogElement,
        title: document.getElementById("dsp1-firmware-title")!,
        text: document.getElementById("dsp1-firmware-text")!,
        chooseButton: document.getElementById("dsp1-firmware-choose") as HTMLButtonElement,
        cancelButton: document.getElementById("dsp1-firmware-cancel") as HTMLButtonElement,
        fileInput: document.getElementById("dsp1-firmware-file") as HTMLInputElement
    };
}

function chooseFile(input: HTMLInputElement, name: string, size: number, fill = 0) {
    const file = new File([new Uint8Array(size).fill(fill)], name);
    Object.defineProperty(input, "files", { value: [file], configurable: true });
    input.dispatchEvent(new Event("change"));
}

const flush = () => new Promise((r) => setTimeout(r, 0));

function memoryStore(initial: Uint8Array | null = null): FirmwareStore & { value: Uint8Array | null } {
    return {
        value: initial,
        async load() {
            return this.value;
        },
        async store(bytes) {
            this.value = bytes;
        },
        async forget() {
            this.value = null;
        }
    };
}

describe("DSP-1 firmware dialog", () => {
    beforeEach(loadMarkup);

    it("opens with the agreed words and focus on Choose file…", () => {
        const el = dialogElements();
        void requestDsp1Firmware(el);
        expect(el.dialog.open).toBe(true);
        expect(el.title.textContent).toBe("This game needs the DSP-1 firmware");
        expect(el.text.textContent).toBe(
            "Choose your dsp1b.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(el.chooseButton.textContent).toBe("Choose file…");
        expect(el.cancelButton.textContent).toBe("Cancel");
        expect(document.activeElement).toBe(el.chooseButton);
    });

    it("keeps the dialog open on a wrong-size file and says why", async () => {
        const el = dialogElements();
        let result: Uint8Array | null | undefined;
        void requestDsp1Firmware(el).then((r) => (result = r));
        chooseFile(el.fileInput, "Super Mario Kart (USA).sfc", 1258291);
        await flush();
        expect(el.dialog.open).toBe(true);
        expect(result).toBeUndefined();
        expect(el.title.textContent).toBe("That isn't a DSP-1 firmware file");
        expect(el.text.textContent).toBe("\"Super Mario Kart (USA).sfc\" is 1.2 MB; the firmware is exactly 8 KB.");
    });

    it("resolves the bytes of a valid file of any name and closes", async () => {
        const el = dialogElements();
        const pending = requestDsp1Firmware(el);
        chooseFile(el.fileInput, "my-dump.bin", 8192, 0x1b);
        const bytes = await pending;
        expect(bytes?.length).toBe(8192);
        expect(bytes?.[0]).toBe(0x1b);
        expect(el.dialog.open).toBe(false);
    });

    it("resolves null on Cancel", async () => {
        const el = dialogElements();
        const pending = requestDsp1Firmware(el);
        el.cancelButton.click();
        expect(await pending).toBeNull();
        expect(el.dialog.open).toBe(false);
    });

    it("resolves null when Esc closes it", async () => {
        const el = dialogElements();
        const pending = requestDsp1Firmware(el);
        el.dialog.dispatchEvent(new Event("cancel"));
        el.dialog.close();
        expect(await pending).toBeNull();
    });

    it("opens with the first words again after a wrong file last time", async () => {
        const el = dialogElements();
        const first = requestDsp1Firmware(el);
        chooseFile(el.fileInput, "x.bin", 3);
        await flush();
        el.cancelButton.click();
        await first;
        void requestDsp1Firmware(el);
        expect(el.title.textContent).toBe("This game needs the DSP-1 firmware");
    });
});

describe("SNES firmware sidebar block", () => {
    beforeEach(loadMarkup);

    function sidebar(store: FirmwareStore) {
        const messages: string[] = [];
        const elements = {
            section: document.getElementById("snes-firmware-section")!,
            replaceButton: document.getElementById("snes-firmware-replace") as HTMLButtonElement,
            forgetButton: document.getElementById("snes-firmware-forget") as HTMLButtonElement,
            fileInput: document.getElementById("snes-firmware-replace-file") as HTMLInputElement
        };
        const controller = createFirmwareSidebar({ elements, store, showMessage: (m) => messages.push(m) });
        return { elements, controller, messages };
    }

    it("is absent until a firmware is stored", async () => {
        const store = memoryStore();
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(elements.section.classList.contains("hidden")).toBe(true);
        store.value = new Uint8Array(8192);
        await controller.refresh();
        expect(elements.section.classList.contains("hidden")).toBe(false);
    });

    it("Forget removes the firmware at once and hides the block", async () => {
        const store = memoryStore(new Uint8Array(8192));
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        elements.forgetButton.click();
        await flush();
        expect(store.value).toBeNull();
        expect(elements.section.classList.contains("hidden")).toBe(true);
    });

    it("Replace… stores a valid file and says so", async () => {
        const store = memoryStore(new Uint8Array(8192));
        const { elements, messages } = sidebar(store);
        chooseFile(elements.fileInput, "dsp1b.rom", 8192, 7);
        await flush();
        await flush();
        expect(store.value?.[0]).toBe(7);
        expect(messages).toEqual(["DSP-1 firmware stored"]);
    });

    it("Replace… does not claim it stored the file when storing fails", async () => {
        const { elements, messages } = sidebar(failingStore(new Uint8Array(8192)));
        chooseFile(elements.fileInput, "dsp1b.rom", 8192);
        await flush();
        await flush();
        expect(messages).toEqual([]);
    });

    it("Replace… with a wrong-size file keeps the old one and shows the words at the bottom", async () => {
        const old = new Uint8Array(8192).fill(1);
        const store = memoryStore(old);
        const { elements, messages } = sidebar(store);
        chooseFile(elements.fileInput, "big.bin", 12800);
        await flush();
        expect(store.value).toBe(old);
        expect(messages).toEqual([
            "That isn't a DSP-1 firmware file\n\"big.bin\" is 12.5 KB; the firmware is exactly 8 KB."
        ]);
    });
});

function failingStore(initial: Uint8Array | null = null): FirmwareStore {
    return {
        async load() {
            return initial;
        },
        async store() {
            throw new Error("QuotaExceededError");
        },
        async forget() {}
    };
}

describe("obtaining the DSP-1 firmware for a game", () => {
    it("uses the stored firmware without asking", async () => {
        const stored = new Uint8Array(8192).fill(3);
        let asked = false;
        const result = await obtainDsp1Firmware({
            store: memoryStore(stored),
            request: async () => {
                asked = true;
                return null;
            },
            showMessage: () => {},
            onStored: async () => {}
        });
        expect(result).toBe(stored);
        expect(asked).toBe(false);
    });

    it("returns null when the player cancels", async () => {
        const messages: string[] = [];
        const result = await obtainDsp1Firmware({
            store: memoryStore(),
            request: async () => null,
            showMessage: (m) => messages.push(m),
            onStored: async () => {}
        });
        expect(result).toBeNull();
        expect(messages).toEqual([]);
    });

    it("stores a chosen file, says so and reveals the sidebar block", async () => {
        const store = memoryStore();
        const chosen = new Uint8Array(8192).fill(9);
        const messages: string[] = [];
        let revealed = false;
        const result = await obtainDsp1Firmware({
            store,
            request: async () => chosen,
            showMessage: (m) => messages.push(m),
            onStored: async () => {
                revealed = true;
            }
        });
        expect(result).toBe(chosen);
        expect(store.value).toBe(chosen);
        expect(messages).toEqual(["DSP-1 firmware stored"]);
        expect(revealed).toBe(true);
    });

    it("still starts the game but does not claim it was stored when storing fails", async () => {
        const chosen = new Uint8Array(8192);
        const messages: string[] = [];
        const result = await obtainDsp1Firmware({
            store: failingStore(),
            request: async () => chosen,
            showMessage: (m) => messages.push(m),
            onStored: async () => {}
        });
        expect(result).toBe(chosen);
        expect(messages).toEqual([]);
    });
});

describe("SNES firmware markup", () => {
    it("has the agreed sidebar words, hidden by default", () => {
        const doc = new DOMParser().parseFromString(indexHtml, "text/html");
        const section = doc.getElementById("snes-firmware-section")!;
        expect(section.classList.contains("hidden")).toBe(true);
        expect(section.textContent).toContain("SNES firmware");
        expect(section.textContent).toContain("DSP-1: stored ✓");
        expect(doc.getElementById("snes-firmware-replace")!.textContent).toBe("Replace…");
        expect(doc.getElementById("snes-firmware-forget")!.textContent).toBe("Forget");
    });
});
