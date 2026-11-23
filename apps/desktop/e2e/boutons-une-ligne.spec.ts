import { expect, test } from "@playwright/test";
import { oneLineButtons } from "./oneLine";

// HRT-249 : le texte d'un bouton ne passe jamais à la ligne ; quand la place manque, c'est la barre qui passe à la ligne.
// Mesuré à la largeur minimale de la fenêtre (1 100 px) et à 1 280 px, sur les écrans qui portent des barres d'actions.
// Pour chaque bouton visible : `scrollWidth <= clientWidth` (aucune troncature) et une seule ligne de texte.

const SCREENS = [
  { name: "Catalogue", hash: "#/servers/forge/catalogue" },
  { name: "Fiche", hash: "#/servers/forge/catalogue/terraria" },
  { name: "Installation", hash: "#/servers/forge/catalogue/terraria/installer" },
  { name: "Déploiement libre", hash: "#/servers/forge/services/deployer" },
  { name: "Services", hash: "#/servers/forge/services" },
  { name: "Assistant d'ajout", hash: "#/servers/new" },
  { name: "Réglages", hash: "#/settings" },
] as const;

const WIDTHS = [1100, 1280] as const;

for (const width of WIDTHS) {
  for (const screen of SCREENS) {
    test(`aucun bouton sur deux lignes : ${screen.name} à ${width} px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });
      await page.goto(`/?nodev${screen.hash}`);
      await page.waitForLoadState("networkidle");
      await page.waitForTimeout(400);
      const result = await oneLineButtons(page);
      expect(result.seen, "l'écran affiche au moins un bouton").toBeGreaterThan(0);
      expect(result.failures).toEqual([]);
    });
  }
}
