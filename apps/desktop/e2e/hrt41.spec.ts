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
    const spans = await page.locator(".series .series__span-full").allTextContents();
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
    // Jauge de la mémoire à 22 % (normal) : braise unie ; jamais un dégradé qui finit en rose d'alerte.
    const arc = page.locator("figure.gauge", { hasText: "Mémoire" }).locator(".arc__value").first();
    const strokeOf = () => arc.evaluate((element) => getComputedStyle(element).stroke);
    expect(await strokeOf(), "normal").toBe("rgb(255, 123, 61)");
    // Aux niveaux d'alerte de BR-DASH-003 (valeurs épinglées du pont simulé), les couleurs d'alerte.
    await page.waitForFunction(() =>
      Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
    );
    const pin = (level: string | null) =>
      page.evaluate(
        (l) =>
          (
            window as unknown as {
              __hearthSim: {
                machine: {
                  pin(id: string, key: string, level: string | null): void;
                  tick(id: string): void;
                };
              };
            }
          ).__hearthSim.machine.pin("forge", "mem", l),
        level,
      );
    await pin("attention");
    await expect.poll(strokeOf, { timeout: 4000 }).toBe("rgb(255, 192, 77)");
    await pin("critical");
    await expect.poll(strokeOf, { timeout: 4000 }).toBe("rgb(255, 90, 90)");
  });
}

test("le survol d'une courbe se fait au clavier : flèches, valeur et heure, Échap", async ({
  page,
}) => {
  await open(page, SIZES[2]);
  const chart = page.locator(".chart-box").first();
  await chart.focus();
  await expect(chart).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  const tip = page.locator(".chart__tip").first();
  await expect(tip).toBeVisible();
  await expect(tip).toContainText(/\d{2}:\d{2}:\d{2}/);
  const before = await tip.innerText();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  expect(await tip.innerText()).not.toBe(before);
  expect(await chart.getAttribute("aria-valuetext")).toContain(":");
  await page.keyboard.press("Escape");
  await expect(page.locator(".chart__tip")).toHaveCount(0);
});

test("aucun chiffre en chasse fixe ne contient l'espace fine insécable absente de la police", async ({
  page,
}) => {
  await open(page, SIZES[3]);
  const bad = await page.evaluate(() =>
    Array.from(document.querySelectorAll("body *"))
      .filter(
        (element) =>
          getComputedStyle(element).fontFamily.includes("DM Mono") &&
          Array.from(element.childNodes).some(
            (node) => node.nodeType === 3 && (node.textContent ?? "").includes(" "),
          ),
      )
      .map((element) => element.textContent),
  );
  expect(bad).toEqual([]);
});
