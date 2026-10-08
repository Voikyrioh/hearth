import { expect, type Page, test } from "@playwright/test";

// Parcours du tableau de bord avec le pont SIMULÉ (mesures plausibles, une par seconde). Captures
// dans e2e/screenshots/ (non commitées) : 1366, 1920 et 2560 px, comme la revue UX.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 1000;

type Sim = {
  setState(id: string, state: string): void;
  machine: {
    pin(id: string, measure: string, level: string | null): void;
    tick(id: string): unknown;
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T) {
  await page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    new Function("sim", source)(bridge);
  }, `(${run.toString()})(sim)`);
}

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const section = (page: Page, title: string) =>
  page
    .locator("section.card")
    .filter({ has: page.getByRole("heading", { name: title, exact: true }) });

const cpuCurve = (page: Page) =>
  section(page, "Processeur").locator("path.chart__line--ac").first();

test("les mesures arrivent : toutes les sections, des courbes qui avancent", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  for (const title of [
    "Machine",
    "Durée de fonctionnement",
    "Processeur",
    "Mémoire",
    "Carte graphique",
    "Réseau",
    "Disques",
    "Températures",
  ]) {
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
  }
  await expect(section(page, "Machine")).toContainText("NixOS 25.05");
  await expect(section(page, "Processeur")).toContainText("Charge globale");
  await expect(section(page, "Mémoire")).toContainText("Utilisée / Totale");
  await expect(section(page, "Réseau")).toContainText(/\d+\.\d Mo\/s|\d+ Ko\/s/);
  await expect(page.locator("figure.gauge")).toHaveCount(4);
  await expect(page.locator("svg.chart")).toHaveCount(6);
  await expect(page.locator("[data-level='attention'], [data-level='critical']")).toHaveCount(0);
  // Une mesure par seconde : le tracé de la courbe du processeur change.
  const first = await cpuCurve(page).getAttribute("d");
  await expect
    .poll(async () => cpuCurve(page).getAttribute("d"), { timeout: 5000 })
    .not.toBe(first);
  await shoot(page, "tableau-de-bord");
});

test("un seuil franchi se voit (icône et mot), puis s'efface", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const memory = section(page, "Mémoire");
  await expect(memory).toBeVisible();
  await sim(page, (s) => {
    s.machine.pin("forge", "mem", "critical");
    s.machine.pin("forge", "gpuTemp", "attention");
  });
  await expect(memory.locator("figure[data-level='critical']")).toContainText("Critique", {
    timeout: 5000,
  });
  await expect(memory.locator("figure[data-level='critical'] svg").first()).toBeVisible();
  await expect(section(page, "Carte graphique")).toContainText("Attention");
  await shoot(page, "tableau-de-bord-alerte");
  await sim(page, (s) => {
    s.machine.pin("forge", "mem", null);
    s.machine.pin("forge", "gpuTemp", null);
  });
  await expect(page.locator("[data-level='attention'], [data-level='critical']")).toHaveCount(0, {
    timeout: 5000,
  });
});

test("perte du lien : dernières valeurs grisées et datées, puis retour en direct", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(section(page, "Processeur")).toBeVisible();
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await sim(page, (s) => s.setState("forge", "offline"));
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByText(/^Vu il y a \d+ s$/)).toBeVisible();
  // Les valeurs sont toujours là, pas vidées.
  await expect(section(page, "Processeur")).toContainText("Charge globale");
  await expect(section(page, "Machine")).toContainText("NixOS 25.05");
  // Aucune mesure n'arrive : la courbe ne bouge pas.
  const frozen = await cpuCurve(page).getAttribute("d");
  // La machine est mesurée pendant la coupure mais rien n'est annoncé : pas d'attente de vrai temps.
  await sim(page, (s) => {
    s.machine.tick("forge");
    s.machine.tick("forge");
  });
  expect(await cpuCurve(page).getAttribute("d")).toBe(frozen);
  await shoot(page, "tableau-de-bord-hors-ligne");

  await sim(page, (s) => s.setState("forge", "connected"));
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await expect
    .poll(async () => cpuCurve(page).getAttribute("d"), { timeout: 5000 })
    .not.toBe(frozen);
});

test("l'heure écoulée est déjà là à l'ouverture : courbe d'une heure remplie (captures 1920 et 2560)", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const radios = page.getByRole("radiogroup", { name: "Durée des courbes" }).getByRole("radio");
  await radios.nth(2).click();
  await expect(radios.nth(2)).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
  // Les six courbes ont des points sur presque toute la largeur : le tracé du processeur est long.
  const path = await cpuCurve(page).getAttribute("d");
  expect((path ?? "").split(/[ML]/).filter(Boolean).length).toBeGreaterThan(300);
  for (const width of [1920, 2560] as const) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/dashboard-heure-ouverture-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
});

test("les courbes basculent entre 1 min, 5 min et 1 h sans attente", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const radios = page.getByRole("radiogroup", { name: "Durée des courbes" }).getByRole("radio");
  await expect(radios).toHaveText(["1 min", "5 min", "1 h"]);
  await expect(radios.nth(1)).toHaveAttribute("aria-checked", "true");
  await radios.nth(2).click();
  await expect(radios.nth(2)).toHaveAttribute("aria-checked", "true");
  // L'heure est lue à la connexion (BR-DASH-010) : la courbe est déjà remplie dès l'ouverture.
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
  await radios.nth(0).click();
  await expect(radios.nth(0)).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
});

test("une machine sans carte graphique ni sonde garde ses sections, avec leur explication", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/salon/dashboard");
  await expect(section(page, "Carte graphique")).toContainText("Non disponible sur cette machine");
  await expect(section(page, "Températures")).toContainText(
    "Sondes non disponibles sur cette machine. Ce matériel n'expose pas sa température au système.",
  );
  await expect(section(page, "Processeur")).toBeVisible();
  await shoot(page, "tableau-de-bord-sans-materiel");
});
