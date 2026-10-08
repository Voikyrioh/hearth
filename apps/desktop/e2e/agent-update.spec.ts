import { expect, type Page, test } from "@playwright/test";

// HRT-17, lot interface : mettre à jour l'agent depuis l'app. Pont SIMULÉ (`window.__hearthSim`, aucun
// réseau) : l'agent y est piloté étape par étape. Les règles réelles (rôle, signature, somme,
// rétrogradation, adresse, une seule mise à jour à la fois, coupure attendue de la liaison) sont prouvées
// côté Rust contre un vrai agent (`tests/agent_update_runtime.rs`, `hearth-link/tests/agent_update.rs`).
// Ici : les parcours complets de l'écran. Captures dans e2e/screenshots/ (non commitées) : 1366, 1920
// et 2560 px, comme la revue UX.

type Step = "download" | "verify" | "install" | "restart" | "check";
type Sim = {
  setState(id: string, state: string): void;
  actionMode: "ok" | "cut";
  executeBeforeCut: boolean;
  calls: string[];
  agentUpdates: {
    seed(id: string, change: Record<string, unknown>): void;
    advance(id: string, step: Step, percent?: number | null): void;
    complete(id: string, outcome: string, reason?: string | null): void;
  };
};

const WIDTHS = [1366, 1920, 2560] as const;

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

