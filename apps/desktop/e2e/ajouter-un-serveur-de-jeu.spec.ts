import { expect, type Page, test } from "@playwright/test";

// « Ajouter un serveur de jeu » de bout en bout (HRT-243 à HRT-247), sur le pont SIMULÉ : de la page des services au
// serveur de jeu en marche, par l'écran des trois chemins, le Catalogue, les trois étapes dans la page (Réglages, Mémoire et
// processeur, Vérifiez avant de lancer) et l'écran de fin. La chaîne réelle agent, liaison, coquille est prouvée côté Rust.
const PASSWORD = "Correct-Horse-9";

type Sim = {
  deploys: {
    running(serverId: string): Array<{ operationId: string; key: string }>;
    succeed(serverId: string, opId: string): void;
  };
};

const sim = <T>(page: Page, run: (sim: Sim) => T) =>
  page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString());

test("de la page des services à l'écran de fin : trois chemins, Catalogue, trois étapes, fin", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/services");
  await page.locator("[data-deploy-open]").click();
  await expect(page.locator("[data-add-game-server]")).toBeVisible();
  await page
    .locator("[data-add-path-catalog]")
    .getByRole("button", { name: "Ouvrir le Catalogue" })
    .click();
  await expect(page.locator("[data-catalogue]")).toContainText("Terraria");
  await page.getByRole("button", { name: "Voir la fiche de Terraria" }).click();
  await page.locator("[data-fiche-install]").click();

  // Réglages : l'essentiel, l'encart de résumé, la barre qui nomme le champ à corriger.
  await expect(page.locator("[data-install-aside]")).toContainText(
    "Rien n'est créé avant l'étape « Vérifiez avant de lancer ».",
  );
  await page.getByLabel("Mot de passe du serveur").fill("hunter2-jeu");
  await page.locator("[data-install-advanced-summary]").click();
  await page.getByLabel("Nombre de joueurs").fill("999");
  await page.getByLabel("Nombre de joueurs").blur();
  await expect(page.locator("[data-install-reason]")).toContainText(
    "Corrigez le champ « Nombre de joueurs » pour continuer.",
  );
  await page.getByLabel("Nombre de joueurs").fill("12");
  await page.locator("[data-install-next]").click();

  // Mémoire et processeur, puis la vérification dans la page.
  await expect(page.locator("[data-install-resources]")).toBeVisible();
  await page.locator("[data-install-next]").click();
  const review = page.locator("[data-review]");
  await expect(review).toContainText("Voici ce que Hearth va faire sur");
  await expect(review.locator("[data-review-what] li")).toHaveCount(4);
  await expect(review).not.toContainText("hunter2-jeu");
  // Les points d'attention (réglages gravés, processeur non vérifié) se cochent avant le lancement.
  await expect(review.locator("[data-review-reason]")).toHaveText(
    "Cochez les points d'attention et saisissez votre mot de passe pour lancer.",
  );
  for (const box of await review.locator("[data-sum-attention] input[type=checkbox]").all()) {
    await box.check();
  }
  await expect(review.locator("[data-review-reason]")).toHaveText(
    "Saisissez votre mot de passe pour lancer.",
  );
  await review.locator("[data-reauth-field] input").fill(PASSWORD);
  await review.getByRole("button", { name: "Installer le serveur de jeu" }).click();

  // Progression, puis l'écran de fin.
  await expect(page.locator("[data-deploy-progress]")).toBeVisible();
  const op = (await sim(page, (s) => s.deploys.running("forge")))[0];
  await sim(page, (s) => {
    const first = s.deploys.running("forge")[0];
    if (first) s.deploys.succeed("forge", first.operationId);
  });
  const done = page.locator("[data-install-done]");
  await expect(done.locator("[data-done-title]")).toContainText("Le serveur de jeu « terraria » ");
  await expect(done.locator("[data-game-address]")).toBeVisible();
  await expect(done.locator("[data-game-ports]")).toBeVisible();
  await done.locator("[data-done-open]").click();
  await expect(page).toHaveURL(new RegExp(`services[?]service=${op?.key}`));
});

test("le badge d'état d'une carte du Catalogue s'affiche entier, à 1 100 px comme plus étroit (HRT-241)", async ({
  page,
}) => {
  for (const width of [1100, 900]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto("/?nodev#/servers/forge/catalogue");
    await expect(page.locator("[data-fiche-card]").first()).toBeVisible();
    const cards = page.locator("[data-fiche-card]");
    for (let index = 0; index < (await cards.count()); index += 1) {
      const card = cards.nth(index);
      const badge = card.locator("[data-fiche-badge]");
      const cardBox = await card.boundingBox();
      const badgeBox = await badge.boundingBox();
      expect(badgeBox && cardBox && badgeBox.x + badgeBox.width <= cardBox.x + cardBox.width).toBe(
        true,
      );
      const clipped = await badge.evaluate((el) => el.scrollWidth > el.clientWidth);
      expect(clipped, `largeur ${width}, carte ${index}`).toBe(false);
    }
  }
});
