import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev server port (see src-tauri/tauri.conf.json build.devUrl).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Help imports ../docs/USER_GUIDE.md?raw from outside app/: let the dev
    // server serve files from the repo root (build-time bundling is unaffected).
    fs: { allow: [fileURLToPath(new URL("..", import.meta.url))] },
  },
  build: {
    target: "es2022",
    outDir: "dist",
  },
});
