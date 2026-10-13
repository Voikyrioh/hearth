import { expect, type Page, test } from "@playwright/test";

// HRT-133 : fermer les sessions d'un compte qui peut gérer les services, et l'information du compte visé.
// Pont SIMULÉ (aucun réseau) ; la chaîne réelle (agent, liaison, coquille) est prouvée côté Rust. Serveur
// « forge » : marie (toi, administratrice) et paul (2 sessions ouvertes).
// Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const OWN = "Correct-Horse-9";

type Sim = {
  setServiceRightOnServer(serverId: string, username: string, granted: boolean): void;
  emitNotice(notice: { kind: string; serverId: string | null; by?: string; at?: string }): void;
};

async function setRights(page: Page, rights: { marie: boolean; paul: boolean }) {
  await page.evaluate((value) => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    sim.setServiceRightOnServer("forge", "marie", value.marie);
    sim.setServiceRightOnServer("forge", "paul", value.paul);
  }, rights);
}

async function openDashboard(page: Page) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
}

async function goTo(page: Page, route: "accounts" | "dashboard") {
  await page.evaluate((target) => {
    window.location.hash = `#/servers/forge/${target}`;
  }, route);
}

const row = (page: Page, username: string) => page.locator(`tr[data-account="${username}"]`);
const dialog = (page: Page) => page.locator("dialog[open]");

async function openCloseDialog(page: Page) {
  await row(page, "paul")
    .getByRole("button", { name: /^Fermer les 2 sessions/ })
    .click();
  await expect(
    dialog(page).getByRole("heading", { name: "Fermer les sessions de paul ?" }),
  ).toBeVisible();
}

test("tu n'as pas le droit, paul l'a : la fenêtre dit que ses postes de confiance sont gardés", async ({
  page,
}) => {
  await openDashboard(page);
  await setRights(page, { marie: false, paul: true });
  await goTo(page, "accounts");
  await expect(row(page, "paul")).toBeVisible();
  await openCloseDialog(page);
  await expect(dialog(page).locator("[data-sessions-holder-note]")).toHaveText(
    "Ce compte peut gérer les services. Il devra se reconnecter ; ses postes de confiance sont gardés.",
  );
  for (const width of [1366, 2560] as const) {
    await page.setViewportSize({ width, height: width === 1366 ? 800 : 1440 });
    await dialog(page).screenshot({ path: `${SHOTS}/hrt133-confirm-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
  await dialog(page).getByLabel("Ton mot de passe").fill(OWN);
  await dialog(page).getByRole("button", { name: "Fermer les sessions" }).click();
  await expect(page.locator(".toast")).toContainText("Sessions de paul fermées.");
  await expect(row(page, "paul").locator("td").nth(3)).toHaveText("0");
});

test("les autres cas : la fenêtre ne parle pas des postes de confiance", async ({ page }) => {
  await openDashboard(page);
  // Toi et paul avez le droit : le geste garde son effet d'avant, rien n'est promis.
  await setRights(page, { marie: true, paul: true });
  await goTo(page, "accounts");
  await openCloseDialog(page);
  await expect(dialog(page).locator("[data-sessions-holder-note]")).toHaveCount(0);
  await dialog(page).getByRole("button", { name: "Annuler" }).click();
  // Paul n'a pas le droit : rien non plus.
  await goTo(page, "dashboard");
  await setRights(page, { marie: false, paul: false });
  await goTo(page, "accounts");
  await openCloseDialog(page);
  await expect(dialog(page).locator("[data-sessions-holder-note]")).toHaveCount(0);
});

test("à ta connexion suivante : qui a fermé tes sessions et quand, une fois, fermable", async ({
  page,
}) => {
  await openDashboard(page);
  await page.evaluate(() => {
    (window as unknown as { __hearthSim: Sim }).__hearthSim.emitNotice({
      kind: "sessions_closed",
      serverId: "forge",
      by: "paul",
      at: "2026-10-13T18:30:00.000Z",
    });
  });
  const info = page.locator("[data-sessions-closed]");
  await expect(info).toBeVisible();
  await expect(info.locator("[data-sessions-closed-text]")).toContainText(
    "Tes sessions ont été fermées par paul le",
  );
  for (const width of [1366, 2560] as const) {
    await page.setViewportSize({ width, height: width === 1366 ? 800 : 1440 });
    await page.screenshot({ path: `${SHOTS}/hrt133-info-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
  await page.getByRole("button", { name: "Fermer l'information" }).click();
  await expect(info).toHaveCount(0);
  // Elle ne revient pas en changeant de page.
  await goTo(page, "accounts");
  await expect(page.locator("[data-sessions-closed]")).toHaveCount(0);
});

test("supprimer un compte qui peut gérer les services : indisponible sans le droit, avec l'explication", async ({
  page,
}) => {
  await openDashboard(page);
  await setRights(page, { marie: false, paul: true });
  await goTo(page, "accounts");
  const remove = row(page, "paul").getByRole("button", { name: /^Supprimer paul/ });
  await expect(remove).toHaveAttribute("aria-disabled", "true");
  await remove.hover();
  await expect(page.getByRole("tooltip")).toHaveText(
    "Ce compte peut gérer les services. Seul un compte qui le peut aussi, ou la commande sur le serveur, peut le supprimer.",
  );
  // Un compte sans le droit se supprime comme avant ; le rôle d'un titulaire reste modifiable.
  await expect(
    row(page, "lea").getByRole("button", { name: /^Supprimer lea/ }),
  ).not.toHaveAttribute("aria-disabled", "true");
  await expect(
    row(page, "paul").getByRole("button", { name: /^Changer le rôle paul/ }),
  ).not.toHaveAttribute("aria-disabled", "true");
  // Avec le droit, le geste revient.
  await goTo(page, "dashboard");
  await setRights(page, { marie: true, paul: true });
  await goTo(page, "accounts");
  await expect(
    row(page, "paul").getByRole("button", { name: /^Supprimer paul/ }),
  ).not.toHaveAttribute("aria-disabled", "true");
});
