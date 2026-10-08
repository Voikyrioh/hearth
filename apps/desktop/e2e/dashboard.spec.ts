import { expect, type Page, test } from "@playwright/test";

// Parcours du tableau de bord avec le pont SIMULÉ (mesures plausibles, une par seconde). Captures
// dans e2e/screenshots/ (non commitées) : 1366, 1920 et 2560 px, comme la revue UX.
const WIDTHS = [1366, 1920, 2560] as const;
const HEIGHT = 1000;

type Sim = {
  setState(id: string, state: string): void;
  machine: {
    pin(id: string, measure: string, level: string | null): void;
    tick(id: string): unknown;
  };
};

async function sim<T>(page: Page, run: (sim: Sim) => T) {
  await page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    new Function("sim", source)(bridge);
  }, `(${run.toString()})(sim)`);
}

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const section = (page: Page, title: string) =>
  page
    .locator("section.card")
    .filter({ has: page.getByRole("heading", { name: title, exact: true }) });

const cpuCurve = (page: Page) =>
  section(page, "Processeur").locator("path.chart__line--ac").first();

test("les mesures arrivent : toutes les sections, des courbes qui avancent", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  for (const title of [
    "Machine",
    "Durée de fonctionnement",
    "Processeur",
    "Mémoire",
    "Carte graphique",
    "Réseau",
    "Disques",
    "Températures",
  ]) {
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
  }
  await expect(section(page, "Machine")).toContainText("NixOS 25.05");
  await expect(section(page, "Processeur")).toContainText("Charge globale");
  await expect(section(page, "Mémoire")).toContainText("Utilisée / Totale");
  await expect(section(page, "Réseau")).toContainText(/\d+\.\d Mo\/s|\d+ Ko\/s/);
  await expect(page.locator("figure.gauge")).toHaveCount(4);
  await expect(page.locator("svg.chart")).toHaveCount(6);
  await expect(page.locator("[data-level='attention'], [data-level='critical']")).toHaveCount(0);
  // Une mesure par seconde : le tracé de la courbe du processeur change.
  const first = await cpuCurve(page).getAttribute("d");
  await expect
    .poll(async () => cpuCurve(page).getAttribute("d"), { timeout: 5000 })
    .not.toBe(first);
  await shoot(page, "tableau-de-bord");
});

test("un seuil franchi se voit (icône et mot), puis s'efface", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const memory = section(page, "Mémoire");
  await expect(memory).toBeVisible();
  await sim(page, (s) => {
    s.machine.pin("forge", "mem", "critical");
    s.machine.pin("forge", "gpuTemp", "attention");
  });
  await expect(memory.locator("figure[data-level='critical']")).toContainText("Critique", {
    timeout: 5000,
  });
  await expect(memory.locator("figure[data-level='critical'] svg").first()).toBeVisible();
  await expect(section(page, "Carte graphique")).toContainText("Attention");
  await shoot(page, "tableau-de-bord-alerte");
  await sim(page, (s) => {
    s.machine.pin("forge", "mem", null);
    s.machine.pin("forge", "gpuTemp", null);
  });
  await expect(page.locator("[data-level='attention'], [data-level='critical']")).toHaveCount(0, {
    timeout: 5000,
  });
});

test("perte du lien : dernières valeurs grisées et datées, puis retour en direct", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(section(page, "Processeur")).toBeVisible();
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await sim(page, (s) => s.setState("forge", "offline"));
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  await expect(page.getByText(/^Vu il y a \d+ s$/)).toBeVisible();
  // Les valeurs sont toujours là, pas vidées.
  await expect(section(page, "Processeur")).toContainText("Charge globale");
  await expect(section(page, "Machine")).toContainText("NixOS 25.05");
  // Aucune mesure n'arrive : la courbe ne bouge pas.
  const frozen = await cpuCurve(page).getAttribute("d");
  // La machine est mesurée pendant la coupure mais rien n'est annoncé : pas d'attente de vrai temps.
  await sim(page, (s) => {
    s.machine.tick("forge");
    s.machine.tick("forge");
  });
  expect(await cpuCurve(page).getAttribute("d")).toBe(frozen);
  await shoot(page, "tableau-de-bord-hors-ligne");

  await sim(page, (s) => s.setState("forge", "connected"));
  await expect(page.locator('[data-stale="true"]')).toHaveCount(0);
  await expect
    .poll(async () => cpuCurve(page).getAttribute("d"), { timeout: 5000 })
    .not.toBe(frozen);
});

