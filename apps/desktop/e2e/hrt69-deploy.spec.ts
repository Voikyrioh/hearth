import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-69 : déployer une image depuis le client, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison,
// coquille est prouvée côté Rust). L'agent simulé juge comme le vrai (mêmes refus, mêmes constats de ports). Serveur
// « forge » : administrateur avec le droit de gérer les services. Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR`
// (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  deploys: {
    running(serverId: string): Array<{ operationId: string; key: string; name: string }>;
    progress(
      serverId: string,
      opId: string,
      percent: number | null,
      done?: number,
      total?: number,
    ): void;
    toStep(serverId: string, opId: string, step: string): void;
    succeed(serverId: string, opId: string, after?: Record<string, unknown>): void;
    fail(serverId: string, opId: string, cause: string): void;
    received: Array<{ request: Record<string, unknown> }>;
  };
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

async function open(page: Page) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) =>
    s.services.seed("forge", [
      {
        key: "h_base",
        name: "base",
        origin: "hearth",
        ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
      },
    ]),
  );
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await expect(page.locator("[data-row]").first()).toBeVisible();
  await page.locator("[data-deploy-open]").click();
  await expect(page.locator("[data-deploy-form]")).toBeVisible();
  await expect(page.getByLabel("Nom du service")).toBeFocused();
}

async function fill(page: Page) {
  await page.getByLabel("Nom du service").fill("web");
  await page.getByLabel("Image", { exact: true }).fill("nginx");
  await page.getByLabel("Version").fill("1.27");
}

async function launch(page: Page) {
  await page.locator("[data-deploy-submit]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Vérifiez avant de lancer");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Déployer le service" }).click();
  await expect(page.locator("[data-deploy-progress]")).toBeVisible();
}

const running = async (page: Page) => (await sim(page, (s) => s.deploys.running("forge")))[0];

test("de la section au service en marche : formulaire, récapitulatif, progression, succès", async ({
  page,
}) => {
  await open(page);
  await shoot(page, page, "deploy-formulaire-vide");
  await fill(page);
  await page.getByRole("button", { name: "Ajouter une variable" }).click();
  await page.getByLabel("Nom de la variable").fill("DB_PASSWORD");
  await page.getByLabel("Valeur", { exact: true }).fill("hunter2");
  // Le nom évoque un secret : la valeur est masquée d'office.
  await expect(page.getByLabel("Valeur", { exact: true })).toHaveAttribute("type", "password");
  await page.getByRole("button", { name: "Ajouter un port" }).click();
  await page.getByLabel("Port du serveur").fill("9090");
  await page.getByLabel("Port du service").fill("80");
  await expect(
    page.getByText("Ce port sera accessible à tout appareil qui peut joindre votre serveur."),
  ).toBeVisible();
  await shoot(page, page, "deploy-formulaire-rempli");

  await page.locator("[data-deploy-submit]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Vérifiez avant de lancer");
  await expect(dialog).toContainText("9090 vers 80 (TCP)");
  await expect(dialog).toContainText("1 variable");
  await expect(dialog).not.toContainText("hunter2");
  await shoot(dialog, page, "deploy-recapitulatif");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Déployer le service" }).click();

  const progress = page.locator("[data-deploy-progress]");
  await expect(progress).toContainText("Déploiement de web");
  await expect(progress.locator("[data-step='check'][data-state='now']")).toBeVisible();
  const op = await running(page);
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.progress("forge", first.operationId, 40, 400 * 1024 * 1024, 1024 ** 3);
  });
  await expect(progress.locator("[data-progress-text]")).toHaveText("40 % · 400 Mo sur 1 Go");
  await shoot(progress, page, "deploy-progression");
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.succeed("forge", first.operationId);
  });
  await expect(page.getByText("Le service web est en marche.")).toBeVisible();
  await expect(progress).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`services\\?service=${op?.key}`));
  // La saisie est partie avec la demande, jamais dans l'adresse.
  expect(page.url()).not.toContain("hunter2");
  const received = await sim(page, (s) => s.deploys.received[0]?.request);
  expect(received).toMatchObject({
    name: "web",
    image: "nginx",
    tag: "1.27",
    env: [{ name: "DB_PASSWORD", value: "hunter2" }],
  });
  await expect(page.locator(`[data-row][data-key="${op?.key}"]`)).toContainText("En marche");
});

