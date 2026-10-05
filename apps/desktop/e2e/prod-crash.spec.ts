import { expect, test } from "@playwright/test";

// Build de PRODUCTION de test (`vite build --mode e2e`, servi par `vite preview`) : il ajoute une
// page de diagnostic dont le rendu plante une fois. En production Vue donne à la frontière une
// adresse de référence au lieu d'un libellé : le repli doit s'afficher quand même.
test("production : une page dont le rendu plante affiche le repli, la coquille reste, « Réessayer » la remonte", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/#/diagnostic-crash");
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("Cette page a rencontré un problème");
  // La coquille est toujours là et utilisable.
  const rail = page.getByRole("navigation", { name: "Serveurs" });
  await expect(rail).toBeVisible();
  await expect(rail.getByRole("link", { name: "Réglages" })).toBeVisible();
  // L'erreur est signalée discrètement (notification), sans bloquer.
  await expect(page.locator(".toast")).toContainText("problème est survenu");

  await alert.getByRole("button", { name: "Réessayer" }).click();
  await expect(page.getByText("Diagnostic rétabli")).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(errors).toEqual([]);
});
