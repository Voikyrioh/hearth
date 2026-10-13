import { expect, type Page, test } from "@playwright/test";

// Parcours de la section « Services » avec le pont SIMULÉ (moteur de conteneurs et services pilotables). Captures
// dans e2e/screenshots/ (non commitées) : 1366, 1920 et 2560 px, comme la revue UX. Le pont simulé produit les
// mêmes formes que la coquille (voir `simulated-services.test.ts`) ; la coquille et le vrai agent ne sont pas
// de la partie ici.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 1000;

type Svc = Record<string, unknown> & { key: string; name: string };
type Sim = {
  setState(id: string, state: string): void;
  calls: string[];
  services: {
    seed(id: string, services?: Svc[], engine?: Record<string, unknown>): void;
    generate(id: string, count: number, engine?: Record<string, unknown>): void;
    upsert(id: string, service: Svc): void;
    upsertMany(id: string, services: Svc[]): void;
    remove(id: string, key: string): void;
    setEngine(id: string, engine: Record<string, unknown>): void;
    setUnsupported(id: string, version: string): void;
    forget(id: string): void;
    docsOpened: number;
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T) {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return ${source}`)(bridge);
  }, `(${run.toString()})(sim)`);
}

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

/** Aucun défilement horizontal de la page, à aucune largeur de 1 366 à 2 560. */
async function expectNoHorizontalScroll(page: Page) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    const overflow = await page.evaluate(() => {
      const root = document.documentElement;
      const content = document.querySelector<HTMLElement>(".layout__content");
      return {
        page: root.scrollWidth - root.clientWidth,
        content: content ? content.scrollWidth - content.clientWidth : 0,
      };
    });
    expect(overflow, `${width}`).toEqual({ page: 0, content: 0 });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const open = async (page: Page, id = "forge") => {
  await page.goto(`/?nodev#/servers/${id}/services`);
  await expect(
    page.getByRole("heading", { name: "Moteur de conteneurs", exact: true }),
  ).toBeVisible();
};

test("l'entrée « Services » est la deuxième de la navigation, pour les deux rôles", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const items = page.locator(".nav__item");
  await expect(items.nth(1)).toHaveText("Services");
  await items.nth(1).click();
  await expect(page).toHaveURL(/#\/servers\/forge\/services$/);
  await expect(page.getByRole("heading", { name: "Services", exact: true })).toBeVisible();
  // Lecture seule : l'entrée est là aussi (le serveur « salon »).
  await page.goto("/?nodev#/servers/salon/dashboard");
  await expect(page.locator(".nav__item").nth(1)).toHaveText("Services");
  await expect(page.locator(".nav__item")).toHaveText(["Tableau de bord", "Services", "Sécurité"]);
});

test("la carte du moteur d'abord, puis la liste triée, à 1366, 1920 et 2560", async ({ page }) => {
  await open(page);
  await expect(page.locator("[data-engine-state='ready']")).toBeVisible();
  await expect(page.locator("[data-engine-state]")).toContainText(
    "Docker Engine - Community 24.0.7",
  );
  await expect(page.locator("ul[aria-label='Services par état'] li")).toHaveCount(4);
  const rows = page.locator("[data-row]");
  await expect(rows.first()).toBeVisible();
  const names = await page.locator("[data-row] .name").allTextContents();
  const sorted = [...names].sort((a, b) => a.localeCompare(b, "fr", { sensitivity: "base" }));
  expect(names.map((name) => name.trim())).toEqual(sorted.map((name) => name.trim()));
  // Chaque état par un mot ET une icône.
  const badge = page
    .locator("[data-row][data-state='running']")
    .first()
    .locator("[data-state]")
    .first();
  await expect(badge.locator("svg")).toBeVisible();
  await expect(badge).toContainText("En marche");
  // La carte est avant la liste dans le DOM et à l'écran.
  const card = await page.locator("[data-engine-state]").boundingBox();
  const grid = await page.locator("[role='grid']").boundingBox();
  expect(card && grid && card.y < grid.y).toBe(true);
  await expectNoHorizontalScroll(page);
  await shoot(page, "services-liste");
});

test("une ligne change sur place : le défilement, le focus et la ligne repère ne bougent pas", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => s.services.generate("forge", 200));
  await expect(page.locator("[data-row]").first()).toBeVisible();
  const scroller = page.locator(".table__scroll");
  await scroller.evaluate((el) => {
    el.scrollTop = 52 * 60 + 10;
  });
  const anchor = page.locator("[data-row][data-key='c_0060']");
  await expect(anchor).toBeVisible();
  // Le focus clavier est sur une ligne.
  await page.locator("[data-row][data-key='c_0062']").focus();
  const before = await anchor.evaluate((el) => el.getBoundingClientRect().top);
  const scrollBefore = await scroller.evaluate((el) => el.scrollTop);

  // Une ligne AU-DESSUS de la zone visible passe en boucle (+28 px), une autre apparaît tout en haut, une autre change d'état dans la zone.
  await sim(page, (s) => {
    s.services.upsertMany("forge", [
      { key: "c_0003", name: "service-003", state: "restart_loop", exitCode: 1, restarts10m: 4 },
      { key: "c_0000b", name: "a-nouveau", state: "running" },
      { key: "c_0062", name: "service-062", state: "error", exitCode: 137 },
    ]);
  });
  await expect(page.locator("[data-row][data-key='c_0062']")).toHaveAttribute(
    "data-state",
    "error",
  );
  const after = await anchor.evaluate((el) => el.getBoundingClientRect().top);
  expect(Math.abs(after - before)).toBeLessThanOrEqual(1);
  const scrollAfter = await scroller.evaluate((el) => el.scrollTop);
  expect(scrollAfter).not.toBe(scrollBefore);
  // Le focus suit la clé.
  expect(await page.evaluate(() => document.activeElement?.getAttribute("data-key"))).toBe(
    "c_0062",
  );
});

