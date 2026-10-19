import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-65 : démarrer, arrêter, redémarrer un service depuis le client, sur le pont SIMULÉ (aucun réseau ; la chaîne
// réelle agent, liaison, coquille est prouvée côté Rust). Serveur « forge » : administrateur avec le droit.
// Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  serviceOps: { accept(serverId: string, patch: Record<string, unknown>, by?: string): unknown };
  finishGesture(serverId: string, key: string): void;
  gestures: Array<{ key: string; gesture: string }>;
};

const sim = <T>(page: Page, run: (sim: Sim) => T) =>
  page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString());

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
      { key: "h_terraria", name: "terraria", origin: "hearth", graceS: 120 },
      { key: "h_robot", name: "robot-discord", origin: "hearth", state: "stopped" },
      {
        key: "c_vieux",
        name: "ancien-service",
        origin: "external",
        adopted: { by: "marie", at: "2026-10-13T08:00:00Z" },
      },
    ]),
  );
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await expect(page.locator("[data-row]").first()).toBeVisible();
}

const row = (page: Page, key: string) => page.locator(`[data-row][data-key="${key}"]`);

test("le menu de ligne, le détail et la confirmation d'arrêt (avec le délai réel)", async ({
  page,
}) => {
  await open(page);
  await shoot(page, page, "lifecycle-liste");
  await row(page, "h_terraria").locator("[data-row-menu-button]").click();
  await expect(page.locator("[data-row-menu-item]")).toHaveText(["Arrêter", "Redémarrer"]);
  await shoot(page, page, "lifecycle-menu");
  await page.locator('[data-row-menu-item="stop"]').click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Arrêter terraria ?");
  await expect(dialog).toContainText("au bout de 120 secondes");
  await shoot(page, page, "lifecycle-confirmation-arret");
  await dialog.getByRole("button", { name: "Annuler" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await sim(page, (s) => s.gestures.length)).toBe(0);
});

test("service adopté : l'avertissement est toujours visible", async ({ page }) => {
  await open(page);
  await row(page, "c_vieux").click();
  await page.locator('[data-lifecycle-action="stop"]').click();
  await expect(page.locator("[data-lifecycle-warning='adopted']")).toContainText(
    "créé hors de Hearth",
  );
  await shoot(page, page, "lifecycle-arret-adopte");
});

test("arrêter : acte envoyé, service verrouillé, issue annoncée une fois", async ({ page }) => {
  await open(page);
  await row(page, "h_terraria").click();
  await expect(page.locator("[data-lifecycle-action]")).toHaveText(["Arrêter", "Redémarrer"]);
  await shoot(page, page, "lifecycle-detail");
  await page.locator('[data-lifecycle-action="stop"]').click();
  const dialog = page.locator("dialog[open]");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Arrêter le service" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("[data-lifecycle-action]")).toHaveCount(0);
  await shoot(page, page, "lifecycle-en-cours");
  await sim(page, (s) => s.finishGesture("forge", "h_terraria"));
  await expect(page.getByText("Le service terraria est arrêté.")).toBeVisible();
  await expect(page.locator('[data-lifecycle-action="start"]')).toBeVisible();
  await shoot(page, page, "lifecycle-arrete");
});

test("une action d'un autre compte bloque les boutons, la raison est écrite dessous", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) =>
    s.serviceOps.accept("forge", { key: "h_terraria", kind: "backup" }, "claire"),
  );
  await row(page, "h_terraria").click();
  await expect(page.locator('[data-lifecycle-action="stop"]')).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await expect(page.locator("[data-lifecycle-reason]")).toHaveText(
    "Une action est déjà en cours sur ce service. Lancée par claire.",
  );
  await shoot(page, page, "lifecycle-verrou");
});
