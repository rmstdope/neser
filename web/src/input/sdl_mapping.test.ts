import { expect, it } from "vitest";

import {
    extractVendorProduct,
    parseGameControllerDb,
    sdlPlatformForUserAgent,
    vendorProductFromGuid,
} from "./sdl_mapping";

const REPLICA_LINE =
    "030000001f08000001e4000006010000,USB SNES gamepad,a:b2,b:b1,x:b3,y:b0,back:b8,start:b9,leftx:a0,lefty:a1,platform:Mac OS X,";

it("extracts vendor and product from an SDL GUID (little-endian fields)", () => {
    expect(vendorProductFromGuid("030000001f08000001e4000006010000")).toEqual({
        vendor: "081f",
        product: "e401",
    });
});

it("returns null for a malformed GUID", () => {
    expect(vendorProductFromGuid("not-a-guid")).toBe(null);
});

it("extracts vendor and product from a Chrome gamepad id", () => {
    expect(extractVendorProduct("USB gamepad (Vendor: 081f Product: e401)")).toEqual({
        vendor: "081f",
        product: "e401",
    });
});

it("extracts vendor and product from a Firefox gamepad id", () => {
    expect(extractVendorProduct("81f-e401-USB gamepad")).toEqual({
        vendor: "081f",
        product: "e401",
    });
});

it("returns null when a gamepad id carries no vendor/product", () => {
    expect(extractVendorProduct("Some Bluetooth Controller")).toBe(null);
});

it("detects the SDL platform from the user agent", () => {
    expect(sdlPlatformForUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe(
        "Mac OS X"
    );
    expect(sdlPlatformForUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("Windows");
    expect(sdlPlatformForUserAgent("Mozilla/5.0 (X11; Linux x86_64)")).toBe("Linux");
});

it("parses the SNES replica line into a raw button layout", () => {
    const layouts = parseGameControllerDb(REPLICA_LINE, "Mac OS X");
    const layout = layouts.get("081f:e401");
    // SDL a=south, b=east, x=west, y=north; our fields are a=south, b=east,
    // y=west, x=north.
    expect(layout).toEqual({ a: 2, b: 1, y: 3, x: 0, select: 8, start: 9, l: -1, r: -1 });
});

it("skips lines for other platforms and comments", () => {
    const text = [
        "# a comment",
        "030000001f08000001e4000010010000,Super Famicom Controller,a:b2,b:b1,x:b3,y:b0,back:b8,start:b9,platform:Linux,",
        REPLICA_LINE,
    ].join("\n");
    const layouts = parseGameControllerDb(text, "Linux");
    expect(layouts.size).toBe(1);
    expect(layouts.get("081f:e401")?.a).toBe(2);
});

it("later lines override earlier lines for the same pad", () => {
    const text = [
        "030000001f08000001e4000006010000,First,a:b0,b:b1,platform:Mac OS X,",
        "030000001f08000001e4000006010000,Second,a:b2,b:b1,platform:Mac OS X,",
    ].join("\n");
    const layouts = parseGameControllerDb(text, "Mac OS X");
    expect(layouts.get("081f:e401")?.a).toBe(2);
});

it("tolerates SDL3-format lines with a crc field", () => {
    const text =
        "030000001f08000001e4000006010000,NES,platform:Mac OS X,crc:2a4d,a:b1,b:b0,back:b8,start:b9,";
    const layouts = parseGameControllerDb(text, "Mac OS X");
    expect(layouts.get("081f:e401")?.a).toBe(1);
});

it("marks face buttons mapped to axes or hats as absent", () => {
    const text =
        "030000001f08000001e4000006010000,Odd pad,a:b2,b:b1,x:+a3,y:h0.1,back:b8,start:b9,platform:Mac OS X,";
    const layouts = parseGameControllerDb(text, "Mac OS X");
    const layout = layouts.get("081f:e401");
    expect(layout?.a).toBe(2);
    expect(layout?.y).toBe(-1); // SDL x (west) is axis-valued
    expect(layout?.x).toBe(-1); // SDL y (north) is hat-valued
});

it("skips lines whose south or east buttons are not plain buttons", () => {
    const text = "030000001f08000001e4000006010000,Broken,a:+a1,b:b1,platform:Mac OS X,";
    const layouts = parseGameControllerDb(text, "Mac OS X");
    expect(layouts.size).toBe(0);
});
