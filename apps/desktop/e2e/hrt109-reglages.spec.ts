import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-109 : l'onglet « Réglages » d'un serveur de jeu et « Voir tous les réglages » de l'installation, sur le pont SIMULÉ
// (aucun réseau ; la chaîne réelle agent, liaison, coquille est prouvée côté Rust). L'agent simulé juge comme le vrai
// (révision, empreinte du fichier, écart, refus). Terraria a la forme réelle d'une fiche : 32 réglages, 7 thèmes.
// Serveur « forge » : administrateur avec le droit de gérer les services. Captures de 1 100 à 3 440 de large, dans
// `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité) ; aucun défilement horizontal sur ces largeurs.
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1100, 1366, 2560, 3440] as const;
const HEIGHT = 900;
const PASSWORD = "Correct-Horse-9";
const KEY = "h_terraria";
const SENTINEL = "SENTINELLE-mot-de-passe-9f3a";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  catalog: { get(id: string): unknown };
  gameSettings: {
    register(
      serverId: string,
      key: string,
      detail: unknown,
      answers?: Array<{ id: string; value: string }>,
    ): void;
    rewriteFile(serverId: string, key: string, text: string): void;
    fileText(serverId: string, key: string): string;
    received: Array<{ request: { origin: string; changes: Array<{ id: string }> } }>;
  };
  gestures: Array<{ key: string; gesture: string }>;
};

const sim = <T>(page: Page, run: (sim: Sim, arg?: unknown) => T, arg?: unknown) =>
  page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      return new Function("sim", "arg", `return (${source as string})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );

async function noHorizontalScroll(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
}

async function shoot(target: Page | Locator, page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await noHorizontalScroll(page);
    await target.screenshot({ path: `${SHOTS}/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

async function open(page: Page, rewritten?: string) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(
    page,
    (s) => {
      s.services.seed("forge", [
        { key: "h_terraria", name: "terraria", origin: "hearth", state: "running" },
        { key: "h_robot", name: "robot-discord", origin: "hearth", state: "running" },
      ]);
      const detail = s.catalog.get("terraria");
      s.gameSettings.register("forge", "h_terraria", detail, [
        { id: "password", value: "SENTINELLE-mot-de-passe-9f3a" },
        { id: "max_players", value: "12" },
      ]);
    },
    undefined,
  );
  if (rewritten !== undefined) {
    await sim(
      page,
      (s, text) => s.gameSettings.rewriteFile("forge", "h_terraria", text as string),
      rewritten,
    );
  }
  await page.evaluate((key) => {
    window.location.hash = `#/servers/forge/services?service=${key}&onglet=reglages`;
  }, KEY);
  await expect(page.locator("[data-settings-tab]")).toBeVisible();
  await expect(page.locator("[data-setting-theme]").first()).toBeVisible();
}

const row = (page: Page, id: string) => page.locator(`[data-setting="${id}"]`);

test("l'onglet range les réglages par thèmes en mots simples, du plus étroit au plus large", async ({
  page,
}) => {
  await open(page);
  await expect(page.getByRole("tab", { name: "Réglages" })).toBeVisible();
  await expect(page.locator("[data-setting-theme] h2")).toHaveText([
    "Ma partie",
    "Joueurs",
    "Monde",
    "Triche et sécurité",
  ]);
  await expect(row(page, "max_players")).toContainText("Nombre de joueurs");
  await expect(row(page, "max_players")).toContainText("Nom dans le jeu : maxplayers");
  await expect(row(page, "world_name")).toContainText("Figé : ne peut plus changer");
  // Un mot de passe n'est jamais lu ni affiché.
  await expect(row(page, "password")).toContainText("Défini. Saisissez une nouvelle valeur");
  await expect(page.locator("body")).not.toContainText(SENTINEL);
  await shoot(page.locator("[data-settings-tab]"), page, "reglages-courants");

  await page.getByRole("radio", { name: "Tout afficher" }).click();
  await expect(page.locator("[data-setting-theme]")).toHaveCount(7);
  await expect(page.locator("[data-setting]")).toHaveCount(32);
  await page.locator('[data-theme="journey"] summary').click();
  await expect(row(page, "journey_godmode")).toContainText("Invincibilité");
  await expect(row(page, "port")).toContainText("Imposé par la fiche");
  await shoot(page.locator("[data-settings-tab]"), page, "reglages-tout");
});

