import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-102 : le Catalogue et l'installation d'un serveur de jeu depuis une fiche, sur le pont SIMULÉ (aucun réseau ; la
// chaîne réelle agent, liaison, coquille est prouvée côté Rust). L'agent simulé juge comme le vrai (mêmes refus, mêmes
// codes). Terraria a la forme réelle d'une fiche ; la fiche d'essai porte tout ce qu'une fiche peut offrir (variantes,
// contrat, jeton, ressources mesurées). Serveur « forge » : administrateur avec le droit de gérer les services.
// Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  catalog: {
    addRich(): unknown;
    received: Array<{ request: Record<string, unknown> }>;
    get(id: string): { resources: { memory: { minBytes: number | null } } } | undefined;
  };
  deploys: {
    running(serverId: string): Array<{ operationId: string; key: string; name: string }>;
    progress(
      serverId: string,
      opId: string,
      percent: number | null,
      done?: number,
      total?: number,
    ): void;
    succeed(serverId: string, opId: string, after?: Record<string, unknown>): void;
    setCapacity(serverId: string, host: { cpus: number; memoryBytes: number } | null): void;
  };
  openedLinks: Array<Record<string, unknown>>;
};

const sim = <T>(page: Page, run: (sim: Sim) => T, arg?: unknown) =>
  page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      return new Function("sim", "arg", `return (${source as string})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );

async function shoot(target: Page | Locator, page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHTS[width] });
    await target.screenshot({ path: `${SHOTS}/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

async function open(page: Page, hash: string, setup?: (sim: Sim) => unknown) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  if (setup) await sim(page, setup);
  await page.evaluate((target) => {
    window.location.hash = target;
  }, hash);
}

test("du Catalogue au service en marche : liste, fiche, trois étapes, progression, succès", async ({
  page,
}) => {
  await open(page, "#/servers/forge/catalogue");
  const list = page.locator("[data-catalogue-list]");
  await expect(list).toContainText("Terraria");
  await expect(list).toContainText("Livrée avec Hearth");
  await expect(page.locator(".nav__item.router-link-active")).toHaveText("Catalogue");
  await shoot(page, page, "catalogue-liste");

  await page.getByRole("button", { name: "Voir la fiche de Terraria" }).click();
  await expect(page.locator("[data-page-title]")).toHaveText("Terraria");
  await expect(page.locator("[data-fiche-resources] [data-res-none]")).toContainText(
    "Hearth ne connaît pas les besoins de ce jeu",
  );
  await expect(page.locator("[data-fiche-page]")).not.toContainText(/compte exig/i);
  await shoot(page, page, "catalogue-fiche");

  await page.locator("[data-fiche-install]").click();
  await expect(page.locator("[data-page-title]")).toHaveText("Installer Terraria");
  await expect(page.locator('[data-field="name"] input')).toHaveValue("terraria");
  await page.getByLabel("Mot de passe du serveur").fill("hunter2-jeu");
  await page.getByLabel("Nombre de joueurs").fill("12");
  await shoot(page, page, "installer-reponses");
  await page.locator("[data-install-next]").click();

  await expect(page.locator("[data-install-resources]")).toContainText(
    "Hearth ne sait pas combien de processeur ce jeu demande.",
  );
  await shoot(page, page, "installer-ressources");
  await page.locator("[data-install-next]").click();

  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Vérifiez avant de lancer");
  await expect(dialog).toContainText("ryshe/terraria:vanilla-1.4.5.8");
  await expect(dialog).toContainText("7777 vers 7777 (TCP)");
  await expect(dialog).toContainText("••••••••");
  await expect(dialog).not.toContainText("hunter2-jeu");
  await expect(dialog).toContainText("2 réglage(s) différent(s) des valeurs proposées.");
  await shoot(dialog, page, "installer-recapitulatif");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Installer le serveur de jeu" }).click();

  const progress = page.locator("[data-deploy-progress]");
  await expect(progress).toContainText("Installation de terraria");
  const op = (await sim(page, (s) => s.deploys.running("forge")))[0];
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.progress("forge", first.operationId, 40, 400 * 1024 * 1024, 1024 ** 3);
  });
  await expect(progress.locator("[data-progress-text]")).toHaveText("40 % · 400 Mo sur 1 Go");
  await shoot(progress, page, "installer-progression");
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.succeed("forge", first.operationId);
  });
  await expect(page.getByText("Le service terraria est en marche.")).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`services\\?service=${op?.key}`));
  // La saisie est partie avec la demande, jamais dans l'adresse.
  expect(page.url()).not.toContain("hunter2");
  const received = await sim(page, (s) => s.catalog.received[0]?.request);
  expect(received).toMatchObject({
    name: "terraria",
    fiche: "terraria",
    version: 2,
    ports: [{ id: "game", hostPort: 7777 }],
  });
});

