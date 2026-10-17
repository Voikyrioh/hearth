import { expect, type Page, test } from "@playwright/test";

// BR-COMPAT-001 point 10 : une liste dont l'agent a envoyé des éléments illisibles pour cette version le dit, sous la
// liste (comptes, postes de confiance, journal d'activité, services). Pont SIMULÉ : le compte est posé par le test.

type Sim = {
  accounts: { unreadable: Map<string, number> };
  audit: { unreadable: Map<string, number> };
  devices: { seed(server: unknown, devices: unknown[], options: { unreadable?: number }): void };
  services: {
    seed(id: string, services: unknown[]): void;
    setUnreadable(id: string, n: number): void;
  };
};

const MANY =
  "3 éléments ne sont pas lisibles par cette version de Hearth. Mettez Hearth à jour sur ce PC.";
const ONE =
  "1 élément n'est pas lisible par cette version de Hearth. Mettez Hearth à jour sur ce PC.";

const note = (page: Page) => page.locator("[data-unreadable]");

test("comptes : la ligne dit combien de comptes ne sont pas lisibles, et disparaît quand tout l'est", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/accounts");
  await expect(page.locator("table")).toBeVisible();
  await expect(note(page)).toHaveCount(0);
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.accounts.unreadable.set("forge", 3),
  );
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.goto("/?nodev#/servers/forge/accounts");
  await expect(note(page)).toHaveText(MANY);
  await page.setViewportSize({ width: 1366, height: 800 });
  await expect(note(page)).toBeVisible();
});

test("postes de confiance : la ligne est au singulier pour un seul poste illisible", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.evaluate(() => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    sim.devices.seed({ id: "forge" }, [{ name: "salon", current: true }], { unreadable: 1 });
  });
  await page.goto("/?nodev#/servers/forge/security");
  await expect(note(page)).toHaveText(ONE);
});

test("journal d'activité : la ligne dit combien d'entrées ne sont pas lisibles", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.audit.unreadable.set("forge", 3),
  );
  await page.goto("/?nodev#/servers/forge/audit");
  await expect(note(page)).toHaveText(MANY);
});

test("services : la ligne dit combien de services ne sont pas lisibles", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/services");
  await page.evaluate(() => {
    const sim = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    sim.services.seed("forge", [{ key: "c_1", name: "web" }]);
    sim.services.setUnreadable("forge", 3);
  });
  await expect(note(page)).toHaveText(MANY);
});
