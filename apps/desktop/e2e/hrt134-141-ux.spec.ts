import { expect, type Page, test } from "@playwright/test";
import { decodePng } from "../src/assets/png-decode";

// Revue UX du 2026-10-13, sur le pont SIMULÉ, mesures de GÉOMÉTRIE aux largeurs 1100, 1280, 1366, 1920 et 2560 :
// - HRT-134 : toute bulle du tableau des comptes est entière dans la fenêtre, hors du tableau, sans barre de
//   défilement, à la souris comme au clavier, fermée par Échap ;
// - HRT-135 : le repère « Peut gérer les services » s'explique (survol, focus) et ouvre le détail du compte ;
// - HRT-141 : lien d'évitement au premier Tab, zone principale nommée, contrastes des jetons de texte.
// Captures : dossier de `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

type Sim = {
  setServiceRightOnServer(serverId: string, username: string, granted: boolean): void;
  setState(id: string, state: string): void;
};

const sim = <T>(page: Page, run: (sim: Sim) => T) =>
  page.evaluate((source) => {
    const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    return new Function("sim", `return (${source})(sim)`)(bridge);
  }, run.toString());

/** Comptes de « forge » : marie (toi, administrateur sans le droit), paul qui l'a. */
async function openAccounts(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) => {
    s.setServiceRightOnServer("forge", "marie", false);
    s.setServiceRightOnServer("forge", "paul", true);
  });
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/accounts";
  });
  await expect(page.locator("tr[data-account]").first()).toBeVisible();
}

async function expectBubbleWhole(page: Page, text: RegExp, name: string, size: { width: number }) {
  const bubble = page.locator("[role=tooltip]:visible").filter({ hasText: text });
  await expect(bubble).toBeVisible();
  const box = await bubble.boundingBox();
  const viewport = page.viewportSize();
  expect(box, "la bulle a une boîte").not.toBeNull();
  if (!box || !viewport) return;
  expect(box.x, "bord gauche").toBeGreaterThanOrEqual(0);
  expect(box.y, "bord haut").toBeGreaterThanOrEqual(0);
  expect(box.x + box.width, "bord droit").toBeLessThanOrEqual(viewport.width);
  expect(box.y + box.height, "bord bas").toBeLessThanOrEqual(viewport.height);
  const facts = await bubble.evaluate((element) => ({
    clipped:
      element.scrollWidth > element.clientWidth + 1 ||
      element.scrollHeight > element.clientHeight + 1,
    inBody: element.parentElement === document.body,
    // Aucune barre de défilement : ni dans le tableau, ni dans la page.
    tableScroll: (() => {
      const wrap = document.querySelector(".table-wrap");
      return wrap ? wrap.scrollWidth - wrap.clientWidth : 0;
    })(),
    pageScroll: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    // Hors flux, au-dessus de la page : position fixe et couche des bulles.
    fixed: getComputedStyle(element).position === "fixed",
    layer: Number(getComputedStyle(element).zIndex),
  }));
  expect(facts.clipped, "texte coupé dans la bulle").toBe(false);
  expect(facts.inBody, "bulle rendue dans body").toBe(true);
  expect(facts.tableScroll, "défilement horizontal du tableau").toBeLessThanOrEqual(0);
  expect(facts.pageScroll, "défilement horizontal de la page").toBeLessThanOrEqual(0);
  expect(facts.fixed, "bulle en position fixe").toBe(true);
  expect(facts.layer, "couche au-dessus de la page").toBeGreaterThan(1);
  await page.screenshot({ path: `${SHOTS}/${name}-${size.width}.png` });
}

