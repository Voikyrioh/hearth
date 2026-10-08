import { expect, type Page, test } from "@playwright/test";

// Seconde passe UX du 2026-10-08 (T65) : tableau du journal dans sa carte (D1), alignement des en-têtes (C31),
// bandeaux à la largeur de la page (D2), en-tête borné (D3). Barres de défilement CLASSIQUES (Windows) : Playwright
// masque les barres par défaut, ce qui cachait le décalage mesuré par la revue.
test.use({ launchOptions: { ignoreDefaultArgs: ["--hide-scrollbars"] } });

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function shoot(page: Page, name: string, size: { width: number; height: number }) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${size.width}x${size.height}.png` });
}

for (const size of SIZES) {
  test(`journal à ${size.width}×${size.height} : le tableau tient dans sa carte, une seule barre horizontale au plus`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    const m = await page.evaluate(() => {
      const card = document.querySelector(".audit .table") as HTMLElement;
      const grid = document.querySelector(".audit .table__grid") as HTMLElement;
      const scrollers = Array.from(document.querySelectorAll(".audit *")).filter((el) => {
        const style = getComputedStyle(el);
        return (
          /(auto|scroll)/.test(style.overflowX) &&
          (el as HTMLElement).scrollWidth > (el as HTMLElement).clientWidth + 1
        );
      });
      const lastHead = Array.from(document.querySelectorAll("[role=columnheader]")).at(
        -1,
      ) as HTMLElement;
      return {
        card: card.clientWidth,
        grid: grid.getBoundingClientRect().width,
        scrollers: scrollers.length,
        headRight: lastHead.getBoundingClientRect().right,
        cardRight: card.getBoundingClientRect().right,
      };
    });
    if (size.width >= 1280) {
      expect(m.grid, "tableau dans la carte").toBeLessThanOrEqual(m.card + 1);
      expect(m.scrollers, "aucune barre horizontale").toBe(0);
    } else {
      expect(m.scrollers, "une seule barre horizontale").toBeLessThanOrEqual(1);
    }
    await shoot(page, "p2-journal-largeur", size);
  });

  test(`journal à ${size.width}×${size.height} : chaque en-tête est au bord gauche de sa colonne (barres classiques)`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    const row = page.locator("[role=row][data-row-key]").first();
    await expect(row).toBeVisible();
    const heads = await page
      .locator("[role=columnheader]")
      .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().left));
    const cells = await row
      .locator("[role=gridcell]")
      .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().left));
    expect(cells.length).toBe(heads.length);
    for (const [index, head] of heads.entries()) {
      expect(Math.abs(head - (cells[index] ?? 0)), `colonne ${index + 1}`).toBeLessThanOrEqual(1);
    }
    await shoot(page, "p2-journal-colonnes", size);
  });
}

type SimSecurity = {
  security: {
    setAlert(id: string, alert: { own: boolean; others: number | null }): void;
    setMode(id: string, mode: string, options?: object): void;
  };
};

for (const size of SIZES) {
  test(`bandeau de sécurité à ${size.width}×${size.height} : à la largeur du contenu, actions sur une ligne avec de l'air`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.locator(".layout__content")).toBeVisible();
    await page.waitForFunction(() =>
      Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
    );
    await page.evaluate(() =>
      (window as unknown as { __hearthSim: SimSecurity }).__hearthSim.security.setAlert("forge", {
        own: true,
        others: null,
      }),
    );
    const banner = page.locator("section.banner, .banner").first();
    await expect(banner).toBeVisible();
    const m = await page.evaluate(() => {
      const banner = document.querySelector(".banner") as HTMLElement;
      const content = document.querySelector(".layout__content") as HTMLElement;
      const buttons = Array.from(banner.querySelectorAll("button")).map((b) =>
        b.getBoundingClientRect(),
      );
      const box = banner.getBoundingClientRect();
      return {
        bannerWidth: box.width,
        contentWidth: content.getBoundingClientRect().width,
        buttonTops: buttons.map((b) => Math.round(b.top)),
        topAir: Math.min(...buttons.map((b) => b.top)) - box.top,
        bottomAir: box.bottom - Math.max(...buttons.map((b) => b.bottom)),
      };
    });
    expect(
      Math.abs(m.bannerWidth - m.contentWidth),
      "même largeur que le contenu",
    ).toBeLessThanOrEqual(1);
    expect(new Set(m.buttonTops).size, "boutons sur une ligne").toBe(1);
    expect(m.topAir, "air en haut").toBeGreaterThanOrEqual(4);
    expect(m.bottomAir, "air en bas").toBeGreaterThanOrEqual(4);
    await shoot(page, "p2-bandeau", size);
  });
}