test("refus de validation : nom, port tenu par un service et proposition du port libre", async ({
  page,
}) => {
  await open(page);
  await page.getByLabel("Nom du service").fill("-x");
  await page.getByLabel("Image", { exact: true }).fill("nginx");
  await page.getByLabel("Image", { exact: true }).blur();
  await page.getByLabel("Nom du service").blur();
  await expect(page.getByText("Le nom doit commencer par une lettre ou un chiffre")).toBeVisible();
  await page.getByLabel("Nom du service").fill("base");
  await expect(page.getByText("Ce nom est déjà pris")).toBeVisible();
  await page.getByLabel("Nom du service").fill("web");
  await page.getByRole("button", { name: "Ajouter un port" }).click();
  await page.getByLabel("Port du serveur").fill("8080");
  await page.getByLabel("Port du service").fill("80");
  const row = page.locator("[data-port-row]");
  await expect(row).toContainText("Ce port est pris par le service « base »");
  await expect(row).toContainText("Le port libre le plus proche est le 8081");
  const submit = page.locator("[data-deploy-submit]");
  await expect(submit).toHaveAttribute("aria-disabled", "true");
  await expect(page.locator("[data-deploy-reason]")).toHaveText(
    "Corrigez les champs signalés pour continuer.",
  );
  await shoot(page, page, "deploy-port-refuse");
  await row.getByRole("button", { name: "Utiliser le port 8081" }).click();
  await expect(row).not.toContainText("Ce port est pris par le service");
  await expect(submit).not.toHaveAttribute("aria-disabled", "true");
});

test("annulation pendant la récupération : rien n'est créé, le formulaire reste rempli", async ({
  page,
}) => {
  await open(page);
  await fill(page);
  await launch(page);
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.progress("forge", first.operationId, 20, 200, 1000);
  });
  const progress = page.locator("[data-deploy-progress]");
  await expect(progress.locator("[data-deploy-cancel]")).not.toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await progress.locator("[data-deploy-cancel]").click();
  const confirm = page.locator("dialog[open]").last();
  await expect(confirm).toContainText("Annuler le déploiement de web ?");
  // Le mot de passe du lancement vient d'ouvrir l'élévation de 5 minutes : annuler (acte couvert) ne le redemande pas.
  await expect(confirm.locator("[data-reauth-elevated]")).toContainText(
    "Mot de passe déjà confirmé",
  );
  await shoot(confirm, page, "deploy-annulation");
  await confirm.getByRole("button", { name: "Annuler le déploiement" }).click();
  await expect(page.getByText("Déploiement annulé. Rien n'a été créé.")).toBeVisible();
  await expect(progress).toHaveCount(0);
  await expect(page.getByLabel("Nom du service")).toHaveValue("web");
  expect(await sim(page, (s) => s.deploys.running("forge").length)).toBe(0);
});

test("après la création, l'annulation n'existe plus", async ({ page }) => {
  await open(page);
  await fill(page);
  await launch(page);
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.toStep("forge", first.operationId, "create");
  });
  const progress = page.locator("[data-deploy-progress]");
  await expect(progress.locator("[data-deploy-cancel]")).toHaveAttribute("aria-disabled", "true");
  await expect(progress).toContainText(
    "Trop tard pour annuler : le service est en cours de création.",
  );
});

test("erreur de l'agent : la cause est dite, « Modifier » rouvre le formulaire rempli et signale la version", async ({
  page,
}) => {
  await open(page);
  await fill(page);
  await launch(page);
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.fail("forge", first.operationId, "tag_not_found");
  });
  const progress = page.locator("[data-deploy-progress]");
  await expect(progress.locator("[data-deploy-failed]")).toHaveText(
    "Cette version n'existe pas pour cette image.",
  );
  await shoot(progress, page, "deploy-echec");
  await progress.getByRole("button", { name: "Modifier" }).click();
  await expect(progress).toHaveCount(0);
  await expect(page.getByLabel("Nom du service")).toHaveValue("web");
  await expect(page.getByLabel("Version")).toBeFocused();
  await expect(page.locator("[data-deploy-refusal]")).toContainText(
    "Cette version n'existe pas pour cette image.",
  );
});

test("fermer un formulaire rempli demande de confirmer ; la page ne déborde jamais en largeur", async ({
  page,
}) => {
  await open(page);
  await fill(page);
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHTS[width] });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    expect(overflow, `largeur ${width}`).toBe(false);
  }
  await page.setViewportSize({ width: 1366, height: 800 });
  await page.getByRole("button", { name: "Fermer" }).click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Abandonner votre saisie ?");
  await shoot(dialog, page, "deploy-abandon");
  await dialog.getByRole("button", { name: "Continuer la saisie" }).click();
  await expect(page.getByLabel("Nom du service")).toHaveValue("web");
  await page.getByRole("button", { name: "Fermer" }).click();
  await page.locator("dialog[open]").getByRole("button", { name: "Abandonner" }).click();
  await expect(page).toHaveURL(/services$/);
});
