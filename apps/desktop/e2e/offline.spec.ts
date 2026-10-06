import { expect, type Page, test } from "@playwright/test";

// HRT-12 : l'app reste utilisable hors ligne. Pont SIMULÉ (aucun réseau) ; les durées (3 s, 30 s)
// et la reconnexion elle-même sont prouvées côté Rust contre un vrai agent et un mandataire à
// pannes (`crates/hearth-link/tests/fault_proxy.rs`, `apps/desktop/src-tauri/tests/link_runtime.rs`).
// Ici : ce que l'écran montre pour chaque état, et ce qu'il ne montre jamais (fenêtre bloquante).

type State = "connected" | "reconnecting" | "offline" | "session_expired" | "access_revoked";
type Extra = { reason?: string; failedAttempts?: number };

type Sim = {
  publish(id: string, state: string, extra?: Extra): void;
  setState(id: string, state: string): void;
  emitOperation(event: { opId: string; serverId: string; outcome: string }): void;
  addAgent(agent: unknown): void;
  actionMode: "ok" | "cut";
  lastUnknownOpId: string | null;
};

async function publish(page: Page, serverId: string, state: State, extra: Extra = {}) {
  await page.evaluate(
    ([id, next, more]) => {
      (window as unknown as { __hearthSim: Sim }).__hearthSim.publish(
        id as string,
        next as string,
        more as Extra,
      );
    },
    [serverId, state, extra],
  );
}

async function noBlockingWindow(page: Page) {
  await expect(page.locator('[role="dialog"], [role="alertdialog"], dialog[open]')).toHaveCount(0);
}

const pillOf = (page: Page) => page.locator(".head").getByRole("status");

test("une coupure courte ne change rien à l'écran", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  const pill = pillOf(page);
  await expect(pill).toHaveText("Connecté");
  // Moins de 3 s : la bibliothèque n'annonce rien de neuf (même état, nouvelle mesure).
  for (let n = 0; n < 3; n += 1) await publish(page, "forge", "connected");
  await expect(pill).toHaveText("Connecté");
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await expect(page.locator(".toast")).toHaveCount(0);
  await noBlockingWindow(page);
});

test("reconnexion : discrète, sans bandeau ni fenêtre, actions expliquées", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await publish(page, "forge", "reconnecting");
  await expect(pillOf(page)).toHaveText("Reconnexion…");
  await expect(page.getByText("Serveur hors ligne.")).toHaveCount(0);
  await expect(page.locator(".toast")).toHaveCount(0);
  await noBlockingWindow(page);
  const action = page.getByRole("button", { name: "Ajouter un compte" });
  await expect(action).toHaveAttribute("aria-disabled", "true");
  await action.focus();
  await expect(page.getByRole("tooltip")).toHaveText(
    "Indisponible pendant la reconnexion au serveur.",
  );
  // Les dernières données restent là, désaturées, avec leur âge.
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByText(/^Vu il y a \d+ s$/)).toBeVisible();
});

test("hors ligne : bandeau, données périmées, retour du lien", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await publish(page, "forge", "offline");
  await expect(pillOf(page)).toHaveText("Hors ligne");
  await expect(
    page.getByText(
      /Serveur hors ligne\. Dernier contact à \d\dh\d\d\. Nouvelle tentative automatique en cours\./,
    ),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Réessayer maintenant" })).toBeVisible();
  // Opacité des données périmées (transition de 200 ms comprise) : environ 60 %.
  const body = page.locator('[data-stale="true"] > div').first();
  const opacity = () => body.evaluate((el) => Number(getComputedStyle(el).opacity));
  await expect.poll(opacity).toBeLessThan(0.7);
  expect(await opacity()).toBeGreaterThan(0.5);
  await noBlockingWindow(page);
  // Le lien revient tout seul.
  await publish(page, "forge", "connected");
  await expect(pillOf(page)).toHaveText("Connecté");
  await expect(page.getByText("Serveur hors ligne.")).toHaveCount(0);
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
});

test("l'état de chaque serveur est indépendant", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await publish(page, "forge", "offline");
  await expect(pillOf(page)).toHaveText("Hors ligne");
  await expect(page.locator('a[data-server="salon"] [role=img]')).toHaveAttribute(
    "aria-label",
    "nas-salon, Connecté",
  );
  await page.locator('a[data-server="salon"]').click();
  await expect(pillOf(page)).toHaveText("Connecté");
  await expect(page.getByText("Serveur hors ligne.")).toHaveCount(0);
  await page.locator('a[data-server="forge"]').click();
  await expect(pillOf(page)).toHaveText("Hors ligne");
});

test("action lancée à la coupure : résultat inconnu, jamais rejouée, issue au retour", async ({
  page,
}) => {
  await page.goto("/#/servers/forge/dashboard");
  const launch = page.locator("[data-sim-action]");
  await page.evaluate(() => {
    (window as unknown as { __hearthSim: Sim }).__hearthSim.actionMode = "cut";
  });
  await expect(launch).not.toHaveAttribute("aria-disabled", "true");
  await launch.click();
  await expect(page.locator(".toast")).toHaveCount(2);
  await expect(page.locator(".toast").first()).toContainText(
    "Le résultat de cette action n'est pas connu.",
  );
  await expect(page.locator(".toast").nth(1)).toContainText(
    "Vérifie l'état du serveur, puis relance l'action si besoin.",
  );
  // Le lien est tombé : le bouton est désactivé, l'action ne repart pas toute seule.
  await expect(pillOf(page)).toHaveText("Reconnexion…");
  await expect(launch).toHaveAttribute("aria-disabled", "true");
  await noBlockingWindow(page);
  // Le lien revient : la bibliothèque a lu l'opération et annonce son issue.
  await page.evaluate(() => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    sim.setState("forge", "connected");
    sim.emitOperation({
      opId: sim.lastUnknownOpId ?? "",
      serverId: "forge",
      outcome: "not_executed",
    });
  });
  await expect(
    page.locator(".toast").filter({ hasText: "Non exécuté. Tu peux relancer." }),
  ).toBeVisible();
});

