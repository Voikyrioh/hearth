import { expect, type Page, test } from "@playwright/test";

// HRT-33 : le parcours CLAVIER réel des formulaires (revue UX du 2026-10-08, C1, C2, C17, C18, C19, C56).
// Pont SIMULÉ. On ne remplit pas les champs par leur libellé : on tape au clavier, là où est le curseur,
// et on regarde où il va.
const GOOD = "Sunny-Walk-Home-42";
const OWN = "Correct-Horse-9";
const dialog = (page: Page) => page.locator("dialog[open]");

async function ready(page: Page) {
  await expect(
    dialog(page).locator("[data-reauth-field], [data-reauth-elevated], [data-reauth-no-key]"),
  ).toBeVisible();
}

test("assistant d'ajout : curseur dans le nom, Entrée dit ce qui manque, curseur dans le premier champ de chaque étape", async ({
  page,
}) => {
  await page.goto("/?servers=none&nodev#/servers/new");
  const name = page.getByLabel("Nom du serveur");
  await expect(name).toBeFocused();
  // Tab : l'adresse ; on la tape, le nom reste vide.
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Adresse IP ou nom")).toBeFocused();
  await page.keyboard.type("192.168.1.50");
  // « Suivant » n'est pas grisé ; Entrée dit ce qui manque et ramène le curseur au nom.
  await expect(page.getByRole("button", { name: "Suivant" })).not.toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await page.keyboard.press("Enter");
  await expect(page.getByText("Le nom du serveur est requis")).toBeVisible();
  await expect(name).toBeFocused();
  await page.keyboard.type("Atelier");
  await page.keyboard.press("Enter");
  // Étape 2 puis 3 : le curseur est dans l'identifiant, sans clic.
  await expect(page.getByRole("heading", { name: "Vérifie l'identité du serveur" })).toBeVisible();
  await page.getByRole("button", { name: "Confirmer" }).click();
  await expect(page.getByLabel("Identifiant")).toBeFocused();
});

test("créer un compte : le curseur est dans l'identifiant, ce qui est tapé n'atterrit pas dans la confirmation", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await ready(page);
  await expect(dialog(page).getByLabel("Identifiant")).toBeFocused();
  // La confirmation des actes porte son propre exemple (C19).
  await expect(dialog(page).getByLabel("Ton mot de passe")).toHaveAttribute(
    "placeholder",
    "Le tien, pour confirmer",
  );
  await page.keyboard.type("sophie");
  await expect(dialog(page).getByLabel("Identifiant")).toHaveValue("sophie");
  await expect(dialog(page).getByLabel("Ton mot de passe")).toHaveValue("");
  // Tab : le mot de passe du NOUVEAU compte, puis la confirmation, avant « Ton mot de passe ».
  await page.keyboard.press("Tab");
  await expect(dialog(page).getByLabel("Mot de passe du nouveau compte")).toBeFocused();
  await page.keyboard.type(GOOD);
  // (Le bouton « Afficher » du champ est entre les deux.)
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(dialog(page).getByLabel("Confirme le mot de passe du compte")).toBeFocused();
  await page.keyboard.type(GOOD);
});

test("un mot de passe de confirmation faux ne vide que la confirmation (C18)", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  await ready(page);
  await page.keyboard.type("sophie");
  await dialog(page).getByLabel("Mot de passe du nouveau compte").fill(GOOD);
  await dialog(page).getByLabel("Confirme le mot de passe du compte").fill(GOOD);
  await dialog(page).getByLabel("Ton mot de passe").fill("Mauvais-Mot-De-Passe-1");
  await dialog(page).getByRole("button", { name: "Créer", exact: true }).click();
  await expect(dialog(page).getByText("Mot de passe actuel incorrect.")).toBeVisible();
  await expect(dialog(page).getByLabel("Identifiant")).toHaveValue("sophie");
  await expect(dialog(page).getByLabel("Mot de passe du nouveau compte")).toHaveValue(GOOD);
  await expect(dialog(page).getByLabel("Confirme le mot de passe du compte")).toHaveValue(GOOD);
  await expect(dialog(page).getByLabel("Ton mot de passe")).toHaveValue("");
  // Le curseur est revenu dans la confirmation : on la retape au clavier, sans clic.
  await expect(dialog(page).getByLabel("Ton mot de passe")).toBeFocused();
  await page.keyboard.type(OWN);
  await dialog(page).getByRole("button", { name: "Créer", exact: true }).click();
  await expect(page.locator(".toast").filter({ hasText: "Compte sophie créé." })).toBeVisible();
});

test("changer mon mot de passe : le curseur est dans le nouveau mot de passe, l'ancien vient à la fin", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Changer mon mot de passe" }).first().click();
  await ready(page);
  await expect(dialog(page).getByLabel("Nouveau mot de passe", { exact: true })).toBeFocused();
});
