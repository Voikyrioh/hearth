import { expect, type Page, test } from "@playwright/test";

// Revue UX de Nora du 24/10/2026 (déploiement d'une image et Catalogue), sur le pont SIMULÉ : un test par constat dont
// la correction est vérifiable dans un navigateur (les autres sont prouvés par Vitest). Serveur « forge » :
// administrateur avec le droit de gérer les services.
const PASSWORD = "Correct-Horse-9";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
};

const sim = <T>(page: Page, run: (sim: Sim) => T, arg?: unknown) =>
  page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      return new Function("sim", "arg", `return (${source as string})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );

async function boot(page: Page, hash = "#/servers/forge/services") {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) =>
    s.services.seed("forge", [
      {
        key: "h_base",
        name: "base",
        origin: "hearth",
        ports: [{ hostPort: 5432, containerPort: 5432, protocol: "tcp", scope: "all" }],
      },
    ]),
  );
  await page.evaluate((target) => {
    window.location.hash = target;
  }, hash);
}

async function openDeploy(page: Page) {
  await boot(page);
  await expect(page.locator("[data-row]").first()).toBeVisible();
  await page.locator("[data-deploy-open]").click();
  await expect(page.locator("[data-deploy-form]")).toBeVisible();
  await expect(page.getByLabel("Nom du service")).toBeFocused();
  // Les bornes de la machine sont lues : le formulaire est prêt.
  await expect(page.locator("[data-resources]")).toContainText("Valeur exacte");
}

async function fillDeploy(page: Page) {
  await page.getByLabel("Nom du service").fill("web");
  await page.getByLabel("Image", { exact: true }).fill("nginx");
  await page.getByLabel("Version").fill("1.27");
}

const dialog = (page: Page) => page.locator("dialog[open]");

test.describe("constat 2 : le menu répond toujours après une séance de déploiement agitée", () => {
  test("abandons, précédent, rechargement, mots de passe refusés, puis chaque entrée du menu", async ({
    page,
  }) => {
    await openDeploy(page);
    await fillDeploy(page);
    // Précédent : la question est posée, on continue la saisie ; Fermer : même question.
    await page.goBack();
    await expect(dialog(page)).toContainText("Abandonner votre saisie ?");
    await dialog(page).getByRole("button", { name: "Continuer la saisie" }).click();
    await page.getByRole("button", { name: "Fermer" }).click();
    await dialog(page).getByRole("button", { name: "Continuer la saisie" }).click();
    // Sept mots de passe refusés : l'agent fait attendre, la fenêtre reste ouverte.
    await page.locator("[data-deploy-submit]").click();
    await expect(dialog(page)).toContainText("Vérifiez avant de lancer");
    for (let attempt = 0; attempt < 7; attempt++) {
      await dialog(page).locator("[data-reauth-field] input").fill(`faux-${attempt}`);
      await dialog(page).getByRole("button", { name: "Déployer le service" }).click();
    }
    await expect(dialog(page)).toContainText("Trop d'essais.");
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
    // Rechargement (avec la question de fermeture acceptée), puis une saisie abandonnée par le menu.
    page.once("dialog", (native) => void native.accept());
    await page.reload();
    await expect(page.locator("[data-deploy-form]")).toBeVisible();
    await fillDeploy(page);
    await page.locator(".nav__item", { hasText: "Catalogue" }).click();
    await dialog(page).getByRole("button", { name: "Abandonner" }).click();
    await expect(page).toHaveURL(/catalogue$/);
    // Chaque entrée du menu et de la barre répond, l'une après l'autre.
    await page.locator(".nav__item", { hasText: "Comptes" }).click();
    await expect(page).toHaveURL(/accounts$/);
    await page.getByRole("link", { name: "Mes serveurs" }).click();
    await expect(page).toHaveURL(/#\/servers$/);
    await page.goBack();
    await page.locator(".nav__item", { hasText: "Services" }).click();
    await expect(page).toHaveURL(/services$/);
    await page.locator(".nav__item", { hasText: "Catalogue" }).click();
    await expect(page).toHaveURL(/catalogue$/);
  });
});

test("constat 1 : la confirmation dit le mot de passe du compte simulé", async ({ page }) => {
  await openDeploy(page);
  await fillDeploy(page);
  await page.locator("[data-deploy-submit]").click();
  await expect(dialog(page).locator("[data-dev-password]")).toContainText(PASSWORD);
});

test("constat 3 : le tableau de bord, les curseurs et la fiche disent les mêmes chiffres", async ({
  page,
}) => {
  await boot(page, "#/servers/forge/dashboard");
  await expect(page.getByText("16 cœurs").first()).toBeVisible();
  await expect(page.getByText("64 Go").first()).toBeVisible();
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services/deployer";
  });
  await expect(page.locator("[data-resources]")).toContainText("64 Go");
  await expect(page.locator("[data-resources]")).toContainText("16 cœurs");
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/catalogue/terraria";
  });
  await expect(page.locator("[data-fiche-resources]")).toContainText("16 cœurs");
  await expect(page.locator("[data-fiche-resources]")).toContainText("64 Go");
});

test.describe("fiche et installation", () => {
  test("constats 4, 17, 19 : une valeur proposée par son libellé, un seul titre, une phrase sur les besoins inconnus", async ({
    page,
  }) => {
    await boot(page, "#/servers/forge/catalogue/terraria");
    await expect(page.locator("[data-page-title]")).toHaveText("Terraria");
    await expect(page.getByRole("heading", { name: "Terraria", exact: true })).toHaveCount(1);
    await expect(
      page.locator("[data-fiche-question]").filter({ hasText: "Taille du monde" }),
    ).not.toContainText(/La valeur proposée est \d/);
    await expect(page.locator("[data-fiche-questions]")).toContainText(
      "La valeur proposée est Grand.",
    );
    await expect(page.locator("[data-res-none]")).toHaveText(
      "Hearth ne connaît pas les besoins de ce jeu en processeur, en mémoire ni en espace disque.",
    );
    await expect(page.locator("[data-fiche-resources]")).not.toContainText("Non vérifié");
    await expect(page.locator("[data-fiche-disk]")).toHaveCount(0);
  });

  test("constats 16, 5, 17, 15, 13 : curseur dans le nom, caractère refusé en mots, titre à chaque étape, récapitulatif en haut, raison sous le bouton", async ({
    page,
  }) => {
    await boot(page, "#/servers/forge/catalogue/terraria/installer");
    await expect(page.locator('[data-field="name"] input')).toBeFocused();
    const world = page.getByLabel("Nom du monde");
    await world.fill("Mon monde");
    await world.blur();
    await expect(page.locator('[data-question="world_name"]')).toContainText(
      "un caractère que ce jeu ne sait pas lire : l'espace. Essayez « Mon_monde ».",
    );
    await world.fill("Mon_monde");
    await page.locator("[data-install-next]").click();
    await expect(page.locator("[data-page-title]")).toHaveText("Installer Terraria");
    await page.locator("[data-install-next]").click();
    const summary = dialog(page);
    await expect(summary).toContainText("Vérifiez avant de lancer");
    await expect(page.locator("[data-page-title]")).toHaveText("Installer Terraria");
    // La fenêtre s'ouvre en haut, lisible de haut en bas.
    const scrolled = await summary.locator(".dialog__scroll").evaluate((node) => node.scrollTop);
    expect(scrolled).toBe(0);
    await expect(summary.locator("[data-dialog-reason]")).toHaveText(
      "Saisissez votre mot de passe pour continuer.",
    );
    await summary.locator("[data-reauth-field] input").fill(PASSWORD);
    await expect(summary.locator("[data-dialog-reason]")).toHaveCount(0);
  });

  test("constat 20 : Échap sur « Installer avec les valeurs proposées » revient à la fiche", async ({
    page,
  }) => {
    await boot(page, "#/servers/forge/catalogue/terraria");
    await page.locator("[data-fiche-install-proposed]").click();
    await expect(dialog(page)).toContainText("Vérifiez avant de lancer");
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/catalogue\/terraria$/);
    await expect(page.locator("[data-fiche-page]")).toBeVisible();
  });

  test("constat 18 : une aide ne se répète pas dans la même phrase", async ({ page }) => {
    await boot(page, "#/servers/forge/catalogue/terraria/installer");
    const players = page.locator('[data-question="max_players"]');
    await expect(players).toContainText("de 1 à 255");
    await expect(players).not.toContainText("Le nombre est compris entre 1 et 255");
    const world = page.locator('[data-question="world_name"]');
    await expect(world).not.toContainText("Lettres, chiffres, tirets et soulignés seulement.");
  });

  test("constat 21 : toute la carte du Catalogue ouvre la fiche", async ({ page }) => {
    await boot(page, "#/servers/forge/catalogue");
    await page.locator(".fcard__summary").click();
    await expect(page).toHaveURL(/catalogue\/terraria$/);
  });
});

test.describe("formulaire de déploiement", () => {
  test("constats 22, 23 : exemple court dans le champ Image, libellé court sur une ligne", async ({
    page,
  }) => {
    await openDeploy(page);
    const image = page.getByLabel("Image", { exact: true });
    await expect(image).toHaveAttribute("placeholder", "nginx");
    await expect(page.locator('[data-field="image"]')).toContainText(
      "Le nom complet s'écrit registre/organisation/nom",
    );
    const label = page.locator("[data-resources] label", { hasText: "Valeur exacte" }).first();
    const box = await label.boundingBox();
    expect(box?.height ?? 99).toBeLessThan(24);
  });

  test("constat 12 : « Utiliser le port » est un bouton avec un bord, hors de l'encart", async ({
    page,
  }) => {
    await openDeploy(page);
    // Le nom d'abord : une erreur « Le nom est requis » qui apparaît au départ du champ décalerait le bouton sous le clic.
    await page.getByLabel("Nom du service").fill("web");
    await page.getByRole("button", { name: "Ajouter un port" }).click();
    await page.getByLabel("Port du serveur").fill("5432");
    await page.getByLabel("Port du service").fill("80");
    const use = page.locator("[data-port-use]");
    await expect(use).toBeVisible();
    await expect(page.locator("[data-port-suggested] [data-port-use]")).toHaveCount(0);
    const border = await use.evaluate((node) => getComputedStyle(node).borderTopColor);
    expect(border).not.toBe("rgba(0, 0, 0, 0)");
    await use.click();
    await expect(page.getByLabel("Port du serveur")).toHaveValue("5433");
  });

  test("constat 7 : fermer ou recharger avec une saisie en cours pose la question, sans saisie non", async ({
    page,
  }) => {
    await openDeploy(page);
    await page.getByLabel("Nom du service").fill("web");
    const asked = page.waitForEvent("dialog");
    await page.close({ runBeforeUnload: true });
    const question = await asked;
    expect(question.type()).toBe("beforeunload");
    await question.dismiss();
    // « Rester » : la page est toujours là, la saisie aussi.
    expect(page.isClosed()).toBe(false);
    await expect(page.getByLabel("Nom du service")).toHaveValue("web");
    // Sans saisie, la fermeture se fait sans question.
    await page.getByLabel("Nom du service").fill("");
    const closed = page.waitForEvent("close");
    await page.close({ runBeforeUnload: true });
    await closed;
  });
});
