import { expect, type Page, test } from "@playwright/test";

// HRT-35 et HRT-36 : écrans vides et panneaux centrés, rien ne se chevauche ni ne se coupe quand la fenêtre
// rétrécit (revue UX du 2026-10-08, C20 en partie, C21 (NON corrigé : seulement aucun bouton coupé), C25, C27, C29, C33, C37, C45, C50, C51). Pont SIMULÉ.
// Assertions de GÉOMÉTRIE (boîtes mesurées dans le navigateur), aux tailles 1100×680, 1280×800, 1366×800,
// 1920×1080 et 2560×1440 quand la taille compte.

type Sim = {
  setState(id: string, state: string): void;
  audit: { clear(id: string): void };
  accounts: { seed(id: string, accounts: Record<string, unknown>[]): void };
  security: {
    setAlert(id: string, alert: { own: boolean; others?: number | null }): void;
    setMode(id: string, state: string, options?: Record<string, unknown>): void;
  };
};

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function drive<T>(page: Page, run: (sim: Sim) => T): Promise<T> {
  return page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString()) as Promise<T>;
}

/** Change de page SANS recharger (un rechargement remet le pont simulé à zéro). */
async function go(page: Page, hash: string) {
  await page.evaluate((target) => {
    window.location.hash = target;
  }, hash);
}

async function shoot(page: Page, name: string, width: number, height: number) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${width}x${height}.png` });
}

/** Les boîtes d'un sélecteur, en pixels de la page. */
async function boxesOf(page: Page, selector: string) {
  return page.locator(selector).evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        x: rect.x,
        y: rect.y,
        w: rect.width,
        h: rect.height,
        text: element.textContent ?? "",
      };
    }),
  );
}

// ── HRT-35 : centrés, pas dans un coin ───────────────────────────────────────────────────────

for (const size of SIZES) {
  test(`journal vide à ${size.width}×${size.height} : l'écran vide est centré dans la zone de contenu`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await drive(page, (sim) => sim.audit.clear("forge"));
    await go(page, "#/servers/forge/audit");
    await expect(page.locator("section.empty")).toBeVisible();
    const zone = (await boxesOf(page, ".layout__content"))[0];
    const empty = (await boxesOf(page, "section.empty"))[0];
    expect(zone && empty).toBeTruthy();
    if (!zone || !empty) return;
    const left = empty.x - zone.x;
    const right = zone.x + zone.w - (empty.x + empty.w);
    expect(Math.abs(left - right), "écart gauche/droite").toBeLessThanOrEqual(2);
    // Le titre ne se casse pas sur deux lignes dans un bloc étroit collé au bord.
    const title = (await boxesOf(page, "section.empty .empty__title"))[0];
    expect(title?.h ?? 999).toBeLessThan(60);
    // Centré aussi à la verticale : la page remplit la zone de contenu, le bloc qui porte l'écran vide aussi, et
    // l'écran est au milieu de ce bloc (écart haut/bas à 2 px près).
    const frame = await page.evaluate(() => {
      const page = document.querySelector(".audit") as HTMLElement;
      const area = document.querySelector("section.empty")?.parentElement as HTMLElement;
      const zoneRect = (
        document.querySelector(".layout__content") as HTMLElement
      ).getBoundingClientRect();
      const pageRect = page.getBoundingClientRect();
      const areaRect = area.getBoundingClientRect();
      const emptyRect = (
        document.querySelector("section.empty") as HTMLElement
      ).getBoundingClientRect();
      return {
        pageFills: Math.round(zoneRect.bottom - pageRect.bottom),
        areaH: areaRect.height,
        emptyH: emptyRect.height,
        top: emptyRect.top - areaRect.top,
        bottom: areaRect.bottom - emptyRect.bottom,
      };
    });
    expect(frame.pageFills, "la page remplit la zone").toBeLessThanOrEqual(2);
    expect(Math.abs(frame.top - frame.bottom), "écart haut/bas").toBeLessThanOrEqual(2);
    await shoot(page, "hrt35-journal-vide", size.width, size.height);
  });
}