test("la recherche trouve sans tenir compte des accents ; « / » met le curseur dans le champ", async ({
  page,
}) => {
  await open(page);
  // Le curseur n'est dans aucun champ.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("/");
  await expect(page.locator("[data-settings-search] input")).toBeFocused();
  await page.keyboard.type("FLUIDITE");
  await expect(page.locator("[data-setting]")).toHaveCount(1);
  await expect(page.locator("[data-settings-found]")).toHaveText("1 réglage(s) trouvé(s).");
  await expect(row(page, "npc_stream")).toContainText("Fluidité des ennemis");
  await page.locator("[data-settings-search] input").fill("zzz");
  await expect(page.locator("[data-settings-none]")).toHaveText("Aucun réglage ne correspond.");
});

test("modifier, enregistrer avec le mot de passe, voir « À appliquer », puis redémarrer à la demande", async ({
  page,
}) => {
  await open(page);
  await row(page, "motd").locator("input").fill("Bienvenue chez nous");
  await expect(row(page, "motd")).toContainText("Modifié");
  await expect(page.locator("[data-settings-count]")).toHaveText("1 changement(s) à enregistrer.");
  await page.locator("[data-settings-save]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Les changements s'appliqueront au prochain démarrage.");
  await expect(dialog).toContainText("Message d'accueil");
  // La case « Redémarrer maintenant » est décochée.
  await expect(dialog.getByLabel("Redémarrer maintenant")).not.toBeChecked();
  await shoot(dialog, page, "reglages-enregistrer");
  await dialog.getByLabel("Votre mot de passe").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Enregistrer" }).click();
  await expect(page.locator("[data-settings-pending]")).toContainText(
    "À appliquer au prochain démarrage",
  );
  expect(await sim(page, (s) => s.gameSettings.fileText("forge", "h_terraria"))).toContain(
    "motd=Bienvenue chez nous",
  );
  // Rien n'est redémarré tant que la personne ne le demande pas : le bouton ouvre la confirmation habituelle.
  await page.locator("[data-settings-restart]").click();
  await expect(page.locator("dialog[open]")).toContainText("Redémarrer");
});

test("un mot de passe saisi part, ne revient jamais et s'efface de la page", async ({ page }) => {
  await open(page);
  await row(page, "password")
    .getByLabel("Mot de passe du serveur")
    .first()
    .fill("nouveau-secret-xyz");
  await page.locator("[data-settings-save]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Mot de passe du serveur (mot de passe modifié)");
  await expect(dialog).not.toContainText("nouveau-secret-xyz");
  await dialog.getByLabel("Votre mot de passe").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Enregistrer" }).click();
  await expect(page.locator("[data-settings-count]")).toHaveText("Aucun changement à enregistrer.");
  await expect(page.locator("body")).not.toContainText("nouveau-secret-xyz");
  const received = await sim(page, (s) => s.gameSettings.received.at(-1)?.request.changes);
  expect(received).toEqual([{ id: "password", action: "set", value: "nouveau-secret-xyz" }]);
});

