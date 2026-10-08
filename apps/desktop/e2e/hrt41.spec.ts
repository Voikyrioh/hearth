import { expect, type Page, test } from "@playwright/test";

// HRT-41 : lire une courbe (durée, échelle, valeur au survol, légende, équivalent texte) et jauges neutres
// tant que tout va bien. Pont SIMULÉ, aux tailles 1100×680, 1280×800, 1366×800, 1920×1080, 2560×1440.

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function open(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".chart-box").first()).toBeVisible();
}

for (const size of SIZES) {
  test(`tableau de bord à ${size.width}×${size.height} : chaque courbe dit sa durée et son échelle`, async ({
    page,
  }) => {
    await open(page, size);
    const spans = await page.locator(".series .series__span").allInnerTexts();
    const scales = await page.locator(".series .series__scale").allInnerTexts();
    const charts = await page.locator(".series").count();
    expect(charts).toBeGreaterThanOrEqual(5);
    expect(spans).toHaveLength(charts);
    for (const span of spans)
      expect(span).toMatch(/Dernière minute|5 dernières minutes|Dernière heure/);
    expect(scales).toHaveLength(charts);
    for (const scale of scales) expect(scale).toMatch(/^0 à .*\d/);
    await page.screenshot({
      path: `e2e/screenshots/hrt41-courbes-${size.width}x${size.height}.png`,
    });
  });

  test(`tableau de bord à ${size.width}×${size.height} : le survol donne la valeur et l'heure, un équivalent texte existe`, async ({
    page,
  }) => {
    await open(page, size);
    const chart = page.locator(".chart-box").first();
    await chart.hover();
    const tip = page.locator(".chart__tip").first();
    await expect(tip).toBeVisible();
    await expect(tip).toContainText(/\d{2}:\d{2}:\d{2}/);
    await expect(tip).toContainText(/\d/);
    const label = (await chart.locator("svg").getAttribute("aria-label")) ?? "";
    expect(label).toMatch(/dernière valeur .*minimum .*maximum/i);
    await page.screenshot({
      path: `e2e/screenshots/hrt41-survol-${size.width}x${size.height}.png`,
    });
  });

  test(`tableau de bord à ${size.width}×${size.height} : légende des disques et des températures, jauges neutres`, async ({
    page,
  }) => {
    await open(page, size);
    await expect(
      page.locator(".series__legend", { hasText: "disque le plus plein" }),
    ).toBeVisible();
    await expect(
      page.locator(".series__legend", { hasText: "sonde la plus chaude" }),
    ).toBeVisible();
    // Une jauge à l'état normal n'a ni dégradé ni rose d'alerte.
    const strokes = await page
      .locator(".arc__value--normal")
      .evaluateAll((arcs) => arcs.map((arc) => getComputedStyle(arc).stroke));
    expect(strokes.length).toBeGreaterThan(0);
    for (const stroke of strokes) {
      expect(stroke, "trait d'une jauge normale").not.toMatch(/url|255, 79, 122/);
    }
  });
}
