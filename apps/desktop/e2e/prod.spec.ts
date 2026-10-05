import { expect, test } from "@playwright/test";

// Le build de production (servi tel quel par `vite preview`), pont vide : l'application
// démarre sur l'accueil, sans erreur dans la console, sans aucun reste de simulation.
test("build livré : accueil affiché, aucune erreur, aucune simulation", async ({ page }) => {
  const problems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") {
      problems.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
  page.on("requestfailed", (request) => problems.push(`requestfailed: ${request.url()}`));

  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await expect(page.getByText("Ajoute ton premier serveur pour commencer.")).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Serveurs" })).toBeVisible();
  // Un chemin de serveur ramène à l'accueil (aucun serveur enregistré).
  await page.goto("/#/servers/forge/dashboard");
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();

  expect(await page.evaluate(() => "__hearthSim" in window)).toBe(false);
  await expect(page.getByText("Simulation du lien")).toHaveCount(0);
  await expect(page.locator("a[data-server]")).toHaveCount(0);
  expect(problems).toEqual([]);
});
