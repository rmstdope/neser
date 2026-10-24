/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { execSync } from "child_process";
import { existsSync, readFileSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { findRomFiles } from "./web/src/rom/rom_manifest";
import { wasmWatchPlugin } from "./web/src/dev/wasm_watch";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** The dev and preview servers' port: `NESER_WEB_PORT`, as `pnpm run serve` reads it, or 8000. */
// `||`, not `??`: an empty variable means 8000, as `${NESER_WEB_PORT:-8000}` does in `serve`.
const port = Number(process.env.NESER_WEB_PORT || 8000);

/** NESER's version, as Cargo.toml declares it: the version a network game compares. */
function neserVersion(): string {
  const cargo = readFileSync(join(__dirname, "Cargo.toml"), "utf8");
  return /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1] ?? "unknown";
}

/**
 * The commit this page is built from. Two browsers play together only on the same build
 * (nr-ayk5.3), and the version alone does not change between releases.
 */
function neserCommit(): string {
  try {
    return execSync("git rev-parse --short=7 HEAD", { cwd: __dirname, stdio: ["ignore", "pipe", "ignore"] })
      .toString()
      .trim();
  } catch {
    return "unknown";
  }
}

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
  define: {
    __NESER_VERSION__: JSON.stringify(neserVersion()),
    __NESER_COMMIT__: JSON.stringify(neserCommit()),
  },
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
