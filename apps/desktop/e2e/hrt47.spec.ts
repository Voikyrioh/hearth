import { expect, type Page, test } from "@playwright/test";

// HRT-47, premier smoke de Voiky : S1b (un point de montage ou un nom de sonde long ne sort jamais de sa carte),
// S2 (anneau du serveur sélectionné entier), S3 (chiffres des étapes centrés), S4 (exemples neutres). Pont SIMULÉ :
// un volume Docker au chemin de plus de 80 caractères et une sonde au nom très long y sont posés.

const SIZES = [
  { width: 1100, height: 680 },
  { width: 1280, height: 800 },
  { width: 1366, height: 800 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
] as const;

const OVERLAY = {
  name: "overlay",
  mount:
    "/var/lib/docker/rootfs/overlayfs/516ea538beeb5e0443f9fa80d43d3f8c4c7b5fb99d71715882d9cb491826482b",
  fs: "overlay",
  totalBytes: 438 * 1024 ** 3,
  removable: false,
};

/** Pose le cas du premier smoke dans le pont simulé : un volume Docker au chemin très long et une sonde au nom très long. */
async function longNames(page: Page) {
  await page.waitForFunction(() =>
    Boolean((window as unknown as { __hearthSim?: object }).__hearthSim),
  );
  await page.evaluate((overlay) => {
    const sim = (
      window as unknown as {
        __hearthSim: {
          machine: {
            machineOf(id: string): { disks: unknown[] };
            setMachine(id: string, machine: unknown): void;
            setLongSensor(id: string, on: boolean): void;
          };
        };
      }
    ).__hearthSim.machine;
    const machine = sim.machineOf("forge");
    sim.setMachine("forge", { ...machine, disks: [...machine.disks, overlay] });
    sim.setLongSensor("forge", true);
  }, OVERLAY);
}

async function shoot(page: Page, name: string, size: { width: number; height: number }) {
  await page.screenshot({ path: `e2e/screenshots/${name}-${size.width}x${size.height}.png` });
}

for (const size of SIZES) {
  test(`tableau de bord à ${size.width}×${size.height} : un chemin et un nom de sonde longs restent dans leur carte`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await longNames(page);
    await expect(page.locator(".disk").nth(2)).toBeVisible();
    const m = await page.evaluate(() => {
      const outside: string[] = [];
      for (const card of Array.from(document.querySelectorAll("section.card"))) {
        const cardBox = card.getBoundingClientRect();
        for (const element of Array.from(card.querySelectorAll("*"))) {
          const box = element.getBoundingClientRect();
          if (box.width === 0 || box.height === 0) continue;
          if (box.right > cardBox.right + 1 || box.left < cardBox.left - 1) {
            outside.push(
              `${card.querySelector("h2")?.textContent} : ${element.className} ${Math.round(box.right - cardBox.right)}`,
            );
          }
        }
      }
      // Aucune valeur en colonne (une lettre par ligne) : chaque valeur d'une ligne « libellé : valeur » tient sur une ligne.
      const stacked = Array.from(
        document.querySelectorAll(".stat:not(.stat--stacked) .stat__value"),
      ).filter((el) => el.getBoundingClientRect().height > 30).length;
      const names = Array.from(document.querySelectorAll(".disk__head")).map((head) => {
        const parts = Array.from(head.children).map((child) => child.getBoundingClientRect());
        return parts.some((a, i) =>
          parts.some(
            (b, j) =>
              j > i && a.right > b.left + 1 && a.left < b.right - 1 && a.width > 0 && b.width > 0,
          ),
        );
      });
      const name = document.querySelectorAll(".disk__name")[2]?.getBoundingClientRect();
      const percent = document.querySelectorAll(".disk__percent")[2]?.getBoundingClientRect();
      return {
        outside,
        stacked,
        overlapping: names.filter(Boolean).length,
        nameHeight: name?.height ?? 999,
        percentHeight: percent?.height ?? 999,
      };
    });
    expect(m.outside, "éléments hors de leur carte").toEqual([]);
    expect(m.stacked, "valeurs écrites en colonne").toBe(0);
    expect(m.overlapping, "éléments qui se recouvrent dans une ligne de disque").toBe(0);
    expect(m.nameHeight, "nom du périphérique sur une ligne").toBeLessThanOrEqual(24);
    expect(m.percentHeight, "pourcentage sur une ligne").toBeLessThanOrEqual(24);
    await shoot(page, "smoke-disques-longs", size);
  });
}