test("200 services : la liste s'affiche vite, ne garde que les lignes visibles et se parcourt au clavier", async ({
  page,
}) => {
  await open(page);
  const started = Date.now();
  await sim(page, (s) => s.services.generate("forge", 200));
  await expect(page.locator("[data-row]").first()).toBeVisible();
  expect(Date.now() - started).toBeLessThan(1000);
  expect(await page.locator("[data-row]").count()).toBeLessThan(60);
  await expect(page.locator("[role='grid']")).toHaveAttribute("aria-rowcount", "201");
  // Un seul arrêt de tabulation dans la grille.
  await page.locator("[data-row][tabindex='0']").focus();
  await page.keyboard.press("ArrowDown");
  expect(await page.evaluate(() => document.activeElement?.getAttribute("data-key"))).toBe(
    "c_0001",
  );
  await page.keyboard.press("End");
  await expect(page.locator("[data-row][data-key='c_0199']")).toBeFocused();
  await page.keyboard.press("Home");
  await expect(page.locator("[data-row][data-key='c_0000']")).toBeFocused();
  await page.keyboard.press("Tab");
  expect(await page.evaluate(() => document.activeElement?.closest("[role='grid']"))).toBeNull();
  // Au moins 14 lignes visibles à 2560 (critère de la revue).
  await page.setViewportSize({ width: 2560, height: 1440 });
  expect(await page.locator("[data-row]").count()).toBeGreaterThanOrEqual(14);
  await shoot(page, "services-200");
});

test("un nom, une image de 255 caractères ne cassent pas la mise en page", async ({ page }) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [
      {
        key: "c_long",
        name: "n".concat("a".repeat(253), "z"),
        image: { ref: "registre.exemple/".concat("b".repeat(236)), tag: "9.9" },
        ports: [
          { hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" },
          { hostPort: 8081, containerPort: 81, protocol: "tcp", scope: "all" },
          { hostPort: 8082, containerPort: 82, protocol: "tcp", scope: "all" },
          { hostPort: 8083, containerPort: 83, protocol: "tcp", scope: "all" },
        ],
      },
      { key: "c_court", name: "court" },
    ]),
  );
  const row = page.locator("[data-row][data-key='c_long']");
  await expect(row).toBeVisible();
  const box = await row.boundingBox();
  expect(box?.height).toBeLessThanOrEqual(53);
  // Le texte complet est lu par le lecteur d'écran, la ligne n'a pas grandi et la page ne défile pas de côté.
  await expect(row).toHaveAttribute("aria-label", new RegExp(`^n${"a".repeat(253)}z, `));
  await expect(row.getByText("+1")).toBeVisible();
  await expectNoHorizontalScroll(page);
  await shoot(page, "services-long");
});

test("moteur arrêté : l'administrateur recopie la commande, la lecture seule lit l'explication", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "stopped" }),
  );
  await expect(page.locator("[data-engine-state='stopped']")).toBeVisible();
  await expect(page.getByText("Le moteur de conteneurs est installé mais arrêté.")).toBeVisible();
  await expect(page.getByText("À lancer sur le serveur, pas dans Hearth")).toBeVisible();
  const block = page.getByLabel("Commande à lancer sur le serveur");
  await expect(block).toHaveText("sudo systemctl enable --now docker");
  await expect(block).toHaveAttribute("tabindex", "0");
  await page.locator("[data-copy]").click();
  await expect(page.locator("[data-copy]")).toHaveText("Copié");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    "sudo systemctl enable --now docker",
  );
  // La liste précédente reste, grisée et datée.
  await expect(page.locator("[data-services-stale]")).toContainText("Liste du moteur d'il y a");
  await shoot(page, "services-moteur-arrete");

  // Lecture seule : même explication, aucune commande.
  await page.goto("/?nodev#/servers/salon/services");
  await sim(page, (s) => s.services.seed("salon", [], { state: "stopped" }));
  await expect(page.getByText("Le moteur de conteneurs est installé mais arrêté.")).toBeVisible();
  await expect(page.locator("pre")).toHaveCount(0);
  await expect(page.locator("[data-copy]")).toHaveCount(0);
  await expect(page.locator("[data-readonly-note]")).toHaveText(
    "Demande à un administrateur de ce serveur de régler cela.",
  );
});