for (const size of SIZES) {
  test(`HRT-134 à ${size.width} : la raison du refus de suppression se lit en entier, à la souris et au clavier`, async ({
    page,
  }) => {
    await openAccounts(page, size);
    const remove = page
      .locator("tr[data-account='paul']")
      .getByRole("button", { name: /^Supprimer paul/ });
    await remove.hover();
    await expectBubbleWhole(
      page,
      /Seul un compte qui le peut aussi/,
      "hrt134-supprimer-survol",
      size,
    );
    // Échap ferme la bulle (le focus reste sur le bouton).
    await remove.focus();
    await page.keyboard.press("Escape");
    await expect(page.locator("[role=tooltip]:visible")).toHaveCount(0);
    // Au clavier : même bulle, entière.
    await page.mouse.move(0, 0);
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    await expect(remove).toBeFocused();
    await expectBubbleWhole(
      page,
      /Seul un compte qui le peut aussi/,
      "hrt134-supprimer-focus",
      size,
    );
  });

  test(`HRT-134 à ${size.width} : chaque bouton inactif du tableau a sa bulle entière, hors ligne comprise`, async ({
    page,
  }) => {
    await openAccounts(page, size);
    // Un compte sans session : « Fermer les sessions » dit pourquoi il est inactif.
    const close = page
      .locator("tr[data-account='lea']")
      .getByRole("button", { name: /^Fermer les sessions/ });
    await close.hover();
    await expectBubbleWhole(page, /aucune session ouverte/, "hrt134-sessions-survol", size);
    // Hors ligne : chaque bouton du tableau explique le blocage, sans couper ni faire défiler.
    await sim(page, (s) => s.setState("forge", "offline"));
    await page.mouse.move(0, 0);
    const remove = page
      .locator("tr[data-account='lea']")
      .getByRole("button", { name: /^Supprimer lea/ });
    await remove.hover();
    await expectBubbleWhole(
      page,
      /Indisponible tant que le serveur est hors ligne/,
      "hrt134-horsligne-survol",
      size,
    );
  });

  test(`HRT-135 à ${size.width} : le repère du droit s'explique et ouvre le détail, au clavier aussi`, async ({
    page,
  }) => {
    await openAccounts(page, size);
    const tag = page.locator("tr[data-account='paul'] [data-services-right]");
    await expect(tag).toHaveAccessibleName(/Peut gérer les services/);
    await tag.hover();
    await expectBubbleWhole(
      page,
      /démarrer, arrêter, redémarrer et déployer/,
      "hrt135-repere-survol",
      size,
    );
    // Le nom du compte a l'air d'un lien : souligné sans survol.
    const underline = await page
      .locator("tr[data-account='paul'] .table__open")
      .evaluate((element) => getComputedStyle(element).textDecorationLine);
    expect(underline).toBe("underline");
    // Au clavier : focus puis Entrée ouvre le détail, qui commence par ce que le droit permet.
    await page.mouse.move(0, 0);
    await tag.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText(
      "Ce droit permet de démarrer, arrêter, redémarrer et déployer",
    );
    await expect(dialog).toContainText(
      /seul un compte qui le peut aussi, ou la commande sur le serveur/,
    );
    await expect(dialog.locator("pre")).toHaveText(
      /^sudo hearth-agent account services paul --revoke$/,
    );
    await page.screenshot({ path: `${SHOTS}/hrt135-detail-${size.width}.png` });
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(tag).toBeFocused();
  });
}

test("HRT-135 : sur son propre compte, l'avertissement précède la commande qui retire le droit", async ({
  page,
}) => {
  await openAccounts(page, { width: 1366, height: 800 });
  await sim(page, (s) => s.setServiceRightOnServer("forge", "marie", true));
  await page.reload();
  await page.locator("tr[data-account='marie'] .table__open").click();
  const dialog = page.getByRole("dialog");
  const warning = dialog.locator("[data-rights-own-warning]");
  await expect(warning).toContainText("C'est votre propre compte");
  const order = await dialog.evaluate((root) => {
    const warn = root.querySelector("[data-rights-own-warning]");
    const command = root.querySelector("[data-rights-command]");
    if (!warn || !command) return false;
    return Boolean(warn.compareDocumentPosition(command) & Node.DOCUMENT_POSITION_FOLLOWING);
  });
  expect(order).toBe(true);
});

for (const size of SIZES) {
  test(`HRT-141 à ${size.width} : un lien d'évitement au premier Tab, une zone principale nommée`, async ({
    page,
  }) => {
    await openAccounts(page, size);
    await page.keyboard.press("Tab");
    const skip = page.locator("[data-skip-link]");
    await expect(skip).toBeFocused();
    const box = await skip.boundingBox();
    expect(box && box.x >= 0 && box.y >= 0, "le lien est à l'écran quand il a le focus").toBe(true);
    await page.screenshot({ path: `${SHOTS}/hrt141-lien-evitement-${size.width}.png` });
    await page.keyboard.press("Enter");
    const main = page.getByRole("main", { name: "Comptes" });
    await expect(main).toBeFocused();
    // Hors focus, le lien n'est pas à l'écran.
    const hidden = await skip.evaluate((element) => element.getBoundingClientRect().bottom <= 0);
    expect(hidden).toBe(true);
  });
}

