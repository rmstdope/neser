import { readFileSync } from "node:fs";
import path from "node:path";
import { test, expect } from "@playwright/test";
import { loadRomFromFileInput, startFromBundledRom, waitForIdleState, waitForPausedState, waitForRunningState } from "../helpers/lifecycle.helpers";

const ESSENTIAL_CONTROL_SELECTORS = ["#library", "#library-choose", "#library-open", "#settings-open"];

test.describe("web app shell", () => {
    test("renders essential controls", async ({ page }) => {
        await page.goto("/");

        for (const selector of ESSENTIAL_CONTROL_SELECTORS) {
            await expect(page.locator(selector)).toBeVisible();
        }
        // Before a game runs the library shows, not the picture and its control bar.
        await expect(page.locator("#emulation-controls")).toBeHidden();
    });

    test("starts a picked game at once, names it in the top bar, and Eject returns to the library with it ready to start again", async ({ page }) => {
        await page.goto("/");
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await expect(page.locator("#library")).toBeHidden();
        await expect(page.locator("#pause")).toBeVisible();
        await expect(page.locator("#game-title")).toBeVisible();
        await expect(page.locator("#game-system")).not.toBeEmpty();

        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expect(page.locator("#library")).toBeVisible();
        await expect(page.locator("#game-title")).toBeHidden();
        await expect(page.locator("#library-again-name")).toHaveText("cpu");
        await page.locator("#start").click();
        await waitForRunningState(page);
    });

    test("opens the library over a running game, which waits paused, and goes back with Back to the game or Esc", async ({ page }) => {
        await startFromBundledRom(page);

        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await waitForPausedState(page);
        await page.locator("#library-back").click();
        await expect(page.locator("#library")).toBeHidden();
        await waitForRunningState(page);

        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await page.keyboard.press("Escape");
        await expect(page.locator("#library")).toBeHidden();
        await waitForRunningState(page);
    });

    test("searches the bundled list and filters it by system", async ({ page }) => {
        await page.route("**/roms/roms.json", (route) =>
            route.fulfill({ json: { roms: ["nes/one.nes", "nes/two.nes", "gb/three.gb", "gbc/four.gbc"] } })
        );
        await page.goto("/");
        const items = page.locator(".library-item .library-item-name");
        await expect(items).toHaveText(["three", "four", "one", "two"]);
        await expect(page.locator("#library-count")).toHaveText("4");

        await page.locator("#library-search").fill("TW");
        await expect(items).toHaveText(["two"]);
        await page.locator("#library-search").fill("");
        await page.locator('#library-systems [data-system="GB"]').click();
        await expect(items).toHaveText(["three", "four"]);
        await page.locator("#library-search").fill("zzz");
        await expect(page.locator(".library-empty")).toHaveText("No test ROMs match “zzz”.");
    });

    test("loads a game file dropped anywhere on the page", async ({ page }) => {
        await page.goto("/");
        const bytes = [...readFileSync(path.join(process.cwd(), "roms", "nes", "automated_tests", "blargg_nes_cpu_test5", "cpu.nes"))];
        const dataTransfer = await page.evaluateHandle((data) => {
            const transfer = new DataTransfer();
            transfer.items.add(new File([new Uint8Array(data)], "dropped.nes"));
            return transfer;
        }, bytes);
        await page.dispatchEvent("body", "dragover", { dataTransfer });
        await expect(page.locator("#library-drop")).toHaveClass(/library-hero-drag/);
        await page.dispatchEvent("body", "drop", { dataTransfer });
        await waitForRunningState(page);
        await expect(page.locator("#game-name")).toHaveText("dropped");
    });

    test("shows a refused file's message as one line in the theme's error colour, and no REC badge while idle", async ({ page }) => {
        await page.goto("/");
        await expect(page.locator("#rec-overlay")).toBeHidden();

        await page.locator("#rom").setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("not a game") });

        const alert = page.locator("#library-error");
        await expect(alert).toBeVisible();
        await expect(alert.locator("#library-error-title")).toHaveText("Couldn’t open “notes.txt”");
        await expect(alert.locator("#library-error-choose")).toHaveText("Choose another…");
        const status = page.locator("#status");
        await expect(status).toHaveText(".txt isn’t a game file NESER knows. Try a .nes, .gb, .gbc, .cgb, .gba, .sfc or .smc file.");
        await expect(status).toBeVisible();
        const box = (await status.boundingBox())!;
        expect(box.height).toBeLessThan(40);
        expect(box.width).toBeGreaterThan(box.height * 3);
        await expect(status).toHaveCSS("color", "rgb(242, 113, 138)");
        // Readable: the error colours the words, never a box behind them.
        await expect(status).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
    });

    test("accepts every supported ROM file in the file picker once start-up installs the wasm table", async ({ page }) => {
        await page.goto("/");

        await expect(page.locator("#rom")).toHaveAttribute(
            "accept",
            ".nes,.gb,.gbc,.cgb,.gba,.sfc,.smc,application/octet-stream"
        );
    });
});