/** Pilote l'agent simulé : `run` s'exécute dans la page avec le pont simulé. */
async function drive<A>(page: Page, run: (sim: Sim, arg: A) => void, arg?: A) {
  await page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      new Function("sim", "arg", `return (${source})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );
}

const card = (page: Page, id: string) => page.locator(`[data-agent-card][data-server="${id}"]`);
const states = (page: Page, id: string) =>
  card(page, id)
    .locator("[data-agent-steps] li")
    .evaluateAll((items) => items.map((item) => item.getAttribute("data-state")));
const pill = (page: Page, id: string) => card(page, id).locator(".pill");

async function open(page: Page) {
  await page.goto("/?nodev#/settings");
  await expect(card(page, "forge")).toBeVisible();
}

async function confirm(page: Page) {
  await card(page, "forge").locator("[data-agent-update-button]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog.getByRole("heading", { name: "Mettre à jour l'agent ?" })).toBeVisible();
  await expect(dialog).toContainText("Cette opération redémarrera l'agent brièvement.");
  // Remplacer le binaire de l'agent n'est jamais couvert par le délai de 5 minutes : le mot de passe est
  // toujours demandé (HRT-30).
  await dialog.getByLabel("Ton mot de passe").fill("Correct-Horse-9");
  await dialog.getByRole("button", { name: "Oui, mettre à jour" }).click();
}

test("disponible, confirmation, étapes, reconnexion sans erreur, résultat", async ({ page }) => {
  await open(page);
  const forge = card(page, "forge");
  await expect(forge.getByRole("heading", { level: 3 })).toHaveText("État du serveur : forge");
  await expect(forge.locator("[data-agent-versions]")).toContainText("Agent 0.1.0");
  await expect(forge.locator("[data-agent-tag]")).toHaveText("Mise à jour disponible");
  await expect(forge.locator("[data-agent-update-button]")).toBeEnabled();
  await shoot(page, "agent-disponible");

  await confirm(page);
  await expect(forge.locator("[data-agent-progress]")).toHaveText(
    "Mise à jour de l'agent en cours.",
  );
  await drive(page, (sim) => sim.agentUpdates.advance("forge", "download", 35));
  await expect(forge.locator('[data-step="download"]')).toContainText("Téléchargement : 35 %");
  expect(await states(page, "forge")).toEqual(["now", "later", "later", "later", "later"]);
  // HRT-46 (C43) : le bouton disparaît pendant la mise à jour.
  await expect(forge.locator("[data-agent-update-button]")).toHaveCount(0);
  await shoot(page, "agent-telechargement");

  for (const [step, expected] of [
    ["verify", ["done", "now", "later", "later", "later"]],
    ["install", ["done", "done", "now", "later", "later"]],
  ] as const) {
    await drive(page, (sim, s) => sim.agentUpdates.advance("forge", s as Step), step);
    expect(await states(page, "forge")).toEqual(expected);
  }

  // Le redémarrage : le lien tombe, « Reconnexion… », aucun message d'erreur.
  await drive(page, (sim) => sim.agentUpdates.advance("forge", "restart"));
  await expect(pill(page, "forge")).toHaveText("Reconnexion…");
  expect(await states(page, "forge")).toEqual(["done", "done", "done", "now", "later"]);
  await expect(forge).toContainText("Le lien avec le serveur sera coupé brièvement");
  await expect(forge.getByRole("alert")).toHaveCount(0);
  await expect(page.locator('[data-kind="error"]')).toHaveCount(0);
  await shoot(page, "agent-reconnexion");
  await drive(page, (sim) => sim.agentUpdates.advance("forge", "check"));
  expect(await states(page, "forge")).toEqual(["done", "done", "done", "done", "now"]);

  // Le résultat, au retour du lien.
  await drive(page, (sim) => sim.agentUpdates.complete("forge", "succeeded"));
  await expect(pill(page, "forge")).toHaveText("Connecté");
  await expect(forge.locator("[data-agent-result]")).toContainText(
    "Mise à jour de l'agent réussie. L'agent est en version 0.2.0.",
  );
  await expect(forge.locator("[data-agent-versions]")).toContainText("Agent 0.2.0");
  await expect(forge.locator("[data-agent-update-button]")).toHaveCount(0);
  await expect(forge.locator("[data-agent-tag]")).toHaveCount(0);
  await shoot(page, "agent-reussie");
});

test("retour arrière : le nouvel agent n'a pas répondu, l'ancienne version reste", async ({
  page,
}) => {
  await open(page);
  const forge = card(page, "forge");
  await confirm(page);
  for (const step of ["download", "verify", "install", "restart", "check"] as const) {
    await drive(page, (sim, s) => sim.agentUpdates.advance("forge", s as Step), step);
  }
  await drive(page, (sim) => sim.agentUpdates.complete("forge", "rolled_back", "no_answer"));
  await expect(pill(page, "forge")).toHaveText("Connecté");
  await expect(forge.locator("[data-agent-result]")).toHaveText(
    /Mise à jour de l'agent annulée\. Le nouvel agent n'a pas répondu\. Retour à la version précédente\./,
  );
  await expect(forge.locator("[data-agent-result]")).toHaveAttribute("data-tone", "warn");
  await expect(forge.locator("[data-agent-versions]")).toContainText("Agent 0.1.0");
  // La version reste proposée : on peut réessayer.
  await expect(forge.locator("[data-agent-update-button]")).toBeVisible();
  await shoot(page, "agent-retour-arriere");
});

test("échec avant tout échange : le serveur n'a pas accès à Internet", async ({ page }) => {
  await open(page);
  await confirm(page);
  await drive(page, (sim) => sim.agentUpdates.complete("forge", "failed", "unreachable"));
  await expect(card(page, "forge").locator("[data-agent-result]")).toContainText(
    "Le serveur n'a pas accès à Internet pour télécharger la mise à jour de l'agent.",
  );
  await expect(card(page, "forge").locator("[data-agent-result]")).toHaveAttribute(
    "data-tone",
    "crit",
  );
});

test("coupure du lien pendant la mise à jour : coupure normale, puis le résultat réel au retour", async ({
  page,
}) => {
  await open(page);
  const forge = card(page, "forge");
  await confirm(page);
  await drive(page, (sim) => sim.agentUpdates.advance("forge", "install"));
  // Le réseau tombe : « Reconnexion… », les étapes restent, aucune alarme.
  await drive(page, (sim) => sim.setState("forge", "reconnecting"));
  await expect(pill(page, "forge")).toHaveText("Reconnexion…");
  await expect(forge.locator("[data-agent-running]")).toBeVisible();
  await expect(forge.getByRole("alert")).toHaveCount(0);
  await expect(page.locator('[data-kind="error"]')).toHaveCount(0);
  // La mise à jour s'est terminée chez l'agent pendant la coupure.
  await drive(page, (sim) => {
    sim.agentUpdates.seed("forge", {
      progress: null,
      current: "0.2.0",
      last: {
        version: "0.2.0",
        previous: "0.1.0",
        outcome: "succeeded",
        reason: null,
        at: new Date().toISOString(),
        recent: true,
      },
    });
    sim.setState("forge", "connected");
  });
  await expect(pill(page, "forge")).toHaveText("Connecté");
  await expect(forge.locator("[data-agent-result]")).toContainText("réussie");
  await expect(forge.locator("[data-agent-versions]")).toContainText("Agent 0.2.0");
  await expect(forge.locator("[data-agent-running]")).toHaveCount(0);
});

test("action coupée avant la réponse : résultat inconnu, jamais rejouée", async ({ page }) => {
  await open(page);
  await drive(page, (sim) => {
    sim.actionMode = "cut";
    sim.executeBeforeCut = true;
  });
  await confirm(page);
  await expect(page.getByText("on ne sait pas si la mise à jour a démarré")).toBeVisible();
  const sent = await page.evaluate(
    () =>
      (window as unknown as { __hearthSim: Sim }).__hearthSim.calls.filter((call) =>
        /^agent-update \d/.test(call),
      ).length,
  );
  expect(sent).toBe(1);
  await drive(page, (sim) => sim.setState("forge", "connected"));
  await expect(card(page, "forge").locator("[data-agent-running]")).toBeVisible();
  const again = await page.evaluate(
    () =>
      (window as unknown as { __hearthSim: Sim }).__hearthSim.calls.filter((call) =>
        /^agent-update \d/.test(call),
      ).length,
  );
  expect(again).toBe(1);
});

test("lecture seule : bouton désactivé, infobulle exacte, aucune confirmation", async ({
  page,
}) => {
  await open(page);
  const salon = card(page, "salon");
  const button = salon.locator("[data-agent-update-button]");
  await expect(button).toBeVisible();
  await expect(button).toHaveAttribute("aria-disabled", "true");
  await button.hover();
  await expect(salon.getByRole("tooltip")).toHaveText(
    "Seul un administrateur peut mettre à jour l'agent",
  );
  await button.click({ force: true });
  await expect(page.locator("dialog[open]")).toHaveCount(0);
  await shoot(page, "agent-lecture-seule");
});

test("installation gérée : pas de bouton, l'explication", async ({ page }) => {
  await open(page);
  await drive(page, (sim) => sim.agentUpdates.seed("forge", { managed: true }));
  // La relecture suit un retour du lien.
  await drive(page, (sim) => sim.setState("forge", "offline"));
  await drive(page, (sim) => sim.setState("forge", "connected"));
  const forge = card(page, "forge");
  await expect(forge.locator("[data-agent-managed]")).toHaveText(
    "Cette installation est gérée par le système : l'agent ne se met pas à jour à distance. Mets-le à jour par la configuration du système.",
  );
  await expect(forge.locator("[data-agent-update-button]")).toHaveCount(0);
  await expect(forge.locator("[data-agent-tag]")).toHaveCount(0);
  await shoot(page, "agent-geree");
});

test("versions incompatibles : le message dit lequel mettre à jour, selon le rôle", async ({
  page,
}) => {
  await open(page);
  await page.evaluate(() => {
    const sim = (window as unknown as { __hearthSim: { publish: (...args: unknown[]) => void } })
      .__hearthSim;
    sim.publish("forge", "offline", { blocked: "incompatible_agent" });
    sim.publish("salon", "offline", { blocked: "incompatible_agent" });
  });
  await expect(card(page, "forge").locator("[data-agent-incompat]")).toHaveText(
    "Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent.",
  );
  await expect(card(page, "salon").locator("[data-agent-incompat]")).toHaveText(
    "Les versions du client et de l'agent ne sont pas compatibles. Demande à un administrateur de mettre à jour l'agent.",
  );
  await shoot(page, "agent-incompatible");
});

test("la liste des serveurs ne nomme que celui qui a une mise à jour", async ({ page }) => {
  await page.goto("/?nodev#/servers");
  await drive(page, (sim) => {
    sim.agentUpdates.seed("salon", { target: null });
    // La relecture suit un retour du lien.
    sim.setState("salon", "offline");
  });
  await drive(page, (sim) => sim.setState("salon", "connected"));
  const forge = page.locator('[data-server-row="forge"]');
  await expect(forge.locator("[data-update-available]")).toHaveText("Mise à jour disponible");
  await expect(page.locator('[data-server-row="salon"] [data-update-available]')).toHaveCount(0);
});
