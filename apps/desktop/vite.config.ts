import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // No source maps in shipping builds — prevents reconstructing original TSX.
    sourcemap: false,
    minify: "esbuild",
    cssMinify: true,
    target: "esnext",
    reportCompressedSize: false,
    chunkSizeWarningLimit: 2000,
  },
  esbuild: {
    // Strip console / debugger from production UI bundle (keep them in `tauri dev`).
    drop: process.env.TAURI_ENV_DEBUG === "true" ? [] : ["console", "debugger"],
    legalComments: "none",
  },
});
