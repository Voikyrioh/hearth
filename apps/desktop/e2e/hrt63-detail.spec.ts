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
    setDetailsKnown(id: string, known: boolean): void;
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
    "Programme et commande",
    "Ports",
    "Dossiers de données",
    "Réglages (variables d'environnement)",
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
  // Les onglets : un seul arrêt, « Journaux » atteignable aux flèches et actif (HRT-67), Entrée l'ouvre.
  await page.locator("[data-tab='detail']").focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.locator("[data-tab='logs']")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("[data-tab='logs']")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowLeft");
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
    "Votre compte ne peut pas gérer les services de ce serveur.",
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

// ── Corrections de la revue de la PR 93 ──────────────────────────────────────────────────────
// Captures « après » : seulement si `HRT63_SHOTS` désigne un dossier (jamais dans la CI).
const shot = async (page: Page, name: string) => {
  const dir = process.env.HRT63_SHOTS;
  if (dir) await page.screenshot({ path: `${dir}/${name}.png` });
};

test("redémarrage hors ligne : la liste relue du disque n'affirme ni « aucune variable » ni « commande par défaut »", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => s.services.setDetailsKnown("forge", false));
  await page.locator("[data-row][data-key='c_web']").click();
  const detail = page.locator("[data-service-detail]");
  await expect(detail).toContainText(
    "Pas encore lue : elle se lira quand le lien avec le serveur sera revenu.",
  );
  await expect(detail).toContainText(
    "Pas encore lus : ils se liront quand le lien avec le serveur sera revenu.",
  );
  await expect(detail).not.toContainText("Aucune variable");
  await expect(detail).not.toContainText("Commande par défaut du programme");
  await expect(page.locator("[data-env-mask]")).toHaveCount(0);
  await shot(page, "1-hors-ligne-detail-non-lu");
  // Le serveur répond de nouveau : la vraie liste revient, la phrase part.
  // (un instantané complet : seul lui porte des détails lus, un lot ne change pas ce que dit l'écran)
  await sim(page, (s) =>
    s.services.seed("forge", [
      { key: "c_web", name: "web", command: ["nginx", "-g", "daemon off;"] },
    ]),
  );
  await expect(detail).not.toContainText("Pas encore lue");
  await expect(page.locator("[data-detail-command]")).toHaveText("nginx -g daemon off;");
});

test("texte de 64 Ko : la commande, l'image, les chemins et les noms défilent dans leur zone, le reste de la page reste atteignable", async ({
  page,
}) => {
  await open(page, 1366, 800);
  await sim(page, (s) => {
    const big = "x".repeat(65_536);
    const lines = Array.from({ length: 2000 }, (_, i) => `ligne-${i}`).join("\n");
    s.services.upsert("forge", {
      key: "c_big",
      name: "gros",
      origin: "hearth",
      image: { ref: `registre/${big}`, tag: "1" },
      command: [big, lines],
      mounts: [{ kind: "folder", source: `/${big}`, target: `/${big}`, readOnly: false }],
    });
    s.services.setEnv("forge", "c_big", [{ name: big, value: lines }]);
  });
  await page.locator("[data-row][data-key='c_big']").click();
  await expect(page.locator("[data-detail-command] [data-bounded]")).toBeVisible();
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  await expect(page.locator("[data-env-value]")).toBeVisible();
  const heights = await page.evaluate(() => {
    const zones = [
      ...document.querySelectorAll<HTMLElement>("[data-service-detail] [data-bounded]"),
    ];
    const root = document.querySelector<HTMLElement>("[data-service-detail]");
    return {
      tallest: Math.max(...zones.map((zone) => zone.getBoundingClientRect().height)),
      scrolling: zones.filter((zone) => zone.scrollHeight > zone.clientHeight + 1).length,
      detail: root ? root.scrollHeight : -1,
    };
  });
  // Chaque zone tient dans sa borne (160 px au plus) ; le détail entier reste de l'ordre de quelques écrans.
  expect(heights.tallest).toBeLessThanOrEqual(170);
  expect(heights.scrolling).toBeGreaterThanOrEqual(5);
  expect(heights.detail).toBeLessThan(3500);
  // Les sections qui suivent sont atteignables.
  await page.getByRole("heading", { name: "Redémarrage" }).scrollIntoViewIfNeeded();
  await expect(page.getByRole("heading", { name: "Redémarrage" })).toBeInViewport();
  // Au clavier : la zone de la commande prend le focus et défile (Page suivante).
  const command = page.locator("[data-detail-command] [data-bounded]");
  await command.focus();
  await expect(command).toBeFocused();
  await page.keyboard.press("PageDown");
  await expect.poll(() => command.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  await command.scrollIntoViewIfNeeded();
  await shot(page, "2-texte-64ko-commande");
  await page.locator("[data-env-section]").scrollIntoViewIfNeeded();
  await shot(page, "2-texte-64ko-variables");
  const overflowX = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflowX).toBe(0);
});

