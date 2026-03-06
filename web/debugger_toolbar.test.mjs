import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const appJs = readFileSync(join(__dirname, "app.js"), "utf8");

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

test("debugger toolbar: Continue appears before Step over", () => {
    const continuePos = posOf(controlsRegion, "dbg-continue");
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    assert.ok(continuePos !== -1, "dbg-continue button should exist in toolbar");
    assert.ok(stepOverPos !== -1, "dbg-step-over button should exist in toolbar");
    assert.ok(continuePos < stepOverPos, `Continue (pos ${continuePos}) should appear before Step over (pos ${stepOverPos})`);
});

test("debugger toolbar: Step over appears before Step into", () => {
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    assert.ok(stepOverPos !== -1, "dbg-step-over button should exist in toolbar");
    assert.ok(stepIntoPos !== -1, "dbg-step-into button should exist in toolbar");
    assert.ok(stepOverPos < stepIntoPos, `Step over (pos ${stepOverPos}) should appear before Step into (pos ${stepIntoPos})`);
});

test("debugger toolbar: PPU viewer button appears after Continue, Step over, Step into (upper row right)", () => {
    const ppuPos = posOf(controlsRegion, "dbg-toggle-ppu-viewer");
    const continuePos = posOf(controlsRegion, "dbg-continue");
    const stepOverPos = posOf(controlsRegion, "dbg-step-over");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    assert.ok(ppuPos !== -1, "dbg-toggle-ppu-viewer button should exist in toolbar");
    assert.ok(ppuPos > continuePos, `PPU viewer (pos ${ppuPos}) should appear after Continue (pos ${continuePos})`);
    assert.ok(ppuPos > stepOverPos, `PPU viewer (pos ${ppuPos}) should appear after Step over (pos ${stepOverPos})`);
    assert.ok(ppuPos > stepIntoPos, `PPU viewer (pos ${ppuPos}) should appear after Step into (pos ${stepIntoPos})`);
});

test("debugger toolbar lower row: Run to next frame appears before Run to next scanline", () => {
    const framePos = posOf(controlsRegion, "dbg-run-next-frame");
    const scanlinePos = posOf(controlsRegion, "dbg-run-next-scanline");
    assert.ok(framePos !== -1, "dbg-run-next-frame button should exist");
    assert.ok(scanlinePos !== -1, "dbg-run-next-scanline button should exist");
    assert.ok(framePos < scanlinePos, `Run to next frame (pos ${framePos}) should appear before Run to next scanline (pos ${scanlinePos})`);
});

test("debugger toolbar lower row: Run to next scanline appears before Run to NMI", () => {
    const scanlinePos = posOf(controlsRegion, "dbg-run-next-scanline");
    const nmiPos = posOf(controlsRegion, "dbg-run-to-nmi");
    assert.ok(scanlinePos !== -1, "dbg-run-next-scanline button should exist");
    assert.ok(nmiPos !== -1, "dbg-run-to-nmi button should exist");
    assert.ok(scanlinePos < nmiPos, `Run to next scanline (pos ${scanlinePos}) should appear before Run to NMI (pos ${nmiPos})`);
});

test("debugger toolbar lower row: Run to NMI appears before Run to IRQ", () => {
    const nmiPos = posOf(controlsRegion, "dbg-run-to-nmi");
    const irqPos = posOf(controlsRegion, "dbg-run-to-irq");
    assert.ok(nmiPos !== -1, "dbg-run-to-nmi button should exist");
    assert.ok(irqPos !== -1, "dbg-run-to-irq button should exist");
    assert.ok(nmiPos < irqPos, `Run to NMI (pos ${nmiPos}) should appear before Run to IRQ (pos ${irqPos})`);
});

test("debugger toolbar lower row buttons appear after upper row buttons", () => {
    const ppuPos = posOf(controlsRegion, "dbg-toggle-ppu-viewer");
    const stepIntoPos = posOf(controlsRegion, "dbg-step-into");
    const upperRowEnd = Math.max(ppuPos, stepIntoPos);
    const framePos = posOf(controlsRegion, "dbg-run-next-frame");
    assert.ok(framePos > upperRowEnd,
        `Run to next frame (pos ${framePos}) should appear after all upper row buttons (last at pos ${upperRowEnd})`);
});
