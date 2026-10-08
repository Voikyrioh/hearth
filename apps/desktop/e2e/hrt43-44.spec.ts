import { expect, type Page, test } from "@playwright/test";

// HRT-43 (journal : chercher à la frappe, colonnes alignées, textes) et restes de HRT-44 (repère du rôle en
// cours de changement, en-tête sans chasse fixe, date de création au survol et au focus). Pont SIMULÉ,
// assertions de GÉOMÉTRIE et de style calculé, aux tailles 1100×680, 1280×800, 1366×800, 1920×1080, 2560×1440.

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

async function shoot(page: Page, name: string, width: number, height: number) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${width}x${height}.png` });
}

// Largeur nécessaire au texte choisi d'une liste (police réelle, marges et bordures comprises) : aucune
// option sélectionnée ne doit être coupée (« Lecture seul », « Tout l'hist »).
async function truncatedSelects(page: Page) {
  return page.locator("select").evaluateAll((selects) => {
    const canvas = document.createElement("canvas").getContext("2d");
    const out: string[] = [];
    for (const select of selects as HTMLSelectElement[]) {
      if (!canvas || select.offsetWidth === 0) continue;
      const style = getComputedStyle(select);
      canvas.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      const label = select.selectedOptions[0]?.textContent ?? "";
      const need =
        canvas.measureText(label).width +
        Number.parseFloat(style.paddingLeft) +
        Number.parseFloat(style.paddingRight) +
        Number.parseFloat(style.borderLeftWidth) +
        Number.parseFloat(style.borderRightWidth);
      if (need > select.offsetWidth + 0.5)
        out.push(`${label} : ${Math.ceil(need)} > ${select.offsetWidth}`);
    }
    return out;
  });
}

// ── HRT-44 ───────────────────────────────────────────────────────────────────────────────────

for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : le rôle en cours de changement a un repère, l'en-tête n'a qu'une police`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator("tr[data-account]").first()).toBeVisible();
    // C26 : toutes les cellules d'en-tête ont la même police (pas de chasse fixe sur « Sessions ouvertes »).
    const families = await page
      .locator("thead th")
      .evaluateAll((cells) => cells.map((cell) => getComputedStyle(cell).fontFamily));
    expect(new Set(families).size, `polices d'en-tête : ${families.join(" | ")}`).toBe(1);
    // C22 : la cellule du rôle en cours de changement se distingue des autres (fond et trait).
    await page
      .locator("tr[data-account='paul']")
      .getByRole("button", { name: /^Changer le rôle/ })
      .click();
    const editing = page.locator("tr[data-account='paul'] td[data-editing]");
    await expect(editing).toBeVisible();
    const look = await editing.evaluate((cell) => {
      const style = getComputedStyle(cell);
      const other = getComputedStyle(
        cell.parentElement?.querySelector("td:not([data-editing])") as Element,
      );
      return {
        background: style.backgroundColor,
        otherBackground: other.backgroundColor,
        shadow: style.boxShadow,
        outline: style.outlineStyle,
      };
    });
    expect(look.background, "fond différent").not.toBe(look.otherBackground);
    expect(look.shadow !== "none" || look.outline !== "none", "trait visible").toBe(true);
    await shoot(page, "hrt44-role-en-cours", size.width, size.height);
  });
}

