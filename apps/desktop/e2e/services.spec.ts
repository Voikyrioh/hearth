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
  await expect(page.locator(".nav__item")).toHaveText([
    "Tableau de bord",
    "Services",
    "Catalogue",
    "Sécurité",
  ]);
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
  // Quatre ports : deux puces tiennent sur les deux lignes de la cellule, « +2 » dit le reste (jamais une puce coupée).
  await expect(row.getByText("+2")).toBeVisible();
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
    "Demandez à un administrateur de ce serveur de régler cela.",
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

test("moteur prêt sans service : l'état vide, avec le bouton « Déployer un service » (HRT-69)", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => s.services.seed("forge", []));
  await expect(page.getByRole("heading", { name: "Aucun service pour l'instant" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Déployer un service" })).toHaveCount(1);
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

test("moteur prêt, liste non relisible : la liste reste grisée et datée, 1366 × 800", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1366, height: 800 });
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [{ key: "c_1", name: "web" }], {
      listDegraded: true,
      listCurrentAt: new Date(Date.now() - 180_000).toISOString(),
    }),
  );
  await expect(page.locator("[data-services-degraded]")).toContainText(
    "Impossible de lire les services pour le moment.",
  );
  await expect(page.locator("[data-services-stale]")).toHaveText("Liste du moteur d'il y a 3 min");
  await expect(page.locator("[data-row]")).toHaveCount(1);
  await expect(page.locator("[data-engine-state='ready']")).toBeVisible();
  await page.screenshot({
    path: process.env.HRT62_SHOT ?? "e2e/screenshots/services-degrade-1366.png",
  });
  await sim(page, (s) => s.services.setEngine("forge", { listDegraded: false }));
  await expect(page.locator("[data-services-degraded]")).toHaveCount(0);
});

test("âge de la liste : l'écart entre les deux dates de l'agent, quelle que soit son horloge ; un lot qui ne republie pas la vue ne le remet pas à zéro", async ({
  page,
}) => {
  await open(page);
  // L'agent a 3 h d'avance sur ce poste : seules ses deux dates comptent, 3 min d'écart.
  await sim(page, (s) => {
    const ahead = Date.now() + 3 * 3_600_000;
    s.services.seed("forge", [{ key: "c_1", name: "web" }], {
      listDegraded: true,
      at: new Date(ahead).toISOString(),
      listCurrentAt: new Date(ahead - 180_000).toISOString(),
    });
  });
  await expect(page.locator("[data-services-stale]")).toHaveText("Liste du moteur d'il y a 3 min");
  // Un lot qui ne touche qu'un service : l'âge continue, il ne repart pas de zéro.
  await sim(page, (s) => s.services.upsert("forge", { key: "c_2", name: "api" }));
  await expect(page.locator("[data-row]")).toHaveCount(2);
  await expect(page.locator("[data-services-stale]")).toHaveText("Liste du moteur d'il y a 3 min");
  // Heure de l'agent présente, liste jamais lue : non relisible, sans âge.
  await sim(page, (s) =>
    s.services.setEngine("forge", { listCurrentAt: null, listReadAt: new Date().toISOString() }),
  );
  await expect(page.locator("[data-services-degraded]")).toBeVisible();
  await expect(page.locator("[data-services-stale]")).toHaveCount(0);
  // Agent plus ancien, sans heure : la date de sa dernière lecture, comme avant.
  await sim(page, (s) =>
    s.services.setEngine("forge", {
      at: null,
      listCurrentAt: null,
      listReadAt: new Date(Date.now() - 240_000).toISOString(),
    }),
  );
  await expect(page.locator("[data-services-stale]")).toHaveText("Liste du moteur d'il y a 4 min");
});

