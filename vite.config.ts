/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { existsSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { findRomFiles } from "./web/src/rom/rom_manifest";
import { wasmWatchPlugin } from "./web/src/dev/wasm_watch";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** The dev and preview servers' port: `NESER_WEB_PORT`, as `pnpm run serve` reads it, or 8000. */
const port = Number(process.env.NESER_WEB_PORT ?? 8000);

/** Vite plugin that generates web/roms/roms.json so the ROM picker works. */
function romManifestPlugin(): Plugin {
  const romsDir = join(__dirname, "web", "roms");
  const manifestPath = join(romsDir, "roms.json");

  function generate() {
    const roms = existsSync(romsDir) ? findRomFiles(romsDir) : [];
    writeFileSync(manifestPath, JSON.stringify({ roms }, null, 2) + "\n");
    console.log(`[rom-manifest] wrote ${roms.length} entries to web/roms/roms.json`);
  }

  return {
    name: "rom-manifest",
    buildStart() { generate(); },
    configureServer() { generate(); },
  };
}

export default defineConfig({
  root: "web",
  publicDir: false,
  plugins: [tailwindcss(), romManifestPlugin(), wasmWatchPlugin()],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    port,
    strictPort: true,
  },
  preview: {
    port,
    strictPort: true,
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
