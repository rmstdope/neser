/// <reference types="vitest/config" />
import { defineConfig } from "vite";

export default defineConfig({
  root: "web",
  publicDir: false,
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