test("le chemin long est tronqué au milieu, complet au clavier", async ({ page }) => {
  await page.setViewportSize(SIZES[2]);
  await page.goto("/?nodev#/servers/forge/dashboard");
  await longNames(page);
  const mid = page.locator(".disk").nth(2).locator(".disk__mount [data-middle]");
  await expect(mid).toBeVisible();
  const full =
    "/var/lib/docker/rootfs/overlayfs/516ea538beeb5e0443f9fa80d43d3f8c4c7b5fb99d71715882d9cb491826482b";
  // Début ET fin visibles : le texte affiché commence par le début et finit par la fin du chemin.
  const shown = await mid.innerText();
  expect(shown.startsWith("/var/lib")).toBe(true);
  expect(shown.endsWith("1826482b")).toBe(true);
  expect(await mid.getAttribute("aria-label")).toBe(full);
  await mid.focus();
  await expect(page.getByRole("tooltip").filter({ hasText: full })).toBeVisible();
  // La carte Machine l'écrit aussi sans déborder.
  await expect(page.locator(".machine [data-middle]").first()).toBeVisible();
});

// S2 : l'anneau du serveur sélectionné est entier (pas rogné par la zone qui défile de la barre des serveurs).
for (const size of SIZES) {
  test(`barre des serveurs à ${size.width}×${size.height} : l'anneau du serveur sélectionné est entier`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    const avatar = page.locator(".rail .avatar--active").first();
    await expect(avatar).toBeVisible();
    const m = await avatar.evaluate((el) => {
      const ringWidth =
        Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue("--ring-width"),
        ) || 2;
      const box = el.getBoundingClientRect();
      const list = (el.closest(".rail__list") as HTMLElement).getBoundingClientRect();
      const rail = (el.closest(".rail") as HTMLElement).getBoundingClientRect();
      const grow = ringWidth * 2;
      return {
        topGap: box.top - grow - list.top,
        bottomGap: list.bottom - (box.bottom + grow),
        leftGap: box.left - grow - rail.left,
        rightGap: rail.right - (box.right + grow),
      };
    });
    expect(m.topGap, "haut de l'anneau").toBeGreaterThanOrEqual(0);
    expect(m.bottomGap, "bas de l'anneau").toBeGreaterThanOrEqual(0);
    expect(m.leftGap, "gauche de l'anneau").toBeGreaterThanOrEqual(0);
    expect(m.rightGap, "droite de l'anneau").toBeGreaterThanOrEqual(0);
    await page.screenshot({
      path: `e2e/screenshots/smoke-anneau-${size.width}x${size.height}.png`,
      clip: { x: 0, y: 0, width: 80, height: 220 },
    });
  });
}