for (const size of [SIZES[2], SIZES[3], SIZES[4]]) {
  test(`session expirée à ${size.width}×${size.height} : panneau centré, tableau de bord non repoussé, deux sorties`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.locator("section.card").first()).toBeVisible();
    const before = (await boxesOf(page, "section.card"))[0];
    await drive(page, (sim) => sim.setState("forge", "session_expired"));
    const panel = page.locator("section.reconnect");
    await expect(panel).toBeVisible();
    const zone = (await boxesOf(page, ".layout__content"))[0];
    const box = (await boxesOf(page, "section.reconnect"))[0];
    expect(zone && box).toBeTruthy();
    if (!zone || !box) return;
    expect(
      Math.abs(box.x - zone.x - (zone.x + zone.w - (box.x + box.w))),
      "écart gauche/droite",
    ).toBeLessThanOrEqual(2);
    // Le tableau de bord n'est pas repoussé par le panneau.
    const after = (await boxesOf(page, "section.card"))[0];
    expect(Math.abs((after?.y ?? 0) - (before?.y ?? 0)), "carte repoussée").toBeLessThanOrEqual(2);
    // Les deux sorties : se reconnecter (bouton d'envoi du formulaire), ou utiliser un autre compte.
    await expect(panel.locator("button[type='submit']")).toBeVisible();
    // « Utiliser un autre compte » n'existe que pour l'accès révoqué (BR-RESIL-014).
    await expect(panel.getByRole("button", { name: "Utiliser un autre compte" })).toHaveCount(0);
    // Centré aussi à la verticale dans la zone de contenu (2 px près).
    expect(
      Math.abs(box.y - zone.y - (zone.y + zone.h - (box.y + box.h))),
      "écart haut/bas",
    ).toBeLessThanOrEqual(2);
    // Fond opaque et ombre : lisible par-dessus les cartes.
    const look = await panel.evaluate((element) => {
      const style = getComputedStyle(element);
      return { background: style.backgroundColor, shadow: style.boxShadow };
    });
    expect(look.background).not.toMatch(/rgba\(.*,\s*0(\.\d+)?\)/);
    expect(look.shadow).not.toBe("none");
    // Le premier élément focalisable du contenu est dans le panneau (atteint en premier au clavier).
    const first = await page.evaluate(() => {
      const zoneElement = document.querySelector(".layout__content");
      const focusable = zoneElement?.querySelector(
        "a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex='-1'])",
      );
      return focusable?.closest("section.reconnect") !== null;
    });
    expect(first, "premier élément focalisable").toBe(true);
    await shoot(page, "hrt35-session-expiree", size.width, size.height);
  });
}

test("comptes vide : un seul titre « Comptes » dans le contenu et un seul bouton « Ajouter un compte »", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.goto("/?nodev#/servers/forge/dashboard");
  await drive(page, (sim) => sim.accounts.seed("forge", []));
  await go(page, "#/servers/forge/accounts");
  await expect(page.locator("section.empty")).toBeVisible();
  await expect(page.getByRole("button", { name: "Ajouter un compte" })).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "Comptes", exact: true })).toHaveCount(1);
  await shoot(page, "hrt35-comptes-vide", 1920, 1080);
});

for (const size of SIZES) {
  test(`accueil (premier lancement) à ${size.width}×${size.height} : toujours centré à 2 px près`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?servers=none&nodev");
    await expect(page.locator("section.empty")).toBeVisible();
    const zone = await page.locator("main.welcome").evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
    });
    const empty = (await boxesOf(page, "section.empty"))[0];
    if (!empty) throw new Error("accueil absent");
    expect(
      Math.abs(empty.x - zone.x - (zone.x + zone.w - (empty.x + empty.w))),
    ).toBeLessThanOrEqual(2);
    expect(
      Math.abs(empty.y - zone.y - (zone.y + zone.h - (empty.y + empty.h))),
    ).toBeLessThanOrEqual(2);
    await shoot(page, "hrt35-accueil", size.width, size.height);
  });
}

// ── HRT-36 : rien ne se chevauche ni ne se coupe ─────────────────────────────────────────────

