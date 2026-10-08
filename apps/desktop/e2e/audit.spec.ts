import { expect, type Page, test } from "@playwright/test";

// HRT-14 : le journal d'activité. Pont SIMULÉ (aucun réseau) : l'agent simulé filtre, cherche et
// pagine comme le vrai (`link/simulated-audit.ts`) ; ce qui est prouvé contre un vrai agent
// (filtres, curseur, export, refus, direct) l'est côté Rust (`crates/hearth-link/tests/audit.rs`,
// `apps/desktop/src-tauri/tests/audit_runtime.rs`). Ici : le parcours de l'écran.

type Sim = {
  setState(id: string, state: string): void;
  audit: {
    add(id: string, entry?: Record<string, unknown>): { id: number };
    addBurst(id: string, count: number, addr?: string, everyMs?: number): unknown[];
    seed(id: string, count: number): void;
    clear(id: string): void;
    exports: { filter: Record<string, unknown> }[];
    exportMode: "save" | "cancel" | "fail";
    failReads: boolean;
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T): Promise<T> {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString()) as Promise<T>;
}

const scroller = (page: Page) => page.locator(".table__scroll");
const rows = (page: Page) => page.locator('[role="row"][data-row-key]');

async function open(page: Page) {
  await page.goto("/?nodev#/servers/forge/audit");
  await expect(page.getByRole("heading", { name: "Journal d'activité" })).toBeVisible();
  await expect(page.getByRole("grid")).toBeVisible();
}

test("le tableau, ses colonnes, le compteur et l'indicateur de conservation", async ({ page }) => {
  await open(page);
  const headers = page.getByRole("columnheader");
  await expect(headers).toHaveText([
    "Date et heure",
    "Compte",
    "Origine",
    "Action",
    "Cible",
    "Résultat",
    "Raison",
  ]);
  await expect(page.getByText("100 événements ou plus")).toBeVisible();
  await expect(page.getByText("Journal conservé pendant 90 jours ou 50 000 entrées")).toBeVisible();
  // Une liste virtualisée : des centaines d'entrées, des dizaines de lignes dans le DOM.
  expect(await rows(page).count()).toBeLessThan(60);
  // Plus on défile, plus on charge (la page suivante arrive par le curseur).
  await scroller(page).evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await expect(
    page.getByText("200 événements ou plus").or(page.getByText("250 événements")),
  ).toBeVisible();
  await page.screenshot({ path: "e2e/screenshots/audit-1366.png" });
});

test("filtres : appliquer, recherche, plusieurs valeurs, effacer d'un clic", async ({ page }) => {
  await open(page);
  // Plus de bouton « Appliquer » : la recherche s'applique à la frappe.
  await expect(page.getByRole("button", { name: /Appliquer/ })).toHaveCount(0);
  await page.getByRole("searchbox", { name: "Rechercher" }).fill("léa");
  await expect(page.getByText(/événements( ou plus)?$/).first()).toBeVisible();
  await expect(page.getByText("100 événements ou plus")).toHaveCount(0, { timeout: 3000 });
  const accounts = await page
    .locator('[role="row"][data-row-key] [role="gridcell"]:nth-child(2)')
    .allTextContents();
  expect(accounts.length).toBeGreaterThan(0);
  expect(accounts.every((text) => text.trim() === "léa")).toBe(true);

  // Type d'action et résultat : cases à cocher ; « Connexion réussie » avec « Refusé » = rien.
  await page.getByRole("button", { name: /Type d'action/ }).click();
  await page.getByLabel("Connexion réussie").check();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: /^Résultat/ }).click();
  await page.getByLabel("Refusé").check();
  await page.keyboard.press("Escape");
  await expect(page.getByText("Aucun événement ne correspond")).toBeVisible();

  // Effacer : un seul clic, tout revient.
  await page.getByRole("button", { name: "Effacer les filtres" }).first().click();
  await expect(page.getByText("Aucun événement ne correspond")).toHaveCount(0);
  await expect(page.getByRole("searchbox", { name: "Rechercher" })).toHaveValue("");
  await expect(page.getByText("100 événements ou plus")).toBeVisible();
  await expect(page.getByRole("button", { name: "Effacer les filtres" })).toHaveCount(0);
});

test("période personnalisée : messages de la spec, puis application", async ({ page }) => {
  await open(page);
  await page.getByLabel("Période").selectOption("custom");
  const day = (offset: number) => {
    const date = new Date(Date.now() + offset * 86_400_000);
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  };
  await page.getByLabel("Du", { exact: true }).fill(day(-1));
  await page.getByLabel("Au", { exact: true }).fill(day(-3));
  await expect(page.getByText("La fin de la période doit suivre le début")).toBeVisible();
  await page.getByLabel("Du", { exact: true }).fill("2001-01-01");
  await expect(page.getByText("La date dépasse l'historique conservé (90 jours)")).toBeVisible();
  await page.getByLabel("Période").selectOption("today");
  await expect(page.getByText("La date dépasse l'historique conservé (90 jours)")).toHaveCount(0);
});

test("une rafale de refus est regroupée et se déploie, sans masquer une autre entrée", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => {
    s.audit.addBurst("forge", 6, "203.0.113.9", 10_000);
    s.audit.add("forge", {
      account: "léa",
      actionLabel: "Entrée d'un autre type",
      action: "logout",
    });
  });
  await expect(page.getByText("Entrée d'un autre type")).toBeVisible();
  const burst = page.getByRole("row", { name: /6 tentatives refusées en 1 min/ });
  await expect(burst).toBeVisible();
  await expect(burst).toHaveAttribute("aria-expanded", "false");
  await burst.click();
  await expect(burst).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByRole("gridcell", { name: "203.0.113.9 (inconnu)" })).toHaveCount(6);
  await burst.press("Enter");
  await expect(burst).toHaveAttribute("aria-expanded", "false");
});

