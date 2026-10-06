import { expect, type Page, test } from "@playwright/test";

// HRT-13 : gérer les comptes depuis l'app. Pont SIMULÉ (aucun réseau) ; les règles réelles (rôles,
// dernier administrateur, sessions, journal, action coupée, aucune commande générique) sont prouvées
// côté Rust contre un vrai agent (`apps/desktop/src-tauri/tests/accounts_runtime.rs`). Ici : les
// parcours complets de l'écran, dont une action lancée au moment d'une coupure.

type Sim = {
  setState(id: string, state: string): void;
  emitOperation(event: { opId: string; serverId: string; outcome: string }): void;
  actionMode: "ok" | "cut";
  executeBeforeCut: boolean;
  lastUnknownOpId: string | null;
  calls: string[];
};

const WIDTHS = [1366, 1920, 2560] as const;
const GOOD = "Sunny-Walk-Home-42";

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const sim = (page: Page) => ({
  set: (mode: "ok" | "cut", executes = true) =>
    page.evaluate(
      ([m, e]) => {
        const s = (window as unknown as { __hearthSim: Sim }).__hearthSim;
        s.actionMode = m as "ok" | "cut";
        s.executeBeforeCut = e as boolean;
      },
      [mode, executes],
    ),
  calls: () => page.evaluate(() => (window as unknown as { __hearthSim: Sim }).__hearthSim.calls),
});

const pill = (page: Page) => page.locator(".head").getByRole("status");
const row = (page: Page, username: string) => page.locator(`tr[data-account="${username}"]`);
const dialog = (page: Page) => page.locator("dialog[open]");

async function fillCreate(page: Page, username: string, password: string, confirm = password) {
  await dialog(page).getByLabel("Identifiant").fill(username);
  await dialog(page).getByLabel("Mot de passe", { exact: true }).fill(password);
  await dialog(page).getByLabel("Confirme le mot de passe").fill(confirm);
}

test("la page Comptes : tableau, colonnes, étiquette « toi », actions de la ligne", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await expect(page.getByRole("heading", { level: 1, name: "Comptes" })).toBeVisible();
  const headers = page.locator("thead th");
  await expect(headers).toHaveText([
    "Identifiant",
    "Rôle",
    "Créé le",
    "Dernière connexion",
    "Sessions ouvertes",
    "Actions",
  ]);
  await expect(row(page, "marie")).toContainText("toi");
  await expect(row(page, "marie").getByRole("button")).toHaveText(["Changer mon mot de passe"]);
  await expect(row(page, "paul").getByRole("button")).toHaveText([
    "Changer le rôle",
    "Mot de passe",
    "Fermer les 2 sessions",
    "Supprimer",
  ]);
  await expect(row(page, "lea").locator("td").nth(2)).toHaveText("Jamais");
  await shoot(page, "comptes-liste");
});

test("création : critères du mot de passe en direct (coche et croix), bouton inerte tant que tout n'est pas valide", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await expect(dialog(page)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Créer un compte" })).toBeVisible();
  const create = dialog(page).getByRole("button", { name: "Créer", exact: true });
  await expect(create).toHaveAttribute("aria-disabled", "true");
  // L'identifiant : message sous le champ dès qu'il est invalide.
  await dialog(page).getByLabel("Identifiant").fill("a b");
  await expect(
    dialog(page).getByText("L'identifiant contient des caractères non autorisés"),
  ).toBeVisible();
  await dialog(page).getByLabel("Identifiant").fill("sophie");
  await expect(dialog(page).getByText("L'identifiant contient")).toHaveCount(0);
  // Le mot de passe : chaque critère passe de la croix à la coche en direct.
  await dialog(page).getByLabel("Mot de passe", { exact: true }).fill("abc");
  const rule = (name: string) => dialog(page).locator(`[data-rule="${name}"]`);
  await expect(rule("min_length")).toHaveAttribute("data-met", "false");
  await expect(rule("digit")).toHaveAttribute("data-met", "false");
  await expect(rule("uppercase")).toHaveAttribute("data-met", "false");
  await expect(rule("lowercase")).toHaveAttribute("data-met", "true");
  await expect(rule("min_length")).toContainText("Non respecté");
  await dialog(page).getByLabel("Mot de passe", { exact: true }).fill("xxSophiexx12A");
  await expect(rule("contains_username")).toHaveAttribute("data-met", "false");
  await dialog(page).getByLabel("Mot de passe", { exact: true }).fill(GOOD);
  for (const name of ["min_length", "digit", "lowercase", "uppercase", "contains_username"]) {
    await expect(rule(name)).toHaveAttribute("data-met", "true");
  }
  await expect(rule("min_length")).toContainText("Respecté");
  await expect(create).toHaveAttribute("aria-disabled", "true");
  await dialog(page).getByLabel("Confirme le mot de passe").fill("autre chose");
  await expect(dialog(page).getByText("Les deux mots de passe ne correspondent pas")).toBeVisible();
  await expect(create).toHaveAttribute("aria-disabled", "true");
  await dialog(page).getByLabel("Confirme le mot de passe").fill(GOOD);
  await expect(create).not.toHaveAttribute("aria-disabled", "true");
  await shoot(page, "comptes-creation");
});