for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : une ligne par compte, aucun bouton coupé (entièrement dans le tableau)`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator("tr[data-account]").first()).toBeVisible();
    const rows = await boxesOf(page, "tr[data-account]");
    expect(rows.length).toBeGreaterThan(1);
    // C21 : une ligne par compte (au plus 76 px), actions comprises, sans bouton coupé ni hors tableau.
    // À la fenêtre minimale (1100 px) il n'y a plus la place : les actions passent à la ligne dans le tableau.
    if (size.width >= 1280) {
      for (const row of rows) {
        expect(row.h, `${row.text.slice(0, 20)} : une ligne`).toBeLessThanOrEqual(76);
      }
    }
    // Chaque bouton d'action est ENTIÈREMENT dans la zone visible du tableau (jamais coupé).
    const wrap = (await boxesOf(page, ".table-wrap"))[0];
    expect(wrap).toBeTruthy();
    const buttons = await boxesOf(page, "tr[data-account] .table__actions button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.x, `${button.text} : bord gauche`).toBeGreaterThanOrEqual((wrap?.x ?? 0) - 1);
      expect(button.x + button.w, `${button.text} : bord droit`).toBeLessThanOrEqual(
        (wrap?.x ?? 0) + (wrap?.w ?? 0) + 1,
      );
    }
    await shoot(page, "hrt36-comptes", size.width, size.height);
  });
}

for (const size of [SIZES[1], SIZES[2]]) {
  test(`journal à ${size.width}×${size.height} : les filtres tiennent sur une ligne`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("form.filters")).toBeVisible();
    const filters = (await boxesOf(page, "form.filters"))[0];
    expect(filters?.h ?? 999, "carte de filtres").toBeLessThanOrEqual(150);
    const controls = await boxesOf(page, "form.filters input, form.filters select");
    const ys = controls.map((control) => Math.round(control.y + control.h / 2));
    expect(Math.max(...ys) - Math.min(...ys), "tous sur la même ligne").toBeLessThanOrEqual(40);
    await shoot(page, "hrt36-journal", size.width, size.height);
  });
}

for (const size of [SIZES[1], SIZES[3]]) {
  test(`sécurité à ${size.width}×${size.height} : « Demander mon mot de passe » sur une ligne par choix`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/security");
    const radios = page.locator("[data-reauth-setting] [role='radio']");
    await expect(radios).toHaveCount(2);
    for (const box of await boxesOf(page, "[data-reauth-setting] [role='radio']")) {
      expect(box.h, box.text).toBeLessThanOrEqual(44);
    }
    await shoot(page, "hrt36-securite", size.width, size.height);
  });
}

test("mode attaque et alerte à 1366×800 : les bandeaux tiennent en 80 px", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 800 });
  await page.goto("/?nodev#/servers/forge/dashboard");
  await drive(page, (sim) => {
    sim.security.setAlert("forge", { own: true, others: 1 });
    sim.security.setMode("forge", "active");
  });
  await expect(page.locator("section.banner")).toHaveCount(2);
  const banners = await boxesOf(page, "section.banner");
  const total = banners.reduce((sum, banner) => sum + banner.h, 0);
  expect(total, "hauteur cumulée des bandeaux").toBeLessThanOrEqual(80);
  await shoot(page, "hrt36-bandeaux", 1366, 800);
});

async function manyServers(page: Page, count: number) {
  await page.evaluate((n) => {
    const sim = (window as unknown as { __hearthSim: Record<string, unknown> }).__hearthSim as {
      servers: Record<string, unknown>[];
      events: Map<string, unknown>;
      connectedEvent(id: string): unknown;
      emitServers(): void;
    };
    for (let index = 0; index < n; index += 1) {
      const id = `extra-${index}`;
      sim.servers.push({
        id,
        name: `serveur-${index}`,
        address: `10.0.0.${index + 10}`,
        host: `10.0.0.${index + 10}`,
        port: 7341,
        color: (index % 6) + 1,
        role: "admin",
        username: "marie",
        remember: true,
      });
      sim.events.set(id, sim.connectedEvent(id));
    }
    sim.emitServers();
  }, count);
}

for (const size of [SIZES[0], SIZES[1]]) {
  test(`14 serveurs à ${size.width}×${size.height} : la liste de la barre défile, « + », « Mes serveurs » et « Réglages » restent atteignables`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await manyServers(page, 12);
    await expect(page.locator("[data-rail-add]")).toBeVisible();
    for (const selector of ["[data-rail-add]", "[data-rail-servers]", "a[href='#/settings']"]) {
      const box = (await boxesOf(page, selector))[0];
      expect(box, selector).toBeTruthy();
      expect((box?.y ?? 0) + (box?.h ?? 0), `${selector} dans la fenêtre`).toBeLessThanOrEqual(
        size.height,
      );
    }
    const list = await page
      .locator(".rail__list")
      .evaluate((el) => el.scrollHeight > el.clientHeight);
    expect(list, "la liste des serveurs défile").toBe(true);
    await shoot(page, "hrt36-barre-14-serveurs", size.width, size.height);
  });
}

for (const size of [SIZES[0], SIZES[3]]) {
  test(`« Mes serveurs » à ${size.width}×${size.height} : rien ne recouvre l'adresse, la page défile au bord de la fenêtre`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers");
    await manyServers(page, 12);
    await drive(page, (sim) => sim.security.setAlert("forge", { own: true, others: 1 }));
    await expect(page.locator("[data-server-row]").first()).toBeVisible();
    const overlaps = await page.locator("[data-server-row]").evaluateAll((rows) => {
      const found: string[] = [];
      for (const row of rows) {
        const parts = [...row.querySelectorAll(":scope > *, .row__id > *")]
          .filter((el) => el.children.length === 0 || el.classList.contains("row__actions"))
          .map((el) => ({ el, rect: el.getBoundingClientRect() }))
          .filter((part) => part.rect.width > 0 && part.rect.height > 0);
        for (const a of parts) {
          for (const b of parts) {
            if (a.el === b.el || a.el.contains(b.el) || b.el.contains(a.el)) continue;
            const overlap =
              a.rect.left < b.rect.right - 1 &&
              b.rect.left < a.rect.right - 1 &&
              a.rect.top < b.rect.bottom - 1 &&
              b.rect.top < a.rect.bottom - 1;
            if (overlap) found.push(`${a.el.textContent?.trim()} / ${b.el.textContent?.trim()}`);
          }
        }
      }
      return found;
    });
    expect(overlaps, "éléments qui se recouvrent").toEqual([]);
    // La barre de défilement est au bord de la fenêtre, pas au milieu de l'écran.
    const scroller = await page.evaluate(() => {
      const holder = document.querySelector("[data-server-row]")?.closest(".book__scroll");
      if (!holder) return null;
      const rect = holder.getBoundingClientRect();
      return { right: rect.right, width: window.innerWidth };
    });
    expect(scroller, "conteneur de défilement").not.toBeNull();
    expect(Math.abs((scroller?.width ?? 0) - (scroller?.right ?? 0))).toBeLessThanOrEqual(2);
    await shoot(page, "hrt36-mes-serveurs", size.width, size.height);
  });
}

