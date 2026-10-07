import { expect, type Page, test } from "@playwright/test";

// HRT-19 : les trois illustrations d'écran vide (accueil, journal, hors ligne), et le logo, vus
// dans l'application aux trois largeurs de recette. Captures dans e2e/screenshots/ (non commitées).
// Pont SIMULÉ, aucun réseau.

type Sim = {
  setState(id: string, state: string): void;
  audit: { clear(id: string): void };
  machine: { subscribe: unknown; resync: unknown };
};

const WIDTHS = [1366, 1920, 2560] as const;

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

async function sim(page: Page, run: (sim: Sim) => void) {
  await page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    new Function("sim", source)(bridge);
  }, `(${run.toString()})(sim)`);
}

/** L'image est affichée pour de bon : chargée, de la taille du jeton, sans texte alternatif. */
async function expectIllustration(page: Page, name: string, width: number) {
  const img = page.locator(`img[src*="${name}"]`);
  await expect(img).toHaveCount(1);
  await expect(img).toHaveAttribute("alt", "");
  await expect
    .poll(() => img.evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth > 0))
    .toBe(true);
  expect(Math.round((await img.boundingBox())?.width ?? 0)).toBe(width);
}

test("accueil : l'illustration du premier lancement", async ({ page }) => {
  await page.goto("/?servers=none&nodev");
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await expectIllustration(page, "vide-premier-lancement", 280);
  await shoot(page, "identite-accueil");
});

test("journal vide : l'illustration du journal", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await sim(page, (s) => s.audit.clear("forge"));
  await page.goto("/?nodev#/servers/forge/audit");
  await expect(page.getByText("Aucune activité enregistrée pour l'instant")).toBeVisible();
  await expectIllustration(page, "vide-journal", 220);
  await shoot(page, "identite-journal-vide");
});

test("hors ligne sans aucune mesure : l'illustration hors ligne", async ({ page }) => {
  // Serveur injoignable dont rien n'est en mémoire : aucune mesure n'arrive jamais. Le pont simulé
  // se pose sur `window` au démarrage ; on coupe ses mesures à l'instant où il apparaît.
  await page.addInitScript(() => {
    let bridge: unknown;
    Object.defineProperty(window, "__hearthSim", {
      configurable: true,
      get: () => bridge,
      set: (value: Sim) => {
        value.machine.subscribe = () => () => {};
        value.machine.resync = () => {};
        bridge = value;
      },
    });
  });
  await page.goto("/?nodev#/servers/salon/dashboard");
  await sim(page, (s) => s.setState("salon", "offline")); // le lien se coupe
  await expect(page.getByRole("heading", { name: "Aucune mesure pour l'instant" })).toBeVisible();
  await expectIllustration(page, "vide-hors-ligne", 220);
  await shoot(page, "identite-hors-ligne");
});

test("comptes vides : sans illustration", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await expect(page.locator(".empty__img")).toHaveCount(0);
});
