import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-84 : le suivi des opérations de service vu par le client, sur le pont SIMULÉ (aucun réseau, aucune
// opération réelle n'existe encore : le simulateur en fournit trois de démonstration). Quatre profils :
// A administrateur avec le droit (serveur « forge », compte marie), B administrateur sans, C lecture seule sans
// (serveur « salon », compte paul), D lecture seule avec. La chaîne réelle (agent, liaison, coquille) est prouvée
// côté Rust ; les formes du simulateur sont comparées à la coquille (`link/simulated-service-ops.test.ts`).
// Captures : 1366 et 2560, dans le dossier de `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const SUFFIX = process.env.HEARTH_SHOTS_SUFFIX ?? "";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  setServiceRightOnServer(serverId: string, username: string, granted: boolean): void;
  reauth: { setTrust(serverId: string, trust: "normal" | "alert" | "attack" | null): void };
  seedDemoOperations(serverId: string): void;
  publish(serverId: string, state: string): void;
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  serviceOps: {
    accept(serverId: string, patch: Record<string, unknown>, by?: string): unknown;
    finish(serverId: string, id: string, status: string): void;
    decisions: unknown[];
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
    await target.screenshot({ path: `${SHOTS}/${name}-${width}${SUFFIX}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

/** Charge l'application sur le tableau de bord, règle le simulateur, puis ouvre la section Services. */
async function openAs(page: Page, server: "forge" | "salon", setup: (sim: Sim) => void) {
  await page.goto(`/?nodev#/servers/${server}/dashboard`);
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, setup);
  await page.evaluate((target) => {
    window.location.hash = `#/servers/${target}/services`;
  }, server);
  await expect(page.locator("[data-operations-strip]")).toBeVisible();
}

const strip = (page: Page) => page.locator("[data-operations-strip]");
const rows = (page: Page) => strip(page).locator("li");

test("A, administrateur avec le droit : trois opérations, qui les a lancées, la décision active", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => s.seedDemoOperations("forge"));
  await expect(rows(page)).toHaveCount(3);
  await expect(rows(page).nth(0).locator("[data-operation-title]")).toHaveText(
    "Déploiement de terraria",
  );
  await expect(rows(page).nth(0).locator("[data-operation-by]")).toHaveText("Lancée par vous.");
  await expect(rows(page).nth(1).locator("[data-operation-by]")).toHaveText("Lancée par claire.");
  await expect(rows(page).nth(0).locator("[data-operation-progress]")).toContainText("40 %");
  // « Modification en cours » est compté « en transition ».
  await expect(page.getByText("2 en transition")).toBeVisible();
  const decide = strip(page).locator("[data-operation-decide]");
  await expect(decide).toHaveCount(1);
  await expect(decide).toHaveText("Répondre");
  await expect(decide).not.toHaveAttribute("aria-disabled", "true");
  await shoot(page, page, "hrt84-A-bande");
});

test("A : en état normal, répondre ne demande pas le mot de passe et l'opération repart (HRT-194)", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => s.seedDemoOperations("forge"));
  await strip(page).locator("[data-operation-decide]").click();
  const dialog = page.locator("dialog[open]");
  await dialog.getByLabel("Revenir à l'état d'avant").check();
  await expect(dialog.locator("[data-reauth-field]")).toHaveCount(0);
  await dialog.getByRole("button", { name: "Valider la réponse" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await sim(page, (s) => s.serviceOps.decisions.length)).toBe(1);
});

