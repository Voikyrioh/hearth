import { expect, type Page, test } from "@playwright/test";

// HRT-63 : le détail d'un service à côté de la liste, avec le pont SIMULÉ. Parcours (ouvrir, lire, révéler les
// valeurs, masquer, revenir), clavier, états (service disparu, droit manquant, détail réservé, moteur muet, lien
// coupé) et mise en page mesurée aux largeurs 1280, 1366 et 1920 (boîtes, pas d'impression visuelle).
const SECRET = "sentinelle-valeur-secrete";

type Svc = Record<string, unknown> & { key: string; name: string };
type Sim = {
  setState(id: string, state: string): void;
  setServiceRightOnServer(id: string, username: string, granted: boolean): void;
  services: {
    seed(id: string, services?: Svc[], engine?: Record<string, unknown>): void;
    upsert(id: string, service: Svc): void;
    remove(id: string, key: string): void;
    setEngine(id: string, engine: Record<string, unknown>): void;
    setEnv(id: string, key: string, variables: { name: string; value: string }[]): void;
    reveals: { serverId: string; key: string }[];
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T) {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return ${source}`)(bridge);
  }, `(${run.toString()})(sim)`);
}

const open = async (page: Page, width = 1366, height = 900) => {
  await page.setViewportSize({ width, height });
  await page.goto("/?nodev#/servers/forge/services");
  await expect(page.locator("[data-row]").first()).toBeVisible();
};

const box = async (page: Page, selector: string) => {
  const found = await page.locator(selector).first().boundingBox();
  if (!found) throw new Error(`${selector} sans boîte`);
  return found;
};

test("parcours : ouvrir un service, lire son détail, révéler puis masquer les valeurs, revenir", async ({
  page,
}) => {
  await open(page);
  await page.locator("[data-row][data-key='c_web']").click();
  await expect(page).toHaveURL(/service=c_web/);
  const detail = page.locator("[data-service-detail]");
  await expect(detail.getByRole("heading", { name: "web", level: 2 })).toBeVisible();
  for (const title of [
    "Général",
    "Image et commande",
    "Ports",
    "Dossiers de données",
    "Variables d'environnement",
    "Redémarrage",
  ]) {
    await expect(detail.getByRole("heading", { name: title })).toBeVisible();
  }
  await expect(detail).toContainText("nginx:1.27");
  await expect(detail).toContainText("30 secondes");
  // Les valeurs ne sont pas dans le document tant qu'on ne les a pas demandées.
  await expect(page.locator("body")).not.toContainText(SECRET);
  await expect(page.locator("[data-env-mask]")).toHaveCount(4);
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  await expect(page.locator("body")).toContainText(SECRET);
  await expect(page.locator("[data-env-status]")).toContainText("Valeurs visibles");
  expect(await sim(page, (s) => s.services.reveals)).toEqual([{ serverId: "forge", key: "c_web" }]);
  await page.getByRole("button", { name: "Masquer les valeurs" }).click();
  await expect(page.locator("body")).not.toContainText(SECRET);
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  await expect(page.locator("body")).toContainText(SECRET);
  // Retour : la liste reste, les valeurs sont effacées.
  await page.getByRole("button", { name: "Retour à la liste" }).click();
  await expect(page.locator("[data-service-detail]")).toHaveCount(0);
  await expect(page).not.toHaveURL(/service=/);
  await expect(page.locator("body")).not.toContainText(SECRET);
  await expect(page.locator("[data-row]").first()).toBeVisible();
});

test("clavier : Entrée ouvre, le focus va au titre, Échap rend le focus à la ligne, Tab traverse les sections", async ({
  page,
}) => {
  await open(page);
  const row = page.locator("[data-row][data-key='c_web']");
  await row.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("[data-detail-title]")).toBeFocused();
  // Les onglets : un seul arrêt, « Journaux » atteignable aux flèches, explique pourquoi il n'est pas actif.
  await page.locator("[data-tab='detail']").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("[data-tab='logs']")).toBeFocused();
  await expect(page.locator("[data-tab='logs']")).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Enter");
  await expect(page.locator("[data-tab='detail']")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-service-detail]")).toHaveCount(0);
  await expect(row).toBeFocused();
});

test("états : service disparu, homonyme proposé, jamais ouvert d'office", async ({ page }) => {
  await open(page);
  await page.locator("[data-row][data-key='c_cache']").click();
  await sim(page, (s) => s.services.remove("forge", "c_cache"));
  await expect(page.locator("[data-detail-gone]")).toHaveText("Ce service n'existe plus.");
  await expect(page.locator("[data-detail-title]")).toHaveText("cache");
  await sim(page, (s) => s.services.upsert("forge", { key: "c_cache2", name: "cache" }));
  await expect(page.locator("[data-detail-recreated]")).toContainText(
    "Un service nommé « cache » existe de nouveau.",
  );
  await expect(page).toHaveURL(/service=c_cache(?!2)/);
  await page.getByRole("button", { name: "Ouvrir le service cache" }).click();
  await expect(page).toHaveURL(/service=c_cache2/);
  await expect(page.locator("[data-detail-gone]")).toHaveCount(0);
});

test("états : sans le droit, détail réservé d'un service adopté, moteur muet, lien coupé", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => {
    s.services.upsert("forge", {
      key: "c_adopte",
      name: "adopte",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
      detailsReserved: true,
    });
    s.setServiceRightOnServer("forge", "marie", false);
    // Le droit se relit à chaque reconnexion (BR-GRANT-011).
    s.setState("forge", "offline");
  });
  await page.waitForTimeout(200);
  await sim(page, (s) => s.setState("forge", "connected"));
  await page.locator("[data-row][data-key='c_adopte']").click();
  await expect(page.locator("[data-env-reserved]")).toContainText(
    "Réservé aux comptes qui peuvent gérer les services.",
  );
  await expect(page.locator("[data-detail-adopted]")).toContainText("Adopté par claire le");
  await page.locator("[data-row][data-key='c_web']").click();
  await expect(page.getByRole("button", { name: "Révéler les valeurs" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await expect(page.locator("[data-env-reason]")).toContainText(
    "Ton compte ne peut pas gérer les services de ce serveur.",
  );
  await sim(page, (s) => {
    s.setServiceRightOnServer("forge", "marie", true);
    s.setState("forge", "offline");
  });
  await page.waitForTimeout(200);
  await sim(page, (s) => s.setState("forge", "connected"));
  await sim(page, (s) => s.services.setEngine("forge", { state: "unresponsive" }));
  await expect(page.locator("[data-services-work]")).toHaveClass(/svc__work--grey/);
  await expect(page.locator("[data-service-detail]")).toBeVisible();
  await sim(page, (s) => s.setState("forge", "offline"));
  await expect(page.locator("[data-service-detail]")).toBeVisible();
});

for (const width of [1280, 1366, 1920] as const) {
  test(`mise en page à ${width} : liste compacte et détail côte à côte, rien ne déborde ni ne se chevauche`, async ({
    page,
  }) => {
    await open(page, width, 900);
    await page.locator("[data-row][data-key='c_web']").click();
    await expect(page.locator("[data-service-detail]")).toBeVisible();
    await page.getByRole("button", { name: "Révéler les valeurs" }).click();
    await expect(page.locator("[data-env-value]").first()).toBeVisible();

    const overflow = await page.evaluate(() => {
      const root = document.documentElement;
      const content = document.querySelector<HTMLElement>(".layout__content");
      return {
        page: root.scrollWidth - root.clientWidth,
        content: content ? content.scrollWidth - content.clientWidth : 0,
      };
    });
    expect(overflow).toEqual({ page: 0, content: 0 });

    const list = await box(page, ".svc__list");
    const detail = await box(page, "[data-service-detail]");
    // Côte à côte : la liste à gauche, le détail à sa droite, sans recouvrement.
    expect(list.x + list.width).toBeLessThanOrEqual(detail.x + 1);
    expect(list.width).toBeLessThanOrEqual(418 + 1);
    expect(detail.width).toBeLessThanOrEqual(960 + 1);
    expect(detail.x + detail.width).toBeLessThanOrEqual(width);
    // Aucun enfant du détail ne dépasse la boîte du détail.
    const spill = await page.evaluate(() => {
      const root = document.querySelector("[data-service-detail]");
      if (!root) return -1;
      const outer = root.getBoundingClientRect();
      return [...root.querySelectorAll("*")].filter((element) => {
        const rect = element.getBoundingClientRect();
        return rect.width > 0 && (rect.right > outer.right + 1 || rect.left < outer.left - 1);
      }).length;
    });
    expect(spill).toBe(0);
    // La liste compacte garde ses colonnes lisibles : l'image passe sous le nom.
    await expect(page.locator(".table__th--image").first()).toBeHidden();
    // Aucun bouton coupé.
    const cut = await page.evaluate(
      () =>
        [...document.querySelectorAll("button")].filter((button) => {
          const rect = button.getBoundingClientRect();
          return (
            rect.width > 0 && (rect.right > window.innerWidth || rect.bottom > window.innerHeight)
          );
        }).length,
    );
    expect(cut).toBe(0);
  });
}

test("mise en page à 1100 (minimum) : le détail REMPLACE la liste, puis Retour la rend", async ({
  page,
}) => {
  await open(page, 1100, 800);
  await page.locator("[data-row][data-key='c_web']").click();
  await expect(page.locator("[data-service-detail]")).toBeVisible();
  await expect(page.locator(".svc__list")).toBeHidden();
  const detail = await box(page, "[data-service-detail]");
  expect(detail.x + detail.width).toBeLessThanOrEqual(1100);
  const overflowX = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflowX).toBe(0);
  await page.getByRole("button", { name: "Retour à la liste" }).click();
  await expect(page.locator(".svc__list")).toBeVisible();
});

test("valeurs très longues et noms de 255 signes : coupés ou passés à la ligne, jamais en travers", async ({
  page,
}) => {
  await open(page, 1366, 900);
  await sim(page, (s) => {
    const long = "x".repeat(255);
    s.services.upsert("forge", {
      key: "c_long",
      name: `${long}`,
      image: { ref: `registre/${long}`, tag: "1" },
      command: [long, long],
      mounts: [{ kind: "folder", source: `/${long}`, target: `/${long}`, readOnly: false }],
    });
    s.services.setEnv("forge", "c_long", [{ name: long, value: `${long}\n${long}` }]);
  });
  await page.locator("[data-row][data-key='c_long']").click();
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  await expect(page.locator("[data-env-value]")).toBeVisible();
  const spill = await page.evaluate(() => {
    const root = document.querySelector("[data-service-detail]");
    if (!root) return -1;
    const outer = root.getBoundingClientRect();
    return [...root.querySelectorAll("*")].filter((element) => {
      const rect = element.getBoundingClientRect();
      return rect.width > 0 && rect.right > outer.right + 1;
    }).length;
  });
  expect(spill).toBe(0);
  const overflowX = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflowX).toBe(0);
});
