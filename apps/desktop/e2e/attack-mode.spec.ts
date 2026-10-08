import { expect, type Page, test } from "@playwright/test";

// HRT-26 : l'alerte « attaque probable », le mode attaque, la marque d'un serveur, le réglage « Alertes
// de sécurité » et la case « Garder ce poste reconnu ». Pont SIMULÉ (aucun réseau, alerte et mode
// pilotables) ; la preuve de la clé, le mot de passe, les refus de l'agent et les notifications Windows
// sont prouvés côté Rust contre un vrai agent (`crates/hearth-link/tests/attack_mode.rs`,
// `apps/desktop/src-tauri/tests/security_runtime.rs`, `security_alerts.rs`). Ici : les parcours de l'écran.

type Sim = {
  setState(id: string, state: string): void;
  calls: string[];
  lastKeepAddress: boolean | null;
  security: {
    setAlert(id: string, alert: { own: boolean; others?: number | null }): void;
    setMode(id: string, state: string, options?: { resumesInS?: number; lastEnd?: string }): void;
    setDevice(id: string, device: "proven" | "none", keyAtHand?: boolean): void;
    setSupported(id: string, supported: boolean): void;
  };
};

const WIDTHS = [1366, 1920, 2560] as const;
const GOOD = "Correct-Horse-9";

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const sim = (page: Page) => ({
  alert: (id: string, own: boolean, others?: number) =>
    page.evaluate(
      ([i, o, n]) =>
        (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setAlert(i as string, {
          own: o as boolean,
          others: (n as number | undefined) ?? null,
        }),
      [id, own, others],
    ),
  mode: (id: string, state: string, resumesInS?: number, lastEnd?: string) =>
    page.evaluate(
      ([i, s, r, e]) =>
        (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setMode(
          i as string,
          s as string,
          { resumesInS: r as number | undefined, lastEnd: e as string | undefined },
        ),
      [id, state, resumesInS, lastEnd],
    ),
  device: (id: string, device: "proven" | "none", keyAtHand: boolean) =>
    page.evaluate(
      ([i, d, k]) =>
        (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setDevice(
          i as string,
          d as "proven" | "none",
          k as boolean,
        ),
      [id, device, keyAtHand],
    ),
  calls: () => page.evaluate(() => (window as unknown as { __hearthSim: Sim }).__hearthSim.calls),
});

const dialog = (page: Page) => page.locator("dialog[open]");
const alertBanner = (page: Page) => page.locator("[data-security-alert]");
const modeBanner = (page: Page) => page.locator("[data-attack-mode-banner]");

test("alerte : le bandeau s'affiche sur toutes les pages, activer depuis l'alerte, mot de passe, mode actif", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(alertBanner(page)).toHaveCount(0);
  await sim(page).alert("forge", true);
  await expect(alertBanner(page)).toBeVisible();
  await expect(alertBanner(page)).toContainText("Attaque probable détectée");
  await expect(alertBanner(page)).toContainText(
    "Une attaque probable vise ton identifiant. Clique pour plus d'infos et activer le mode attaque.",
  );
  // La marque dans la barre des serveurs : écrite dans le nom accessible.
  await expect(page.locator('[data-server="forge"] [role="img"]')).toHaveAttribute(
    "aria-label",
    /alerte de sécurité/,
  );
  await shoot(page, "attaque-alerte");

  await alertBanner(page).getByRole("button", { name: "Activer le mode attaque" }).click();
  await expect(dialog(page)).toContainText("Activer le mode attaque ?");
  // Comme les autres fenêtres à mot de passe : le champ reçoit le focus à l'ouverture.
  await expect(dialog(page).getByLabel("Ton mot de passe")).toBeFocused();
  // Mauvais mot de passe : refusé DANS la fenêtre, le champ est vidé.
  await dialog(page).getByLabel("Ton mot de passe").fill("Faux-Mot-De-Passe-1");
  await dialog(page).getByRole("button", { name: "Activer le mode attaque" }).click();
  await expect(dialog(page)).toContainText("Mot de passe actuel incorrect.");
  await expect(dialog(page).getByLabel("Ton mot de passe")).toHaveValue("");
  await dialog(page).getByLabel("Ton mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Activer le mode attaque" }).click();
  await expect(dialog(page)).toHaveCount(0);
  // Les deux bandeaux coexistent ; la marque passe au mode attaque.
  await expect(modeBanner(page)).toContainText("Mode attaque actif");
  await expect(modeBanner(page)).toContainText("Seuls les postes reconnus peuvent se connecter.");
  await expect(alertBanner(page)).toBeVisible();
  await expect(page.locator('[data-server="forge"] [role="img"]')).toHaveAttribute(
    "aria-label",
    /mode attaque actif/,
  );
  expect(await sim(page).calls()).not.toContain(GOOD);
  await shoot(page, "attaque-actif");
});

test("page Sécurité : carte du mode attaque, désactivation, variante suspendue, arrêt automatique", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  await expect(page.getByRole("heading", { level: 1, name: "Sécurité" })).toBeVisible();
  await expect(page.locator("[data-attack-mode-state]")).toHaveText("Inactif");
  await sim(page).mode("forge", "suspended", 1200);
  await expect(page.locator("[data-attack-mode-state]")).toHaveText("Suspendu");
  await expect(page.locator("[data-attack-mode-panel]")).toContainText("Il reprend dans 20 min.");
  await expect(modeBanner(page)).toContainText("Mode attaque suspendu");
  await shoot(page, "attaque-suspendu");
  await sim(page).mode("forge", "active");
  await expect(page.locator("[data-attack-mode-state]")).toHaveText("Actif");
  await page.getByRole("button", { name: "Désactiver le mode attaque" }).click();
  await expect(dialog(page)).toContainText("Désactiver le mode attaque ?");
  await dialog(page).getByLabel("Ton mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Désactiver le mode attaque" }).click();
  await expect(page.locator("[data-attack-mode-state]")).toHaveText("Inactif");
  await expect(modeBanner(page)).toHaveCount(0);
  // Fin automatique : un mot discret, la carte repasse en « Inactif ».
  await sim(page).mode("forge", "active");
  await sim(page).mode("forge", "off", undefined, "auto");
  await expect(
    page.getByText("L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement."),
  ).toBeVisible();
});