test("A : en état d'alerte, répondre demande le mot de passe puis l'opération repart", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => {
    s.seedDemoOperations("forge");
    s.reauth.setTrust("forge", "alert");
  });
  await strip(page).locator("[data-operation-decide]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Répondre pour robot-discord");
  // Aucune réponse n'est préchoisie.
  await expect(dialog.getByRole("button", { name: "Valider la réponse" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await dialog.getByLabel("Revenir à l'état d'avant").check();
  await shoot(dialog, page, "hrt84-A-decision");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Valider la réponse" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(strip(page).locator("[data-operation-decide]")).toHaveCount(0);
  expect(await sim(page, (s) => s.serviceOps.decisions.length)).toBe(1);
});

test("B, administrateur sans le droit : le bouton est désactivé avec sa raison", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => {
    s.setServiceRightOnServer("forge", "marie", false);
    s.seedDemoOperations("forge");
  });
  await expect(rows(page)).toHaveCount(3);
  const decide = strip(page).locator("[data-operation-decide]");
  await expect(decide).toHaveAttribute("aria-disabled", "true");
  await expect(strip(page).locator("[data-operation-reason]")).toHaveText(
    "Il faut le droit de gérer les services pour répondre.",
  );
  await shoot(page, page, "hrt84-B-bande");
});

test("C, lecture seule sans le droit : l'effacement se dit sans chemin ni progression", async ({
  page,
}) => {
  await openAs(page, "salon", (s) => {
    s.setServiceRightOnServer("salon", "paul", false);
    s.services.seed("salon", [
      { key: "c_web", name: "web" },
      { key: "h_terraria", name: "terraria", origin: "hearth" },
    ]);
    s.serviceOps.accept("salon", {
      key: "h_terraria",
      kind: "delete",
      step: "erase",
      percent: 30,
      itemsDone: 120,
    });
  });
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toHaveText(/Effacement des données de terraria en cours\./);
  await expect(rows(page).first().locator("[data-operation-progress]")).toHaveCount(0);
  await shoot(page, page, "hrt84-C-bande");
});

test("D, lecture seule avec le droit : la progression de l'effacement est visible", async ({
  page,
}) => {
  await openAs(page, "salon", (s) => {
    s.services.seed("salon", [
      { key: "c_web", name: "web" },
      { key: "h_terraria", name: "terraria", origin: "hearth" },
    ]);
    s.serviceOps.accept("salon", {
      key: "h_terraria",
      kind: "delete",
      step: "erase",
      percent: 30,
      itemsDone: 120,
    });
  });
  await expect(rows(page).first().locator("[data-operation-title]")).toHaveText(
    "Effacement des données de terraria",
  );
  await expect(rows(page).first().locator("[data-operation-progress]")).toHaveText(
    "30 % · 120 éléments effacés",
  );
  await shoot(page, page, "hrt84-D-bande");
});

test("le retour du lien : l'opération en cours reprend sans message, l'issue se dit une fois", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => s.seedDemoOperations("forge"));
  await sim(page, (s) => {
    s.publish("forge", "reconnecting");
    s.serviceOps.finish("forge", "01SIMDEMO000000000000DEPL", "succeeded");
  });
  await sim(page, (s) => s.publish("forge", "connected"));
  await expect(rows(page)).toHaveCount(2);
  const toasts = page.getByLabel("Notifications");
  await expect(toasts).toContainText("Le service terraria est en marche.");
  await expect(toasts.getByText("Le service terraria est en marche.")).toHaveCount(1);
  await shoot(page, page, "hrt84-retour-du-lien");
});

test("le détail d'un service tenu par une action dit qui l'a lancée et porte la carte", async ({
  page,
}) => {
  await openAs(page, "forge", (s) => s.seedDemoOperations("forge"));
  await rows(page).nth(0).locator("[data-operation-see]").click();
  const detail = page.locator("[data-service-detail]");
  await expect(detail.locator("[data-detail-busy]")).toContainText(
    "Une action est en cours sur ce service.",
  );
  // marie est le compte connecté : sa propre action se dit « par vous » (FIX:01M5A5VH5GR2M9PF9Y1SB04D65).
  await expect(detail.locator("[data-detail-busy]")).toContainText("Lancée par vous.");
  await expect(detail.locator("[data-operation]")).toContainText("Déploiement de terraria");
  await shoot(page, page, "hrt84-A-detail");
});
