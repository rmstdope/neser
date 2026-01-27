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