// Contraste WCAG calculé sur les valeurs réelles des jetons (le thème est unique, sombre).
function luminance(hex: string): number {
  const channel = (i: number) => {
    const value = Number.parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}
function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}
test("HRT-141 : trois niveaux de texte lisibles (4,5 pour 1) et distincts sur chaque fond", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  const tokens = await page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    const read = (name: string) => style.getPropertyValue(`--${name}`).trim();
    return {
      bg: read("bg"),
      side: read("side"),
      card: read("card"),
      "card-2": read("card-2"),
      tx: read("tx"),
      "tx-secondary": read("tx-secondary"),
      tx3: read("tx3"),
    };
  });
  for (const surface of ["bg", "side", "card", "card-2"] as const) {
    for (const text of ["tx", "tx-secondary", "tx3"] as const) {
      expect(ratio(tokens[text], tokens[surface]), `${text} sur ${surface}`).toBeGreaterThanOrEqual(
        4.5,
      );
    }
  }
  // La hiérarchie reste visible : au moins 1,5 pour 1 entre deux niveaux voisins.
  expect(ratio(tokens.tx, tokens["tx-secondary"]), "principal / secondaire").toBeGreaterThanOrEqual(
    1.5,
  );
  expect(ratio(tokens["tx-secondary"], tokens.tx3), "secondaire / discret").toBeGreaterThanOrEqual(
    1.5,
  );
});

/** Couleur calculée `rgb(r, g, b)` en `#rrggbb`. */
function hexOf(rgb: string): string {
  const parts = (rgb.match(/[\d.]+/g) ?? []).slice(0, 3).map((n) => Math.round(Number(n)));
  return `#${parts.map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}

type Look = {
  color: string;
  background: string;
  opacity: string;
  shadow: string;
  cursor: string;
  border: string;
};

test("HRT-141 : un bouton inactif se reconnaît d'un coup d'œil et reste lisible, pour les quatre variantes", async ({
  page,
}) => {
  await openAccounts(page, { width: 1366, height: 800 });
  const tokens = await page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    const read = (name: string) => style.getPropertyValue(`--${name}`).trim();
    return { "card-2": read("card-2"), ac: read("ac") };
  });
  // Un vrai bouton inactif du tableau (« Fermer les sessions » d'un compte sans session) sert de modèle : on en
  // pose une copie par variante, avec la vraie feuille de style du composant, active puis inactive.
  const model = page
    .locator("tr[data-account='lea']")
    .getByRole("button", { name: /^Fermer les sessions/ });
  await expect(model).toHaveAttribute("aria-disabled", "true");
  const measures = await model.evaluate((button) => {
    const host = button.parentElement;
    if (!host) throw new Error("pas d'hôte");
    const read = (element: Element): Look => {
      const style = getComputedStyle(element);
      return {
        color: style.color,
        background: style.backgroundColor,
        opacity: style.opacity,
        shadow: style.boxShadow,
        cursor: style.cursor,
        border: style.borderTopStyle,
      };
    };
    const out: Record<string, { active: Look; inactive: Look }> = {};
    for (const variant of ["primary", "danger", "secondary", "ghost"]) {
      const looks: Look[] = [];
      for (const inactive of [false, true]) {
        const copy = button.cloneNode(true) as HTMLElement;
        copy.className = copy.className.replace(
          /btn--(primary|danger|secondary|ghost)/,
          `btn--${variant}`,
        );
        if (inactive) copy.setAttribute("aria-disabled", "true");
        else copy.removeAttribute("aria-disabled");
        copy.setAttribute("data-probe", `${variant}-${inactive ? "inactif" : "actif"}`);
        host.appendChild(copy);
        looks.push(read(copy));
      }
      out[variant] = { active: looks[0] as Look, inactive: looks[1] as Look };
    }
    return out;
  });
  for (const [variant, { active, inactive }] of Object.entries(measures)) {
    const label = `variante ${variant}`;
    // Lisible : texte contre son fond, sans passer par l'opacité.
    expect(inactive.opacity, `${label} : aucune opacité`).toBe("1");
    expect(inactive.shadow, `${label} : pas d'ombre`).toBe("none");
    expect(inactive.cursor, `${label} : curseur`).toBe("not-allowed");
    expect(inactive.border, `${label} : contour en tirets`).toBe("dashed");
    expect(hexOf(inactive.background), `${label} : fond de surface neutre`).toBe(tokens["card-2"]);
    expect(
      ratio(hexOf(inactive.color), hexOf(inactive.background)),
      `${label} : texte inactif contre son fond`,
    ).toBeGreaterThanOrEqual(4.5);
    // Reconnaissable : ni la couleur d'accent, ni le texte de l'actif.
    expect(hexOf(inactive.background), `${label} : pas d'accent`).not.toBe(tokens.ac);
    expect(hexOf(inactive.color), `${label} : texte différent de l'actif`).not.toBe(
      hexOf(active.color),
    );
  }
  // Le bouton principal inactif contre l'actif : bien plus que les 1,65 pour 1 d'une simple opacité.
  const primary = measures.primary;
  if (!primary) throw new Error("pas de variante principale");
  expect(
    ratio(hexOf(primary.active.background), hexOf(primary.inactive.background)),
    "principal actif contre inactif",
  ).toBeGreaterThanOrEqual(3);
});