for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : la date de création est lue une seule fois par compte, sans arrêt de tabulation de plus`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator("tr[data-account]").first()).toBeVisible();
    const dates = await page.locator("tr[data-account]").evaluateAll((rows) =>
      rows.map((row) => {
        const hint = row.querySelector<HTMLElement>(".table__hint");
        const shown = hint && hint.getBoundingClientRect().height > 0 ? 1 : 0;
        const column = row.querySelector<HTMLElement>("td.table__created");
        const inColumn = column && column.getBoundingClientRect().width > 0 ? 1 : 0;
        return { user: (row as HTMLElement).dataset.account, shown, inColumn };
      }),
    );
    for (const entry of dates) {
      expect(entry.shown + entry.inColumn, `date de ${entry.user}`).toBe(1);
    }
    // Aucun arrêt de tabulation sur l'identifiant, aucune description portée par un élément sans rôle.
    await expect(
      page.locator(".table__name[tabindex], .table__name[aria-describedby]"),
    ).toHaveCount(0);
    // Sous l'identifiant, la date ne grandit pas la ligne (la cellule « Dernière connexion » fait déjà deux lignes).
    if (size.width <= 1366 && size.width >= 1280) {
      for (const user of ["paul", "lea"]) {
        const height = await page
          .locator(`tr[data-account='${user}']`)
          .evaluate((row) => row.getBoundingClientRect().height);
        expect(height, `hauteur de la ligne de ${user}`).toBeLessThanOrEqual(76);
      }
    }
    await shoot(page, "hrt44-date-au-survol", size.width, size.height);
  });
}

for (const size of SIZES) {
  test(`comptes à ${size.width}×${size.height} : la liste du rôle montre son libellé en entier`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator("tr[data-account]").first()).toBeVisible();
    await page
      .locator("tr[data-account='paul']")
      .getByRole("button", { name: /^Changer le rôle/ })
      .click();
    const select = page.locator("tr[data-account='paul'] td[data-editing] select");
    await expect(select).toBeVisible();
    for (const value of ["readonly", "admin"]) {
      await select.evaluate((el, v) => {
        (el as HTMLSelectElement).value = v;
      }, value);
      expect(await truncatedSelects(page), `rôle ${value}`).toEqual([]);
    }
  });

  test(`journal à ${size.width}×${size.height} : aucune liste de la carte de filtres n'est coupée, la recherche garde 200 px`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    expect(await truncatedSelects(page)).toEqual([]);
    const search = await page
      .locator(".filters__search")
      .evaluate((el) => el.getBoundingClientRect().width);
    expect(search, "largeur du champ de recherche").toBeGreaterThanOrEqual(200);
    // Sans filtre actif (donc sans « Effacer »), la carte tient déjà sur une rangée : une seule hauteur de carte.
    const height = await page
      .locator("form.filters")
      .evaluate((el) => el.getBoundingClientRect().height);
    expect(height, "hauteur de la carte de filtres").toBeLessThanOrEqual(140);
  });
}

// HRT-37 : une seule barre aussi quand la page est plus chargée (seconde rangée de filtres, bandeaux).
for (const size of [SIZES[0], SIZES[2]]) {
  for (const state of [
    "période personnalisée",
    "alerte de sécurité",
    "mode attaque",
    "période personnalisée et mode attaque",
    "période personnalisée et alerte de sécurité",
  ] as const) {
    test(`journal à ${size.width}×${size.height} avec ${state} : le tableau défile, pas la page`, async ({
      page,
    }) => {
      await page.setViewportSize(size);
      await page.goto("/?nodev#/servers/forge/audit");
      await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
      if (state.startsWith("période personnalisée")) {
        await page.getByLabel("Période").selectOption("custom");
        await expect(page.getByLabel("Du", { exact: true })).toBeVisible();
      }
      if (state !== "période personnalisée") {
        await page.waitForFunction(() =>
          Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
        );
        await page.evaluate((kind) => {
          const sim = (
            window as unknown as {
              __hearthSim: {
                security: {
                  setAlert(id: string, alert: { own: boolean; others: number | null }): void;
                  setMode(id: string, mode: string, options?: object): void;
                };
              };
            }
          ).__hearthSim;
          if (kind.includes("alerte")) sim.security.setAlert("forge", { own: true, others: null });
          else sim.security.setMode("forge", "active");
        }, state);
        await expect(
          page.getByText(
            state.includes("alerte") ? "Attaque probable détectée" : "Mode attaque actif",
          ),
        ).toBeVisible();
      }
      await page.waitForTimeout(300);
      const scroll = await page.evaluate(() => {
        const content = document.querySelector(".layout__content") as HTMLElement;
        const root = document.scrollingElement as HTMLElement;
        return {
          page: content.scrollHeight - content.clientHeight,
          layoutMain:
            (document.querySelector(".layout__main") as HTMLElement).scrollHeight -
            (document.querySelector(".layout__main") as HTMLElement).clientHeight,
          document: root.scrollHeight - root.clientHeight,
        };
      });
      expect(scroll.page, "la zone de contenu défile").toBeLessThanOrEqual(1);
      expect(scroll.document, "le document défile").toBeLessThanOrEqual(1);
      expect(scroll.layoutMain, "la mise en page déborde").toBeLessThanOrEqual(1);
    });
  }
}

// ── HRT-43 ───────────────────────────────────────────────────────────────────────────────────

