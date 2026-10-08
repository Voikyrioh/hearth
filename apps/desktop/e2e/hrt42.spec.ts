import { expect, type Page, test } from "@playwright/test";

// HRT-42 (partie sans risque, conception `2026-10-08-design-grand-ecran.md`, décisions 3 à 6) : bornes de largeur du
// contenu, Réglages en deux colonnes égales, Sécurité en deux colonnes à partir de 1 500 px de page. Pas d'échelle
// automatique. À 1100, 1280 et 1366 rien ne change (valeurs d'avant mesurées). Pont SIMULÉ.

const SMALL = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
] as const;
const WIDE = [
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

const box = async (page: Page, selector: string) =>
  page
    .locator(selector)
    .first()
    .evaluate((el) => {
      const rect = el.getBoundingClientRect();
      return { x: rect.x, w: rect.width, h: rect.height };
    });

async function shoot(page: Page, name: string, size: { width: number; height: number }) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${size.width}x${size.height}.png` });
}

for (const size of [...SMALL, ...WIDE]) {
  test(`pages de travail à ${size.width}×${size.height} : contenu borné à 1 760 px, comptes à 1 400 px`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    for (const name of ["dashboard", "audit", "accounts"]) {
      await page.goto(`/?nodev#/servers/forge/${name}`);
      await expect(page.locator(".layout__content")).toBeVisible();
      const main = await box(page, ".layout__main");
      expect(main.w, `${name} : largeur du contenu`).toBeLessThanOrEqual(1760 + 48 + 1);
      if (size.width <= 1920) {
        // Jusqu'à 1920 la page est plus étroite que la borne : rien ne change (toute la place disponible).
        expect(main.w, `${name} : toute la place`).toBeGreaterThan(size.width - 272 - 2);
      }
    }
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator(".table-wrap")).toBeVisible();
    const table = await box(page, ".table-wrap");
    expect(table.w, "tableau des comptes").toBeLessThanOrEqual(1400 + 1);
    if (size.width <= 1366) expect(table.w).toBeGreaterThan(size.width - 272 - 48 - 2);
    await shoot(page, "hrtx-largeur-comptes", size);
  });

  test(`réglages à ${size.width}×${size.height}`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/settings");
    await expect(page.locator(".settings__column").first()).toBeVisible();
    const columns = await page
      .locator(".settings__column")
      .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().width));
    expect(columns).toHaveLength(2);
    const [left = 0, right = 0] = columns;
    if (size.width <= 1366) {
      // Avant : colonne de gauche à 640 px, celle de droite prend le reste. Rien ne change.
      expect(left).toBeCloseTo(640, 0);
      expect(right).toBeLessThan(left);
    } else {
      expect(Math.abs(left - right), "colonnes égales").toBeLessThanOrEqual(1);
      expect(left, "colonne bornée").toBeLessThanOrEqual(600 + 1);
    }
    await shoot(page, "hrtx-reglages", size);
  });

  test(`sécurité à ${size.width}×${size.height}`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/security");
    const attack = page.locator("[data-attack-mode-toggle]").first();
    await expect(attack).toBeVisible();
    const panel = await box(page, ".security");
    await expect(page.locator("[data-device]").first()).toBeVisible();
    const xs = await page.evaluate(() => {
      const attackCard = document.querySelector("[data-attack-mode-toggle]")?.closest("section");
      const devicesCard = document.querySelector("[data-device]")?.closest("section");
      return [
        attackCard?.getBoundingClientRect().x ?? 0,
        devicesCard?.getBoundingClientRect().x ?? 0,
        attackCard?.getBoundingClientRect().width ?? 0,
        devicesCard?.getBoundingClientRect().width ?? 0,
      ];
    });
    const [attackX = 0, devicesX = 0, attackW = 0, devicesW = 0] = xs;
    if (size.width <= 1366) {
      expect(panel.w, "une colonne").toBeLessThanOrEqual(880 + 1);
      expect(Math.abs(devicesX - attackX), "empilées").toBeLessThanOrEqual(1);
    } else {
      expect(panel.w, "deux colonnes").toBeLessThanOrEqual(1520 + 1);
      expect(devicesX - attackX, "côte à côte").toBeGreaterThan(500);
      expect(Math.abs(devicesW - attackW), "colonnes égales").toBeLessThanOrEqual(1);
    }
    await shoot(page, "hrtx-securite", size);
  });
}

// Seuils en LARGEUR DE PAGE (requête de conteneur à 1 500 px), pas en largeur de fenêtre.
test("réglages : deux colonnes égales dès 1 500 px de page (fenêtre de 1 612 px), une seule colonne de 640 px avant", async ({
  page,
}) => {
  const widthsAt = async (width: number) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/?nodev#/settings");
    await expect(page.locator(".settings__column").first()).toBeVisible();
    return page
      .locator(".settings__column")
      .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().width));
  };
  const [narrowLeft = 0] = await widthsAt(1590);
  expect(narrowLeft).toBeCloseTo(640, 0);
  const [left = 0, right = 0] = await widthsAt(1640);
  expect(Math.abs(left - right)).toBeLessThanOrEqual(1);
});

test("sécurité : deux colonnes dès 1 500 px de page (fenêtre de 1 820 px)", async ({ page }) => {
  const sideBySide = async (width: number) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/?nodev#/servers/forge/security");
    await expect(page.locator("[data-device]").first()).toBeVisible();
    return page.evaluate(() => {
      const a = document.querySelector("[data-attack-mode-toggle]")?.closest("section");
      const d = document.querySelector("[data-device]")?.closest("section");
      return (
        Math.abs((d?.getBoundingClientRect().x ?? 0) - (a?.getBoundingClientRect().x ?? 0)) > 300
      );
    });
  };
  expect(await sideBySide(1800)).toBe(false);
  expect(await sideBySide(1840)).toBe(true);
});

for (const size of [...SMALL, ...WIDE]) {
  test(`sécurité à ${size.width}×${size.height} : la note « Effacement en attente » reste en dernier`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.evaluate(() =>
      (
        window as unknown as {
          __hearthSim: { security: { setErasurePending(id: string, pending: boolean): void } };
        }
      ).__hearthSim.security.setErasurePending("forge", true),
    );
    await page.evaluate(() => {
      window.location.hash = "#/servers/forge/security";
    });
    await expect(page.locator("[data-device]").first()).toBeVisible();
    const note = page.locator("[data-erasure-pending]");
    await expect(note).toBeVisible();
    const geometry = await page.evaluate(() => {
      const rect = (el: Element | null | undefined) => el?.getBoundingClientRect();
      const reauth = rect(document.querySelector("[data-reauth-setting]")?.closest("section"));
      const note = rect(document.querySelector("[data-erasure-pending]"));
      const attack = rect(document.querySelector("[data-attack-mode-toggle]")?.closest("section"));
      return {
        noteTop: note?.top ?? 0,
        reauthBottom: reauth?.bottom ?? 0,
        noteX: note?.x ?? 0,
        reauthX: reauth?.x ?? 0,
        attackBottom: attack?.bottom ?? 0,
      };
    });
    // Comme avant le lot : après la confirmation du mot de passe, à toutes les largeurs (en deux colonnes, sous elle).
    expect(geometry.noteTop, "note sous la confirmation").toBeGreaterThanOrEqual(
      geometry.reauthBottom - 1,
    );
    expect(Math.abs(geometry.noteX - geometry.reauthX), "même colonne").toBeLessThanOrEqual(1);
  });
}
