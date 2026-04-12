import { expect, it } from "vitest";
import { handleRomSelection } from "./rom_selection.js";

it("handleRomSelection stops running emulator before loading new ROM", async () => {
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

    expect(calls).toEqual(["stop", "apply"]);
});

it("handleRomSelection does not stop when not running", async () => {
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

    expect(calls).toEqual(["apply"]);
});

it("handleRomSelection auto-starts after loading", async () => {
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

    expect(calls).toEqual(["apply", "start"]);
});

it("handleRomSelection focuses canvas after ROM is loaded and started", async () => {
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

    expect(calls).toEqual(["apply", "start", "focus"]);
});

it("handleRomSelection focuses canvas even when no start callback is provided", async () => {
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

    expect(calls).toEqual(["apply", "focus"]);
});
