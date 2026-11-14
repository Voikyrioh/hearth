import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-104 : un serveur de jeu au quotidien, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison, coquille
// est prouvée côté Rust). L'agent simulé juge comme le vrai. Serveur « forge » : administrateur avec le droit de gérer
// les services. Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";
const GAME_PASSWORD = "Sentinelle-pw-91";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  games: {
    register(serverId: string, key: string, view: Record<string, unknown>, pw: string | null): void;
  };
};

const sim = <T>(page: Page, run: (sim: Sim, arg: unknown) => T, arg?: unknown) =>
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

async function open(page: Page, sequenceTested = true, worldSaved = "by_sequence") {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) => {
    s.services.seed("forge", [
      { key: "h_terraria", name: "terraria", origin: "hearth", graceS: 30 },
      { key: "h_robot", name: "robot-discord", origin: "hearth" },
      {
        key: "c_vieux",
        name: "terraria-forge",
        origin: "external",
        adopted: { by: "marie", at: Date.parse("2026-10-13T08:00:00Z") },
      },
    ]);
  });
  await sim(
    page,
    (s, arg) => {
      const options = arg as { tested: boolean; saved: string; password: string };
      s.games.register(
        "forge",
        "h_terraria",
        {
          name: "Terraria",
          stop: { graceSeconds: 30, worldSaved: options.saved, sequenceTested: options.tested },
          ports: [
            { hostPort: 7777, protocol: "tcp", role: "game", changed: false },
            { hostPort: 7778, protocol: "udp", role: "query", changed: true },
          ],
        },
        options.password,
      );
    },
    { tested: sequenceTested, saved: worldSaved, password: GAME_PASSWORD },
  );
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await expect(page.locator("[data-row]").first()).toBeVisible();
}

/** La confirmation ne demande le mot de passe que si l'agent le demande (jamais en état normal, HRT-194). */
async function typePassword(dialog: Locator) {
  await page_wait(dialog);
  const field = dialog.locator("[data-reauth-field] input");
  if ((await field.count()) > 0) await field.fill(PASSWORD);
}
async function page_wait(dialog: Locator) {
  await expect(dialog.locator("[data-reauth-loading]")).toHaveCount(0);
}

const row = (page: Page, key: string) => page.locator(`[data-row][data-key="${key}"]`);

test("les quatre cartes d'un serveur de jeu, aucune pour un service adopté", async ({ page }) => {
  await open(page);
  await row(page, "h_terraria").click();
  await expect(page.locator("[data-game-address]")).toBeVisible();
  await expect(page.locator("[data-game-ports]")).toContainText("Les ports ont changé");
  await expect(page.locator("[data-game-password]")).toContainText("Révéler le mot de passe");
  await expect(page.locator("[data-game-stop]")).toContainText("Délai d'arrêt : 30 secondes");
  await shoot(page.locator("[data-service-detail]"), page, "game-cartes");
  await row(page, "c_vieux").click();
  await expect(page.locator("[data-game-address]")).toHaveCount(0);
  await expect(page.locator("[data-game-password]")).toHaveCount(0);
  await row(page, "h_robot").click();
  await expect(page.locator("[data-game-address]")).toHaveCount(0);
});

test("le mot de passe se révèle, se remasque et ne reste nulle part", async ({ page }) => {
  await open(page);
  await row(page, "h_terraria").click();
  const card = page.locator("[data-game-password]");
  await expect(card).not.toContainText(GAME_PASSWORD);
  await card.getByRole("button", { name: "Révéler le mot de passe" }).click();
  await expect(card.locator("[data-game-password-value]")).toHaveText(GAME_PASSWORD);
  await expect(card).toContainText("Mot de passe visible");
  await shoot(card, page, "game-mot-de-passe-visible");
  await row(page, "h_robot").click();
  await row(page, "h_terraria").click();
  await expect(page.locator("[data-game-password]")).not.toContainText(GAME_PASSWORD);
});

test("arrêter un jeu : la phrase de la fiche, puis la séquence", async ({ page }) => {
  await open(page);
  await row(page, "h_terraria").click();
  await page.locator('[data-lifecycle-action="stop"]').click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText(
    "Hearth va d'abord demander à Terraria d'enregistrer son monde",
  );
  await shoot(dialog, page, "game-arret-confirmation");
  await typePassword(dialog);
  await dialog.getByRole("button", { name: "Arrêter le service" }).click();
  const sequence = page.locator("[data-game-sequence]");
  await expect(sequence).toContainText("Arrêt de terraria");
  await expect(sequence.locator('[data-step="save_world"]')).toHaveAttribute("data-state", "now");
  await shoot(sequence, page, "game-sequence");
});

test("l'arrêt d'un jeu dont la suite n'est pas essayée demande de choisir, « Annuler » d'abord", async ({
  page,
}) => {
  await open(page, false);
  await row(page, "h_terraria").click();
  await page.locator('[data-lifecycle-action="stop"]').click();
  const confirm = page.locator("dialog[open]");
  await expect(confirm).toContainText(
    "Hearth ne peut pas demander à Terraria d'enregistrer son monde.",
  );
  await typePassword(confirm);
  await confirm.getByRole("button", { name: "Arrêter le service" }).click();
  const decision = page.locator("dialog[open]");
  await expect(decision).toContainText("Rien n'a été arrêté : le service tourne toujours.");
  const choices = decision.locator("[data-decision-choice]");
  await expect(choices.first()).toHaveAttribute("data-decision-choice", "cancel");
  await expect(choices.first()).toBeChecked();
  await shoot(decision, page, "game-decision");
  await decision.locator('[data-decision-choice="continue"]').check();
  await expect(decision.getByRole("button", { name: "Arrêter quand même" })).toBeVisible();
  await shoot(decision, page, "game-decision-continuer");
});
