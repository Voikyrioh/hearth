import { defineConfig, devices } from "@playwright/test";

// Les scénarios lancent l'interface servie par Vite dans Chromium, avec le pont de liaison
// SIMULÉ (aucune coquille Tauri, aucun réseau). Port : celui de `vite.config.ts`.
const PORT = 1420;

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  fullyParallel: false,
  workers: 1,
  reporter: "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    ...devices["Desktop Chrome"],
    viewport: { width: 1366, height: 800 },
    colorScheme: "dark",
  },
  webServer: {
    command: "npm run dev",
    url: `http://localhost:${PORT}`,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
