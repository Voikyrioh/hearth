import { expect, type Page, test } from "@playwright/test";

// HRT-67 : l'onglet Journaux d'un service avec le pont SIMULÉ. Parcours (direct, pause en remontant, reprise,
// recherche), états (plafond du compte, frein, arrêté, redémarrage, trou, contenu hostile), mise en page mesurée à
// 1280, 1366 et 1920 (boîtes, pas d'impression visuelle), charge (cinquante mille lignes) et fermeture en quittant.
const KEY = "c_web";

type Line = { text: string; source?: "out" | "err"; t?: number };
type Sim = {
  logs: {
    seed(id: string, key: string, lines: Line[]): void;
    push(id: string, key: string, lines: Line[]): void;
    flood(
      id: string,
      key: string,
      perSecond: number,
      options?: { errorEvery?: number },
    ): () => void;
    stopFlood(id: string, key: string): void;
    restart(id: string, key: string): void;
    gap(id: string, key: string, reason: string, count: number | null): void;
    refuseNext(refusal: string, retryAfterS?: number | null): void;
    setStatus(id: string, key: string, patch: Record<string, unknown>): void;
    interrupt(id: string, key: string, options?: Record<string, unknown>): void;
    openCount(): number;
    closedCount: number;
    batches: number;
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T): Promise<Awaited<T>> {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return ${source}`)(bridge);
  }, `(${run.toString()})(sim)`) as Promise<Awaited<T>>;
}

async function open(page: Page, width = 1366, height = 900, prepare?: (sim: Sim) => void) {
  await page.setViewportSize({ width, height });
  // Une page neuve à chaque ouverture : le pont simulé repart de zéro.
  await page.goto(`/?nodev&t=${Date.now()}#/servers/forge/services`);
  await expect(page.locator("[data-row]").first()).toBeVisible();
  if (prepare) await sim(page, prepare);
  await page.evaluate((hash) => {
    window.location.hash = hash;
  }, `#/servers/forge/services?service=${KEY}&onglet=journaux`);
}

const rows = (page: Page) => page.locator("[data-log-row]");

test("parcours : direct, pause en remontant, reprise, pause manuelle, recherche", async ({
  page,
}) => {
  await open(page);
  await expect(page.locator("[data-logs-notice]")).toContainText("secrets compris");
  await sim(page, (s) => {
    s.logs.seed("forge", "c_web", [{ text: "demarrage" }, { text: "boum", source: "err" }]);
    s.logs.flood("forge", "c_web", 1000, { errorEvery: 40 });
  });
  await expect(page.locator("[data-logs-state]")).toHaveText("En direct");
  await expect.poll(() => rows(page).count()).toBeGreaterThan(10);
  // Environ soixante lignes dans le DOM, jamais plus d'une centaine.
  expect(await rows(page).count()).toBeLessThan(100);
  // Le repère d'erreur est un mot et une icône, pas seulement une couleur.
  await expect(page.locator("[data-log-row][data-source='err']").first()).toContainText("Erreur");

  // Remonter pose la pause, fige l'écran, compte.
  const scroller = page.locator("[data-logs-scroller]");
  await scroller.evaluate((el) => {
    el.scrollTop -= 500;
  });
  await expect(page.locator("[data-logs-state]")).toHaveText("En pause");
  const frozen = await rows(page).first().innerText();
  await page.waitForTimeout(500);
  expect(await rows(page).first().innerText()).toBe(frozen);
  const first = Number.parseInt(
    (await page.locator("[data-logs-waiting]").innerText()).split(" ")[0] ?? "0",
    10,
  );
  await page.waitForTimeout(500);
  const second = Number.parseInt(
    (await page.locator("[data-logs-waiting]").innerText()).split(" ")[0] ?? "0",
    10,
  );
  expect(second).toBeGreaterThan(first);
  // « Revenir au direct » est le seul bouton principal, et il n'apparaît que dans ce cas.
  const live = page.locator("[data-logs-live]");
  await expect(live).toBeVisible();
  await live.click();
  await expect(page.locator("[data-logs-state]")).toHaveText("En direct");
  await expect(live).toHaveCount(0);
  await expect
    .poll(() => scroller.evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight))
    .toBeLessThan(30);

  // Pause manuelle puis reprise.
  await page.locator("[data-logs-pause]").click();
  await expect(page.locator("[data-logs-pause]")).toContainText("Reprendre le direct");
  await page.locator("[data-logs-pause]").click();
  await expect(page.locator("[data-logs-state]")).toHaveText("En direct");

  // Recherche dans ce qui est chargé.
  await sim(page, (s) => s.logs.stopFlood("forge", "c_web"));
  await sim(page, (s) => s.logs.push("forge", "c_web", [{ text: "ligne rare : ZEBRE" }]));
  await page.locator("[data-log-search-input]").fill("zebre");
  await expect(page.locator("[data-log-search-count]")).toHaveText("1 sur 1");
  await expect(page.locator("mark")).toContainText(/zebre/i);
});