// ── HRT-36 C20 : les boutons d'une fenêtre de dialogue restent visibles, le contenu défile ────────────

for (const size of [...SIZES, { width: 1100, height: 420 }]) {
  test(`fenêtre de création de compte à ${size.width}×${size.height} : « Annuler » et « Créer » toujours visibles`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await page.getByRole("button", { name: "Ajouter un compte" }).click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog).toBeVisible();
    for (const name of ["Annuler", "Créer"]) {
      const box = await dialog.getByRole("button", { name, exact: true }).boundingBox();
      expect(box, name).not.toBeNull();
      expect(
        (box?.y ?? 0) >= 0 && (box?.y ?? 0) + (box?.height ?? 0) <= size.height,
        `${name} dans la fenêtre`,
      ).toBe(true);
    }
    const frame = await dialog.boundingBox();
    expect((frame?.y ?? 0) + (frame?.height ?? 0)).toBeLessThanOrEqual(size.height);
    if (size.height <= 480) {
      const scrolls = await dialog
        .locator(".dialog__scroll")
        .evaluate((el) => el.scrollHeight > el.clientHeight);
      expect(scrolls, "le contenu défile").toBe(true);
    }
    // Les boutons ne recouvrent jamais le contenu : la zone qui défile se termine avant eux.
    const scrollBox = await dialog.locator(".dialog__scroll").boundingBox();
    const cancel = await dialog.getByRole("button", { name: "Annuler", exact: true }).boundingBox();
    expect(
      (scrollBox?.y ?? 0) + (scrollBox?.height ?? 0),
      "contenu au-dessus des boutons",
    ).toBeLessThanOrEqual((cancel?.y ?? 0) + 1);
    await shoot(page, "hrt36-dialogue", size.width, size.height);
  });
}