test.describe("a ROM chosen before the wasm module has loaded", () => {
    test("starts once the module arrives instead of being rejected as unsupported", async ({ page }) => {
        let releaseWasm: () => void = () => {};
        const wasmHeld = new Promise<void>((resolve) => {
            releaseWasm = resolve;
        });
        await page.route("**/*.wasm", async (route) => {
            await wasmHeld;
            await route.continue();
        });
        await page.goto("/");
        await expect(page.locator("#library-choose")).toBeVisible();

        await loadRomFromFileInput(page);
        releaseWasm();

        await waitForRunningState(page);
        await expect(page.locator("#library-error")).toBeHidden();
    });
});

test.describe("the library over a running game", () => {
    const servedList = async (page: import("@playwright/test").Page, roms: string[]) =>
        page.route("**/roms/roms.json", (route) => route.fulfill({ json: { roms } }));

    test("Given a pick that fails to load, then the game under the library can still be returned to with Back or Esc", async ({ page }) => {
        await servedList(page, ["test/cpu.nes", "test/missing.nes"]);
        await page.route("**/roms/test/missing.nes", (route) => route.fulfill({ status: 404, body: "" }));
        await page.route("**/roms/test/cpu.nes", (route) =>
            route.fulfill({ body: readFileSync(path.join(process.cwd(), "roms", "nes", "automated_tests", "blargg_nes_cpu_test5", "cpu.nes")) })
        );
        await page.goto("/");
        await page.locator('.library-item[data-path="test/cpu.nes"]').click();
        await waitForRunningState(page);

        await page.locator("#library-open").click();
        await page.locator('.library-item[data-path="test/missing.nes"]').click();
        await expect(page.locator("#status")).toHaveText("Failed to load bundled ROM");
        await page.locator("#library-back").click();
        await expect(page.locator("#library")).toBeHidden();
        await waitForRunningState(page);

        await page.locator("#library-open").click();
        await page.locator('.library-item[data-path="test/missing.nes"]').click();
        await expect(page.locator("#status")).toHaveText("Failed to load bundled ROM");
        await page.keyboard.press("Escape");
        await expect(page.locator("#library")).toBeHidden();
    });

    test("Given a game the player paused, when the library opens and Back is pressed, then it is still paused", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#pause").click();
        await waitForPausedState(page);
        await page.locator("#library-open").click();
        await page.locator("#library-back").click();
        await waitForPausedState(page);
    });

    test("Given a game running under the library, when another is picked, then it replaces the first", async ({ page }) => {
        await servedList(page, ["test/cpu.nes", "test/other.nes"]);
        const bytes = readFileSync(path.join(process.cwd(), "roms", "nes", "automated_tests", "blargg_nes_cpu_test5", "cpu.nes"));
        await page.route("**/roms/test/*.nes", (route) => route.fulfill({ body: bytes }));
        await page.goto("/");
        await page.locator('.library-item[data-path="test/cpu.nes"]').click();
        await waitForRunningState(page);
        await page.locator("#library-open").click();
        await page.locator('.library-item[data-path="test/other.nes"]').click();
        await waitForRunningState(page);
        await expect(page.locator("#library")).toBeHidden();
        await expect(page.locator("#library-back")).toBeHidden();
        await expect(page.locator("#game-name")).toHaveText("other");
    });
});

