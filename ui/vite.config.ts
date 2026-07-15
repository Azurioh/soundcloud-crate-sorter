import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri serves the built assets; keep the dev server on a fixed port for the webview.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist" },
});