test("états : plafond du compte et frein", async ({ page }) => {
  await open(page, 1366, 900, (s) => s.logs.refuseNext("logs_account_limit"));
  const notice = page.locator("[data-logs-notice-end]");
  await expect(notice).toContainText("Tu lis déjà les journaux de 3 services");
  await expect(notice).toHaveAttribute("role", "alert");
  await page.screenshot({ path: "test-results/hrt67-plafond.png" });

  await open(page, 1366, 900, (s) => s.logs.refuseNext("logs_rate_limited", 9));
  await expect(page.locator("[data-logs-notice-end]")).toContainText(/Attends [89] s/);
  await expect(page.locator("[data-logs-retry]")).toHaveAttribute("aria-disabled", "true");
  // Aucune relance seule : une seule ouverture après plusieurs secondes.
  await page.waitForTimeout(1500);
  expect(await sim(page, (s) => s.logs.openCount())).toBe(0);
});

test("états : arrêté, redémarrage, trou, coupure et reprise", async ({ page }) => {
  await open(page, 1366, 900, (s) => {
    s.logs.seed("forge", "c_web", [{ text: "avant" }]);
    s.logs.setStatus("forge", "c_web", {
      state: "ended",
      exitCode: 137,
      endedAt: Date.now() - 90_000,
    });
  });
  await expect(page.locator("[data-logs-stopped]")).toContainText("Ce service est arrêté");
  await expect(page.locator("[data-logs-stopped-code]")).toContainText("code 137");
  await expect(page.locator("[data-logs-state]")).toHaveText("Arrêté");
  await sim(page, (s) => s.logs.restart("forge", "c_web"));
  await expect(page.locator("[data-log-marker][data-marker-kind='restart']")).toContainText(
    "Le service a redémarré à",
  );
  await expect(page.locator("[data-logs-stopped]")).toHaveCount(0);
  await sim(page, (s) => s.logs.gap("forge", "c_web", "resume_lost", null));
  await expect(page.locator("[data-log-marker][data-marker-kind='gap']")).toContainText(
    "Des lignes manquent ici",
  );
  await sim(page, (s) =>
    s.logs.interrupt("forge", "c_web", {
      afterMs: 400,
      whileCut: [{ text: "pendant la coupure" }],
    }),
  );
  await expect(page.locator("[data-logs-state]")).toContainText("Lecture interrompue");
  await expect(page.locator("[data-log-row]", { hasText: "pendant la coupure" })).toBeVisible();
  await page.screenshot({ path: "test-results/hrt67-marqueurs.png" });
});

test("contenu hostile : rien n'est interprété", async ({ page }) => {
  await open(page);
  await sim(page, (s) => {
    const esc = String.fromCharCode(27);
    const bel = String.fromCharCode(7);
    s.logs.push("forge", "c_web", [
      { text: `${esc}[31mrouge${esc}[0m ${esc}[2Jefface` },
      { text: `${esc}]0;titre${bel}apres le titre` },
      { text: "bidi ‮errof‬ fin" },
      { text: "<img src=x onerror=window.__pwned=1> <script>window.__pwned=1</script>" },
      { text: "https://exemple.invalid/lien www.exemple.invalid" },
      { text: "a".repeat(64 * 1024) },
      { text: "2026-01-01T00:00:00Z faux horodatage" },
    ]);
  });
  await expect(page.locator("[data-log-row]", { hasText: "apres le titre" })).toBeVisible();
  const state = await page.evaluate(() => ({
    pwned: (window as unknown as { __pwned?: number }).__pwned ?? 0,
    injected: document.querySelectorAll(
      "[data-logs-rows] img, [data-logs-rows] script, [data-logs-rows] a, [data-logs-rows] iframe",
    ).length,
    escapes:
      document.querySelector("[data-logs-rows]")?.textContent?.includes(String.fromCharCode(27)) ??
      false,
    bidi: /[‪-‮⁦-⁩]/.test(document.querySelector("[data-logs-rows]")?.textContent ?? ""),
    html: document.querySelector("[data-logs-rows]")?.textContent?.includes("<img src=x") ?? false,
  }));
  expect(state).toEqual({ pwned: 0, injected: 0, escapes: false, bidi: false, html: true });
  // La ligne de 64 Ko est bornée et repliée : on peut la déplier, jamais l'écran n'enfle.
  const fold = page.locator("[data-log-fold]").first();
  await expect(fold).toBeVisible();
  const height = (await page.locator("[data-log-row]", { has: fold }).boundingBox())?.height ?? 0;
  expect(height).toBeLessThan(200);
  await fold.click();
  await expect(page.locator("[data-log-fold]").first()).toContainText("Replier");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
});