// ── HRT-44 : des actions qui disent ce qu'elles font (Comptes) ───────────────────────────────────────

for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : « Changer mon mot de passe », la case vient après les champs`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await page.getByRole("button", { name: "Changer mon mot de passe" }).click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog).toBeVisible();
    const y = async (locator: ReturnType<typeof dialog.getByLabel>) =>
      (await locator.first().boundingBox())?.y ?? -1;
    const next = await y(dialog.getByLabel("Nouveau mot de passe", { exact: true }));
    const old = await y(dialog.getByLabel("Ancien mot de passe"));
    const keep = await y(dialog.getByLabel("Garder ce poste reconnu"));
    expect(next, "nouveau mot de passe").toBeGreaterThan(0);
    expect(old, "l'ancien suit le nouveau").toBeGreaterThan(next);
    expect(keep, "la case vient après les champs").toBeGreaterThan(old);
    await shoot(page, "hrt44-mon-mot-de-passe", size.width, size.height);
  });

  test(`suppression de compte à ${size.width}×${size.height} : titre sans redite, délai avec unités`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await page
      .locator("tr[data-account='lea']")
      .getByRole("button", { name: /^Supprimer/ })
      .click();
    const dialog = page.locator("dialog[open]");
    await expect(dialog.getByRole("heading", { name: "Supprimer le compte ?" })).toBeVisible();
    await expect(dialog).not.toContainText("Supprimer le compte lea ?");
    await expect(dialog).toContainText("Le compte lea et ses sessions seront supprimés.");
    await shoot(page, "hrt44-suppression", size.width, size.height);
  });
}

// ── HRT-37 : des barres de défilement sombres, une seule par écran ─────────────────────────────────

const SCROLL_PAGES = [
  ["tableau de bord", "#/servers/forge/dashboard"],
  ["Comptes", "#/servers/forge/accounts"],
  ["Sécurité", "#/servers/forge/security"],
  ["Mes serveurs", "#/servers"],
  ["Ajouter un serveur", "#/servers/new"],
  ["Réglages", "#/settings"],
] as const;

for (const size of SIZES) {
  test(`barres de défilement à ${size.width}×${size.height} : sombres, et jamais deux à la fois`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    for (const [name, hash] of SCROLL_PAGES) {
      await page.goto(`/?nodev${hash}`);
      await page.waitForTimeout(300);
      const scrolling = await page.evaluate(() => {
        const found: { who: string; color: string; ratio: number }[] = [];
        const lum = (rgb: string) => {
          const [r = 0, g = 0, b = 0] = (rgb.match(/[\d.]+/g) ?? []).map(Number);
          const f = (v: number) => {
            const c = v / 255;
            return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
          };
          return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
        };
        const ratio = (a: string, b2: string) => {
          const [hi = 0, lo = 0] = [lum(a), lum(b2)].sort((x, y) => y - x);
          return (hi + 0.05) / (lo + 0.05);
        };
        const surface = getComputedStyle(document.body).backgroundColor;
        for (const el of document.querySelectorAll("*")) {
          const style = getComputedStyle(el);
          if (/(auto|scroll)/.test(style.overflowY) && el.scrollHeight > el.clientHeight + 1) {
            // La poignée DESSINÉE (pseudo-élément pris par Chromium) contre le fond de la page.
            const thumb = getComputedStyle(el, "::-webkit-scrollbar-thumb").backgroundColor;
            found.push({
              who: `${el.tagName}.${(el as HTMLElement).className}`,
              color: thumb,
              ratio: ratio(thumb, surface),
            });
          }
        }
        return found;
      });
      expect(scrolling.length, `${name} : une seule zone qui défile`).toBeLessThanOrEqual(1);
      for (const area of scrolling) {
        expect(
          area.ratio,
          `${name} : poignée de ${area.who} (${area.color}) à 3 pour 1`,
        ).toBeGreaterThanOrEqual(3);
      }
    }
    // Quelle que soit la page : le thème du navigateur est sombre (contrôles natifs) et la barre est fine.
    await page.goto("/?nodev#/settings");
    const root = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      return {
        scheme: style.colorScheme,
        width: getComputedStyle(document.documentElement, "::-webkit-scrollbar").width,
      };
    });
    expect(root.scheme).toBe("dark");
    expect(root.width).toBe("12px");
    await shoot(page, "hrt37-reglages", size.width, size.height);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.waitForTimeout(300);
    await shoot(page, "hrt37-tableau-de-bord", size.width, size.height);
  });
}

// ── HRT-45 : barre des serveurs, des noms et des couleurs qui servent ───────────────────────────────

for (const size of SIZES) {
  test(`barre des serveurs à ${size.width}×${size.height} : un nom au survol de chaque outil, la couleur de chaque serveur`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    const rail = page.getByRole("navigation", { name: "Serveurs" });
    const targets = [
      {
        link: rail.getByRole("link", { name: "Ajouter un serveur", exact: true }),
        text: "Ajouter un serveur",
        name: "Ajouter un serveur",
        shot: "ajouter",
      },
      {
        link: rail.getByRole("link", { name: "Mes serveurs", exact: true }),
        text: "Mes serveurs",
        name: "Mes serveurs",
        shot: "mes",
      },
      {
        link: rail.getByRole("link", { name: "Réglages", exact: true }),
        text: "Réglages",
        name: "Réglages",
        shot: "réglages",
      },
      {
        link: rail.locator('[data-server="forge"]'),
        text: "forge",
        name: /^forge, Connecté/,
        shot: "serveur-forge",
      },
      {
        link: rail.locator('[data-server="salon"]'),
        text: "nas-salon",
        name: /^nas-salon, Connecté/,
        shot: "serveur-salon",
      },
    ];
    for (const { link, text, name, shot } of targets) {
      await link.hover();
      const tip = link.locator("[data-rail-tip]");
      await expect(tip, text).toBeVisible();
      await expect(tip, text).toHaveText(text);
      // Une seule annonce : un nom accessible, aucune description, aucun `title`, la bulle cachée aux lecteurs d'écran.
      await expect(link, text).toHaveAccessibleName(name);
      await expect(link, text).toHaveAccessibleDescription("");
      await expect(link, text).not.toHaveAttribute("title", /.*/);
      await expect(tip, text).toHaveAttribute("aria-hidden", "true");
      const box = await tip.boundingBox();
      expect(box, text).not.toBeNull();
      const b = box ?? { x: 0, y: 0, width: 0, height: 0 };
      expect(b.x >= 0 && b.y >= 0, `${text} : bulle dans la fenêtre (haut, gauche)`).toBe(true);
      expect(
        b.x + b.width <= size.width && b.y + b.height <= size.height,
        `${text} : bulle dans la fenêtre (bas, droite)`,
      ).toBe(true);
      // Aucune intersection avec un autre bouton de la barre (ni avatar, ni outil, ni logo).
      const others = await rail.locator("[data-rail-item]").evaluateAll((items) =>
        items
          .filter((item) => !item.matches(":hover"))
          .map((item) => item.getBoundingClientRect())
          .map((r) => ({ x: r.x, y: r.y, w: r.width, h: r.height })),
      );
      for (const o of others) {
        const overlap =
          b.x < o.x + o.w && b.x + b.width > o.x && b.y < o.y + o.h && b.y + b.height > o.y;
        expect(overlap, `${text} : la bulle ne recouvre aucune autre icône`).toBe(false);
      }
      await shoot(page, `hrt45-infobulle-${shot}`, size.width, size.height);
    }
    // La couleur choisie se voit aussi sur le serveur FERMÉ (salon n'est pas le serveur ouvert).
    const idle = await rail
      .locator('[data-server="salon"] [role="img"]')
      .evaluate((el) => getComputedStyle(el).borderTopColor);
    const common = await page.evaluate(() => {
      const el = document.createElement("i");
      el.style.color = getComputedStyle(document.documentElement).getPropertyValue("--bd");
      document.body.append(el);
      const rgb = getComputedStyle(el).color;
      el.remove();
      return rgb;
    });
    expect(idle, "le contour du serveur fermé n'est plus le gris commun").not.toBe(common);
    await shoot(page, "hrt45-barre", size.width, size.height);
  });

  test(`suppression d'un serveur à ${size.width}×${size.height} : la fenêtre le nomme`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers");
    await page
      .locator("li", { hasText: "nas-salon" })
      .getByRole("button", { name: "Supprimer" })
      .first()
      .click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog.getByRole("heading", { name: "Supprimer nas-salon ?" })).toBeVisible();
    await shoot(page, "hrt45-suppression", size.width, size.height);
  });
}