// D3 : l'en-tête de page est borné comme le contenu (Comptes : le tableau de 1 400 px).
for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : l'en-tête ne dépasse pas le bord du tableau`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator(".table-wrap")).toBeVisible();
    const m = await page.evaluate(() => {
      const table = document.querySelector(".table-wrap")?.getBoundingClientRect();
      const pill = document
        .querySelector(".layout__main [role=status].pill, .layout__main .pill")
        ?.getBoundingClientRect();
      return { tableRight: table?.right ?? 0, pillRight: pill?.right ?? 0 };
    });
    expect(m.pillRight, "pastille du lien").toBeLessThanOrEqual(m.tableRight + 1);
  });
}

// D4 : la ligne « durée / échelle » d'une courbe reste sur une ligne à 1100.
test("tableau de bord à 1100×680 : la durée et l'échelle d'une courbe tiennent sur une ligne", async ({
  page,
}) => {
  await page.setViewportSize(SIZES[0]);
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".series__span").first()).toBeVisible();
  const heights = await page
    .locator(".series__span, .series__scale")
    .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().height));
  for (const height of heights) expect(height, "une seule ligne").toBeLessThanOrEqual(20);
  await shoot(page, "p2-courbes-legendes", SIZES[0]);
});

// D5 : barre des serveurs à 14 serveurs : aucune barre horizontale parasite (barres classiques).
test("barre des serveurs à 1100×680 avec 14 serveurs : pas de défilement horizontal", async ({
  page,
}) => {
  await page.setViewportSize(SIZES[0]);
  await page.goto("/?nodev#/servers/forge/dashboard");
  await page.waitForFunction(() =>
    Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
  );
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
  }, 12);
  await expect(page.locator("[data-rail-add]")).toBeVisible();
  const overflow = await page
    .locator(".rail__list")
    .evaluate((el) => ({ scrollWidth: el.scrollWidth, clientWidth: el.clientWidth }));
  expect(overflow.scrollWidth, "pas de barre horizontale").toBeLessThanOrEqual(
    overflow.clientWidth + 1,
  );
});

// D6 : Échap sur le changement de rôle rend le curseur au bouton d'origine.
test("comptes : Échap sur « Changer le rôle » rend le curseur au bouton", async ({ page }) => {
  await page.setViewportSize(SIZES[2]);
  await page.goto("/?nodev#/servers/forge/accounts");
  const button = page
    .locator("tr[data-account='paul']")
    .getByRole("button", { name: /^Changer le rôle/ });
  await button.click();
  await expect(page.locator("tr[data-account='paul'] td[data-editing] select")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(button).toBeFocused();
});

// D8 : « Mes serveurs » : la pastille d'état reste sur la ligne du nom.
test("mes serveurs à 1100×680 : la pastille d'état est sur la ligne du nom", async ({ page }) => {
  await page.setViewportSize(SIZES[0]);
  await page.goto("/?nodev#/servers");
  const row = page.locator("[data-server-row='forge']");
  await expect(row).toBeVisible();
  const m = await row.evaluate((el) => {
    const name = el.querySelector(".row__name")?.getBoundingClientRect();
    const pill = el.querySelector(".pill")?.getBoundingClientRect();
    return {
      nameTop: name?.top ?? 0,
      pillTop: pill?.top ?? 0,
      pillBottom: pill?.bottom ?? 0,
      nameBottom: name?.bottom ?? 0,
    };
  });
  expect(m.pillTop, "pastille sur la ligne du nom").toBeLessThan(m.nameBottom);
  await shoot(page, "p2-mes-serveurs", SIZES[0]);
});