// S3 : les chiffres des étapes sont centrés dans leurs ronds (mesure du glyphe, pas de la boîte).
for (const size of SIZES) {
  test(`ajout d'un serveur à ${size.width}×${size.height} : chiffres des étapes centrés dans leurs ronds`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.locator("[data-rail-add]").click();
    await expect(page.locator(".trail__puck").first()).toBeVisible();
    const offsets = await page.locator(".trail__puck").evaluateAll((pucks) => {
      const canvas = document.createElement("canvas").getContext("2d");
      return pucks.flatMap((puck) => {
        const node = Array.from(puck.childNodes).find(
          (child) => child.nodeType === 3 && (child.textContent ?? "").trim() !== "",
        );
        if (!node || !canvas) return [];
        const style = getComputedStyle(puck);
        canvas.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
        const digit = (node.textContent ?? "").trim();
        const metrics = canvas.measureText(digit);
        const range = document.createRange();
        range.selectNodeContents(node);
        const text = range.getBoundingClientRect();
        const baseline = text.top + metrics.fontBoundingBoxAscent;
        const glyphCenterY =
          baseline - (metrics.actualBoundingBoxAscent - metrics.actualBoundingBoxDescent) / 2;
        const glyphCenterX =
          text.left +
          text.width / 2 +
          (metrics.actualBoundingBoxRight - metrics.actualBoundingBoxLeft - metrics.width) / 2;
        const box = puck.getBoundingClientRect();
        return [
          {
            dy: glyphCenterY - (box.top + box.height / 2),
            dx: glyphCenterX - (box.left + box.width / 2),
          },
        ];
      });
    });
    expect(offsets.length).toBeGreaterThan(0);
    for (const offset of offsets) {
      expect(Math.abs(offset.dy), "écart vertical du chiffre").toBeLessThanOrEqual(0.5);
      expect(Math.abs(offset.dx), "écart horizontal du chiffre").toBeLessThanOrEqual(0.5);
    }
    await page.screenshot({ path: `e2e/screenshots/smoke-ajout-${size.width}x${size.height}.png` });
  });
}

// S3, mesuré sur la capture réelle de Voiky (WebView2) : la pastille de la couleur sélectionnée sortait de 4 px à gauche
// de la colonne des champs (anneau de sélection) ; libellés d'étape, champs et boutons à 1 px près.
for (const size of SIZES) {
  test(`ajout d'un serveur à ${size.width}×${size.height} : alignements de la fenêtre (étapes, champs, couleurs, boutons)`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.locator("[data-rail-add]").click();
    await expect(page.locator(".trail__puck").first()).toBeVisible();
    const m = await page.evaluate(() => {
      const left = (el: Element | null) => el?.getBoundingClientRect().left ?? 0;
      const right = (el: Element | null) => el?.getBoundingClientRect().right ?? 0;
      const inputs = Array.from(
        document.querySelectorAll("form input[type=text], form input:not([type])"),
      );
      const ringWidth =
        Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue("--ring-width"),
        ) || 2;
      const swatches = Array.from(document.querySelectorAll(".swatch"));
      const first = swatches[0];
      const on = document.querySelector(".swatch--on");
      const ring = on ? ringWidth * 2 : 0;
      const buttons = Array.from(document.querySelectorAll("form button")).filter((b) =>
        b.textContent?.trim(),
      );
      const puck = document.querySelector(".trail__puck")?.getBoundingClientRect();
      const name = document.querySelector(".trail__name");
      const nameRange = name ? document.createRange() : null;
      nameRange?.selectNodeContents(name as Node);
      const text = nameRange?.getBoundingClientRect();
      return {
        inputLeft: left(inputs[0]?.closest(".field__box") ?? null),
        inputRight: right(inputs[0]?.closest(".field__box") ?? null),
        ringLeft: left(first ?? null) - (first === on ? ring : 0),
        buttonsRight: right(buttons.at(-1) ?? null),
        puckCenter: puck ? puck.top + puck.height / 2 : 0,
        textCenter: text ? text.top + text.height / 2 : 0,
      };
    });
    expect(
      m.ringLeft,
      "la pastille sélectionnée et son anneau ne sortent pas de la colonne des champs",
    ).toBeGreaterThanOrEqual(m.inputLeft - 0.5);
    expect(
      Math.abs(m.buttonsRight - m.inputRight),
      "boutons alignés sur le bord droit des champs",
    ).toBeLessThanOrEqual(1);
    expect(
      Math.abs(m.textCenter - m.puckCenter),
      "libellé d'étape centré sur son rond",
    ).toBeLessThanOrEqual(1);
  });
}