test("création réussie : compte ajouté en bas de la liste, message de succès, aucun mot de passe journalisé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await fillCreate(page, "sophie", GOOD);
  await dialog(page).getByRole("button", { name: "Créer", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(page.locator(".toast")).toContainText("Compte sophie créé");
  await expect(page.locator("tbody tr").last()).toHaveAttribute("data-account", "sophie");
  expect(JSON.stringify(await sim(page).calls())).not.toContain(GOOD);
});

test("création refusée : identifiant déjà utilisé, formulaire conservé, mots de passe vidés", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await fillCreate(page, "PAUL", GOOD);
  await dialog(page).getByRole("button", { name: "Créer", exact: true }).click();
  await expect(dialog(page).getByText("Cet identifiant est déjà utilisé")).toBeVisible();
  await expect(dialog(page).getByLabel("Identifiant")).toHaveValue("PAUL");
  await expect(dialog(page).getByLabel("Mot de passe", { exact: true })).toHaveValue("");
  await expect(dialog(page).getByLabel("Confirme le mot de passe")).toHaveValue("");
  // Annuler : la fenêtre se ferme, rien n'a été créé, la prochaine ouverture est vide.
  await dialog(page).getByRole("button", { name: "Annuler" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(page.locator("tbody tr")).toHaveCount(3);
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await expect(dialog(page).getByLabel("Identifiant")).toHaveValue("");
});

test("changer le rôle par la liste déroulante, et le dernier administrateur est protégé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "paul")
    .getByRole("button", { name: /^Changer le rôle/ })
    .click();
  await expect(row(page, "paul").getByRole("combobox")).toBeFocused();
  await row(page, "paul").getByRole("combobox").selectOption("admin");
  await expect(page.locator(".toast")).toContainText("paul est maintenant Administrateur");
  await expect(row(page, "paul").locator("td").nth(0)).toHaveText("Administrateur");
  // Deux administrateurs : retour à Lecture seule possible ; puis le dernier est grisé.
  await row(page, "paul")
    .getByRole("button", { name: /^Changer le rôle/ })
    .click();
  await row(page, "paul").getByRole("combobox").selectOption("readonly");
  await expect(row(page, "paul").locator("td").nth(0)).toHaveText("Lecture seule");
});

test("fermer les sessions : sans confirmation, le bouton s'éteint ensuite", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  const close = row(page, "paul").getByRole("button", { name: /^Fermer les 2 sessions/ });
  await close.click();
  await expect(page.locator(".toast")).toContainText("Sessions de paul fermées");
  await expect(row(page, "paul").locator("td").nth(3)).toHaveText("0");
  await expect(
    row(page, "paul").getByRole("button", { name: /^Fermer les sessions/ }),
  ).toHaveAttribute("aria-disabled", "true");
});

