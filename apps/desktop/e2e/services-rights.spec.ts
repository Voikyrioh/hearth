import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-78 : le droit « gérer les services » vu par le client, sur le pont SIMULÉ (aucun réseau). Quatre profils :
// A administrateur avec le droit (serveur « forge », compte marie), B administrateur sans, C lecture seule sans
// (serveur « salon », compte paul), D lecture seule avec. La chaîne réelle (agent, liaison, coquille) est prouvée
// côté Rust ; les formes du simulateur sont comparées à la coquille (`link/simulated-rights.test.ts`).
// Captures : 1366 et 2560, dans le dossier de `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
// Suffixe des fichiers de capture (`-r2` : deuxième tour de review).
const SUFFIX = process.env.HEARTH_SHOTS_SUFFIX ?? "";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;

type Sim = {
  setServiceRightOnServer(serverId: string, username: string, granted: boolean): void;
  setServiceRightsKnown(serverId: string, known: boolean): void;
  failRightsReads: boolean;
  rightsReads: string[];
  setState(id: string, state: string): void;
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
    await target.screenshot({ path: `${SHOTS}/${name}-${width}${SUFFIX}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

/** Charge l'application sur le tableau de bord, règle le simulateur, puis ouvre la page : le droit est lu à l'ouverture. */
async function openAs(
  page: Page,
  server: "forge" | "salon",
  route: "services" | "accounts",
  setup?: (sim: Sim) => void,
) {
  await page.goto(`/?nodev#/servers/${server}/dashboard`);
  await expect(page.locator(".nav__item").first()).toBeVisible();
  if (setup) await sim(page, setup);
  await page.evaluate(
    ([target]) => {
      window.location.hash = `#/servers/${target}`;
    },
    [`${server}/${route}`],
  );
}

const notice = (page: Page) => page.locator("[data-services-rights]");

test("A, administrateur avec le droit : la section ne dit rien du droit", async ({ page }) => {
  await openAs(page, "forge", "services");
  await expect(
    page.getByRole("heading", { name: "Moteur de conteneurs", exact: true }),
  ).toBeVisible();
  await expect(notice(page)).toHaveCount(0);
  await expect(page.getByText("Ton compte ne peut pas gérer les services")).toHaveCount(0);
});

test("B, administrateur sans le droit : la raison et la commande à copier, copiée exactement", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await openAs(page, "forge", "services", (s) =>
    s.setServiceRightOnServer("forge", "marie", false),
  );
  const block = notice(page);
  await expect(block).toBeVisible();
  await expect(block.locator("[data-rights-sentence]")).toHaveText(
    "Ton compte ne peut pas gérer les services de ce serveur.",
  );
  await expect(block.locator("[data-rights-follow]")).toHaveText(
    "Le droit se donne depuis le serveur. Lance cette commande sur le serveur :",
  );
  const shown = (await block.locator("pre").textContent()) ?? "";
  expect(shown).toBe("sudo hearth-agent account services marie --grant");
  await block.getByRole("button", { name: "Copier la commande" }).click();
  await expect(block.getByRole("button", { name: "Copiée" })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(shown);
  // Rien dans la section ne laisse croire qu'on donne le droit d'ici.
  await expect(page.locator("main input[type=checkbox], main [role=switch]")).toHaveCount(0);
  await shoot(block, page, "droit-raison-admin");
  await shoot(page, page, "droit-services-admin-sans-droit");
  // C5 : à 2 560 le bandeau est borné (`--panel-max`), il ne s'étire plus sur toute la page.
  await page.setViewportSize({ width: 2560, height: 1440 });
  const wide = await block.boundingBox();
  expect(wide?.width ?? Infinity).toBeLessThanOrEqual(700);
  expect(wide?.width ?? 0).toBeGreaterThan(300);
  await page.setViewportSize({ width: 1366, height: 800 });
});

test("C, lecture seule sans le droit : la raison et à qui s'adresser, jamais de commande", async ({
  page,
}) => {
  await openAs(page, "salon", "services", (s) => s.setServiceRightOnServer("salon", "paul", false));
  const block = notice(page);
  await expect(block.locator("[data-rights-sentence]")).toHaveText(
    "Ton compte ne peut pas gérer les services de ce serveur.",
  );
  await expect(block.locator("[data-rights-follow]")).toHaveText(
    "Demande à la personne qui administre ce serveur.",
  );
  await expect(block.locator("pre")).toHaveCount(0);
  await shoot(block, page, "droit-raison-lecture-seule");
});

test("D, lecture seule avec le droit : aucune raison", async ({ page }) => {
  await openAs(page, "salon", "services");
  await expect(
    page.getByRole("heading", { name: "Moteur de conteneurs", exact: true }),
  ).toBeVisible();
  await expect(notice(page)).toHaveCount(0);
});

test("agent ancien : le texte de la spécification, aucune commande", async ({ page }) => {
  await openAs(page, "forge", "services", (s) => s.setServiceRightsKnown("forge", false));
  await expect(notice(page).locator("[data-rights-sentence]")).toHaveText(
    "Ce serveur doit être mis à jour pour gérer les droits sur les services.",
  );
  await expect(notice(page).locator("pre")).toHaveCount(0);
  await shoot(notice(page), page, "droit-agent-ancien");
});

test("lecture du droit en échec, puis « Vérifier à nouveau »", async ({ page }) => {
  await openAs(page, "forge", "services", (s) => {
    s.failRightsReads = true;
  });
  await expect(notice(page).locator("[data-rights-sentence]")).toHaveText(
    "Impossible de lire ton droit sur les services. Réessaie.",
  );
  await sim(page, (s) => {
    s.failRightsReads = false;
  });
  await notice(page).getByRole("button", { name: "Vérifier à nouveau" }).click();
  await expect(notice(page)).toHaveCount(0);
});

test("la liste des comptes : le repère « Peut gérer les services » du compte qui a le droit", async ({
  page,
}) => {
  await openAs(page, "forge", "accounts", (s) => s.setServiceRightOnServer("forge", "paul", true));
  const rows = page.locator("tbody tr");
  await expect(rows).toHaveCount(3);
  await expect(page.locator('[data-account="marie"] [data-services-right]')).toHaveText(
    "Peut gérer les services",
  );
  await expect(page.locator('[data-account="paul"] [data-services-right]')).toHaveText(
    "Peut gérer les services",
  );
  await expect(page.locator('[data-account="lea"] [data-services-right]')).toHaveCount(0);
  // Aucune case ni commutateur, aucun bouton de ligne qui parle du droit.
  await expect(page.locator("table input, table [role=switch]")).toHaveCount(0);
  await shoot(page, page, "droit-comptes-liste");
});

test("le détail d'un compte : avec le droit (--revoke) et sans (--grant), au clavier", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await openAs(page, "forge", "accounts");
  const opener = page.getByRole("button", { name: "Détail du compte marie" });
  await opener.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.locator("[data-rights-value]")).toHaveText("Autorisée");
  await expect(dialog.locator("pre")).toHaveText(
    "sudo hearth-agent account services marie --revoke",
  );
  await shoot(dialog, page, "droit-detail-avec-droit");
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(opener).toBeFocused();

  await page.getByRole("button", { name: "Détail du compte paul" }).click();
  await expect(dialog.locator("[data-rights-value]")).toHaveText("Non autorisée");
  await expect(dialog.locator("[data-rights-help]")).toHaveText(
    "Ce droit se donne et se retire depuis le serveur, pas depuis cet écran.",
  );
  const shown = (await dialog.locator("pre").textContent()) ?? "";
  expect(shown).toBe("sudo hearth-agent account services paul --grant");
  await dialog.getByRole("button", { name: "Copier la commande" }).click();
  await expect(dialog.getByRole("button", { name: "Copiée" })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(shown);
  // Seule la copie et la fermeture : ni case, ni commutateur, ni champ.
  await expect(dialog.locator("input, select, textarea, [role=switch]")).toHaveCount(0);
  await expect(dialog.getByRole("button")).toHaveText(["Copier la commande", "Fermer"]);
  await shoot(dialog, page, "droit-detail-sans-droit");
  // « Fermer » rend le curseur au nom du compte qui avait ouvert le détail.
  await dialog.getByRole("button", { name: "Fermer" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Détail du compte paul" })).toBeFocused();
});

test("agent ancien : le détail d'un compte dit de mettre le serveur à jour, sans commande", async ({
  page,
}) => {
  await openAs(page, "forge", "accounts", (s) => s.setServiceRightsKnown("forge", false));
  await page.getByRole("button", { name: "Détail du compte paul" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.locator("[data-rights-value]")).toHaveText(
    "Ce serveur doit être mis à jour pour gérer les droits sur les services.",
  );
  await expect(dialog.locator("pre")).toHaveCount(0);
  await shoot(dialog, page, "droit-detail-agent-ancien");
});

test("le filtre du journal d'activité propose « Gestion des services » et « Droits sur les services »", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/audit");
  const kind = page.locator(".multi", { hasText: "Type d'action" });
  await kind.getByRole("button").click();
  await expect(kind.getByText("Gestion des services", { exact: true })).toBeVisible();
  await expect(kind.getByText("Droits sur les services", { exact: true })).toBeVisible();
});