// S3, précision de Voiky : « les éléments ne semblaient pas tous alignés pareil à gauche ». UN seul axe gauche pour tout le
// contenu de la fenêtre d'ajout, anneaux et halos compris : rond de l'étape 1, titre, libellés, bord VISIBLE des champs
// (champ au focus), première pastille de couleur (sélectionnée, avec son anneau).
for (const size of SIZES) {
  test(`ajout d'un serveur à ${size.width}×${size.height} : un seul axe gauche (anneaux et halos compris)`, async ({
    page,
  }) => {
    await page.setViewportSize(size);
    await page.goto("/?nodev#/servers/forge/dashboard");
    await page.locator("[data-rail-add]").click();
    await expect(page.locator(".trail__puck").first()).toBeVisible();
    await page.locator("form input").first().focus();
    // Les transitions du halo de focus sont finies avant de mesurer.
    await page.evaluate(() =>
      Promise.all(document.getAnimations().map((animation) => animation.finished)),
    );
    const edges = await page.evaluate(() => {
      const spread = (el: Element) => {
        const shadow = getComputedStyle(el).boxShadow;
        if (!shadow || shadow === "none" || shadow.includes("inset")) return 0;
        return Math.max(
          0,
          ...Array.from(shadow.matchAll(/(\d+(?:\.\d+)?)px(?=\s*(?:,|$))/g)).map((m) =>
            Number(m[1]),
          ),
        );
      };
      const visibleLeft = (el: Element | null) =>
        el ? el.getBoundingClientRect().left - spread(el) : Number.NaN;
      const ringWidth =
        Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue("--ring-width"),
        ) || 2;
      const swatch = document.querySelector(".swatch--on") ?? document.querySelector(".swatch");
      return {
        puck: visibleLeft(document.querySelector(".trail__puck")),
        title: visibleLeft(document.querySelector("h1")),
        label: visibleLeft(document.querySelector("form label")),
        field: visibleLeft(document.querySelector("form .field__box")),
        swatch: swatch ? swatch.getBoundingClientRect().left - ringWidth * 2 : Number.NaN,
      };
    });
    const values = Object.values(edges);
    const spreadOfEdges = Math.max(...values) - Math.min(...values);
    expect(spreadOfEdges, `bords gauches visibles : ${JSON.stringify(edges)}`).toBeLessThanOrEqual(
      1,
    );
  });
}

// Même vérification sur les fenêtres de formulaire : création de compte et changement de son mot de passe.
for (const [name, open] of [
  [
    "création de compte",
    async (page: Page) => page.getByRole("button", { name: "Ajouter un compte" }).click(),
  ],
  [
    "changement de mot de passe",
    async (page: Page) =>
      page.getByRole("button", { name: "Changer mon mot de passe" }).first().click(),
  ],
] as const) {
  test(`fenêtre « ${name} » : titre, libellés et champs sur le même axe gauche`, async ({
    page,
  }) => {
    await page.setViewportSize(SIZES[2]);
    await page.goto("/?nodev#/servers/forge/accounts");
    await expect(page.locator(".table-wrap")).toBeVisible();
    await open(page);
    const dialog = page.locator("dialog[open]");
    await expect(dialog).toBeVisible();
    await dialog.locator("input").first().focus();
    await page.evaluate(() =>
      Promise.all(document.getAnimations().map((animation) => animation.finished)),
    );
    const edges = await dialog.evaluate((el) => {
      const left = (node: Element | null) => node?.getBoundingClientRect().left ?? Number.NaN;
      return {
        title: left(el.querySelector("h1, h2")),
        label: left(el.querySelector("label")),
        field: left(el.querySelector(".field__box")),
      };
    });
    const values = Object.values(edges);
    expect(Math.max(...values) - Math.min(...values), JSON.stringify(edges)).toBeLessThanOrEqual(1);
  });
}
