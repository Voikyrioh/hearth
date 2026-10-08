import { expect, type Page, test } from "@playwright/test";

// HRT-30 : la confirmation des actes d'administration vue de l'écran, par le pont SIMULÉ qui applique les
// règles de l'agent (mot de passe, délai de 5 minutes, agent ancien, poste sans clé). Le vrai agent, la
// preuve de la clé de ce PC, le défi neuf et la clé d'opération neuve à chaque essai sont prouvés côté
// Rust (`crates/hearth-link/tests/admin_reauth.rs`, `apps/desktop/src-tauri/tests/*_runtime.rs`).

type Sim = {
  calls: string[];
  reauth: { setSupported(id: string, supported: boolean): void; close(id: string): void };
  security: { setDevice(id: string, device: "proven" | "none", keyAtHand?: boolean): void };
};

const WIDTHS = [1366, 1920, 2560] as const;
const OWN = "Correct-Horse-9";
const GOOD = "Sunny-Walk-Home-42";

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const dialog = (page: Page) => page.locator("dialog[open]");
const row = (page: Page, username: string) => page.locator(`tr[data-account="${username}"]`);
const calls = (page: Page) =>
  page.evaluate(() => (window as unknown as { __hearthSim: Sim }).__hearthSim.calls);
const simReauth = (page: Page, run: "close" | "unsupported") =>
  page.evaluate((what) => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    if (what === "close") sim.reauth.close("forge");
    else sim.reauth.setSupported("forge", false);
  }, run);

async function ready(page: Page) {
  const open = dialog(page);
  await expect(
    open.locator("[data-reauth-field], [data-reauth-elevated], [data-reauth-no-key]"),
  ).toBeVisible();
  return open;
}

