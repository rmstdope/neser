/**
 * @vitest-environment jsdom
 */
import { beforeEach, describe, expect, it } from "vitest";

import indexHtml from "../../index.html?raw";
import type { FirmwareStore } from "./snes_firmware_store";
import {
    createFirmwareSidebar,
    obtainSnesFirmware,
    requestSnesFirmware,
    type FirmwareDialogElements,
    type IsGenuine
} from "./snes_firmware_dialog";
import { chipByKey } from "./snes_firmware_words";

const dsp1 = chipByKey("dsp1")!;
const dsp2 = chipByKey("dsp2")!;
const dsp3 = chipByKey("dsp3")!;
const dsp4 = chipByKey("dsp4")!;

/** Tests' stand-in for the wasm check: a file filled with 0x11 is the genuine DSP-1 firmware, 0x22 the DSP-2's, 0x33 the DSP-3's, 0x44 the DSP-4's. */
const FILL: Record<string, number> = { dsp1: 0x11, dsp2: 0x22, dsp3: 0x33, dsp4: 0x44 };
const isGenuine: IsGenuine = (key, bytes) => bytes.length === 8192 && bytes[0] === (FILL[key] ?? -1);

function loadMarkup() {
    document.documentElement.innerHTML = new DOMParser()
        .parseFromString(indexHtml, "text/html")
        .documentElement.innerHTML;
    const dialog = document.getElementById("snes-firmware-modal") as HTMLDialogElement;
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

function dialogElements(): FirmwareDialogElements {
    return {
        dialog: document.getElementById("snes-firmware-modal") as HTMLDialogElement,
        title: document.getElementById("snes-firmware-title")!,
        text: document.getElementById("snes-firmware-text")!,
        chooseButton: document.getElementById("snes-firmware-choose") as HTMLButtonElement,
        cancelButton: document.getElementById("snes-firmware-cancel") as HTMLButtonElement,
        fileInput: document.getElementById("snes-firmware-file") as HTMLInputElement
    };
}

function chooseFile(input: HTMLInputElement, name: string, size: number, fill = 0) {
    const file = new File([new Uint8Array(size).fill(fill)], name);
    Object.defineProperty(input, "files", { value: [file], configurable: true });
    input.dispatchEvent(new Event("change"));
}

const flush = () => new Promise((r) => setTimeout(r, 0));

type MemoryStore = FirmwareStore & { files: Map<string, Uint8Array> };

function memoryStore(initial: Record<string, Uint8Array> = {}): MemoryStore {
    const files = new Map(Object.entries(initial));
    return {
        files,
        async load(key) {
            return files.get(key) ?? null;
        },
        async store(key, bytes) {
            files.set(key, bytes);
        },
        async forget(key) {
            files.delete(key);
        },
        async storedKeys() {
            return [...files.keys()];
        }
    };
}

function failingStore(initial: Record<string, Uint8Array> = {}): FirmwareStore {
    const store = memoryStore(initial);
    return {
        ...store,
        async store() {
            throw new Error("QuotaExceededError");
        }
    };
}

const genuine = (key: "dsp1" | "dsp2" | "dsp3" | "dsp4") => new Uint8Array(8192).fill(FILL[key]);

describe("SNES firmware dialog", () => {
    beforeEach(loadMarkup);

    it("opens with the agreed DSP-1 words and focus on Choose file…", () => {
        const el = dialogElements();
        void requestSnesFirmware(el, dsp1, isGenuine);
        expect(el.dialog.open).toBe(true);
        expect(el.title.textContent).toBe("This game needs the DSP-1 firmware");
        expect(el.text.textContent).toBe(
            "Choose your dsp1b.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(el.chooseButton.textContent).toBe("Choose file…");
        expect(el.cancelButton.textContent).toBe("Cancel");
        expect(document.activeElement).toBe(el.chooseButton);
    });

    it("opens with the agreed DSP-2 words", () => {
        const el = dialogElements();
        void requestSnesFirmware(el, dsp2, isGenuine);
        expect(el.title.textContent).toBe("This game needs the DSP-2 firmware");
        expect(el.text.textContent).toBe(
            "Choose your dsp2.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(document.activeElement).toBe(el.chooseButton);
    });

    it("opens with the agreed DSP-4 words", () => {
        const el = dialogElements();
        void requestSnesFirmware(el, dsp4, isGenuine);
        expect(el.title.textContent).toBe("This game needs the DSP-4 firmware");
        expect(el.text.textContent).toBe(
            "Choose your dsp4.rom file (8 KB). It stays in this browser, so you only do this once."
        );
        expect(document.activeElement).toBe(el.chooseButton);
    });

    it("keeps the DSP-4 dialog open on another chip's 8 KB file", async () => {
        const el = dialogElements();
        let result: Uint8Array | null | undefined;
        void requestSnesFirmware(el, dsp4, isGenuine).then((r) => (result = r));
        chooseFile(el.fileInput, "dsp3.rom", 8192, 0x22);
        await flush();
        await flush();
        expect(el.dialog.open).toBe(true);
        expect(result).toBeUndefined();
        expect(el.title.textContent).toBe("That isn't a DSP-4 firmware file");
        expect(el.text.textContent).toBe("\"dsp3.rom\" is 8 KB but is not the DSP-4 firmware.");
    });

    it("keeps the dialog open on a wrong-size file and says why", async () => {
        const el = dialogElements();
        let result: Uint8Array | null | undefined;
        void requestSnesFirmware(el, dsp2, isGenuine).then((r) => (result = r));
        chooseFile(el.fileInput, "mario.zip", 1258291);
        await flush();
        expect(el.dialog.open).toBe(true);
        expect(result).toBeUndefined();
        expect(el.title.textContent).toBe("That isn't a DSP-2 firmware file");
        expect(el.text.textContent).toBe("\"mario.zip\" is 1.2 MB; the firmware is exactly 8 KB.");
    });

    it("keeps the dialog open on an 8 KB file that is not the chip's firmware", async () => {
        const el = dialogElements();
        let result: Uint8Array | null | undefined;
        void requestSnesFirmware(el, dsp2, isGenuine).then((r) => (result = r));
        chooseFile(el.fileInput, "dsp1b.rom", 8192, 0x11);
        await flush();
        await flush();
        expect(el.dialog.open).toBe(true);
        expect(result).toBeUndefined();
        expect(el.title.textContent).toBe("That isn't a DSP-2 firmware file");
        expect(el.text.textContent).toBe("\"dsp1b.rom\" is 8 KB but is not the DSP-2 firmware.");
        expect(document.activeElement).toBe(el.chooseButton);
    });

    it("resolves the bytes of a genuine file of any name and closes", async () => {
        const el = dialogElements();
        const pending = requestSnesFirmware(el, dsp1, isGenuine);
        chooseFile(el.fileInput, "my-dump.bin", 8192, 0x11);
        const bytes = await pending;
        expect(bytes?.length).toBe(8192);
        expect(bytes?.[0]).toBe(0x11);
        expect(el.dialog.open).toBe(false);
    });

    it("lets the player choose again after a wrong file", async () => {
        const el = dialogElements();
        const pending = requestSnesFirmware(el, dsp2, isGenuine);
        chooseFile(el.fileInput, "dsp1b.rom", 8192, 0x11);
        await flush();
        await flush();
        chooseFile(el.fileInput, "dsp2.rom", 8192, 0x22);
        expect((await pending)?.[0]).toBe(0x22);
    });

    it("resolves null on Cancel", async () => {
        const el = dialogElements();
        const pending = requestSnesFirmware(el, dsp2, isGenuine);
        el.cancelButton.click();
        expect(await pending).toBeNull();
        expect(el.dialog.open).toBe(false);
    });

    it("resolves null when Esc closes it", async () => {
        const el = dialogElements();
        const pending = requestSnesFirmware(el, dsp2, isGenuine);
        el.dialog.dispatchEvent(new Event("cancel"));
        el.dialog.close();
        expect(await pending).toBeNull();
    });

    it("opens with the first words again after a wrong file last time", async () => {
        const el = dialogElements();
        const first = requestSnesFirmware(el, dsp1, isGenuine);
        chooseFile(el.fileInput, "x.bin", 3);
        await flush();
        el.cancelButton.click();
        await first;
        void requestSnesFirmware(el, dsp2, isGenuine);
        expect(el.title.textContent).toBe("This game needs the DSP-2 firmware");
    });
});

describe("SNES firmware sidebar block", () => {
    beforeEach(loadMarkup);

    function sidebar(store: FirmwareStore) {
        const messages: string[] = [];
        const elements = {
            section: document.getElementById("snes-firmware-section")!,
            rows: document.getElementById("snes-firmware-rows")!,
            fileInput: document.getElementById("snes-firmware-replace-file") as HTMLInputElement
        };
        const controller = createFirmwareSidebar({
            elements,
            store,
            isGenuine,
            showMessage: (m) => messages.push(m)
        });
        return { elements, controller, messages };
    }

    const rowTexts = (rows: HTMLElement) =>
        [...rows.querySelectorAll("[data-chip]")].map((row) =>
            [...row.querySelectorAll("span, button")].map((e) => e.textContent).join(" ")
        );
    const button = (rows: HTMLElement, key: string, text: string) =>
        [...rows.querySelectorAll(`[data-chip="${key}"] button`)].find(
            (b) => b.textContent === text
        ) as HTMLButtonElement;

    it("is absent until a firmware is stored", async () => {
        const store = memoryStore();
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(elements.section.classList.contains("hidden")).toBe(true);
        store.files.set("dsp2", genuine("dsp2"));
        await controller.refresh();
        expect(elements.section.classList.contains("hidden")).toBe(false);
        expect(rowTexts(elements.rows)).toEqual(["DSP-2: stored ✓ Replace… Forget"]);
    });

    it("does not list a stored file that is not genuine (stored when only the size was checked)", async () => {
        const store = memoryStore({ dsp1: new Uint8Array(8192), dsp2: genuine("dsp2") });
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(rowTexts(elements.rows)).toEqual(["DSP-2: stored ✓ Replace… Forget"]);

        store.files.delete("dsp2");
        await controller.refresh();
        expect(elements.section.classList.contains("hidden")).toBe(true);
    });

    it("lists one row per stored chip, DSP-1 first whatever the order they were stored in", async () => {
        const store = memoryStore({ dsp2: genuine("dsp2"), dsp1: genuine("dsp1") });
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(rowTexts(elements.rows)).toEqual([
            "DSP-1: stored ✓ Replace… Forget",
            "DSP-2: stored ✓ Replace… Forget"
        ]);
    });

    it("lists DSP-4 after the other chips, in chip order", async () => {
        const store = memoryStore({ dsp4: genuine("dsp4"), dsp2: genuine("dsp2"), dsp1: genuine("dsp1") });
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(rowTexts(elements.rows)).toEqual([
            "DSP-1: stored ✓ Replace… Forget",
            "DSP-2: stored ✓ Replace… Forget",
            "DSP-4: stored ✓ Replace… Forget"
        ]);
    });

    it("lists DSP-3 between DSP-2 and DSP-4, in chip order", async () => {
        const store = memoryStore({
            dsp4: genuine("dsp4"),
            dsp3: genuine("dsp3"),
            dsp1: genuine("dsp1"),
            dsp2: genuine("dsp2")
        });
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        expect(rowTexts(elements.rows)).toEqual([
            "DSP-1: stored ✓ Replace… Forget",
            "DSP-2: stored ✓ Replace… Forget",
            "DSP-3: stored ✓ Replace… Forget",
            "DSP-4: stored ✓ Replace… Forget"
        ]);
    });

    it("Replace… and Forget on the DSP-3 row act on the DSP-3 file only", async () => {
        const old = genuine("dsp3");
        const store = memoryStore({ dsp2: genuine("dsp2"), dsp3: old });
        const { elements, controller, messages } = sidebar(store);
        await controller.refresh();

        button(elements.rows, "dsp3", "Replace…").click();
        chooseFile(elements.fileInput, "dsp2.rom", 8192, 0x22);
        await flush();
        await flush();
        expect(store.files.get("dsp3")).toBe(old);
        expect(messages).toEqual([
            "That isn't a DSP-3 firmware file\n\"dsp2.rom\" is 8 KB but is not the DSP-3 firmware."
        ]);

        button(elements.rows, "dsp3", "Forget").click();
        await flush();
        await flush();
        expect(store.files.has("dsp3")).toBe(false);
        expect(store.files.has("dsp2")).toBe(true);
        expect(rowTexts(elements.rows)).toEqual(["DSP-2: stored ✓ Replace… Forget"]);
    });

    it("Forget on the DSP-4 row removes only the DSP-4 file", async () => {
        const store = memoryStore({ dsp1: genuine("dsp1"), dsp4: genuine("dsp4") });
        const { elements, controller } = sidebar(store);
        await controller.refresh();
        button(elements.rows, "dsp4", "Forget").click();
        await flush();
        await flush();
        expect(store.files.has("dsp4")).toBe(false);
        expect(store.files.has("dsp1")).toBe(true);
        expect(rowTexts(elements.rows)).toEqual(["DSP-1: stored ✓ Replace… Forget"]);
    });

    it("Forget removes only that chip's file at once; the last one hides the block", async () => {
        const store = memoryStore({ dsp1: genuine("dsp1"), dsp2: genuine("dsp2") });
        const { elements, controller } = sidebar(store);
        await controller.refresh();

        button(elements.rows, "dsp2", "Forget").click();
        await flush();
        await flush();
        expect(store.files.has("dsp2")).toBe(false);
        expect(store.files.has("dsp1")).toBe(true);
        expect(rowTexts(elements.rows)).toEqual(["DSP-1: stored ✓ Replace… Forget"]);
        expect(elements.section.classList.contains("hidden")).toBe(false);

        button(elements.rows, "dsp1", "Forget").click();
        await flush();
        await flush();
        expect(store.files.size).toBe(0);
        expect(elements.section.classList.contains("hidden")).toBe(true);
    });

    it("Replace… on a row stores a genuine file for that chip and says so", async () => {
        const old = genuine("dsp2");
        const store = memoryStore({ dsp1: genuine("dsp1"), dsp2: old });
        const { elements, controller, messages } = sidebar(store);
        await controller.refresh();
        button(elements.rows, "dsp2", "Replace…").click();
        chooseFile(elements.fileInput, "dsp2.rom", 8192, 0x22);
        await flush();
        await flush();
        expect(store.files.get("dsp2")).not.toBe(old);
        expect(store.files.get("dsp2")?.[0]).toBe(0x22);
        expect(store.files.get("dsp1")?.[0]).toBe(0x11);
        expect(messages).toEqual(["DSP-2 firmware stored"]);
    });

    it("Replace… with another chip's file keeps the old one and shows the words at the bottom", async () => {
        const old = genuine("dsp2");
        const store = memoryStore({ dsp2: old });
        const { elements, controller, messages } = sidebar(store);
        await controller.refresh();
        button(elements.rows, "dsp2", "Replace…").click();
        chooseFile(elements.fileInput, "dsp1b.rom", 8192, 0x11);
        await flush();
        await flush();
        expect(store.files.get("dsp2")).toBe(old);
        expect(messages).toEqual([
            "That isn't a DSP-2 firmware file\n\"dsp1b.rom\" is 8 KB but is not the DSP-2 firmware."
        ]);
    });

    it("Replace… with a wrong-size file keeps the old one and shows the words at the bottom", async () => {
        const old = genuine("dsp1");
        const store = memoryStore({ dsp1: old });
        const { elements, controller, messages } = sidebar(store);
        await controller.refresh();
        button(elements.rows, "dsp1", "Replace…").click();
        chooseFile(elements.fileInput, "big.bin", 12800);
        await flush();
        expect(store.files.get("dsp1")).toBe(old);
        expect(messages).toEqual([
            "That isn't a DSP-1 firmware file\n\"big.bin\" is 12.5 KB; the firmware is exactly 8 KB."
        ]);
    });

    it("Replace… does not claim it stored the file when storing fails", async () => {
        const { elements, controller, messages } = sidebar(failingStore({ dsp1: genuine("dsp1") }));
        await controller.refresh();
        button(elements.rows, "dsp1", "Replace…").click();
        chooseFile(elements.fileInput, "dsp1b.rom", 8192, 0x11);
        await flush();
        await flush();
        expect(messages).toEqual([]);
    });
});

describe("obtaining the firmware for a game", () => {
    it("uses the stored genuine firmware without asking", async () => {
        const stored = genuine("dsp2");
        let asked = false;
        const result = await obtainSnesFirmware({
            chip: dsp2,
            store: memoryStore({ dsp2: stored }),
            isGenuine,
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

    it("asks for the DSP-2 file even when DSP-1 firmware is stored", async () => {
        let asked = false;
        const result = await obtainSnesFirmware({
            chip: dsp2,
            store: memoryStore({ dsp1: genuine("dsp1") }),
            isGenuine,
            request: async () => {
                asked = true;
                return null;
            },
            showMessage: () => {},
            onStored: async () => {}
        });
        expect(asked).toBe(true);
        expect(result).toBeNull();
    });

    it("asks for the DSP-3 file even when DSP-1, DSP-2 and DSP-4 firmware are stored", async () => {
        let asked = false;
        const result = await obtainSnesFirmware({
            chip: dsp3,
            store: memoryStore({ dsp1: genuine("dsp1"), dsp2: genuine("dsp2"), dsp4: genuine("dsp4") }),
            isGenuine,
            request: async () => {
                asked = true;
                return null;
            },
            showMessage: () => {},
            onStored: async () => {}
        });
        expect(asked).toBe(true);
        expect(result).toBeNull();
    });

    it("asks for the DSP-4 file even when DSP-1 and DSP-2 firmware are stored", async () => {
        let asked = false;
        const result = await obtainSnesFirmware({
            chip: dsp4,
            store: memoryStore({ dsp1: genuine("dsp1"), dsp2: genuine("dsp2") }),
            isGenuine,
            request: async () => {
                asked = true;
                return null;
            },
            showMessage: () => {},
            onStored: async () => {}
        });
        expect(asked).toBe(true);
        expect(result).toBeNull();
    });

    it("asks again when the stored file is not genuine (stored before the check existed)", async () => {
        const store = memoryStore({ dsp1: new Uint8Array(8192) });
        const chosen = genuine("dsp1");
        const result = await obtainSnesFirmware({
            chip: dsp1,
            store,
            isGenuine,
            request: async () => chosen,
            showMessage: () => {},
            onStored: async () => {}
        });
        expect(result).toBe(chosen);
        expect(store.files.get("dsp1")).toBe(chosen);
    });

    it("returns null when the player cancels", async () => {
        const messages: string[] = [];
        const result = await obtainSnesFirmware({
            chip: dsp2,
            store: memoryStore(),
            isGenuine,
            request: async () => null,
            showMessage: (m) => messages.push(m),
            onStored: async () => {}
        });
        expect(result).toBeNull();
        expect(messages).toEqual([]);
    });

    it("stores a chosen file under its chip, says so and refreshes the sidebar", async () => {
        const store = memoryStore();
        const chosen = genuine("dsp2");
        const messages: string[] = [];
        let refreshed = false;
        const result = await obtainSnesFirmware({
            chip: dsp2,
            store,
            isGenuine,
            request: async () => chosen,
            showMessage: (m) => messages.push(m),
            onStored: async () => {
                refreshed = true;
            }
        });
        expect(result).toBe(chosen);
        expect(store.files.get("dsp2")).toBe(chosen);
        expect(messages).toEqual(["DSP-2 firmware stored"]);
        expect(refreshed).toBe(true);
    });

    it("still starts the game but does not claim it was stored when storing fails", async () => {
        const chosen = genuine("dsp1");
        const messages: string[] = [];
        const result = await obtainSnesFirmware({
            chip: dsp1,
            store: failingStore(),
            isGenuine,
            request: async () => chosen,
            showMessage: (m) => messages.push(m),
            onStored: async () => {}
        });
        expect(result).toBe(chosen);
        expect(messages).toEqual([]);
    });
});

describe("SNES firmware markup", () => {
    it("has the agreed sidebar title, hidden by default, with no fixed rows", () => {
        const doc = new DOMParser().parseFromString(indexHtml, "text/html");
        const section = doc.getElementById("snes-firmware-section")!;
        expect(section.classList.contains("hidden")).toBe(true);
        expect(section.textContent).toContain("SNES firmware");
        expect(doc.getElementById("snes-firmware-rows")!.children.length).toBe(0);
        expect(doc.getElementById("snes-firmware-replace-file")).not.toBeNull();
    });

    it("has the firmware dialog with Cancel and Choose file…", () => {
        const doc = new DOMParser().parseFromString(indexHtml, "text/html");
        expect(doc.getElementById("snes-firmware-modal")).not.toBeNull();
        expect(doc.getElementById("snes-firmware-cancel")!.textContent).toBe("Cancel");
        expect(doc.getElementById("snes-firmware-choose")!.textContent).toBe("Choose file…");
        expect(doc.getElementById("snes-firmware-choose")!.hasAttribute("autofocus")).toBe(true);
    });
});
