import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const tauriDevHost = process.env.TAURI_DEV_HOST;
const isStandalone = !tauriDevHost;

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  root: isStandalone ? "." : ".",
  base: "./",
  build: {
    outDir: "dist",
    target: "esnext",
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: isStandalone ? 5173 : 1420,
    strictPort: true,
    host: tauriDevHost || false,
    hmr: tauriDevHost
      ? {
          protocol: "ws",
          host: tauriDevHost,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
