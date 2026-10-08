import { expect, type Page, test } from "@playwright/test";

// HRT-38 (hors ligne : on me le dit une fois) et HRT-40 (action impossible : la fenêtre dit où aller).
// Pont SIMULÉ. Captures aux cinq tailles de la revue UX (préfixes hrt38-, hrt40-).

type Sim = {
  publish(id: string, state: string, extra?: Record<string, unknown>): void;
  reauth: { setSupported(id: string, supported: boolean): void };
  security: { setDevice(id: string, device: "proven" | "none", keyAtHand?: boolean): void };
};

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function sim(page: Page, run: (sim: Sim) => void) {
  await page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString());
}

async function go(page: Page, hash: string) {
  await page.evaluate((target) => {
    window.location.hash = target;
  }, hash);
}

async function shoot(page: Page, name: string, size: { width: number; height: number }) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${size.width}x${size.height}.png` });
}

const retryButtons = (page: Page) => page.getByRole("button", { name: /^Réessayer/ });
const NOT_LOADED = "Pas encore chargé, sera disponible quand le serveur reviendra.";

for (const size of SIZES) {
  test(`hors ligne à ${size.width}×${size.height} : le Journal n'a qu'un « Réessayer », celui du bandeau`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.getByRole("heading", { name: "Journal d'activité" })).toBeVisible();
    await sim(page, (s) => s.publish("forge", "offline", {}));
    await expect(page.locator(".banner")).toContainText("Serveur hors ligne");
    await expect(retryButtons(page)).toHaveCount(1);
    await expect(page.getByText("Rechargement manuel")).toHaveCount(0);
    await expect(page.getByText("Données périmées, serveur injoignable")).toHaveCount(0);
    await expect(page.getByText("Indisponible tant que le lien avec le serveur")).toHaveCount(0);
    await shoot(page, "hrt38-hors-ligne-journal", size);
  });

  test(`hors ligne à ${size.width}×${size.height} : Comptes jamais ouvert dit « pas encore chargé » sans « Vu il y a », Sécurité n'a qu'un « Réessayer »`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.getByRole("heading", { name: "Tableau de bord" })).toBeVisible();
    await sim(page, (s) => s.publish("forge", "offline", {}));
    await go(page, "#/servers/forge/accounts");
    await expect(page.locator("[data-not-loaded-yet]")).toHaveText(NOT_LOADED);
    await expect(page.getByText("Impossible de lire la liste des comptes.")).toHaveCount(0);
    await expect(retryButtons(page)).toHaveCount(1);
    await expect(page.locator(".surface__stamp")).toHaveCount(0);
    await shoot(page, "hrt38-hors-ligne-comptes", size);
    // Sécurité : l'état de chaque serveur est lu dès le démarrage (HRT-26), donc déjà connu ; « pas encore chargé »
    // est prouvé par `pages/notLoadedYet.test.ts`. Ici : un seul « Réessayer », l'état gardé et daté.
    await go(page, "#/servers/forge/security");
    await expect(page.getByRole("heading", { name: "Sécurité" })).toBeVisible();
    await expect(retryButtons(page)).toHaveCount(1);
    await shoot(page, "hrt38-hors-ligne-securite", size);
  });

  test(`accès révoqué à ${size.width}×${size.height} : un titre clair, l'explication, une sortie`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await sim(page, (s) => s.publish("forge", "access_revoked", { reason: "revoked" }));
    await expect(page.getByRole("heading", { name: "Accès révoqué sur forge" })).toBeVisible();
    await expect(page.getByText("il a pu être supprimé ou désactivé")).toBeVisible();
    await expect(page.getByText("Demande à l'administrateur du serveur")).toBeVisible();
    await expect(page.getByRole("button", { name: "Utiliser un autre compte" })).toBeVisible();
    await expect(page.getByText("Connecte-toi à forge")).toHaveCount(0);
    await shoot(page, "hrt38-acces-revoque", size);
  });

  test(`reconnexion à ${size.width}×${size.height} : la page garde son apparence et n'est pas datée`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.getByRole("heading", { name: "Tableau de bord" })).toBeVisible();
    await sim(page, (s) => s.publish("forge", "reconnecting", {}));
    await expect(page.locator(".head").getByRole("status")).toHaveText("Reconnexion…");
    await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
    await expect(page.locator(".surface__stamp")).toHaveCount(0);
    await shoot(page, "hrt38-reconnexion", size);
  });

  test(`empreinte à ${size.width}×${size.height} : le serveur est rappelé, un retour, où la relire`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?servers=none&nodev");
    await page.getByRole("button", { name: "Ajouter un serveur" }).click();
    await page.getByLabel("Nom du serveur").fill("Atelier");
    await page.getByLabel("Adresse IP ou nom").fill("192.168.1.50");
    await page.getByRole("button", { name: "Suivant" }).click();
    await expect(
      page.getByRole("heading", { name: "Vérifie l'identité du serveur" }),
    ).toBeVisible();
    await expect(page.locator("[data-fingerprint-server]")).toContainText("Atelier");
    await expect(page.locator("[data-fingerprint-server]")).toContainText("192.168.1.50");
    await expect(page.locator("[data-fingerprint-where]")).toContainText(
      "sudo hearth-agent fingerprint",
    );
    await expect(page.locator("[data-fingerprint-refuse]")).toContainText("rien n'est enregistré");
    await shoot(page, "hrt40-empreinte", size);
    await page.getByRole("button", { name: "Précédent" }).click();
    await expect(page.getByLabel("Nom du serveur")).toHaveValue("Atelier");
  });

  test(`agent trop ancien à ${size.width}×${size.height} : « Mise à jour requise » et un bouton vers les réglages`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await sim(page, (s) => s.reauth.setSupported("forge", false));
    await page
      .locator("tr[data-account='lea']")
      .getByRole("button", { name: /^Supprimer/ })
      .click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog.getByRole("heading", { name: "Mise à jour requise" })).toBeVisible();
    await expect(dialog.locator("button[type=submit]")).toHaveCount(0);
    await expect(dialog).not.toContainText("Supprimer le compte ?");
    await shoot(page, "hrt40-agent-trop-ancien", size);
    await dialog.getByRole("button", { name: "Aller aux réglages" }).click();
    await expect(page).toHaveURL(/#\/settings$/);
  });

  test(`poste sans clé à ${size.width}×${size.height} : « Poste non enregistré » et un seul geste`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await sim(page, (s) => s.security.setDevice("forge", "none", false));
    await page
      .locator("tr[data-account='lea']")
      .getByRole("button", { name: /^Supprimer/ })
      .click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog.getByRole("heading", { name: "Poste non enregistré" })).toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Me reconnecter pour enregistrer ce poste" }),
    ).toBeVisible();
    await expect(dialog.locator("button[type=submit]")).toHaveCount(0);
    await expect(dialog).not.toContainText("Supprimer le compte ?");
    await shoot(page, "hrt40-poste-sans-cle", size);
  });
}
