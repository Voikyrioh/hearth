import { fileURLToPath, URL } from "node:url";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

// Port fixe : `tauri.conf.json` (devUrl) et la CSP de développement le connaissent.
const DEV_PORT = 1420;

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  clearScreen: false,
  server: { port: DEV_PORT, strictPort: true, host: "localhost" },
  build: {
    target: "chrome105",
    // Aucune ressource en data: (CSP stricte) : polices et images restent des fichiers.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
    restoreMocks: true,
  },
});
