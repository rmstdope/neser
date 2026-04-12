import { expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const appJs = readFileSync(join(__dirname, "..", "app.js"), "utf8");

// Extract the debugger-controls HTML block from app.js.
// The block ends after the last dbg-run-to-irq button to capture both rows.
function extractDebuggerControlsHtml(src) {
    const start = src.indexOf("`<div class=\"debugger-controls\">`");
    if (start === -1) return "";
    const lastButtonId = "dbg-run-to-irq";
    const lastButtonPos = src.indexOf(`id="${lastButtonId}"`, start);
    if (lastButtonPos === -1) return src.slice(start);
    // Extend a bit past the last button to include it fully
    return src.slice(start, lastButtonPos + 100);
}

// Extract all button ids from a snippet in the order they appear
function extractButtonIds(snippet) {
    const pattern = /id="(dbg-[a-z-]+)"/g;
    const ids = [];
    let match;
    while ((match = pattern.exec(snippet)) !== null) {
        ids.push(match[1]);
    }
    return ids;
}

// Find index of an id inside a source string
function posOf(src, id) {
    return src.indexOf(`id="${id}"`);
}

const controlsRegion = extractDebuggerControlsHtml(appJs);

it("debugger toolbar: Continue appears before Step over", () => {
    const continuePos = posOf(controlsRegion, "dbg-continue");
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    expect(continuePos !== -1, "dbg-continue button should exist in toolbar").toBeTruthy();
    expect(stepOverPos !== -1, "dbg-step-over button should exist in toolbar").toBeTruthy();
    expect(continuePos < stepOverPos, `Continue (pos ${continuePos}) should appear before Step over (pos ${stepOverPos})`).toBeTruthy();
});

it("debugger toolbar: Step over appears before Step into", () => {
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    expect(stepOverPos !== -1, "dbg-step-over button should exist in toolbar").toBeTruthy();
    expect(stepIntoPos !== -1, "dbg-step-into button should exist in toolbar").toBeTruthy();
    expect(stepOverPos < stepIntoPos, `Step over (pos ${stepOverPos}) should appear before Step into (pos ${stepIntoPos})`).toBeTruthy();
});

it("debugger toolbar: PPU viewer button appears after Continue, Step over, Step into (upper row right)", () => {
    const ppuPos = posOf(controlsRegion, "dbg-toggle-ppu-viewer");
    const continuePos = posOf(controlsRegion, "dbg-continue");
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    expect(ppuPos !== -1, "dbg-toggle-ppu-viewer button should exist in toolbar").toBeTruthy();
    expect(ppuPos > continuePos, `PPU viewer (pos ${ppuPos}) should appear after Continue (pos ${continuePos})`).toBeTruthy();
    expect(ppuPos > stepOverPos, `PPU viewer (pos ${ppuPos}) should appear after Step over (pos ${stepOverPos})`).toBeTruthy();
    expect(ppuPos > stepIntoPos, `PPU viewer (pos ${ppuPos}) should appear after Step into (pos ${stepIntoPos})`).toBeTruthy();
});

it("debugger toolbar lower row: Run to next frame appears before Run to next scanline", () => {
    const framePos = posOf(controlsRegion, "dbg-run-next-frame");
    const scanlinePos = posOf(controlsRegion, "dbg-run-next-scanline");
    expect(framePos !== -1, "dbg-run-next-frame button should exist").toBeTruthy();
    expect(scanlinePos !== -1, "dbg-run-next-scanline button should exist").toBeTruthy();
    expect(framePos < scanlinePos, `Run to next frame (pos ${framePos}) should appear before Run to next scanline (pos ${scanlinePos})`).toBeTruthy();
});

it("debugger toolbar lower row: Run to next scanline appears before Run to NMI", () => {
    const scanlinePos = posOf(controlsRegion, "dbg-run-next-scanline");
    const nmiPos = posOf(controlsRegion, "dbg-run-to-nmi");
    expect(scanlinePos !== -1, "dbg-run-next-scanline button should exist").toBeTruthy();
    expect(nmiPos !== -1, "dbg-run-to-nmi button should exist").toBeTruthy();
    expect(scanlinePos < nmiPos, `Run to next scanline (pos ${scanlinePos}) should appear before Run to NMI (pos ${nmiPos})`).toBeTruthy();
});

it("debugger toolbar lower row: Run to NMI appears before Run to IRQ", () => {
    const nmiPos = posOf(controlsRegion, "dbg-run-to-nmi");
    const irqPos = posOf(controlsRegion, "dbg-run-to-irq");
    expect(nmiPos !== -1, "dbg-run-to-nmi button should exist").toBeTruthy();
    expect(irqPos !== -1, "dbg-run-to-irq button should exist").toBeTruthy();
    expect(nmiPos < irqPos, `Run to NMI (pos ${nmiPos}) should appear before Run to IRQ (pos ${irqPos})`).toBeTruthy();
});

it("debugger toolbar lower row buttons appear after upper row buttons", () => {
    const ppuPos = posOf(controlsRegion, "dbg-toggle-ppu-viewer");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    const upperRowEnd = Math.max(ppuPos, stepIntoPos);
    const framePos = posOf(controlsRegion, "dbg-run-next-frame");
    expect(framePos > upperRowEnd).toBeTruthy();
});