test("mot de passe d'un autre compte : fenêtre, critères, succès", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "paul")
    .getByRole("button", { name: /^Mot de passe/ })
    .click();
  await expect(
    page.getByRole("heading", { name: "Changer le mot de passe de paul" }),
  ).toBeVisible();
  await expect(dialog(page).getByLabel("Ancien mot de passe")).toHaveCount(0);
  await dialog(page).getByLabel("Nouveau mot de passe", { exact: true }).fill("xxPaulxxxxxx12A");
  await expect(dialog(page).locator('[data-rule="contains_username"]')).toHaveAttribute(
    "data-met",
    "false",
  );
  await dialog(page).getByLabel("Nouveau mot de passe", { exact: true }).fill(GOOD);
  await dialog(page).getByLabel("Confirmer le nouveau mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Changer le mot de passe" }).click();
  await expect(page.locator(".toast")).toContainText("Mot de passe changé");
  await expect(dialog(page)).toHaveCount(0);
});

test("supprimer : confirmation qui nomme le compte, annulation sans effet, puis suppression", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  await expect(page.getByRole("heading", { name: "Supprimer le compte ?" })).toBeVisible();
  await expect(
    page.getByText("Supprimer le compte lea ? Cette action est irréversible."),
  ).toBeVisible();
  await page.getByRole("button", { name: "Annuler" }).click();
  await expect(row(page, "lea")).toHaveCount(1);
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "Supprimer", exact: true })
    .click();
  await expect(page.locator(".toast")).toContainText("Compte lea supprimé");
  await expect(row(page, "lea")).toHaveCount(0);
  await shoot(page, "comptes-apres-suppression");
});

test("lecture seule : pas d'entrée de menu, la route reste fermée", async ({ page }) => {
  await page.goto("/?nodev#/servers/salon/dashboard");
  const nav = page.getByRole("navigation", { name: "Navigation du serveur" });
  await expect(nav.getByRole("link", { name: "Comptes" })).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "Journal d'activité" })).toHaveCount(0);
  await page.goto("/?nodev#/servers/salon/accounts");
  await expect(page).toHaveURL(/#\/servers\/salon\/dashboard$/);
});

test("section personnelle des réglages : tous les rôles changent leur mot de passe, seuls les administrateurs peuvent supprimer leur compte", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await expect(page.getByRole("heading", { level: 2 })).toHaveText([
    "Général",
    "Client",
    "Mises à jour",
    "Mon compte",
  ]);
  const forge = page.locator('.mine[data-server="forge"]');
  const salon = page.locator('.mine[data-server="salon"]');
  await expect(forge.getByRole("button")).toHaveText([
    "Changer mon mot de passe",
    "Supprimer mon compte",
  ]);
  await expect(salon.getByRole("button")).toHaveText(["Changer mon mot de passe"]);
  await shoot(page, "reglages-comptes");
  // Lecture seule : ancien puis nouveau mot de passe ; un ancien incorrect est refusé.
  await salon.getByRole("button", { name: "Changer mon mot de passe" }).click();
  await expect(page.getByRole("heading", { name: "Changer mon mot de passe" })).toBeVisible();
  await dialog(page).getByLabel("Ancien mot de passe").fill("Mauvais-Mot-De-Passe-1");
  await dialog(page).getByLabel("Nouveau mot de passe", { exact: true }).fill(GOOD);
  await dialog(page).getByLabel("Confirmer le nouveau mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Changer le mot de passe" }).click();
  await expect(dialog(page).getByText("L'ancien mot de passe est incorrect")).toBeVisible();
  await expect(dialog(page).getByLabel("Ancien mot de passe")).toHaveValue("");
  await dialog(page).getByLabel("Ancien mot de passe").fill("Correct-Horse-9");
  await dialog(page).getByLabel("Nouveau mot de passe", { exact: true }).fill(GOOD);
  await dialog(page).getByLabel("Confirmer le nouveau mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Changer le mot de passe" }).click();
  await expect(page.locator(".toast")).toContainText("Mot de passe changé");
});