test("aucun moteur : NixOS donne une ligne de configuration, une distribution inconnue renvoie à la documentation", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [], {
      state: "absent",
      name: null,
      version: null,
      host: { osId: "nixos", declarative: true },
    }),
  );
  await expect(page.getByLabel("Texte à ajouter à la configuration du serveur")).toHaveText(
    "virtualisation.docker.enable = true;",
  );
  await expect(page.getByText("À ajouter sur le serveur, pas dans Hearth")).toBeVisible();
  await shoot(page, "services-nixos");

  await sim(page, (s) =>
    s.services.seed("forge", [], { state: "absent", name: null, host: { osId: "gentoo" } }),
  );
  await expect(
    page.getByText("Hearth ne reconnaît pas la distribution de ce serveur."),
  ).toBeVisible();
  await expect(page.locator("pre")).toHaveCount(0);
  await page.locator("[data-engine-docs]").click();
  expect(await sim(page, (s) => s.services.docsOpened)).toBe(1);
});

test("moteur prêt sans service : l'état vide, sans invitation à déployer", async ({ page }) => {
  await open(page);
  await sim(page, (s) => s.services.seed("forge", []));
  await expect(page.getByRole("heading", { name: "Aucun service pour l'instant" })).toBeVisible();
  await expect(page.getByText("Déploie ton premier service")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Déployer/ })).toHaveCount(0);
  await shoot(page, "services-vide");
});

test("agent trop ancien : le message et le lien, et les mesures continuent", async ({ page }) => {
  await open(page);
  await sim(page, (s) => s.services.setUnsupported("forge", "0.1.0"));
  await expect(
    page.getByText("Ce serveur doit être mis à jour pour gérer les services (agent 0.1.0)."),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Voir la mise à jour de l'agent" })).toBeVisible();
  await expect(page.locator("[role='grid']")).toHaveCount(0);
  await shoot(page, "services-agent-ancien");
  // Les mesures (le tableau de bord) ne sont pas touchées.
  await page.locator(".nav__item").first().click();
  await expect(page.getByRole("heading", { name: "Processeur", exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Services" }).click();
  await page.getByRole("link", { name: "Voir la mise à jour de l'agent" }).click();
  await expect(page).toHaveURL(/#\/settings$/);
});

test("lien coupé : la dernière liste grisée et datée, « Vérifier à nouveau » désactivé, un seul mécanisme", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "stopped" }),
  );
  await expect(page.locator("[data-services-stale]")).toBeVisible();
  await sim(page, (s) => s.setState("forge", "offline"));
  await expect(page.locator("[data-stale='true']")).toBeVisible();
  await expect(page.getByText(/Vu il y a \d+ s/)).toBeVisible();
  await expect(page.locator("[data-services-stale]")).toHaveCount(0);
  await expect(page.locator("[data-row]")).toHaveCount(1);
  await expect(page.locator("[data-engine-recheck]")).toHaveAttribute("aria-disabled", "true");
  await shoot(page, "services-lien-coupe");
  await sim(page, (s) => s.setState("forge", "connected"));
  await expect(page.locator("[data-stale='true']")).toHaveCount(0);
});

test("« Vérifier à nouveau » relance la détection ; le retour du moteur est automatique", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "unresponsive" }),
  );
  await expect(page.getByText("Le moteur de conteneurs ne répond pas.")).toBeVisible();
  await page.locator("[data-engine-recheck]").click();
  expect(await sim(page, (s) => s.calls.includes("engine recheck"))).toBe(true);
  await sim(page, (s) => s.services.setEngine("forge", { state: "ready" }));
  await expect(page.locator("[data-engine-state='ready']")).toBeVisible();
  await expect(page.locator("[data-engine-recheck]")).toHaveCount(0);
});

test("1366 × 800 : la section tient dans la fenêtre sans barre de défilement de page", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1366, height: 800 });
  await open(page);
  await expect(page.locator("[data-row]").first()).toBeVisible();
  const overflow = await page.evaluate(() => {
    const root = document.documentElement;
    return { x: root.scrollWidth - root.clientWidth, y: root.scrollHeight - root.clientHeight };
  });
  expect(overflow).toEqual({ x: 0, y: 0 });
  // Aucun bouton coupé : tout bouton visible est entièrement dans la fenêtre.
  const cut = await page.evaluate(
    () =>
      [...document.querySelectorAll("button")].filter((button) => {
        const box = button.getBoundingClientRect();
        return box.width > 0 && (box.right > window.innerWidth || box.bottom > window.innerHeight);
      }).length,
  );
  expect(cut).toBe(0);
});
