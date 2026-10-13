import { expect, type Page, test } from "@playwright/test";

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
  await expect(warning).toContainText("C'est ton propre compte");
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
function mix(fg: string, bg: string, alpha: number): string {
  const part = (i: number) =>
    Math.round(
      Number.parseInt(fg.slice(1 + i * 2, 3 + i * 2), 16) * alpha +
        Number.parseInt(bg.slice(1 + i * 2, 3 + i * 2), 16) * (1 - alpha),
    )
      .toString(16)
      .padStart(2, "0");
  return `#${part(0)}${part(1)}${part(2)}`;
}

test("HRT-141 : le texte atténué atteint 4,5 pour 1 sur chaque fond, un bouton inactif 3 pour 1", async ({
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
      tx2: read("tx2"),
      tx3: read("tx3"),
      ac: read("ac"),
      "on-ac": read("on-ac"),
      crit: read("crit"),
      opacity: Number.parseFloat(read("opacity-disabled")),
    };
  });
  for (const surface of ["bg", "side", "card", "card-2"] as const) {
    for (const text of ["tx", "tx2", "tx3"] as const) {
      expect(ratio(tokens[text], tokens[surface]), `${text} sur ${surface}`).toBeGreaterThanOrEqual(
        4.5,
      );
    }
  }
  // Inactif : tout (texte ET fond) passe par l'opacité sur la carte.
  const under = tokens.card;
  const variants = [
    { name: "secondaire", fg: tokens.tx, bg: under },
    { name: "danger", fg: tokens.crit, bg: under },
    { name: "principal", fg: tokens["on-ac"], bg: tokens.ac },
  ];
  for (const { name, fg, bg } of variants) {
    const shownText = mix(fg, under, tokens.opacity);
    const shownBg = bg === under ? under : mix(bg, under, tokens.opacity);
    expect(ratio(shownText, shownBg), `bouton ${name} inactif`).toBeGreaterThanOrEqual(3);
  }
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