test("le fichier réécrit par le jeu : un choix par réglage, « Garder la valeur du fichier » par défaut", async ({
  page,
}) => {
  await open(page, "# réécrit par le jeu\nmaxplayers=20\nmotd=Bonjour\nautre_ligne=1");
  const banner = page.locator("[data-file-changed]");
  await expect(banner).toContainText(
    "Le fichier a changé depuis la dernière fois que Hearth l'a écrit",
  );
  const drift = page.locator('[data-drift="max_players"]');
  await expect(drift).toContainText("12");
  await expect(drift).toContainText("20");
  await expect(drift.getByRole("radio", { name: "Garder la valeur du fichier" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.locator("[data-others]")).toContainText("autre_ligne");
  await shoot(page.locator("[data-settings-tab]"), page, "reglages-ecart");
  await drift.getByRole("radio", { name: "Remettre la mienne" }).click();
  await page.locator("[data-settings-save]").click();
  const dialog = page.locator("dialog[open]");
  await dialog.getByLabel("Votre mot de passe").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Enregistrer" }).click();
  await expect(page.locator("[data-file-changed]")).toHaveCount(0);
  const text = await sim(page, (s) => s.gameSettings.fileText("forge", "h_terraria"));
  expect(text).toContain("maxplayers=12");
  expect(text).toContain("autre_ligne=1");
});

test("l'import : coller, lire, voir le tableau, appliquer ; le texte s'efface à la fermeture", async ({
  page,
}) => {
  await open(page);
  await page.locator("[data-settings-import]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Le texte collé est envoyé au serveur pour être lu.");
  await dialog
    .locator("[data-import-text] textarea")
    .fill("motd=Importé\nlanguage=en/US\nport=1\nzzz=3");
  await dialog.getByRole("button", { name: "Lire ce texte" }).click();
  await expect(dialog.locator("[data-import-table]")).toContainText("Reconnu");
  await expect(dialog.locator("[data-import-table]")).toContainText(
    "Valeur que ce jeu ne connaît pas",
  );
  await expect(dialog).toContainText("Ce texte retire ou change un réglage dont Hearth a besoin");
  await shoot(dialog, page, "reglages-import");
  // Fermer sans appliquer : le texte est effacé.
  await dialog.getByRole("button", { name: "Annuler" }).click();
  await page.locator("[data-settings-import]").click();
  await expect(page.locator("dialog[open] [data-import-text] textarea")).toHaveValue("");
});

test("le fichier brut avertit, et ce qui attend la console est nommé", async ({ page }) => {
  await open(page);
  await page.locator("[data-settings-raw]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Hearth ne vérifie pas ce que vous écrivez");
  await expect(dialog.getByLabel("Je comprends")).not.toBeChecked();
  await dialog.getByRole("button", { name: "Fermer" }).click();
  await page.locator("[data-console-wait] summary").click();
  await expect(page.locator('[data-console-function="apply"]')).toContainText(
    "Cette fonction attend la console de Hearth, qui n'est pas encore livrée.",
  );
  await shoot(page.locator("[data-console-wait]"), page, "reglages-console");
});

test("un service qui n'est pas né d'une fiche dit « pas de réglages de jeu » à l'ouverture de l'onglet", async ({
  page,
}) => {
  await open(page);
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services?service=h_robot&onglet=reglages";
  });
  await expect(page.locator("[data-settings-absent]")).toContainText(
    "Ce service n'a pas de réglages de jeu.",
  );
});

test("« Voir tous les réglages » dans l'installation : replié, déplié dans la page, compté au récapitulatif", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/catalogue/terraria/installer");
  await expect(page.locator("[data-page-title]")).toHaveText("Installer Terraria");
  await expect(page.locator("[data-install-settings-body]")).toHaveCount(0);
  await page.locator("[data-install-settings-toggle]").click();
  const body = page.locator("[data-install-settings-body]");
  await expect(body).toContainText("28 réglages, dont 6 demandés plus haut.");
  await expect(body.locator("[data-setting]")).toHaveCount(22);
  await body.locator('[data-theme="world"] summary').click();
  await body.locator('[data-setting="seed"] input').fill("graine-du-monde");
  await expect(page.locator("[data-install-settings-changed]")).toHaveText(
    "1 réglage(s) différent(s) des valeurs proposées.",
  );
  await expect(body.locator("[data-install-settings-import]")).toBeDisabled();
  await shoot(page.locator("[data-install-answers]"), page, "installation-reglages");
});
