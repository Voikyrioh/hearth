import { expect, type Page, test } from "@playwright/test";

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

async function oneLineButtons(page: Page): Promise<{ failures: string[]; seen: number }> {
  return page.evaluate(() => {
    const failures: string[] = [];
    let seen = 0;
    for (const button of Array.from(document.querySelectorAll("button"))) {
      const box = button.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      const style = getComputedStyle(button);
      if (style.visibility === "hidden") continue;
      seen += 1;
      const label = (button.textContent ?? "").trim().slice(0, 40);
      if (button.scrollWidth > button.clientWidth + 1) failures.push(`${label} : tronqué`);
      const range = document.createRange();
      range.selectNodeContents(button);
      const lines = new Set(
        Array.from(range.getClientRects()).map((rect) => Math.round(rect.top / 4)),
      );
      if (lines.size > 1) failures.push(`${label} : plusieurs lignes`);
    }
    return { failures, seen };
  });
}

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