test.describe("the library's keys and its news", () => {
    test("Given the library after Eject, then the game's shortcuts wait, O chooses a file and / goes to the search", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#stop").click();
        await waitForIdleState(page);
        await expect(page.locator("#library")).toBeVisible();

        const filterBefore = await page.locator("#filter-toggle").textContent();
        await page.keyboard.press("F4");
        await page.keyboard.press("Control+KeyF");
        await expect(page.locator("#filter-toggle")).toHaveText(filterBefore!);
        expect(await page.evaluate(() => document.fullscreenElement)).toBeNull();

        await page.keyboard.press("Slash");
        await expect(page.locator("#library-search")).toBeFocused();
        await page.locator("#library-search").blur();
        const chooser = page.waitForEvent("filechooser");
        await page.keyboard.press("KeyO");
        await chooser;
    });

    test("Given reduced motion, then the news is still words, in its own row above the heading", async ({ page }) => {
        await page.emulateMedia({ reducedMotion: "reduce" });
        await page.setViewportSize({ width: 390, height: 844 });
        await page.goto("/");
        const news = (await page.locator("#library-news").boundingBox())!;
        const heading = (await page.locator("#library-title").boundingBox())!;
        expect(news.y + news.height).toBeLessThanOrEqual(heading.y);
        await expect(page.locator("#library-news span").first()).toHaveCSS("display", "inline");
        // Every word shows: nothing runs past the hero's edge.
        const clipped = await page.locator("#library-drop").evaluate((hero) => {
            const edge = hero.getBoundingClientRect().right;
            return [...hero.querySelectorAll("#library-news span")].some((span) => span.getBoundingClientRect().right > edge + 1);
        });
        expect(clipped).toBe(false);
        await expect(page.locator("#library-news")).toHaveAttribute("role", "marquee");
    });
});