test("la fiche d'essai : preuves, variante, jeton, contrat à accepter dans sa fenêtre", async ({
  page,
}) => {
  await open(page, "#/servers/forge/catalogue/essai", (s) => s.catalog.addRich());
  await expect(page.locator('[data-res-row="memory"] [data-proof]')).toContainText(
    "Mesuré au repos",
  );
  await expect(page.locator('[data-res-row="cpu"] [data-proof]')).toContainText(
    "Mesuré par Hearth",
  );
  await expect(page.locator('[data-res-row="disk"] [data-proof]')).toContainText(
    "Chiffre de l'éditeur",
  );
  await expect(page.locator("[data-fiche-downloads]")).toContainText("environ 6 Go");
  await shoot(page, page, "fiche-essai-preuves");

  await page.locator("[data-fiche-install]").click();
  await expect(page.locator("[data-variant-card]")).toHaveCount(2);
  await expect(page.locator("[data-variant-frozen]")).toContainText("Ne pourra plus être changée");
  // Le jeton est obligatoire : sans lui, on ne continue pas.
  await expect(page.locator("[data-question-token-needed]")).toBeVisible();
  await page.getByLabel("Jeton de serveur").fill("jeton-du-serveur");
  await shoot(page, page, "fiche-essai-reponses");
  await page.locator("[data-install-next]").click();
  await expect(page.locator("[data-install-resources]")).toContainText(
    "Recommandé : 4 Go (mesuré au repos).",
  );
  await page.locator("[data-install-next]").click();

  const dialog = page.locator("dialog[open]").first();
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Installer le serveur de jeu" }).click();
  // Le contrat a sa fenêtre : rien n'est envoyé tant que « J'accepte » n'est pas cliqué.
  const contract = page.locator("[data-agreement-dialog]");
  await expect(contract).toContainText("Contrat de licence de l'éditeur");
  await expect(contract).toContainText("Lisez le contrat avant de répondre.");
  await shoot(contract, page, "fiche-essai-contrat");
  expect(await sim(page, (s) => s.catalog.received.length)).toBe(0);
  await contract.getByRole("button", { name: "Lire le contrat" }).click();
  expect(await sim(page, (s) => s.openedLinks)).toEqual([
    { serverId: "forge", ficheId: "essai", kind: "agreement", key: "licence" },
  ]);
  await contract.getByRole("button", { name: "J'accepte" }).click();
  await expect(page.locator("[data-deploy-progress]")).toContainText("Installation de jeu-d-essai");
  const received = await sim(page, (s) => s.catalog.received[0]?.request);
  expect(received).toMatchObject({ agreements: ["licence"], variant: "vanilla" });
});

test("une machine trop petite pour un minimum mesuré : dit avant toute saisie, aucune case pour passer outre", async ({
  page,
}) => {
  await open(page, "#/servers/forge/catalogue/essai/installer", (s) => {
    s.catalog.addRich();
    s.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 1024 ** 3 });
  });
  await expect(page.locator("[data-install-too-small]")).toContainText(
    "Votre serveur est trop petit pour ce jeu : il faut au moins 2 Go de mémoire et il en a 1 Go.",
  );
  await page.getByLabel("Jeton de serveur").fill("jeton");
  await expect(page.locator("[data-install-next]")).toHaveAttribute("aria-disabled", "true");
  await expect(page.locator("[data-install-reason]")).toHaveText(
    "Votre serveur est trop petit pour ce jeu.",
  );
  await shoot(page, page, "installer-machine-trop-petite");
});

test("« Installer avec les valeurs proposées » ouvre aussitôt la fenêtre, et fermer une saisie demande confirmation", async ({
  page,
}) => {
  await open(page, "#/servers/forge/catalogue/terraria");
  await page.locator("[data-fiche-install-proposed]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Vérifiez avant de lancer");
  // Sans rien avoir saisi, « Retour » ramène à la fiche, d'où l'on vient (revue UX du 24/10, constat 20).
  await dialog.getByRole("button", { name: "Retour" }).click();
  await expect(page.locator("[data-fiche-page]")).toBeVisible();
  await page.locator("[data-fiche-install]").click();
  await page.getByLabel("Message d'accueil").fill("Bienvenue");
  await page.locator("[data-install-close]").click();
  await expect(page.locator("dialog[open]")).toContainText("Abandonner votre saisie ?");
  await page.getByRole("button", { name: "Abandonner" }).click();
  await expect(page.locator("[data-page-title]")).toHaveText("Terraria");
});