test("charge : cinquante mille lignes, l'interface répond", async ({ page }) => {
  await open(page);
  await sim(page, (s) => {
    for (let batch = 0; batch < 100; batch += 1) {
      s.logs.push(
        "forge",
        "c_web",
        Array.from({ length: 500 }, (_, i) => ({
          text: `ligne ${batch * 500 + i} avec un texte de longueur moyenne pour le test`,
        })),
      );
    }
  });
  await expect(page.locator("[data-logs-trimmed]")).toContainText(
    "les 5 000 dernières sont gardées",
  );
  await expect(page.locator("[data-log-row]", { hasText: "ligne 49999" })).toBeVisible();
  expect(await rows(page).count()).toBeLessThan(120);
  // Clic, saisie et défilement répondent, avec une borne large pour rester stable sur une machine chargée.
  const started = Date.now();
  await page.locator("[data-logs-pause]").click();
  await expect(page.locator("[data-logs-state]")).toHaveText("En pause");
  await page.locator("[data-log-search-input]").fill("ligne 4999");
  await page.locator("[data-logs-scroller]").evaluate((el) => {
    el.scrollTop = 0;
  });
  await page.locator("[data-logs-live]").click();
  await expect(page.locator("[data-logs-state]")).toHaveText("En direct");
  expect(Date.now() - started).toBeLessThan(5000);
  // Un débit continu de mille lignes par seconde ne bloque pas la page.
  await sim(page, (s) => s.logs.flood("forge", "c_web", 1000));
  const latency = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const begin = performance.now();
        setTimeout(() => resolve(performance.now() - begin), 0);
      }),
  );
  expect(latency).toBeLessThan(1000);
  await page.locator("[data-logs-pause]").click();
  await expect(page.locator("[data-logs-state]")).toHaveText("En pause");
  await sim(page, (s) => s.logs.stopFlood("forge", "c_web"));
});

test("quitter l'onglet ferme la lecture et efface les lignes", async ({ page }) => {
  await open(page);
  await sim(page, (s) => s.logs.push("forge", "c_web", [{ text: "SECRET-DE-LIGNE" }]));
  await expect(page.locator("body")).toContainText("SECRET-DE-LIGNE");
  expect(await sim(page, (s) => s.logs.openCount())).toBe(1);
  await page.locator("[data-tab='detail']").click();
  await expect(page.locator("body")).not.toContainText("SECRET-DE-LIGNE");
  expect(await sim(page, (s) => s.logs.openCount())).toBe(0);
  await page.locator("[data-tab='logs']").click();
  await expect(page.locator("[data-logs-scroller]")).toBeVisible();
  await page.getByRole("button", { name: "Retour à la liste" }).click();
  await expect(page.locator("[data-logs]")).toHaveCount(0);
  expect(await sim(page, (s) => s.logs.openCount())).toBe(0);
  expect(await page.evaluate(() => localStorage.length + sessionStorage.length)).toBe(0);
});

for (const [width, height] of [
  [1280, 800],
  [1366, 800],
  [1920, 1080],
] as const) {
  test(`mise en page à ${width} px : la fenêtre de lignes tient, aucun défilement horizontal`, async ({
    page,
  }) => {
    await open(page, width, height);
    await sim(page, (s) => {
      s.logs.seed("forge", "c_web", [{ text: "x".repeat(400) }, { text: "boum", source: "err" }]);
      s.logs.flood("forge", "c_web", 200, { errorEvery: 20 });
    });
    const scroller = page.locator("[data-logs-scroller]");
    await expect(scroller).toBeVisible();
    await expect.poll(() => rows(page).count()).toBeGreaterThan(5);
    const box = await scroller.boundingBox();
    expect(box).not.toBeNull();
    if (!box) return;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    expect(box.y + box.height).toBeLessThanOrEqual(height);
    expect(box.height).toBeGreaterThanOrEqual(200);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    // Une seule barre de défilement verticale pour les lignes : la page et le détail ne défilent pas.
    expect(
      await page
        .locator("[data-service-detail]")
        .evaluate((el) => el.scrollHeight <= el.clientHeight + 1),
    ).toBe(true);
    // Le bouton principal n'existe que pendant une pause.
    await expect(page.locator("[data-logs-live]")).toHaveCount(0);
    await page.screenshot({ path: `test-results/hrt67-${width}.png` });
    await sim(page, (s) => s.logs.stopFlood("forge", "c_web"));
  });
}