test("chaque acte d'une fenêtre demande « Ton mot de passe » quand l'agent l'annonce", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "paul")
    .getByRole("button", { name: /^Changer le mot de passe/ })
    .click();
  const open = await ready(page);
  await expect(open.getByLabel("Ton mot de passe")).toBeVisible();
  await expect(open.getByText("Pour confirmer, redonne ton mot de passe.")).toBeVisible();
  await expect(open.getByRole("button", { name: "Changer le mot de passe" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await shoot(page, "reauth-champ");
});

test("pendant le délai de 5 minutes : un acte couvert n'a pas de champ et dit le temps qu'il reste ; un acte sensible le redemande", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "paul")
    .getByRole("button", { name: /^Fermer les/ })
    .click();
  const first = await ready(page);
  await first.getByLabel("Ton mot de passe").fill(OWN);
  await first.getByRole("button", { name: "Fermer les sessions", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  // Supprimer un compte est couvert : pas de champ, le temps restant est écrit.
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  const second = await ready(page);
  await expect(second.getByLabel("Ton mot de passe")).toHaveCount(0);
  await expect(second.locator("[data-reauth-elevated]")).toContainText(
    /Il te sera redemandé dans \d+ min \d+ s/,
  );
  await shoot(page, "reauth-delai");
  await second.getByRole("button", { name: "Annuler" }).click();
  // Le mot de passe d'un autre compte n'est jamais couvert.
  await row(page, "paul")
    .getByRole("button", { name: /^Changer le mot de passe/ })
    .click();
  const third = await ready(page);
  await expect(third.getByLabel("Ton mot de passe")).toBeVisible();
});

test("le délai s'est fermé chez l'agent : la fenêtre redemande le mot de passe sans perdre la saisie", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  const first = await ready(page);
  await first.getByLabel("Ton mot de passe").fill(OWN);
  await first.getByLabel("Identifiant").fill("sophie");
  await first.getByLabel("Mot de passe du nouveau compte").fill(GOOD);
  await first.getByLabel("Confirme le mot de passe").fill(GOOD);
  await first.getByRole("button", { name: "Créer", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  // Le délai est ouvert : une deuxième création n'a pas de champ…
  await page.getByRole("button", { name: "Ajouter un compte" }).click();
  const second = await ready(page);
  await expect(second.getByLabel("Ton mot de passe")).toHaveCount(0);
  await second.getByLabel("Identifiant").fill("marc");
  await second.getByLabel("Mot de passe du nouveau compte").fill(GOOD);
  await second.getByLabel("Confirme le mot de passe").fill(GOOD);
  // … mais il se ferme chez l'agent avant l'envoi.
  await simReauth(page, "close");
  await second.getByRole("button", { name: "Créer", exact: true }).click();
  await expect(second.getByText("Le délai est terminé.")).toBeVisible();
  await expect(second.getByLabel("Ton mot de passe")).toBeVisible();
  await expect(second.getByLabel("Identifiant")).toHaveValue("marc");
  await expect(second.getByLabel("Mot de passe du nouveau compte")).toHaveValue(GOOD);
  await second.getByLabel("Ton mot de passe").fill(OWN);
  await second.getByRole("button", { name: "Créer", exact: true }).click();
  await expect(page.locator(".toast").filter({ hasText: "Compte marc créé." })).toBeVisible();
});

test("un mot de passe faux reste sous le champ, la fenêtre reste ouverte, le champ est vidé", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await row(page, "paul")
    .getByRole("button", { name: /^Changer le mot de passe/ })
    .click();
  const open = await ready(page);
  await open.getByLabel("Nouveau mot de passe", { exact: true }).fill(GOOD);
  await open.getByLabel("Confirme le nouveau mot de passe").fill(GOOD);
  await open.getByLabel("Ton mot de passe").fill("Faux-Mot-De-Passe-1");
  await open.getByRole("button", { name: "Changer le mot de passe" }).click();
  await expect(open.getByText("Mot de passe actuel incorrect.")).toBeVisible();
  await expect(open.getByLabel("Ton mot de passe")).toHaveValue("");
  await expect(dialog(page)).toHaveCount(1);
});

test("poste sans clé : rien ne part, l'écran dit quoi faire et propose de se reconnecter", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setDevice(
      "forge",
      "none",
      false,
    ),
  );
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  const open = await ready(page);
  await expect(open.locator("[data-reauth-no-key]")).toContainText(
    "Ce poste n'est pas encore enregistré",
  );
  await expect(
    open.getByRole("button", { name: "Me reconnecter pour enregistrer ce poste" }),
  ).toBeVisible();
  await expect(open.getByLabel("Ton mot de passe")).toHaveCount(0);
  await expect(open.getByRole("button", { name: "Supprimer", exact: true })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  expect((await calls(page)).filter((call) => call.startsWith("account delete"))).toHaveLength(0);
  await shoot(page, "reauth-sans-cle");
});

test("agent qui n'annonce pas la confirmation : aucun acte, la fenêtre dit de mettre l'agent à jour", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await simReauth(page, "unsupported");
  await row(page, "lea")
    .getByRole("button", { name: /^Supprimer/ })
    .click();
  const open = dialog(page);
  // Comme la liaison : aucun acte ne part, ni champ ni bouton d'envoi, l'écran le dit.
  await expect(open.locator("[data-reauth-agent-old]")).toContainText("Mets à jour l'agent");
  await expect(open.getByLabel("Ton mot de passe")).toHaveCount(0);
  await expect(open.getByRole("button", { name: "Supprimer", exact: true })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  expect((await calls(page)).filter((call) => call.startsWith("account delete"))).toHaveLength(0);
});

test("page Sécurité : la ligne « Demander mon mot de passe » se lit et se change avec le mot de passe", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  const line = page.locator("[data-reauth-setting]");
  await expect(line).toContainText("Demander mon mot de passe");
  await expect(line.getByRole("radio", { name: "Toutes les 5 minutes" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await line.getByRole("radio", { name: "À chaque action" }).click();
  const open = await ready(page);
  await expect(open.getByText("Ton mot de passe te sera demandé à chaque action")).toBeVisible();
  // Rien n'est parti avant la confirmation ; un réglage est un acte : toujours confirmé.
  expect((await calls(page)).filter((call) => call.startsWith("reauth setting"))).toHaveLength(0);
  await open.getByLabel("Ton mot de passe").fill(OWN);
  await open.getByRole("button", { name: "Changer", exact: true }).click();
  await expect(line.getByRole("radio", { name: "À chaque action" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await shoot(page, "reauth-reglage");
});
