/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { readdirSync, statSync, writeFileSync, existsSync } from "fs";
import { join, relative } from "path";

/** Recursively find all .nes files under a directory, following symlinks. */
function findNesFiles(dir: string, base: string): string[] {
  const results: string[] = [];
  if (!existsSync(dir)) return results;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = join(dir, entry.name);
    // Follow symlinks: use statSync (follows links) instead of entry.isDirectory
    let stat;
    try { stat = statSync(fullPath); } catch { continue; }
    if (stat.isDirectory()) {
      results.push(...findNesFiles(fullPath, base));
    } else if (entry.name.toLowerCase().endsWith(".nes")) {
      results.push(relative(base, fullPath));
    }
  }
  return results;
}

/** Vite plugin that generates web/roms/roms.json so the ROM picker works. */
function romManifestPlugin(): Plugin {
  const romsDir = join(__dirname, "web", "roms");
  const manifestPath = join(romsDir, "roms.json");

  function generate() {
    const roms = findNesFiles(romsDir, romsDir).sort();
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
  plugins: [tailwindcss(), romManifestPlugin()],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    port: 8000,
    strictPort: true,
  },
  preview: {
    port: 8000,
    strictPort: true,
  },
  test: {
    include: ["src/**/*.test.ts"],
  },
});