// Captures de contrôle (Réglages, tableau de bord, comptes) aux quatre largeurs de la revue ; aucune assertion
// de style ici, les mesures sont dans les scénarios ci-dessus.
for (const size of SIZES.filter((entry) => entry.width >= 1280)) {
  test(`captures à ${size.width} : réglages, tableau de bord, comptes`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/settings");
    await expect(page.locator("[data-agent-explain]")).toBeVisible();
    await expect(page.locator("[data-own-accounts]")).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/hrt139-reglages-${size.width}.png` });
    await page.goto("/?nodev#/servers/forge/dashboard");
    await expect(page.locator(".disk").first()).toBeVisible();
    await expect(page.locator(".disk__name").first()).toContainText("Disque système");
    await page.screenshot({ path: `${SHOTS}/hrt142-tableau-de-bord-${size.width}.png` });
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator("tr[data-account]").first()).toBeVisible();
    await page.screenshot({ path: `${SHOTS}/hrt141-comptes-contrastes-${size.width}.png` });
  });
}

// Revue de la PR 91 : la bulle est un `popover` (couche supérieure), donc au-dessus d'une fenêtre modale ouverte
// par `showModal()`. Aucune fenêtre de l'application n'a de bulle aujourd'hui : on ouvre la VRAIE fenêtre du détail
// d'un compte, puis on ouvre une VRAIE bulle de la page (celle du repère, par l'événement que le survol lèverait),
// que l'on recouvre avec la fenêtre : si la bulle passait dessous, le point d'essai renverrait la fenêtre.
test("la bulle ouverte pendant qu'une fenêtre modale est ouverte reste au-dessus de la fenêtre", async ({
  page,
}) => {
  await openAccounts(page, { width: 1366, height: 800 });
  const tag = page.locator("tr[data-account='paul'] [data-services-right]");
  await tag.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  const facts = await page.evaluate(() => {
    const modal = document.querySelector("dialog[open]") as HTMLElement;
    const trigger = document
      .querySelector("tr[data-account='paul'] [data-services-right]")
      ?.closest(".tip") as HTMLElement;
    trigger.dispatchEvent(new MouseEvent("mouseenter"));
    return new Promise<{
      popover: boolean;
      modalIsModal: boolean;
      box: { x: number; y: number };
    }>((resolve) =>
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          const bubble = Array.from(document.querySelectorAll<HTMLElement>("[role=tooltip]")).find(
            (element) => getComputedStyle(element).display !== "none",
          );
          if (!bubble) throw new Error("aucune bulle ouverte");
          const box = bubble.getBoundingClientRect();
          // La fenêtre est ramenée pile sur la bulle.
          modal.style.margin = "0";
          modal.style.position = "fixed";
          modal.style.left = `${box.left - 20}px`;
          modal.style.top = `${box.top - 20}px`;
          resolve({
            popover: bubble.matches(":popover-open"),
            modalIsModal: modal.matches(":modal"),
            box: { x: box.left, y: box.top },
          });
        }),
      ),
    );
  });
  await page.screenshot({ path: `${SHOTS}/bulle-dans-fenetre-modale-1366.png` });
  expect(facts.modalIsModal, "la fenêtre est modale (couche supérieure)").toBe(true);
  expect(facts.popover, "la bulle est dans la couche supérieure").toBe(true);
  // Ce qui est PEINT au coin de la bulle (dans son remplissage) : la couleur de la bulle, pas celle de la fenêtre.
  // (`elementFromPoint` ne sert pas : le reste de la page est inerte pendant une fenêtre modale.)
  const colors = await page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    return {
      bubble: style.getPropertyValue("--card-2").trim(),
      dialog: style.getPropertyValue("--card").trim(),
    };
  });
  const shot = await page.screenshot({
    clip: { x: facts.box.x + 3, y: facts.box.y + 3, width: 2, height: 2 },
  });
  const { rgba } = decodePng(shot);
  const painted = `#${[0, 1, 2].map((i) => (rgba[i] ?? 0).toString(16).padStart(2, "0")).join("")}`;
  // Lissage de la capture : on compare des distances, pas l'égalité exacte.
  const distance = (x: string, y: string) =>
    [1, 3, 5].reduce(
      (sum, i) =>
        sum +
        Math.abs(Number.parseInt(x.slice(i, i + 2), 16) - Number.parseInt(y.slice(i, i + 2), 16)),
      0,
    );
  expect(
    distance(painted, colors.bubble),
    "la couleur peinte est celle de la bulle, pas celle de la fenêtre",
  ).toBeLessThan(distance(painted, colors.dialog));
  expect(distance(painted, colors.bubble)).toBeLessThanOrEqual(12);
});

