/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { readdirSync, realpathSync, statSync, writeFileSync, existsSync } from "fs";
import { dirname, join, relative } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

/** Recursively find all supported ROM files under a directory, following symlinks safely. */
function findRomFiles(
  dir: string,
  base: string,
  visited: Set<string> = new Set(),
  baseReal?: string,
): string[] {
  const results: string[] = [];
  if (!existsSync(dir)) return results;

  let resolvedBase = baseReal;
  try { resolvedBase ??= realpathSync(base); } catch { return results; }

  const isWithinBase = (targetReal: string) => {
    const rel = relative(resolvedBase!, targetReal);
    return rel === "" || (!rel.startsWith("..") && !rel.startsWith("/"));
  };

  let resolvedDir: string;
  try { resolvedDir = realpathSync(dir); } catch { return results; }

  if (!isWithinBase(resolvedDir) || visited.has(resolvedDir)) return results;
  visited.add(resolvedDir);

  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = join(dir, entry.name);
    let stat;
    try { stat = statSync(fullPath); } catch { continue; }
    if (stat.isDirectory()) {
      results.push(...findRomFiles(fullPath, base, visited, resolvedBase));
    } else if (/\.(nes|gb|gbc|cgb|gba)$/i.test(entry.name)) {
      let resolvedFile: string;
      try { resolvedFile = realpathSync(fullPath); } catch { continue; }
      if (isWithinBase(resolvedFile)) {
        results.push(relative(base, fullPath));
      }
    }
  }
  return results;
}

/** Vite plugin that generates web/roms/roms.json so the ROM picker works. */
function romManifestPlugin(): Plugin {
  const romsDir = join(__dirname, "web", "roms");
  const manifestPath = join(romsDir, "roms.json");

  function generate() {
    const roms = findRomFiles(romsDir, romsDir).sort();
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