for (const size of SIZES) {
  test(`journal à ${size.width}×${size.height} : la recherche s'applique à la frappe, sans bouton`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    await page.getByRole("searchbox").fill("zzz-introuvable");
    // Sans Entrée ni bouton : la liste est filtrée toute seule (court délai après la frappe).
    await expect(page.locator("section.empty")).toBeVisible({ timeout: 3000 });
    // « Effacer les filtres » n'apparaît qu'une fois dans l'interface.
    await expect(page.getByRole("button", { name: "Effacer les filtres" })).toHaveCount(1);
    await shoot(page, "hrt43-recherche", size.width, size.height);
  });

  test(`journal à ${size.width}×${size.height} : la carte de filtres tient sur une seule rangée, sans bouton « Appliquer »`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    await page.getByRole("searchbox").fill("marie");
    await expect(page.getByRole("button", { name: "Effacer les filtres" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Appliquer/ })).toHaveCount(0);
    const bottoms = await page
      .locator("form[role=search] :is(input, select, button)")
      .evaluateAll((items) =>
        items
          .filter((item) => item.getBoundingClientRect().width > 0)
          .map((item) => Math.round(item.getBoundingClientRect().bottom)),
      );
    expect(bottoms.length).toBeGreaterThanOrEqual(6);
    expect(
      Math.max(...bottoms) - Math.min(...bottoms),
      `bas des champs : ${bottoms}`,
    ).toBeLessThanOrEqual(2);
  });

  test(`journal à ${size.width}×${size.height} : un choix de liste filtre sans bouton`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    await expect(page.getByText("100 événements ou plus")).toBeVisible();
    // Période personnalisée : s'applique dès que les deux dates sont valides (ici, avant les premières entrées).
    await page.getByLabel("Période").selectOption("custom");
    const day = (offset: number) => {
      const date = new Date(Date.now() + offset * 86_400_000);
      const pad = (n: number) => String(n).padStart(2, "0");
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
    };
    await page.getByLabel("Du", { exact: true }).fill(day(-5));
    await page.getByLabel("Au", { exact: true }).fill(day(-4));
    await expect(page.getByText("100 événements ou plus")).toHaveCount(0, { timeout: 3000 });
  });

  // HRT-37 : une seule barre de défilement dans le journal : celle du tableau ; la page ne défile pas.
  test(`journal à ${size.width}×${size.height} : le tableau défile, pas la page`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    await expect(page.locator("[role=row][data-row-key]").first()).toBeVisible();
    const scroll = await page.evaluate(() => {
      const content = document.querySelector(".layout__content") as HTMLElement;
      const table = document.querySelector(".table__scroll") as HTMLElement;
      return {
        pageOverflow: content.scrollHeight - content.clientHeight,
        documentOverflow:
          (document.scrollingElement?.scrollHeight ?? 0) -
          (document.scrollingElement?.clientHeight ?? 0),
        tableOverflow: table.scrollHeight - table.clientHeight,
      };
    });
    expect(scroll.pageOverflow, "la page défile").toBeLessThanOrEqual(1);
    expect(scroll.documentOverflow, "le document défile").toBeLessThanOrEqual(1);
    expect(scroll.tableOverflow, "le tableau défile").toBeGreaterThan(0);
  });

  test(`journal à ${size.width}×${size.height} : les en-têtes sont alignés sur leurs colonnes`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/audit");
    const row = page.locator("[role=row][data-row-key]").first();
    await expect(row).toBeVisible();
    const heads = await page
      .locator("[role=columnheader]")
      .evaluateAll((cells) => cells.map((cell) => cell.getBoundingClientRect().left));
    const cells = await row
      .locator("[role=gridcell]")
      .evaluateAll((items) => items.map((item) => item.getBoundingClientRect().left));
    expect(cells.length).toBe(heads.length);
    for (const [index, head] of heads.entries()) {
      expect(Math.abs(head - (cells[index] ?? 0)), `colonne ${index + 1}`).toBeLessThanOrEqual(1);
    }
    await shoot(page, "hrt43-colonnes", size.width, size.height);
  });
}

test("journal : textes lisibles (raison avec majuscule, détail sans valeur vide)", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.goto("/?nodev#/servers/forge/audit");
  const row = page.locator("[role=row][data-row-key]").first();
  await expect(row).toBeVisible();
  const reasons = await page
    .locator("[role=row][data-row-key] [role=gridcell]:nth-child(7)")
    .allInnerTexts();
  for (const reason of reasons.filter(Boolean)) {
    expect(reason.charAt(0), reason).toBe(reason.charAt(0).toUpperCase());
  }
  await row.click();
  const detail = page.locator("dialog[open] .detail__list");
  await expect(detail).toBeVisible();
  const labels = await detail.locator("dt").allInnerTexts();
  const values = await detail.locator("dd").allInnerTexts();
  expect(
    values.every((value) => value.trim() !== ""),
    `valeurs vides : ${labels.join(", ")}`,
  ).toBe(true);
  expect(await detail.innerText()).not.toMatch(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z/);
});
