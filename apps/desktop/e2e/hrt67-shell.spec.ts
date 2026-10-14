import { expect, type Page, test } from "@playwright/test";

// HRT-67, corrections de revue (PR 95) : ce que la coquille fait de la lecture des journaux (rechargement, fenêtre
// cachée, place refusée), vu de l'écran, et la mise en page à la taille minimale de la fenêtre (1100 x 680). Le pont
// est SIMULÉ : la coquille simulée garde ses sessions dans le stockage de session (comme la vraie survit à un
// rechargement de page). La preuve côté Rust (sessions, battement, fenêtre) est dans
// `src-tauri/tests/service_logs_runtime.rs` ; la livraison réelle des événements ne se voit que dans la vraie fenêtre.
const KEY = "c_web";
const SHOTS = process.env.HRT67_SHOTS ?? "test-results";

type Line = { text: string; source?: "out" | "err"; t?: number };
type Sim = {
  logs: {
    push(id: string, key: string, lines: Line[]): void;
    flood(
      id: string,
      key: string,
      perSecond: number,
      options?: { errorEvery?: number },
    ): () => void;
    stopFlood(id: string, key: string): void;
    setStatus(id: string, key: string, patch: Record<string, unknown>): void;
    hideWindow(cause?: string): void;
    openCount(): number;
    shellCount(): number;
    orphansClosedAtStart: number;
    shellLimit: number;
    loseCloses: boolean;
    opened: unknown[];
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T): Promise<Awaited<T>> {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return ${source}`)(bridge);
  }, `(${run.toString()})(sim)`) as Promise<Awaited<T>>;
}

const LOGS_HASH = `#/servers/forge/services?service=${KEY}&onglet=journaux`;

async function open(page: Page, width = 1366, height = 900) {
  await page.setViewportSize({ width, height });
  await page.goto(`/?nodev&t=${Date.now()}#/servers/forge/services`);
  await expect(page.locator("[data-row]").first()).toBeVisible();
  await page.evaluate((hash) => {
    window.location.hash = hash;
  }, LOGS_HASH);
}

const rows = (page: Page) => page.locator("[data-log-row]");

test("rechargement : la coquille ferme ce que la page morte avait laissé, trois rechargements ne bloquent rien", async ({
  page,
}) => {
  await open(page);
  await expect(page.locator("[data-logs-state]")).toBeVisible();
  expect(await sim(page, (s) => s.logs.shellCount())).toBe(1);
  for (let n = 0; n < 3; n += 1) {
    // La commande de fermeture se perd pendant que la page meurt : la lecture reste ouverte dans la coquille.
    await sim(page, (s) => {
      s.logs.loseCloses = true;
    });
    // La page est rechargée SANS fermer sa lecture (aucun `pagehide` qui compte, aucun plantage rattrapé).
    await page.reload();
    // Le détail rouvre l'onglet Journaux (l'adresse le dit) : une seule lecture, la neuve.
    await expect(page.locator("[data-logs-state]")).toBeVisible();
    // La nouvelle page s'est annoncée : l'orpheline a été fermée dans la coquille, sa place rendue.
    expect(await sim(page, (s) => s.logs.orphansClosedAtStart)).toBe(1);
    expect(await sim(page, (s) => s.logs.shellCount())).toBe(1);
    await expect(page.locator("[data-logs-notice-end]")).toHaveCount(0);
  }
  await page.screenshot({ path: `${SHOTS}/07-apres-rechargements-1366.png` });
});

test("fenêtre cachée : lecture fermée, lignes effacées, l'écran le dit et propose de reprendre sans reprise seule", async ({
  page,
}) => {
  await open(page, 1366, 800);
  await sim(page, (s) => {
    s.logs.push("forge", "c_web", [{ text: "SECRET-VISIBLE-1" }, { text: "SECRET-VISIBLE-2" }]);
  });
  await expect.poll(() => rows(page).count()).toBeGreaterThan(1);
  await sim(page, (s) => s.logs.hideWindow());
  const notice = page.locator("[data-logs-notice-end]");
  await expect(notice).toContainText("pendant que la fenêtre était fermée");
  await expect(notice).toContainText("effacées");
  await expect(page.locator("[data-logs-retry]")).toHaveText("Reprendre");
  // Les lignes ne sont plus nulle part dans la page.
  await expect(rows(page)).toHaveCount(0);
  expect(await page.locator("[data-logs]").innerText()).not.toContain("SECRET-VISIBLE");
  expect(await sim(page, (s) => s.logs.openCount())).toBe(0);
  expect(await sim(page, (s) => s.logs.shellCount())).toBe(0);
  // Au retour : aucune reprise silencieuse.
  await page.waitForTimeout(600);
  expect(await sim(page, (s) => s.logs.openCount())).toBe(0);
  await page.screenshot({ path: `${SHOTS}/02-fenetre-cachee-1366.png` });
  await page.locator("[data-logs-retry]").click();
  await expect(page.locator("[data-logs-state]")).toBeVisible();
  await expect(page.locator("[data-logs-notice-end]")).toHaveCount(0);
  expect(await sim(page, (s) => s.logs.openCount())).toBe(1);
});

test("la coquille refuse faute de place : l'écran le dit avec ce qu'il faut faire", async ({
  page,
}) => {
  await open(page, 1366, 800);
  await expect(page.locator("[data-logs-state]")).toBeVisible();
  await sim(page, (s) => s.logs.hideWindow());
  await expect(page.locator("[data-logs-retry]")).toBeVisible();
  await sim(page, (s) => {
    s.logs.shellLimit = 0;
  });
  await page.locator("[data-logs-retry]").click();
  const notice = page.locator("[data-logs-notice-end]");
  await expect(notice).toContainText("Trois lectures de journaux sont déjà ouvertes depuis ce PC");
  await expect(notice).toContainText("attends un peu");
  await expect(notice).not.toContainText("Impossible");
  await page.screenshot({ path: `${SHOTS}/03-place-refusee-1366.png` });
  await sim(page, (s) => {
    s.logs.shellLimit = Number.POSITIVE_INFINITY;
  });
  await page.locator("[data-logs-retry]").click();
  await expect(page.locator("[data-logs-state]")).toBeVisible();
});

async function fits(page: Page, width: number, height: number, stacked = false) {
  const scroller = page.locator("[data-logs-scroller]");
  await expect(scroller).toBeVisible();
  await expect.poll(() => rows(page).count()).toBeGreaterThan(2);
  const detail = page.locator("[data-service-detail]");
  if (stacked) {
    // Des avis empilés ne laissent plus la place d'une dizaine de lignes : le volet défile, le cadre reste atteignable.
    await detail.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
  } else {
    // Cas courant : le volet ne défile pas, tout tient.
    expect(await detail.evaluate((el) => el.scrollHeight <= el.clientHeight + 1)).toBe(true);
  }
  const box = await scroller.boundingBox();
  expect(box).not.toBeNull();
  if (!box) return;
  // Le cadre entier tient dans la fenêtre, ligne d'information comprise.
  const info = await page.locator("[data-logs-info]").boundingBox();
  expect(info).not.toBeNull();
  expect(box.y + box.height).toBeLessThanOrEqual(height);
  expect((info?.y ?? 0) + (info?.height ?? 0)).toBeLessThanOrEqual(height);
  expect(box.x + box.width).toBeLessThanOrEqual(width);
  expect(box.height).toBeGreaterThanOrEqual(120);
  // La page ne défile ni en hauteur ni en largeur.
  expect(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight <= window.innerHeight &&
        document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  // Le direct est au bas de la zone des lignes (la plus récente est celle qu'on voit).
  await expect
    .poll(() => scroller.evaluate((el) => el.scrollTop + el.clientHeight >= el.scrollHeight - 2))
    .toBe(true);
}

test("mise en page à la taille minimale 1100 x 680 : le cadre tient dans la fenêtre", async ({
  page,
}) => {
  await open(page, 1100, 680);
  await sim(page, (s) => {
    s.logs.push("forge", "c_web", [{ text: "x".repeat(400) }, { text: "boum", source: "err" }]);
    s.logs.flood("forge", "c_web", 200, { errorEvery: 20 });
  });
  await expect.poll(() => rows(page).count()).toBeGreaterThan(5);
  // Le débit est arrêté avant de mesurer le bas de la liste (sinon la mesure court après les lignes).
  await sim(page, (s) => s.logs.stopFlood("forge", "c_web"));
  await fits(page, 1100, 680);
  await page.screenshot({ path: `${SHOTS}/01-direct-1100x680.png` });
});

test("mise en page à 1100 x 680 avec les avis empilés d'un service arrêté", async ({ page }) => {
  await open(page, 1100, 680);
  await sim(page, (s) => {
    s.logs.setStatus("forge", "c_web", {
      state: "ended",
      exitCode: 137,
      endedAt: Date.now() - 90_000,
    });
    s.logs.push(
      "forge",
      "c_web",
      Array.from({ length: 60 }, (_, i) => ({ text: `ligne ${i}` })),
    );
  });
  await expect(page.locator("[data-logs-stopped]")).toBeVisible();
  await fits(page, 1100, 680, true);
  await page.screenshot({ path: `${SHOTS}/04-arrete-avis-empiles-1100x680.png` });
});

test("l'avis des lignes retirées est en haut de la liste, avant la première ligne", async ({
  page,
}) => {
  await open(page, 1100, 680);
  await sim(page, (s) => {
    s.logs.push(
      "forge",
      "c_web",
      Array.from({ length: 5_300 }, (_, i) => ({ text: `ligne ${i}` })),
    );
  });
  const trimmed = page.locator("[data-logs-trimmed]");
  await expect(trimmed).toBeVisible();
  const notice = await trimmed.boundingBox();
  const scroller = await page.locator("[data-logs-scroller]").boundingBox();
  expect(notice).not.toBeNull();
  expect(scroller).not.toBeNull();
  // Au-dessus de la zone qui défile, donc au-dessus de la plus ancienne ligne, jamais sous la plus récente.
  expect((notice?.y ?? 0) + (notice?.height ?? 0)).toBeLessThanOrEqual(scroller?.y ?? 0);
  await fits(page, 1100, 680);
  // Toujours visible quand on remonte tout en haut.
  await page.locator("[data-logs-scroller]").evaluate((el) => {
    el.scrollTop = 0;
  });
  await expect(trimmed).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/05-avis-retire-en-haut-1100x680.png` });
});

test("la ligne active est annoncée au lecteur d'écran, la liste reste muette", async ({ page }) => {
  await open(page, 1366, 800);
  await sim(page, (s) => {
    s.logs.push("forge", "c_web", [
      { text: "premiere" },
      { text: "deuxieme" },
      { text: "troisieme" },
    ]);
  });
  await expect.poll(() => rows(page).count()).toBe(3);
  const info = page.locator("[data-logs-info]");
  await expect(info).toHaveAttribute("role", "status");
  await expect(info).toHaveAttribute("aria-live", "polite");
  // La zone des lignes, elle, ne lit rien en direct.
  await expect(page.locator("[data-logs-scroller]")).toHaveAttribute("aria-live", "off");
  await page.locator("[data-logs-scroller]").focus();
  await page.keyboard.press("ArrowUp");
  await expect(info).toContainText("troisieme");
  await page.keyboard.press("ArrowUp");
  await expect(info).toContainText("deuxieme");
  await page.screenshot({ path: `${SHOTS}/06-ligne-active-1366.png` });
});