test("direct : en haut l'entrée apparaît ; défilé, un bouton « N nouvelles entrées » la garde", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.audit.add("forge", { account: "léa", actionLabel: "Première en direct", action: "logout" }),
  );
  await expect(rows(page).first()).toContainText("Première en direct");
  await expect(page.getByRole("button", { name: /nouvelles? entrées?/ })).toHaveCount(0);

  await scroller(page).evaluate((el) => {
    el.scrollTop = 600;
  });
  // L'écran a pris acte du défilement (la liste affichée n'est plus celle du haut).
  await expect(rows(page).first()).not.toContainText("Première en direct");
  const before = await rows(page).first().textContent();
  await sim(page, (s) => {
    s.audit.add("forge", { actionLabel: "Deuxième", action: "logout" });
    s.audit.add("forge", { actionLabel: "Troisième", action: "logout" });
    s.audit.add("forge", { actionLabel: "Quatrième", action: "logout" });
  });
  const more = page.getByRole("button", { name: "3 nouvelles entrées" });
  await expect(more).toBeVisible();
  // La liste n'a pas bougé sous les yeux.
  expect(await rows(page).first().textContent()).toBe(before);
  await more.click();
  await expect(more).toHaveCount(0);
  await expect(rows(page).first()).toContainText("Quatrième");
  expect(await scroller(page).evaluate((el) => el.scrollTop)).toBe(0);
  // Annonce aux lecteurs d'écran : zone polie, une fois.
  await expect(page.locator(".audit__sr")).toHaveText(/nouvelles? entrées?/);
  await page.screenshot({ path: "e2e/screenshots/audit-live-1366.png" });
});

test("un filtre actif vaut aussi pour le direct", async ({ page }) => {
  await open(page);
  await page.getByRole("searchbox", { name: "Rechercher" }).fill("léa");
  await expect(page.getByRole("button", { name: "Effacer les filtres" })).toBeVisible();
  await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
  await sim(page, (s) => {
    s.audit.add("forge", { account: "paul", actionLabel: "Hors filtre", action: "logout" });
    s.audit.add("forge", { account: "léa", actionLabel: "Dans le filtre", action: "logout" });
  });
  await expect(page.getByText("Dans le filtre")).toBeVisible();
  await expect(page.getByText("Hors filtre")).toHaveCount(0);
});

test("export : enregistré, annulé, en échec ; le filtre appliqué part, pas le brouillon", async ({
  page,
}) => {
  await open(page);
  const exportButton = page.getByRole("button", { name: "Exporter" });
  await exportButton.click();
  await expect(page.getByText("Export terminé")).toBeVisible();
  await page.getByRole("searchbox", { name: "Rechercher" }).fill("brouillon-non-applique");
  await exportButton.click();
  const sent = await sim(page, (s) => s.audit.exports.map((e) => e.filter));
  expect(sent).toHaveLength(2);
  expect((sent[1] as { text: string | null }).text ?? "").toBe("");
  await sim(page, (s) => {
    s.audit.exportMode = "fail";
  });
  await exportButton.click();
  await expect(page.getByText("Impossible de générer l'export")).toBeVisible();
});

test("perte du lien : données périmées, rechargement manuel ; retour : rattrapage des entrées manquées", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => s.setState("forge", "offline"));
  await expect(page.getByText("Données périmées, serveur injoignable")).toBeVisible();
  await expect(page.getByText("Périmé", { exact: true })).toBeVisible();
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Exporter" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  // Écrite pendant la coupure : jamais reçue en direct.
  await sim(page, (s) =>
    s.audit.add("forge", { actionLabel: "Pendant la coupure", action: "logout" }),
  );
  await expect(page.getByText("Pendant la coupure")).toHaveCount(0);
  await sim(page, (s) => s.setState("forge", "connected"));
  await expect(page.getByText("Pendant la coupure")).toBeVisible();
  await expect(page.getByText("Lien rétabli, données à jour")).toBeVisible();
  await expect(page.getByText("Données périmées, serveur injoignable")).toHaveCount(0);
  await page.screenshot({ path: "e2e/screenshots/audit-offline-1366.png" });
});

test("clavier : flèches dans la grille, Entrée ouvre le détail avec le texte complet", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.audit.add("forge", {
      actionLabel: "Valeur piégée",
      action: "logout",
      target: `<img src=x onerror=alert(1)> ${"très long ".repeat(30)}`,
    }),
  );
  const first = rows(page).first();
  await first.focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowUp");
  await expect(first).toBeFocused();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("<img src=x onerror=alert(1)>");
  await expect(dialog.locator("img")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(first).toBeFocused();
});

test("journal vide", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await sim(page, (s) => s.audit.clear("forge"));
  await page.goto("/?nodev#/servers/forge/audit");
  await expect(page.getByText("Aucune activité enregistrée pour l'instant")).toBeVisible();
  await expect(page.getByRole("grid")).toHaveCount(0);
});

for (const width of [1920, 2560]) {
  test(`captures à ${width} px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1080 });
    await open(page);
    await sim(page, (s) => s.audit.addBurst("forge", 6));
    await expect(page.getByRole("row", { name: /tentatives refusées/ })).toBeVisible();
    await page.screenshot({ path: `e2e/screenshots/audit-${width}.png` });
  });
}