test("ports coupés par l'agent : le « +N » les compte sans les lister, le survol le dit", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [
      {
        key: "c_big",
        name: "beaucoup",
        ports: [
          { hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" },
          { hostPort: 8081, containerPort: 80, protocol: "tcp", scope: "all" },
        ],
        portsOmitted: 40,
      },
      { key: "c_none", name: "coupes", ports: [], portsOmitted: 7 },
    ]),
  );
  const big = page.locator("[data-key='c_big'] .cell--ports");
  await expect(big).toContainText("+40");
  await expect(big.locator("[data-more-ports]")).toHaveAttribute(
    "title",
    /et 40 autres non listés$/,
  );
  const none = page.locator("[data-key='c_none'] .cell--ports");
  await expect(none).toContainText("+7");
  await expect(none).not.toContainText("Aucun port");
  await expect(none.locator("[data-more-ports]")).toHaveAttribute("title", "7 non listés");
  await expectNoHorizontalScroll(page);
});

test("clavier : Tab atteint la liste après un défilement à la souris, Entrée ouvre le détail et ses ports", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => {
    s.services.generate("forge", 200);
    s.services.upsert("forge", {
      key: "c_many",
      name: "zz-beaucoup-de-ports",
      ports: [
        { hostPort: 30001, containerPort: 1, protocol: "tcp", scope: "all" },
        { hostPort: 30002, containerPort: 2, protocol: "tcp", scope: "all" },
        { hostPort: 30003, containerPort: 3, protocol: "tcp", scope: "all" },
        { hostPort: 30004, containerPort: 4, protocol: "tcp", scope: "all" },
        { hostPort: 30005, containerPort: 5, protocol: "tcp", scope: "all" },
      ],
    });
  });
  const rows = page.locator("[data-row]");
  await expect(rows.first()).toBeVisible();
  // Défilement à la souris : la ligne active d'origine sort de la zone dessinée.
  await page.locator(".table__scroll").hover();
  await page.mouse.wheel(0, 52 * 120);
  await expect(page.locator("[data-row][data-key='c_0000']")).toHaveCount(0);
  // Une ligne reste un arrêt de tabulation : Tab depuis le bouton « Vérifier » ou le début de la page l'atteint.
  await page.locator(".layout__content").evaluate((el) => (el as HTMLElement).focus?.());
  await page.keyboard.press("Tab");
  let guard = 0;
  while (
    guard < 40 &&
    !(await page.evaluate(() => document.activeElement?.hasAttribute("data-row")))
  ) {
    await page.keyboard.press("Tab");
    guard += 1;
  }
  expect(await page.evaluate(() => document.activeElement?.hasAttribute("data-row"))).toBe(true);
  // La ligne qui a le focus reste dans le document même si la molette l'éloigne encore.
  const key = await page.evaluate(() => document.activeElement?.getAttribute("data-key"));
  await page.mouse.wheel(0, 52 * 60);
  await expect(page.locator(`[data-row][data-key='${key}']`)).toHaveCount(1);
  // Fin, puis Entrée : le détail s'ouvre (HRT-63) et y donne tous les ports au clavier, Échap rend le focus à la ligne.
  await page.keyboard.press("End");
  await expect(page.locator("[data-row][data-key='c_many']")).toBeFocused();
  await expect(page.locator("[data-row][data-key='c_many']")).toHaveAttribute(
    "aria-label",
    /30005 vers 5 \(TCP\)/,
  );
  await page.keyboard.press("Enter");
  const detail = page.locator("[data-detail-ports]");
  await expect(detail).toBeVisible();
  for (const port of ["30001", "30002", "30003", "30004", "30005"]) {
    await expect(detail).toContainText(`${port} vers`);
  }
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-service-detail]")).toHaveCount(0);
  await expect(page.locator("[data-row][data-key='c_many']")).toBeFocused();
});

test("« Vérifier à nouveau » relance la détection ; le retour du moteur est automatique", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "unresponsive" }),
  );
  await expect(
    page.getByText("Le moteur de conteneurs ne répond pas.", { exact: true }),
  ).toBeVisible();
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
