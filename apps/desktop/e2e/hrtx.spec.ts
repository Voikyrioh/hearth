import { expect, type Page, test } from "@playwright/test";

// Restes courts de la revue UX du 2026-10-08 (T64) : raison d'un bouton grisé écrite sous le bouton (HRT-40, C42),
// fenêtre de retrait d'un poste, filtre tapé non appliqué (HRT-43), bornes de largeur (HRT-42). Pont SIMULÉ.

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function shoot(page: Page, name: string, size: { width: number; height: number }) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${size.width}x${size.height}.png` });
}

for (const size of SIZES) {
  test(`réglages à ${size.width}×${size.height} : la raison d'un bouton grisé est écrite sous le bouton`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/settings");
    const salon = page.locator('[data-agent-card][data-server="salon"]');
    await expect(salon).toBeVisible();
    // Lecture seule : « Mettre à jour l'agent » grisé, la raison sans survol.
    const button = salon.locator("[data-agent-update-button]");
    await expect(button).toHaveAttribute("aria-disabled", "true");
    await expect(salon.locator("[data-button-reason]")).toContainText(
      "Seul un administrateur peut mettre à jour l'agent",
    );
    await shoot(page, "hrtx-raison-sous-bouton", size);
  });
}

for (const size of SIZES) {
  test(`sécurité à ${size.width}×${size.height} : la fenêtre de retrait d'un poste va à l'essentiel`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/security");
    const remove = page
      .locator('button[aria-label^="Retirer"]:not([aria-disabled="true"])')
      .first();
    await expect(remove).toBeVisible();
    await remove.click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog).toBeVisible();
    // Un seul paragraphe visible avant le champ ; le conseil est replié.
    const visibleParagraphs = await dialog
      .locator("p:visible")
      .evaluateAll((items) => items.map((item) => item.textContent));
    expect(visibleParagraphs).toEqual([
      "Es-tu sûr de vouloir retirer ce poste ? Tu ne pourras plus te connecter depuis ce poste sans avoir un mot de passe valide.",
    ]);
    await shoot(page, "hrtx-retrait-poste", size);
  });
}
