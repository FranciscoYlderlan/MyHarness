/// <reference types="vitest/config" />
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const host = process.env.TAURI_DEV_HOST;

/**
 * Modo simulado (`pnpm dev:simulado` = `vite --mode simulado`): põe o núcleo simulado
 * antes do `src/main.tsx`. Só no servidor de desenvolvimento; o build nunca o inclui.
 */
function modoSimulado(): Plugin {
  return {
    name: "myharness-modo-simulado",
    apply: (_config, { command, mode }) => command === "serve" && mode === "simulado",
    transformIndexHtml: () => [
      {
        tag: "script",
        attrs: { type: "module", src: "/src/dev/modo-simulado.ts" },
        injectTo: "head",
      },
    ],
  };
}

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react(), tailwindcss(), modoSimulado()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2022",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
