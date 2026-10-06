import { expect, type Page, test } from "@playwright/test";

// Mise à jour du client avec le pont SIMULÉ (`window.__hearthUpdateSim`) : bandeau, « Plus tard »,
// « Vérifier maintenant », messages d'échec. Aucune coquille Tauri, aucun réseau. Captures dans
// e2e/screenshots/ (non commitées) : 1366, 1920 et 2560 px, comme la revue UX.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 900;
const DAY = 24 * 60 * 60 * 1000;

type Sim = {
  setFeed(release: { version: string; notes: string } | null): void;
  setOnline(online: boolean): void;
  setInstallOutcome(outcome: "ok" | "interrupted" | "corrupted" | "failed"): void;
  setProgress(percent: number): void;
  finishInstall(): void;
  advance(ms: number): void;
  calls: { check: number; postpone: number; install: number };
  installed: number;
};

async function sim<A, T>(page: Page, run: (sim: Sim, arg: A) => T, arg?: A): Promise<T> {
  return page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthUpdateSim: Sim }).__hearthUpdateSim;
      return new Function("sim", "arg", `return (${source})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );
}

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const banner = (page: Page) => page.locator("[data-update-banner]");
const NOTES = {
  version: "1.1.0",
  notes: "Corrections\n- le tableau de bord se rafraîchit mieux\n- un correctif réseau",
};

/** Le flux propose une version plus récente ; « Vérifier maintenant » la trouve. */
async function announce(page: Page) {
  await page.goto("/?nodev#/settings");
  await sim(page, (s, release) => s.setFeed(release), NOTES);
  await page.getByRole("button", { name: "Vérifier maintenant" }).click();
  await expect(banner(page)).toBeVisible();
}

test("aucun bandeau quand le client est à jour, et les réglages le disent", async ({ page }) => {
  await page.goto("/?nodev#/settings");
  await expect(page.getByRole("heading", { name: "Mises à jour" })).toBeVisible();
  await expect(banner(page)).toHaveCount(0);
  await expect(page.getByText("Tu es à jour")).toBeVisible();
  await expect(page.getByText("Dernière vérification : il y a 2 heures")).toBeVisible();
  await shoot(page, "maj-reglages-a-jour");
});

test("une version plus récente : bandeau discret avec notes, « Plus tard » le masque 24 h", async ({
  page,
}) => {
  await announce(page);
  await expect(banner(page)).toContainText("Nouvelle version disponible");
  await expect(banner(page)).toContainText("1.1.0");
  await expect(banner(page).getByRole("button")).toHaveText([
    "Notes de version",
    "Plus tard",
    "Mettre à jour maintenant",
  ]);
  await shoot(page, "maj-bandeau");

  // Les notes s'affichent en texte brut dans une boîte de dialogue.
  await banner(page).getByRole("button", { name: "Notes de version" }).click();
  const dialog = page.getByRole("dialog", { name: "Notes de version" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("un correctif réseau");
  await shoot(page, "maj-notes");
  await dialog.getByRole("button", { name: "Fermer" }).click();
  await expect(dialog).toHaveCount(0);

  // « Plus tard » : le bandeau disparaît, et ne revient pas avant le lendemain.
  await banner(page).getByRole("button", { name: "Plus tard" }).click();
  await expect(banner(page)).toHaveCount(0);
  await sim(page, (s, ms) => s.advance(ms), DAY - 60_000);
  await expect(banner(page)).toHaveCount(0);
  await sim(page, (s, ms) => s.advance(ms), 60_000);
  await expect(banner(page)).toBeVisible();
});

test("« Plus tard » masque le bandeau sur toutes les pages, les réglages gardent la version", async ({
  page,
}) => {
  await announce(page);
  await banner(page).getByRole("button", { name: "Plus tard" }).click();
  await page.getByRole("link", { name: "Mes serveurs" }).click();
  await expect(banner(page)).toHaveCount(0);
  await page.goto("/?nodev#/settings");
  await expect(page.getByText("Nouvelle version disponible : 1.1.0")).toBeVisible();
});

test("« Vérifier maintenant » sans Internet : aucun message d'erreur, la date reste", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await sim(page, (s) => s.setOnline(false));
  await page.getByRole("button", { name: "Vérifier maintenant" }).click();
  // (La page affiche par ailleurs l'erreur de ses réglages, faute de coquille dans le navigateur.)
  await expect(page.locator("[data-updates-panel] [role='alert']")).toHaveCount(0);
  await expect(banner(page)).toHaveCount(0);
  await expect(page.getByText("Dernière vérification : il y a 2 heures")).toBeVisible();
  await expect(page.getByText("Tu es à jour")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Vérifier maintenant" })).toBeVisible();
  expect(await sim(page, (s) => s.calls.check)).toBe(1);
});

test("« Vérifier maintenant » sans nouvelle version : « Tu es à jour » et la date du jour", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await page.getByRole("button", { name: "Vérifier maintenant" }).click();
  await expect(page.getByText("Tu es à jour")).toBeVisible();
  await expect(page.getByText("Dernière vérification : à l'instant")).toBeVisible();
  expect(await sim(page, (s) => s.calls.check)).toBe(1);
});

test("téléchargement puis installation, sur clic seulement", async ({ page }) => {
  await announce(page);
  expect(await sim(page, (s) => s.installed)).toBe(0);
  await banner(page).getByRole("button", { name: "Mettre à jour maintenant" }).click();
  await expect(banner(page)).toContainText("Téléchargement en cours…");
  await expect(banner(page).getByRole("button")).toHaveCount(0);
  await sim(page, (s) => s.setProgress(35));
  await expect(banner(page)).toContainText("Téléchargement : 35 %");
  await shoot(page, "maj-telechargement");
  await sim(page, (s) => s.finishInstall());
  await expect(banner(page)).toContainText("Installation en cours… Redémarrage du client…");
  expect(await sim(page, (s) => s.installed)).toBe(1);
});

test("téléchargement interrompu : message, version en cours utilisable, relançable", async ({
  page,
}) => {
  await announce(page);
  await sim(page, (s) => s.setInstallOutcome("interrupted"));
  await banner(page).getByRole("button", { name: "Mettre à jour maintenant" }).click();
  await sim(page, (s) => s.finishInstall());
  await expect(banner(page)).toContainText(
    "Téléchargement interrompu. La version en cours reste utilisable.",
  );
  await shoot(page, "maj-interrompu");
  // L'application reste utilisable : la navigation ne recharge pas la page.
  await page.getByRole("link", { name: "Mes serveurs" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  // Relance.
  await sim(page, (s) => s.setInstallOutcome("ok"));
  await banner(page).getByRole("button", { name: "Réessayer" }).click();
  await sim(page, (s) => s.finishInstall());
  await expect(banner(page)).toContainText("Installation en cours…");
});

test("mise à jour corrompue : refusée avec le message exact", async ({ page }) => {
  await announce(page);
  await sim(page, (s) => s.setInstallOutcome("corrupted"));
  await banner(page).getByRole("button", { name: "Mettre à jour maintenant" }).click();
  await sim(page, (s) => s.finishInstall());
  await expect(banner(page)).toContainText(
    "Mise à jour corrompue. Refusée. La version en cours reste utilisable.",
  );
  expect(await sim(page, (s) => s.installed)).toBe(0);
  await shoot(page, "maj-corrompue");
  // « Plus tard » ferme le message et masque le bandeau.
  await banner(page).getByRole("button", { name: "Plus tard" }).click();
  await expect(banner(page)).toHaveCount(0);
});
