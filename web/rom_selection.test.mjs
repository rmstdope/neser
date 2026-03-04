import assert from "node:assert/strict";
import test from "node:test";
import { handleRomSelection } from "./rom_selection.js";

test("handleRomSelection stops running emulator before loading new ROM", async () => {
    const calls = [];
    const stop = () => calls.push("stop");
    const applyRomBytes = async () => calls.push("apply");

    await handleRomSelection({
        bytes: new Uint8Array([1, 2, 3]),
        name: "Test.nes",
        running: true,
        stop,
        applyRomBytes
    });

    assert.deepEqual(calls, ["stop", "apply"]);
});

test("handleRomSelection does not stop when not running", async () => {
    const calls = [];
    const stop = () => calls.push("stop");
    const applyRomBytes = async () => calls.push("apply");

    await handleRomSelection({
        bytes: new Uint8Array([1, 2, 3]),
        name: "Test.nes",
        running: false,
        stop,
        applyRomBytes
    });

    assert.deepEqual(calls, ["apply"]);
});

test("handleRomSelection auto-starts after loading", async () => {
    const calls = [];
    const stop = () => calls.push("stop");
    const applyRomBytes = async () => calls.push("apply");
    const start = async () => calls.push("start");

    await handleRomSelection({
        bytes: new Uint8Array([1, 2, 3]),
        name: "Test.nes",
        running: false,
        stop,
        applyRomBytes,
        start
    });

    assert.deepEqual(calls, ["apply", "start"]);
});

test("handleRomSelection focuses canvas after ROM is loaded and started", async () => {
    const calls = [];
    const applyRomBytes = async () => calls.push("apply");
    const start = async () => calls.push("start");
    const focusCanvas = () => calls.push("focus");

    await handleRomSelection({
        bytes: new Uint8Array([1, 2, 3]),
        name: "Test.nes",
        running: false,
        stop: () => {},
        applyRomBytes,
        start,
        focusCanvas
    });

    assert.deepEqual(calls, ["apply", "start", "focus"]);
});

test("handleRomSelection focuses canvas even when no start callback is provided", async () => {
    const calls = [];
    const applyRomBytes = async () => calls.push("apply");
    const focusCanvas = () => calls.push("focus");

    await handleRomSelection({
        bytes: new Uint8Array([1, 2, 3]),
        name: "Test.nes",
        running: false,
        stop: () => {},
        applyRomBytes,
        focusCanvas
    });

    assert.deepEqual(calls, ["apply", "focus"]);
});
