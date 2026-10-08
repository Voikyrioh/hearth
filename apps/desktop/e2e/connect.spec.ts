import { expect, type Page, test } from "@playwright/test";

// Parcours de connexion avec le pont SIMULÉ (réseau d'agents simulé : `192.168.1.50`, compte
// `marie` / `Correct-Horse-9`, voir `link/simulated.ts`). Captures dans e2e/screenshots/ (non
// commitées) : 1366, 1920 et 2560 px de large, comme la revue UX.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 900;
const PASSWORD = "Correct-Horse-9";
const OTHER_FINGERPRINT = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";

type Sim = {
  addAgent(agent: { host: string; fingerprint: string; users?: object }): void;
  reinstallAgent(serverId: string, fingerprint: string): void;
};

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

async function fillAddress(page: Page, name: string, host: string) {
  await page.getByLabel("Nom du serveur").fill(name);
  await page.getByLabel("Adresse IP ou nom").fill(host);
}

test("ajout d'un serveur en trois temps, de l'accueil au tableau de bord", async ({ page }) => {
  await page.goto("/?servers=none&nodev");
  await page.getByRole("button", { name: "Ajouter un serveur" }).click();
  await expect(page).toHaveURL(/#\/servers\/new$/);

  // Temps 1 : « Suivant » n'est jamais grisé sans raison (C1) ; le curseur est dans le nom (C2).
  await expect(page.getByRole("heading", { name: "Ajouter un serveur" })).toBeVisible();
  const next = page.getByRole("button", { name: "Suivant" });
  await expect(page.getByLabel("Nom du serveur")).toBeFocused();
  await expect(next).not.toHaveAttribute("aria-disabled", "true");
  await expect(page.getByLabel("Port (optionnel)")).toHaveAttribute("placeholder", "7341");
  await fillAddress(page, "Atelier", "pas une adresse");
  await expect(page.getByText("Cette adresse n'est pas valide")).toBeVisible();
  await page.getByLabel("Adresse IP ou nom").fill("10.9.9.9");
  await next.click();
  await expect(
    page.getByText("Cette adresse n'est pas joignable. Vérifie l'adresse et essaie de nouveau."),
  ).toBeVisible();
  await page.getByLabel("Adresse IP ou nom").fill("192.168.1.50");
  await page.getByRole("radio", { name: "Couleur 3" }).click();
  await shoot(page, "ajout-1-adresse");
  await next.click();

  // Temps 2 : l'empreinte en 8 groupes de 4, rien d'enregistré avant « Confirmer ».
  await expect(page.getByRole("heading", { name: "Vérifie l'identité du serveur" })).toBeVisible();
  await expect(page.locator("[data-fingerprint] span")).toHaveCount(8);
  await expect(page.locator("[data-fingerprint]")).toContainText("A1B2");
  await expect(
    page.getByText("Compare avec l'empreinte affichée à la fin de l'installation de l'agent."),
  ).toBeVisible();
  await expect(page.locator("a[data-server]")).toHaveCount(0);
  await shoot(page, "ajout-2-empreinte");
  await page.getByRole("button", { name: "Confirmer" }).click();

  // Temps 3 : connexion ; un mauvais mot de passe reste générique, puis le bon.
  await expect(page.getByRole("heading", { name: "Connecte-toi" })).toBeVisible();
  await expect(page.getByLabel("Se souvenir de moi sur ce PC")).toBeChecked();
  await page.getByLabel("Identifiant").fill("marie");
  await page.getByLabel("Mot de passe", { exact: true }).fill("faux-faux-1");
  await page.getByRole("button", { name: "Se connecter" }).click();
  await expect(page.getByText("Identifiant ou mot de passe incorrect.")).toBeVisible();
  await shoot(page, "ajout-3-connexion");
  await page.getByLabel("Mot de passe", { exact: true }).fill(PASSWORD);
  await page.getByRole("button", { name: "Se connecter" }).click();

  await expect(page).toHaveURL(/#\/servers\/sim-1\/dashboard$/);
  await expect(page.locator(".toast")).toContainText("Connecté à Atelier.");
  await expect(page.getByRole("status").filter({ hasText: "Connecté" })).toBeVisible();
  await expect(page.locator("a[data-server]")).toHaveCount(1);
});

test("empreinte refusée : aucun serveur n'est enregistré ; acceptée ensuite : il l'est", async ({
  page,
}) => {
  await page.goto("/?servers=none&nodev#/servers/new");
  await fillAddress(page, "Atelier", "192.168.1.50");
  await page.getByRole("button", { name: "Suivant" }).click();
  await expect(page.locator("[data-fingerprint] span")).toHaveCount(8);
  await page.getByRole("button", { name: "Refuser" }).click();
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await expect(page.locator("a[data-server]")).toHaveCount(0);

  // Une deuxième tentative : cette fois l'utilisateur confirme.
  await page.getByRole("button", { name: "Ajouter un serveur" }).click();
  await fillAddress(page, "Atelier", "192.168.1.50");
  await page.getByRole("button", { name: "Suivant" }).click();
  await page.getByRole("button", { name: "Confirmer" }).click();
  await expect(page.getByRole("heading", { name: "Connecte-toi" })).toBeVisible();
  await page.getByLabel("Identifiant").fill("marie");
  await page.getByLabel("Mot de passe", { exact: true }).fill(PASSWORD);
  await page.getByRole("button", { name: "Se connecter" }).click();
  await expect(page.locator("a[data-server]")).toHaveCount(1);
});

test("quitter l'assistant avant la connexion ne laisse aucun serveur", async ({ page }) => {
  await page.goto("/?servers=none&nodev#/servers/new");
  await fillAddress(page, "Atelier", "192.168.1.50");
  await page.getByRole("button", { name: "Suivant" }).click();
  await page.getByRole("button", { name: "Confirmer" }).click();
  await expect(page.getByRole("heading", { name: "Connecte-toi" })).toBeVisible();
  // Rien n'existe encore : ni dans la barre, ni au carnet, ni au coffre simulé.
  await expect(page.locator("a[data-server]")).toHaveCount(0);
  await page.getByRole("link", { name: "Accueil" }).click();
  await expect(page.getByRole("heading", { name: "Bienvenue dans Hearth" })).toBeVisible();
  await expect(page.locator("a[data-server]")).toHaveCount(0);
});

test("empreinte changée : alerte bloquante, refusée puis acceptée", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.getByRole("status").filter({ hasText: "Connecté" })).toBeVisible();
  await page.evaluate((fingerprint) => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    sim.addAgent({ host: "192.168.1.120", fingerprint: "a".repeat(64) });
    sim.reinstallAgent("forge", fingerprint);
  }, OTHER_FINGERPRINT);

  const alert = page.locator("dialog[data-fingerprint-alert]");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText(
    "L'identité de ce serveur a changé. Cela peut signifier que l'agent a été réinstallé ou que la machine a changé. Compare avec l'empreinte du serveur et accepte le changement ou refuse la connexion.",
  );
  await expect(alert).toContainText("Empreinte mémorisée");
  await expect(alert).toContainText("Empreinte reçue");
  await expect(alert.getByRole("button", { name: "Ne pas se connecter" })).toBeFocused();
  await shoot(page, "empreinte-changee");

  // Refuser : l'alerte se ferme, le lien reste suspendu, on peut la rouvrir.
  await alert.getByRole("button", { name: "Ne pas se connecter" }).click();
  await expect(alert).toBeHidden();
  await expect(page.getByRole("status").filter({ hasText: "Hors ligne" })).toBeVisible();
  await expect(page.getByText("L'identité de ce serveur a changé. La connexion")).toBeVisible();
  await page.getByRole("button", { name: "Voir l'alerte" }).click();
  await expect(alert).toBeVisible();

  // Accepter : le lien repart.
  await alert.getByRole("button", { name: "Accepter la nouvelle empreinte" }).click();
  await expect(alert).toBeHidden();
  await expect(page.getByRole("status").filter({ hasText: "Connecté" })).toBeVisible();
});

