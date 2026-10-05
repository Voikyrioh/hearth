import { expect, type Page, test } from "@playwright/test";

// Scénarios de la coquille avec le pont SIMULÉ. Captures dans e2e/screenshots/ (non commitées) :
// 1366, 1920 et 2560 px de large, comme la revue UX.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 900;

type LinkState = "connected" | "reconnecting" | "offline" | "session_expired" | "access_revoked";

async function setLink(page: Page, serverId: string, state: LinkState) {
  await page.evaluate(
    ([id, next]) => {
      const sim = (window as unknown as { __hearthSim: { setState(i: string, s: string): void } })
        .__hearthSim;
      sim.setState(id as string, next as string);
    },
    [serverId, state],
  );
}

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

test("accueil sans serveur", async ({ page }) => {
  await page.goto("/?servers=none&nodev");
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await expect(page.getByText("Ajoute ton premier serveur pour commencer.")).toBeVisible();
  const rail = page.getByRole("navigation", { name: "Serveurs" });
  await expect(rail.locator("a[data-server]")).toHaveCount(0);
  await expect(page.getByRole("navigation", { name: "Navigation du serveur" })).toHaveCount(0);
  // Un serveur absent ramène toujours à l'accueil.
  await page.goto("/?servers=none&nodev#/servers/forge/dashboard");
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await shoot(page, "accueil");
});

test("coquille avec deux serveurs", async ({ page }) => {
  await page.goto("/?nodev");
  await expect(page).toHaveURL(/#\/servers\/forge\/dashboard$/);
  const rail = page.getByRole("navigation", { name: "Serveurs" });
  await expect(rail.locator("a[data-server]")).toHaveCount(2);
  await expect(page.getByRole("heading", { level: 1, name: "Tableau de bord" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Connecté" })).toBeVisible();

  const nav = page.getByRole("navigation", { name: "Navigation du serveur" });
  await expect(nav.getByRole("link")).toHaveText([
    "Tableau de bord",
    "Comptes",
    "Journal d'activité",
  ]);

  // Le serveur en lecture seule n'a ni « Comptes » ni « Journal d'activité ».
  await rail.locator('a[data-server="salon"]').click();
  await expect(page).toHaveURL(/#\/servers\/salon\/dashboard$/);
  await expect(nav.getByRole("link")).toHaveText(["Tableau de bord"]);
  await page.goto("/?nodev#/servers/salon/accounts");
  await expect(page).toHaveURL(/#\/servers\/salon\/dashboard$/);

  await rail.locator('a[data-server="forge"]').click();
  await shoot(page, "coquille-connecte");
});

test("connecté, reconnexion, hors ligne puis retour", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  const pill = page.locator(".head").getByRole("status");
  const action = page.getByRole("button", { name: "Ajouter un compte" });
  await expect(pill).toHaveText("Connecté");
  await expect(action).not.toHaveAttribute("aria-disabled", "true");
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);

  await setLink(page, "forge", "reconnecting");
  await expect(pill).toHaveText("Reconnexion…");
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByText("Serveur hors ligne.")).toHaveCount(0);

  await setLink(page, "forge", "offline");
  await expect(pill).toHaveText("Hors ligne");
  await expect(
    page.getByText(/Serveur hors ligne\. Dernier contact à \d\dh\d\d\. Nouvelle tentative/),
  ).toBeVisible();
  await expect(page.getByText(/^Vu il y a \d+ s$/)).toBeVisible();
  await expect(action).toHaveAttribute("aria-disabled", "true");
  // L'explication s'affiche au focus clavier (infobulle), pas par `title`.
  await action.focus();
  await expect(page.getByRole("tooltip")).toHaveText(
    "Indisponible tant que le serveur est hors ligne.",
  );
  await expect(action).toHaveAttribute("aria-describedby", /.+/);
  // `force` : Playwright refuse de cliquer un contrôle `aria-disabled`, on veut justement tenter.
  await action.click({ force: true });
  await expect(page.locator(".toast")).toHaveCount(0);
  // L'état de l'autre serveur ne change pas.
  await expect(page.locator('a[data-server="salon"] [role=img]')).toHaveAttribute(
    "aria-label",
    "nas-salon, Connecté",
  );
  await shoot(page, "coquille-hors-ligne");

  // « Réessayer maintenant » : reconnexion, puis retour.
  await page.getByRole("button", { name: "Réessayer maintenant" }).click();
  await expect(pill).toHaveText("Reconnexion…");
  await expect(pill).toHaveText("Connecté", { timeout: 5000 });
  await expect(page.getByText("Serveur hors ligne.")).toHaveCount(0);
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await expect(action).not.toHaveAttribute("aria-disabled", "true");
  await action.click();
  await expect(page.locator(".toast")).toContainText("Bientôt disponible");

  for (const state of ["session_expired", "access_revoked"] as const) {
    await setLink(page, "forge", state);
    await expect(pill).toHaveText(
      state === "session_expired" ? "Session expirée" : "Accès révoqué",
    );
    await expect(action).toHaveAttribute("aria-disabled", "true");
  }
});

test("clavier : focus visible et flèches dans la barre et la navigation", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const rail = page.getByRole("navigation", { name: "Serveurs" });
  await rail.getByRole("link", { name: "Accueil" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(rail.locator('a[data-server="forge"]')).toBeFocused();
  const outline = await rail
    .locator('a[data-server="forge"]')
    .evaluate((el) => getComputedStyle(el).outlineStyle);
  expect(outline).toBe("solid");

  const nav = page.getByRole("navigation", { name: "Navigation du serveur" });
  await nav.getByRole("link", { name: "Tableau de bord" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(nav.getByRole("link", { name: "Comptes" })).toBeFocused();
  await page.keyboard.press("End");
  await expect(nav.getByRole("link", { name: "Journal d'activité" })).toBeFocused();
});

test("mouvement réduit : la pastille de reconnexion ne clignote pas", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await setLink(page, "forge", "reconnecting");
  const dot = page.locator(".pill__dot");
  await expect(dot).toBeVisible();
  const animated = () => dot.evaluate((el) => getComputedStyle(el).animationName);
  expect(await animated()).not.toBe("none");
  await page.emulateMedia({ reducedMotion: "reduce" });
  expect(await animated()).toBe("none");
});

test("notifications : 3 visibles au plus, compteur sur les répétitions", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const bridge = (kind: "done" | "not_executed" | "unknown") =>
    page.evaluate((outcome) => {
      const sim = (window as unknown as { __hearthSim: { emitOperation(e: unknown): void } })
        .__hearthSim;
      sim.emitOperation({ opId: "x", outcome });
    }, kind);
  await bridge("done");
  await bridge("not_executed");
  await bridge("unknown");
  await bridge("unknown");
  await bridge("unknown");
  const toasts = page.locator(".toast");
  await expect(toasts).toHaveCount(3);
  await expect(toasts.last()).toContainText("Résultat inconnu. Vérifie l'état du serveur.");
  await expect(toasts.last().locator(".toast__count")).toHaveText("3 fois");
  await toasts.first().getByRole("button", { name: "Fermer la notification" }).click();
  await expect(toasts).toHaveCount(2);
});