test("l'heure écoulée est déjà là à l'ouverture : courbe d'une heure remplie (captures 1920 et 2560)", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const radios = page.getByRole("radiogroup", { name: "Durée des courbes" }).getByRole("radio");
  await radios.nth(2).click();
  await expect(radios.nth(2)).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
  // Les six courbes ont des points sur presque toute la largeur : le tracé du processeur est long.
  const path = await cpuCurve(page).getAttribute("d");
  expect((path ?? "").split(/[ML]/).filter(Boolean).length).toBeGreaterThan(300);
  for (const width of [1920, 2560] as const) {
    await page.setViewportSize({ width, height: HEIGHT });
    await page.screenshot({ path: `e2e/screenshots/dashboard-heure-ouverture-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
});

test("un pic d'il y a 40 minutes se voit encore sur la fenêtre 1 h (maximum par pas, capture 1920)", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const radios = page.getByRole("radiogroup", { name: "Durée des courbes" }).getByRole("radio");
  await radios.nth(2).click();
  await expect(radios.nth(2)).toHaveAttribute("aria-checked", "true");
  const chart = section(page, "Processeur").locator("svg.chart").first();
  const height = await chart.evaluate(
    (svg) => (svg as unknown as SVGSVGElement).viewBox.baseVal.height,
  );
  const path = (await cpuCurve(page).getAttribute("d")) ?? "";
  const ys = [...path.matchAll(/[ML]\s*(-?[\d.]+)[ ,](-?[\d.]+)/g)].map((match) =>
    Number(match[2]),
  );
  // La courbe touche le haut du graphique : le pic à 100 % n'a pas été moyenné.
  expect(Math.min(...ys)).toBeLessThan(height * 0.1);
  await page.setViewportSize({ width: 1920, height: HEIGHT });
  await page.screenshot({ path: "e2e/screenshots/dashboard-1h-pic-ancien-1920.png" });
  await page.setViewportSize({ width: 1366, height: 800 });
});

test("les courbes basculent entre 1 min, 5 min et 1 h sans attente", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  const radios = page.getByRole("radiogroup", { name: "Durée des courbes" }).getByRole("radio");
  await expect(radios).toHaveText(["1 min", "5 min", "1 h"]);
  await expect(radios.nth(1)).toHaveAttribute("aria-checked", "true");
  await radios.nth(2).click();
  await expect(radios.nth(2)).toHaveAttribute("aria-checked", "true");
  // L'heure est lue à la connexion (BR-DASH-010) : la courbe est déjà remplie dès l'ouverture.
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
  await radios.nth(0).click();
  await expect(radios.nth(0)).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText(/^Depuis/)).toHaveCount(0);
});

test("une machine sans carte graphique ni sonde garde ses sections, avec leur explication", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/salon/dashboard");
  await expect(section(page, "Carte graphique")).toContainText(
    "Aucune carte graphique mesurable sur cette machine",
  );
  await expect(section(page, "Températures")).toContainText(
    "Sondes non disponibles sur cette machine. Ce matériel n'expose pas sa température au système.",
  );
  await expect(section(page, "Processeur")).toBeVisible();
  await shoot(page, "tableau-de-bord-sans-materiel");
});

// HRT-34 : la grille se remplit sans trou ni chevauchement, aux cinq tailles (revue UX C6, C7, C14).
const LAYOUT_SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

for (const size of LAYOUT_SIZES) {
  test(`tableau de bord à ${size.width}×${size.height} : rien ne se chevauche, une rangée = une hauteur, courbes lisibles`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.getByRole("heading", { name: "Processeur", exact: true })).toBeVisible();
    const boxes = await page.locator("section.card").evaluateAll((cards) =>
      cards.map((card) => {
        const rect = card.getBoundingClientRect();
        const title = card.querySelector("h2");
        return {
          title: title?.textContent ?? "",
          x: rect.x,
          y: rect.y + window.scrollY,
          w: rect.width,
          h: rect.height,
          titleLines: title ? Math.round(title.getBoundingClientRect().height / 16) : 0,
        };
      }),
    );
    expect(boxes.length).toBeGreaterThanOrEqual(7);
    // Aucun chevauchement entre deux cartes.
    for (const a of boxes) {
      for (const b of boxes) {
        if (a === b) continue;
        const overlap =
          a.x < b.x + b.w - 1 && b.x < a.x + a.w - 1 && a.y < b.y + b.h - 1 && b.y < a.y + a.h - 1;
        expect(overlap, `${a.title} chevauche ${b.title}`).toBe(false);
      }
    }
    // Une rangée = une hauteur : les cartes qui commencent à la même ordonnée (hors colonnes empilées, dont
    // la somme des hauteurs compte) finissent à la même ordonnée que la plus haute de leur rangée.
    const rows = new Map<number, typeof boxes>();
    for (const box of boxes) {
      const key = Math.round(box.y / 4) * 4;
      rows.set(key, [...(rows.get(key) ?? []), box]);
    }
    const grid = await page.locator(".dash__grid").boundingBox();
    expect(grid).not.toBeNull();
    for (const group of rows.values()) {
      const bottoms = group.map((box) => Math.round(box.y + box.h));
      // Les cartes d'une même rangée se terminent à moins de 2 px près, sauf une colonne empilée qui
      // contient plusieurs cartes (sa dernière carte se termine, elle, avec la rangée).
      const last = Math.max(...bottoms);
      for (const box of group) {
        const stackedUnder = boxes.some(
          (other) => other !== box && Math.abs(other.x - box.x) < 2 && other.y > box.y,
        );
        if (!stackedUnder) {
          expect(Math.abs(box.y + box.h - last), `${box.title} finit avec sa rangée`).toBeLessThan(
            3,
          );
        }
      }
    }
    // REMPLISSAGE (revue UX C7) : dans chaque carte, l'espace vide sous le dernier élément de contenu ne dépasse
    // pas le remplissage normal de la carte (32 px de tolérance) : les trous ne passent pas DANS les cartes.
    const gaps = await page.locator("section.card").evaluateAll((cards) =>
      cards.map((card) => {
        const box = card.getBoundingClientRect();
        const padding = Number.parseFloat(getComputedStyle(card).paddingBottom);
        let bottom = 0;
        for (const element of card.querySelectorAll("*")) {
          const rect = element.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) bottom = Math.max(bottom, rect.bottom);
        }
        return {
          title: card.querySelector("h2")?.textContent ?? "",
          empty: Math.round(box.bottom - bottom - padding),
        };
      }),
    );
    for (const gap of gaps) {
      expect(gap.empty, `${gap.title} : vide sous son contenu`).toBeLessThanOrEqual(32);
    }
    // VIDES INTERNES (retour de revue HRT-34) : une jauge n'est jamais perdue au milieu d'un grand vide : l'espace
    // au-dessus et au-dessous d'elle, dans sa rangée, reste sous 100 px (80 px de la moitié d'une jauge, plus la ligne « durée / échelle » que HRT-41 pose au-dessus des courbes).
    const voids = await page.locator("figure.gauge").evaluateAll((gauges) =>
      gauges.map((gauge) => {
        const row = (gauge.parentElement as HTMLElement).getBoundingClientRect();
        // Le contenu de la jauge (cadran puis légende), pas sa boîte (qui peut être étirée).
        const top = (gauge.querySelector(".gauge__dial") as HTMLElement).getBoundingClientRect()
          .top;
        const bottom = (
          gauge.querySelector(".gauge__caption") as HTMLElement
        ).getBoundingClientRect().bottom;
        return {
          name: gauge.getAttribute("aria-label") ?? "",
          above: Math.round(top - row.top),
          below: Math.round(row.bottom - bottom),
        };
      }),
    );
    for (const gap of voids) {
      expect(gap.above, `${gap.name} : vide au-dessus`).toBeLessThanOrEqual(100);
      expect(gap.below, `${gap.name} : vide au-dessous`).toBeLessThanOrEqual(100);
    }
    // Pas de trou entre le contenu d'une carte et sa courbe (hors rangées jauge + courbe).
    const chartGaps = await page.locator(".series").evaluateAll((series) =>
      series.flatMap((element) => {
        const parent = element.parentElement as HTMLElement;
        if (getComputedStyle(parent).flexDirection === "row") return [];
        const before = element.previousElementSibling;
        if (!before) return [];
        return [
          Math.round(element.getBoundingClientRect().top - before.getBoundingClientRect().bottom),
        ];
      }),
    );
    for (const gap of chartGaps) expect(gap, "vide avant une courbe").toBeLessThanOrEqual(24);
    // Le point de montage se lit : même taille que le nom du disque (pas une note en petit).
    const sizes = await page.evaluate(() => {
      const mount = document.querySelector(".disk__mount");
      const name = document.querySelector(".disk__name");
      return mount && name
        ? [
            Number.parseFloat(getComputedStyle(mount).fontSize),
            Number.parseFloat(getComputedStyle(name).fontSize),
          ]
        : null;
    });
    expect(sizes).not.toBeNull();
    expect(sizes?.[0] ?? 0).toBeGreaterThanOrEqual((sizes?.[1] ?? 99) - 0.5);
    // Pas de titre cassé sur plusieurs lignes, courbes d'au moins 150 px de large.
    for (const box of boxes) expect(box.titleLines, box.title).toBeLessThanOrEqual(1);
    const charts = await page
      .locator("svg.chart")
      .evaluateAll((svgs) => svgs.map((svg) => svg.getBoundingClientRect().width));
    for (const width of charts) expect(width).toBeGreaterThanOrEqual(150);
    // À 1920 et plus, l'essentiel tient sans défiler.
    // Les cartes sont ENTIÈRES dans la zone de contenu (pas seulement leur haut) quand tout peut tenir.
    const zoneBottom = await page.evaluate(
      () => document.querySelector(".layout__content")?.getBoundingClientRect().bottom ?? 0,
    );
    if (size.width >= 1920) {
      for (const box of boxes) {
        expect(box.y + box.h, `${box.title} entière dans la zone`).toBeLessThanOrEqual(
          zoneBottom - 1,
        );
      }
    }
    if (size.width >= 1920) {
      const needed = [
        "Processeur",
        "Mémoire",
        "Carte graphique",
        "Réseau",
        "Disques",
        "Températures",
      ];
      for (const title of needed) {
        const box = boxes.find((b) => b.title.toLowerCase() === title.toLowerCase());
        expect(box, title).toBeDefined();
        expect((box?.y ?? 0) + (box?.h ?? 0), `${title} visible sans défiler`).toBeLessThanOrEqual(
          size.height,
        );
      }
    }
    await page.screenshot({
      path: `e2e/screenshots/hrt34-tableau-de-bord-${size.width}x${size.height}.png`,
    });
  });
}