test("carnet : oubli des identifiants, déconnexion, suppression avec confirmation", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.getByRole("link", { name: "Mes serveurs" }).click();
  await expect(page.getByRole("heading", { name: "Mes serveurs" })).toBeVisible();
  const forge = page.locator('[data-server-row="forge"]');
  const salon = page.locator('[data-server-row="salon"]');
  await expect(forge).toContainText("Identifiants mémorisés");
  await expect(salon).not.toContainText("Identifiants mémorisés");
  await shoot(page, "carnet");

  // Oubli des identifiants : la mention disparaît, la session continue.
  await forge.getByRole("button", { name: "Oublier mes identifiants" }).click();
  await expect(page.locator(".toast")).toContainText("Identifiants oubliés pour forge.");
  await expect(forge).not.toContainText("Identifiants mémorisés");
  await expect(forge.getByRole("status")).toHaveText("Connecté");

  // Se déconnecter d'un serveur ne ferme pas l'autre.
  await forge.getByRole("button", { name: "Se déconnecter" }).click();
  await expect(forge.getByRole("status")).toHaveText("Session expirée");
  await expect(salon.getByRole("status")).toHaveText("Connecté");

  // Suppression : Annuler ne change rien, Supprimer retire le serveur.
  await salon.getByRole("button", { name: "Supprimer" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("Supprimer ce serveur ?");
  await expect(dialog).toContainText("Ses identifiants mémorisés seront aussi supprimés.");
  await dialog.getByRole("button", { name: "Annuler" }).click();
  await expect(salon).toBeVisible();
  await salon.getByRole("button", { name: "Supprimer" }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Supprimer" }).click();
  await expect(salon).toHaveCount(0);
  await expect(page.locator("a[data-server]")).toHaveCount(1);
});

test("serveur déconnecté : le formulaire de connexion apparaît, identifiant prérempli", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.evaluate(() => {
    const sim = (window as unknown as { __hearthSim: { publish(...a: unknown[]): void } })
      .__hearthSim;
    sim.publish("forge", "session_expired", { reason: "expired" });
  });
  await expect(page.getByText("Ta session a expiré.")).toBeVisible();
  await expect(page.getByLabel("Identifiant")).toHaveValue("marie");
  await shoot(page, "reconnexion");
});