test("session expirée : un panneau non bloquant, mot de passe refusé puis accepté", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.evaluate(() => {
    (window as unknown as { __hearthSim: Sim }).__hearthSim.addAgent({
      host: "192.168.1.120",
      fingerprint: "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90",
      users: { marie: { password: "Correct-Horse-9", role: "admin" } },
    });
  });
  await publish(page, "forge", "session_expired", { reason: "expired" });
  await expect(pillOf(page)).toHaveText("Session expirée");
  await expect(page.getByText("Ta session a expiré.", { exact: true })).toBeVisible();
  await expect(page.getByText("Rentre ton mot de passe pour reprendre.")).toBeVisible();
  await noBlockingWindow(page);
  // Les dernières données restent visibles, grisées ; les actions sont désactivées.
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Ajouter un compte" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await page.getByLabel("Mot de passe", { exact: true }).fill("Mauvais-1");
  await page.getByRole("button", { name: "Me reconnecter" }).click();
  await expect(page.getByText("Identifiant ou mot de passe incorrect.")).toBeVisible();
  await expect(page.getByText("Ta session a expiré.", { exact: true })).toBeVisible();
  await page.getByLabel("Mot de passe", { exact: true }).fill("Correct-Horse-9");
  await page.getByRole("button", { name: "Me reconnecter" }).click();
  await expect(pillOf(page)).toHaveText("Connecté");
  await expect(page.getByText("Ta session a expiré.", { exact: true })).toHaveCount(0);
});

test("accès révoqué : explication, pas de reconnexion automatique, un autre compte", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await publish(page, "forge", "access_revoked", { reason: "revoked" });
  await expect(pillOf(page)).toHaveText("Accès révoqué");
  await expect(page.getByText("Ton compte n'est plus accessible.", { exact: true })).toBeVisible();
  await expect(page.getByText("Connecte-toi avec un compte valide.")).toBeVisible();
  await expect(page.getByLabel("Mot de passe", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Ajouter un compte" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await noBlockingWindow(page);
  await page.getByRole("button", { name: "Utiliser un autre compte" }).click();
  await expect(page.getByLabel("Identifiant")).toHaveValue("");
  await expect(page.getByLabel("Mot de passe", { exact: true })).toBeVisible();
});

test("coupures répétées : une seule notification, son compteur monte", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  for (let n = 1; n <= 10; n += 1) {
    await publish(page, "forge", n > 4 ? "offline" : "reconnecting", { failedAttempts: n });
  }
  await expect(page.locator(".toast")).toHaveCount(1);
  await expect(page.locator(".toast")).toContainText("forge : Reconnexion échouée 10 fois.");
  await noBlockingWindow(page);
  await publish(page, "forge", "connected");
  await expect(page.locator(".toast")).toHaveCount(0);
});

test("réglages : l'option de notifications du lien est proposée", async ({ page }) => {
  await page.goto("/?nodev#/settings");
  await expect(page.getByRole("heading", { name: "Notifications" })).toBeVisible();
  await expect(
    page.getByText("Notifier quand un serveur devient hors ligne ou revient"),
  ).toBeVisible();
  await expect(
    page.getByText("Une notification Windows au plus par minute et par serveur."),
  ).toBeVisible();
});

test("mise à jour du client disponible ET serveur hors ligne : deux bandeaux empilés, sans recouvrement", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.evaluate(() => {
    const update = (window as unknown as { __hearthUpdateSim: { setFeed(r: unknown): void } })
      .__hearthUpdateSim;
    update.setFeed({ version: "1.1.0", notes: "Corrections" });
  });
  // « Vérifier maintenant » est aux réglages : on annonce la version depuis la page des réglages, puis
  // on revient sur le serveur (le bandeau est global).
  await page.goto("/?nodev#/settings");
  await page.getByRole("button", { name: "Vérifier maintenant" }).click();
  await expect(page.locator("[data-update-banner]")).toBeVisible();
  await page.goto("/?nodev#/servers/forge/accounts");
  await publish(page, "forge", "offline");
  const update = page.locator("[data-update-banner]");
  const offline = page.getByText(/Serveur hors ligne\./);
  await expect(update).toBeVisible();
  await expect(offline).toBeVisible();
  for (const width of [1366, 1920]) {
    await page.setViewportSize({ width, height: 900 });
    const a = await update.boundingBox();
    const b = await page.locator(".banner").boundingBox();
    expect(a && b).toBeTruthy();
    if (a && b) {
      // Empilés : celui de la mise à jour (global) au-dessus, celui du lien dans la page, sans recouvrement.
      expect(a.y + a.height).toBeLessThanOrEqual(b.y + 1);
      expect(Math.abs(a.x - b.x)).toBeLessThan(400);
    }
    await expect(page.locator(".head").getByRole("status")).toHaveText("Hors ligne");
  }
  // Deux régions « status » (polies), aucune région « alert » : l'une ne coupe pas la parole à l'autre.
  await expect(page.locator('[role="alert"]')).toHaveCount(0);
  await page.screenshot({ path: "e2e/screenshots/maj-et-hors-ligne-1366.png" });
});