test.describe("errors where you are", () => {
    test("Given a game whose picture can no longer be drawn, then it stops on its picture with Reset and Back to library", async ({ page }) => {
        await startFromBundledRom(page);
        // Drawing a frame now fails, as a broken graphics driver would make it.
        await page.evaluate(() => {
            WebGLRenderingContext.prototype.drawArrays = () => {
                throw new Error("the picture cannot be drawn");
            };
        });
        const overlay = page.locator("#game-error");
        await expect(overlay).toBeVisible({ timeout: 15_000 });
        await expect(overlay.locator(".game-error-title")).toHaveText("The game stopped");
        await expect(overlay.locator("#game-error-text")).toContainText("the picture cannot be drawn");
        await expect(page.locator("#library")).toBeHidden();

        // The way on is under the player's finger.
        await expect(overlay.locator("#game-error-reset")).toBeFocused();

        await overlay.locator("#game-error-library").click();
        await expect(page.locator("#library")).toBeVisible();
        await expect(overlay).toBeHidden();
    });

    test("Given a stopped game, when Reset is pressed, then it starts again on a fresh console with focus on the picture", async ({ page }) => {
        await startFromBundledRom(page);
        await page.evaluate(() => {
            const draw = WebGLRenderingContext.prototype.drawArrays;
            (window as unknown as { restoreDraw: () => void }).restoreDraw = () => {
                WebGLRenderingContext.prototype.drawArrays = draw;
            };
            WebGLRenderingContext.prototype.drawArrays = () => {
                throw new Error("the picture cannot be drawn");
            };
        });
        const overlay = page.locator("#game-error");
        await expect(overlay).toBeVisible({ timeout: 15_000 });
        await page.evaluate(() => (window as unknown as { restoreDraw: () => void }).restoreDraw());
        await overlay.locator("#game-error-reset").click();
        await expect(overlay).toBeHidden();
        await waitForRunningState(page);
        await expect(page.locator("#screen")).toBeFocused();
    });

    test("Given a stopped game, when the top bar's Library is pressed, then the library shows, as Back to library would", async ({ page }) => {
        await startFromBundledRom(page);
        await page.evaluate(() => {
            WebGLRenderingContext.prototype.drawArrays = () => {
                throw new Error("the picture cannot be drawn");
            };
        });
        await expect(page.locator("#game-error")).toBeVisible({ timeout: 15_000 });
        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await expect(page.locator("#game-error")).toBeHidden();
    });

    test("Given a pick that fails over a paused game, when the player goes back to it and opens the library again, then the old error is gone", async ({ page }) => {
        await startFromBundledRom(page);
        await page.locator("#pause").click();
        await page.locator("#library-open").click();
        // The game file can no longer be fetched, so the pick fails before the paused game is left.
        await page.unroute("**/roms/**");
        await page.route("**/roms/*/**", (route) => route.fulfill({ status: 404, body: "" }));
        await page.locator(".library-item").first().click();
        await expect(page.locator("#library-error")).toBeVisible();
        await page.locator("#library-back").click();
        await page.locator("#library-open").click();
        await expect(page.locator("#library")).toBeVisible();
        await expect(page.locator("#library-error")).toBeHidden();
    });

    test("Given a DSP game whose firmware dialog is cancelled, then the library says so in a brief note, not as a file it couldn't open", async ({ page }) => {
        await page.goto("/");
        // The smallest LoROM with a DSP-1 cartridge header, as the core's own tests build it.
        const rom = Buffer.alloc(0x10000);
        rom.write("SNES TEST ROM        ", 0x7fc0, "latin1");
        rom[0x7fc0 + 0x15] = 0x20;
        rom[0x7fc0 + 0x16] = 0x03;
        rom[0x7fc0 + 0x17] = 0x07;
        rom[0x7fc0 + 0x1c] = 0x34;
        rom[0x7fc0 + 0x1d] = 0x12;
        rom[0x7fc0 + 0x1e] = 0xcb;
        rom[0x7fc0 + 0x1f] = 0xed;
        rom[0x7fc0 + 0x3d] = 0x80;
        rom[0] = 0xea;
        await page.locator("#rom").setInputFiles({ name: "Pilotwings.sfc", mimeType: "application/octet-stream", buffer: rom });
        await page.locator("#snes-firmware-cancel").click();
        await expect(page.locator("#library")).toBeVisible();
        await expect(page.locator(".neser-toast", { hasText: /^Not started: .* needs the DSP-1 firmware$/ })).toBeVisible();
        await expect(page.locator("#library-error")).toBeHidden();
    });

    test("Given a browser that cannot draw games, then the library says so of the browser and offers no other file", async ({ page }) => {
        await page.addInitScript(() => {
            const getContext = HTMLCanvasElement.prototype.getContext;
            HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...rest: unknown[]) {
                if (kind.startsWith("webgl")) return null;
                return (getContext as (...a: unknown[]) => unknown).call(this, kind, ...rest);
            } as typeof getContext;
        });
        await page.goto("/");
        const alert = page.locator("#library-error");
        await expect(alert).toBeVisible();
        await expect(alert.locator("#library-error-title")).toHaveText("This browser can’t show games");
        await expect(page.locator("#status")).toHaveText("WebGL is turned off or not supported here. Try another browser, or turn on hardware acceleration.");
        await expect(alert.locator("#library-error-choose")).toBeHidden();
    });

    test("Given the library's error, when a good game is then chosen, then the error goes", async ({ page }) => {
        await page.goto("/");
        await page.locator("#rom").setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("x") });
        await expect(page.locator("#library-error")).toBeVisible();
        await loadRomFromFileInput(page);
        await waitForRunningState(page);
        await page.locator("#stop").click();
        await expect(page.locator("#library-error")).toBeHidden();
    });
});