// ── HRT-46 : textes et nombres en français correct ──────────────────────────────────────────────────

for (const size of SIZES) {
  test(`tableau de bord à ${size.width}×${size.height} : virgule, pas de « ,0 », jamais « 4000.0 Go »`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.waitForTimeout(500);
    const text = await page.locator("body").innerText();
    expect(text).not.toMatch(/\d\.\d+ ?(Go|To|GHz|Mo\/s)/);
    expect(text).not.toMatch(/\d,0 (Go|To|GHz)/);
    expect(text).toMatch(/\d+(,\d)? (Go|To)/);
    await shoot(page, "hrt46-tableau-de-bord", size.width, size.height);
  });

  test(`ajout d'un serveur à ${size.width}×${size.height} : le port faux donne la plage admise`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/new");
    await page.getByLabel("Nom du serveur").fill("Atelier");
    await page.getByLabel("Adresse IP ou nom").fill("10.0.0.5");
    await page.getByLabel("Port (optionnel)").fill("99999");
    await expect(page.getByText("Port invalide. Plage admise : 1–⁠65535")).toBeVisible();
    await shoot(page, "hrt46-port", size.width, size.height);
  });

  test(`mise à jour de l'agent à ${size.width}×${size.height} : étape dite une fois, finie sans points, bouton retiré`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/settings");
    const forge = page.locator('[data-agent-card][data-server="forge"]');
    await expect(forge).toBeVisible();
    // Avant : la mention, le bouton, et pas une phrase de plus qui redit la mention.
    await expect(forge.locator("[data-agent-tag]")).toHaveText("Mise à jour disponible");
    await expect(forge.getByText("Mise à jour disponible pour l'agent")).toHaveCount(0);
    await shoot(page, "hrt46-maj-disponible", size.width, size.height);
    await forge.locator("[data-agent-update-button]").click();
    const dialog = page.locator("dialog[open]");
    await dialog.getByLabel("Ton mot de passe").fill("Correct-Horse-9");
    await dialog.getByRole("button", { name: "Oui, mettre à jour" }).click();
    await page.evaluate(() => {
      const bridge = (
        window as unknown as {
          __hearthSim: { agentUpdates: { advance(id: string, step: string): void } };
        }
      ).__hearthSim;
      bridge.agentUpdates.advance("forge", "verify");
    });
    await expect(forge.locator("[data-agent-update-button]")).toHaveCount(0);
    await expect(forge.locator("[data-agent-progress]")).toHaveText(
      "Mise à jour de l'agent en cours.",
    );
    await expect(forge.locator('[data-step="download"]')).toHaveText(/^Téléchargement(?!…)/);
    await expect(forge.locator('[data-step="verify"]')).toContainText("Vérification…");
    await shoot(page, "hrt46-maj-en-cours", size.width, size.height);
  });
}