test("poste sans clé enregistrée : bouton indisponible, la raison est écrite sous le bouton et rien n'est envoyé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  await sim(page).device("forge", "none", false);
  await page.getByRole("link", { name: "Tableau de bord" }).click();
  await page.getByRole("link", { name: "Sécurité", exact: true }).click();
  const toggle = page.locator("[data-attack-mode-toggle]");
  await expect(toggle).toHaveAttribute("aria-disabled", "true");
  await expect(page.locator("[data-attack-mode-reason]")).toContainText(
    "Ce poste n'est pas encore enregistré, tu ne peux donc pas changer le mode attaque d'ici.",
  );
  await toggle.focus();
  await expect(page.getByRole("tooltip")).toContainText("Ce poste n'est pas encore enregistré");
  await toggle.click({ force: true });
  await expect(dialog(page)).toHaveCount(0);
  expect((await sim(page).calls()).some((call) => call.startsWith("attack-mode"))).toBe(false);
  await shoot(page, "attaque-sans-cle");
});

test("compte en lecture seule : voit l'alerte, bouton grisé avec son infobulle, aucune confirmation", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/salon/dashboard");
  await sim(page).alert("salon", true);
  await expect(alertBanner(page)).toContainText(
    "Tu vois l'alerte, mais seul un administrateur peut activer le mode attaque.",
  );
  const button = alertBanner(page).getByRole("button", { name: "Activer le mode attaque" });
  await expect(button).toHaveAttribute("aria-disabled", "true");
  await button.focus();
  await expect(page.getByRole("tooltip")).toContainText(
    "Tu vois l'alerte mais ne peux pas activer le mode attaque. Contacte un administrateur.",
  );
  await button.click({ force: true });
  await expect(dialog(page)).toHaveCount(0);
  await shoot(page, "attaque-lecture-seule");
});

test("hors ligne : les bandeaux de sécurité restent en couleur, datés, et le bouton dit que le serveur manque", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await sim(page).alert("forge", true);
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.setState("forge", "offline"),
  );
  await expect(alertBanner(page)).toBeVisible();
  await expect(
    alertBanner(page).getByRole("button", { name: "Activer le mode attaque" }),
  ).toHaveAttribute("aria-disabled", "true");
  await sim(page).mode("forge", "active");
  await expect(modeBanner(page)).toContainText("Dernier état connu à");
});

test("réglage « Alertes de sécurité » : activé par défaut, à part des autres notifications", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await expect(page.getByText("Alertes de sécurité")).toBeVisible();
  await expect(
    page.getByText("Notifier quand un serveur devient hors ligne ou revient"),
  ).toBeVisible();
  await expect(
    page.getByText(
      "Une notification Windows quand une attaque probable vise ton identifiant, et quand le mode attaque s'arrête tout seul.",
    ),
  ).toBeVisible();
  await shoot(page, "attaque-reglage");
});

test("changer son mot de passe : la case « Garder ce poste reconnu » est décochée par défaut", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await page.locator('.mine[data-server="forge"]').getByRole("button").first().click();
  const box = dialog(page).getByLabel("Garder ce poste reconnu");
  await expect(box).not.toBeChecked();
  await expect(dialog(page)).toContainText(
    "Si la case est cochée, ce poste restera reconnu après le changement de mot de passe.",
  );
  await box.check();
  await expect(box).toBeChecked();
  await shoot(page, "attaque-garder-poste");
});