test("supprimer son propre compte : on retape son identifiant ; le dernier administrateur est refusé", async ({
  page,
}) => {
  await page.goto("/?nodev#/settings");
  await page
    .locator('.mine[data-server="forge"]')
    .getByRole("button", { name: "Supprimer mon compte" })
    .click();
  await expect(page.getByRole("heading", { name: "Supprimer ton compte ?" })).toBeVisible();
  await expect(dialog(page).getByText("Retape ton identifiant pour confirmer")).toBeVisible();
  await dialog(page).getByLabel("Retape ton identifiant pour confirmer").fill("paul");
  await dialog(page).getByRole("button", { name: "Supprimer mon compte" }).click();
  await expect(dialog(page).getByText("L'identifiant ne correspond pas, réessaye")).toBeVisible();
  await dialog(page).getByLabel("Retape ton identifiant pour confirmer").fill("marie");
  await dialog(page).getByRole("button", { name: "Supprimer mon compte" }).click();
  await expect(
    dialog(page).getByText("Tu es le dernier administrateur, ce compte ne peut pas être supprimé"),
  ).toBeVisible();
});

test("hors ligne : les actions sont inertes et expliquées, la liste d'avant reste, rien n'est envoyé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await expect(row(page, "paul")).toBeVisible();
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.setState("forge", "offline"),
  );
  await expect(pill(page)).toHaveText("Hors ligne");
  const add = page.getByRole("button", { name: "Ajouter un compte" });
  await expect(add).toHaveAttribute("aria-disabled", "true");
  const remove = row(page, "paul").getByRole("button", { name: /^Supprimer/ });
  await expect(remove).toHaveAttribute("aria-disabled", "true");
  await remove.focus();
  await expect(page.getByRole("tooltip")).toHaveText(
    "Indisponible tant que le serveur est hors ligne.",
  );
  await remove.click({ force: true });
  await expect(dialog(page)).toHaveCount(0);
  await expect(row(page, "paul")).toBeVisible();
  expect((await sim(page).calls()).filter((c) => c.startsWith("account delete"))).toHaveLength(0);
});

test("action lancée au moment d'une coupure : résultat inconnu, jamais rejouée, liste relue au retour du lien", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await sim(page).set("cut", true);
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "Supprimer", exact: true })
    .click();
  const unknown = page
    .locator(".toast")
    .filter({ hasText: "Le résultat de cette opération n'est pas connu." });
  await expect(unknown).toContainText("Elle n'a pas été rejouée automatiquement.");
  await expect(unknown).toContainText("À la reconnexion, la liste se mettra à jour.");
  await expect(pill(page)).toHaveText("Reconnexion…");
  // Hors lien : la liste d'avant, désaturée, et plus aucune action possible.
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(row(page, "lea")).toHaveCount(1);
  await expect(row(page, "paul").getByRole("button", { name: /^Supprimer/ })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  // Le lien revient : la liste se relit, l'état réel du serveur (« lea » a bien été supprimé).
  await page.evaluate(() => {
    const s = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    s.setState("forge", "connected");
    s.emitOperation({ opId: s.lastUnknownOpId ?? "", serverId: "forge", outcome: "done" });
  });
  await expect(pill(page)).toHaveText("Connecté");
  await expect(row(page, "lea")).toHaveCount(0);
  await expect(
    page.locator(".toast").filter({ hasText: "Fait pendant la coupure." }),
  ).toBeVisible();
  // Jamais rejouée : un seul envoi de la suppression.
  expect((await sim(page).calls()).filter((c) => c.startsWith("account delete"))).toHaveLength(1);
  await shoot(page, "comptes-apres-coupure");
});

test("action coupée et non exécutée : la liste relue montre que rien n'a changé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await sim(page).set("cut", false);
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  await page
    .locator("dialog[open]")
    .getByRole("button", { name: "Supprimer", exact: true })
    .click();
  await expect(pill(page)).toHaveText("Reconnexion…");
  await page.evaluate(() => {
    const s = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    s.setState("forge", "connected");
    s.emitOperation({ opId: s.lastUnknownOpId ?? "", serverId: "forge", outcome: "not_executed" });
  });
  await expect(pill(page)).toHaveText("Connecté");
  await expect(
    page.locator(".toast").filter({ hasText: "Non exécuté. Tu peux relancer." }),
  ).toBeVisible();
  await expect(row(page, "lea")).toHaveCount(1);
});
