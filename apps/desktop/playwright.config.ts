import { defineConfig, devices } from "@playwright/test";

// Deux cibles :
// - `dev` : l'interface servie par Vite avec le pont de liaison SIMULÉ (scénarios de la coquille) ;
// - `prod` : le build livré (`dist/`, servi en statique par `vite preview`) avec le pont vide :
//   ce qui est expédié démarre, affiche l'accueil, sans erreur ni code de simulation.
// Aucune coquille Tauri, aucun réseau. Ports : 1420 (celui de `vite.config.ts`) et 4173.
const DEV_PORT = 1420;
const PROD_PORT = 4173;
const PROD_E2E_PORT = 4174;

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  fullyParallel: false,
  workers: 1,
  reporter: "list",
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1366, height: 800 },
    colorScheme: "dark",
  },
  projects: [
    {
      name: "dev",
      testMatch: [
        "shell.spec.ts",
        "connect.spec.ts",
        "dashboard.spec.ts",
        "offline.spec.ts",
        "audit.spec.ts",
        "updates.spec.ts",
        "accounts.spec.ts",
        "agent-update.spec.ts",
      ],
      use: { baseURL: `http://localhost:${DEV_PORT}` },
    },
    {
      name: "prod",
      testMatch: "prod.spec.ts",
      use: { baseURL: `http://localhost:${PROD_PORT}` },
    },
    {
      // Build de production de TEST (`--mode e2e`, dossier `dist-e2e`) : même code que le livré
      // plus une page de diagnostic dont le rendu plante, absente du build livré.
      name: "prod-e2e",
      testMatch: "prod-crash.spec.ts",
      use: { baseURL: `http://localhost:${PROD_E2E_PORT}` },
    },
  ],
  webServer: [
    {
      command: "npm run dev",
      url: `http://localhost:${DEV_PORT}`,
      reuseExistingServer: true,
      timeout: 60_000,
    },
    {
      command: `npm run preview -- --port ${PROD_PORT} --strictPort`,
      url: `http://localhost:${PROD_PORT}`,
      reuseExistingServer: true,
      timeout: 60_000,
    },
    {
      command: `npm run preview -- --outDir dist-e2e --port ${PROD_E2E_PORT} --strictPort`,
      url: `http://localhost:${PROD_E2E_PORT}`,
      reuseExistingServer: true,
      timeout: 60_000,
    },
  ],
});