// Revue de la PR 91 : la bulle suit son déclencheur quand il bouge sans défilement, et se ferme quand il disparaît
// ou sort du cadre de son conteneur.
test("la bulle suit son déclencheur quand il bouge, et se ferme quand il disparaît ou sort du cadre", async ({
  page,
}) => {
  await openAccounts(page, { width: 1366, height: 345 });
  const remove = page
    .locator("tr[data-account='paul']")
    .getByRole("button", { name: /^Supprimer paul/ });
  // Au focus : la souris ne quitte pas le contrôle quand la page défile.
  await remove.focus();
  const bubble = page.locator("[role=tooltip]:visible");
  await expect(bubble).toBeVisible();
  const gap = async () => {
    const [b, r] = await Promise.all([bubble.boundingBox(), remove.boundingBox()]);
    if (!b || !r) throw new Error("pas de boîte");
    return Math.round(
      Math.abs(b.y + b.height - r.y) < Math.abs(b.y - (r.y + r.height))
        ? r.y - (b.y + b.height)
        : b.y - (r.y + r.height),
    );
  };
  const before = await gap();
  // 1. Le déclencheur bouge sans défilement ni redimensionnement (une ligne s'ajoute au-dessus).
  await page.evaluate(() => {
    const spacer = document.createElement("div");
    spacer.id = "sonde-espace";
    spacer.style.height = "24px";
    document.querySelector("tr[data-account='paul']")?.closest("table")?.before(spacer);
  });
  await expect.poll(gap).toBe(before);
  // 2. Le déclencheur défile avec la page : la bulle l'accompagne.
  await page.evaluate(() => {
    const row = document.querySelector("tr[data-account='paul']");
    let node: HTMLElement | null = row as HTMLElement | null;
    while (node && node.scrollHeight <= node.clientHeight + 1) node = node.parentElement;
    node?.scrollBy(0, 12);
  });
  await expect.poll(gap).toBe(before);
  // 3. Le déclencheur sort du cadre du conteneur qui défile (même en restant dans la fenêtre) : la bulle ne flotte pas sans lui.
  const outside = await page.evaluate(() => {
    const row = document.querySelector("tr[data-account='paul']") as HTMLElement;
    // Du contenu de plus sous le tableau : de quoi défiler loin, jusqu'à faire sortir la ligne par le haut.
    const tall = document.createElement("div");
    tall.style.height = "3000px";
    row.closest("table")?.after(tall);
    const button = row.querySelector("button[aria-label^='Supprimer'], button") as HTMLElement;
    let node: HTMLElement | null = row;
    while (node && node.scrollHeight <= node.clientHeight + 1) node = node.parentElement;
    if (!node) return "pas de conteneur qui défile";
    node.scrollTo(0, node.scrollHeight);
    return new Promise<string>((resolve) =>
      requestAnimationFrame(() => {
        const frame = node.getBoundingClientRect();
        const box = button.getBoundingClientRect();
        resolve(box.top >= frame.bottom || box.bottom <= frame.top ? "dehors" : "dedans");
      }),
    );
  });
  expect(outside, "le bouton est sorti du cadre au défilement").toBe("dehors");
  await expect(page.locator("[role=tooltip]:visible")).toHaveCount(0);
  // 4. Le déclencheur disparaît (masqué) pendant que la bulle est ouverte : elle se ferme.
  await remove.blur();
  await remove.focus();
  await expect(bubble).toBeVisible();
  await page.evaluate(() => {
    const row = document.querySelector("tr[data-account='paul']") as HTMLElement;
    row.style.display = "none";
  });
  await expect(page.locator("[role=tooltip]:visible")).toHaveCount(0);
});