test("sans le droit de gérer les services : ni commande ni noms de variables, une phrase dit pourquoi", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => {
    s.setServiceRightOnServer("forge", "marie", false);
    s.setState("forge", "offline");
  });
  await page.waitForTimeout(200);
  await sim(page, (s) => s.setState("forge", "connected"));
  await page.locator("[data-row][data-key='c_web']").click();
  const detail = page.locator("[data-service-detail]");
  await expect(detail.locator("[data-detail-command-reserved]")).toContainText(
    "Réservé aux comptes qui peuvent gérer les services.",
  );
  await expect(detail.locator("[data-env-note]")).toContainText(
    "Réservé aux comptes qui peuvent gérer les services.",
  );
  await expect(detail).not.toContainText("daemon off");
  await expect(page.locator("[data-env-mask]")).toHaveCount(0);
  await shot(page, "3-sans-le-droit-commande-reservee");
});

test("l'en-tête du détail reste visible au défilement, et « Ce service n'existe plus. » ne sort pas de l'écran", async ({
  page,
}) => {
  await open(page, 1366, 700);
  await page.locator("[data-row][data-key='c_web']").click();
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  const detail = page.locator("[data-service-detail]");
  await detail.evaluate((element) => element.scrollTo(0, element.scrollHeight));
  const before = await box(page, "[data-detail-title]");
  const frame = await box(page, "[data-service-detail]");
  // Le titre reste dans le haut du volet alors qu'on est tout en bas.
  expect(before.y).toBeGreaterThanOrEqual(frame.y - 1);
  expect(before.y).toBeLessThan(frame.y + 120);
  await shot(page, "7-en-tete-colle");
  await sim(page, (s) => s.services.remove("forge", "c_web"));
  await expect(page.locator("[data-detail-gone]")).toBeInViewport();
  await shot(page, "7-en-tete-colle-service-disparu");
});

test("lien coupé : la raison est écrite près du bouton « Révéler les valeurs » désactivé", async ({
  page,
}) => {
  await open(page);
  await page.locator("[data-row][data-key='c_web']").click();
  await sim(page, (s) => s.setState("forge", "offline"));
  const toggle = page.getByRole("button", { name: "Révéler les valeurs" });
  await expect(toggle).toHaveAttribute("aria-disabled", "true");
  const reason = page.locator("[data-env-engine]");
  await expect(reason).toHaveText("Indisponible tant que le serveur est hors ligne.");
  await expect(toggle).toHaveAttribute("aria-describedby", /.+/);
  await page.locator("[data-env-section]").scrollIntoViewIfNeeded();
  await shot(page, "7-lien-coupe-raison");
});

test("caractères bidirectionnels et invisibles : remplacés par un repère visible, nulle part dans le document", async ({
  page,
}) => {
  await open(page);
  await sim(page, (s) => {
    const bidi = String.fromCharCode(0x202e);
    const zero = String.fromCharCode(0x200b);
    s.services.upsert("forge", {
      key: "c_h",
      name: `hostile${bidi}etoN`,
      origin: "hearth",
      image: { ref: `reg${zero}istre`, tag: bidi },
      command: [`run${bidi}`],
      mounts: [{ kind: "folder", source: `/a${bidi}b`, target: `/c${zero}d`, readOnly: false }],
    });
    s.services.setEnv("forge", "c_h", [{ name: `N${bidi}`, value: `v${bidi}${zero}` }]);
  });
  await page.locator("[data-row][data-key='c_h']").click();
  await page.getByRole("button", { name: "Révéler les valeurs" }).click();
  await expect(page.locator("[data-env-value]")).toBeVisible();
  const found = await page.evaluate(() => {
    const text = document.body.innerText + document.body.innerHTML;
    return [0x202e, 0x200b].filter((code) => text.includes(String.fromCharCode(code)));
  });
  expect(found).toEqual([]);
  await expect(page.locator("[data-detail-title]")).toHaveText("hostile·etoN");
});
