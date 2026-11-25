import { expect, type Page, test } from "@playwright/test";

// Revue Nora HRT-261 à 264, 294 (seconde passe) : la mise en page de l'onglet Mods et de l'onglet Sauvegardes à 1024, 1100 et
// 1280 px de large (mesures de largeur et de position), et la barre du lot qui reste à portée en défilant.
type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  games: { register(serverId: string, key: string, view: Record<string, unknown>): void };
  catalog: { addRich(): { mods?: { ways: string[] } }; put(detail: unknown): void };
  mods: {
    seed(serverId: string, key: string, view: Record<string, unknown>): void;
    seedBackupMods(serverId: string, key: string, id: string, mods: Record<string, unknown>): void;
  };
  backups: { seed(serverId: string, key: string, backups: Array<Record<string, unknown>>): void };
};

async function open(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await page.evaluate(() => {
    const s = (window as unknown as { __hearthSim: Sim }).__hearthSim;
    s.services.seed("forge", [
      {
        key: "h_mc",
        name: "Prominence",
        origin: "hearth",
        state: "running",
        mounts: [
          { kind: "folder", source: "/srv/hearth/data/mc", target: "/data", readOnly: false },
        ],
      },
    ]);
    s.games.register("forge", "h_mc", { name: "Minecraft", fiche: "essai" });
    const fiche = s.catalog.addRich();
    fiche.mods?.ways.push("drop_jars");
    s.catalog.put(fiche);
    const identity = (id: string, name: string, version: string) => ({
      id,
      name,
      version,
      loader: "fabric",
      minecraftDeclared: true,
      environment: "both",
    });
    const ok = { code: "compatible", missing: [], neededBy: [] };
    const mod = (file: string, id: string, name: string, version: string, extra = {}) => ({
      file,
      rank: 0,
      place: "active",
      enabled: true,
      origin: "server_pack",
      readableName: true,
      identity: identity(id, name, version),
      verdict: ok,
      ...extra,
    });
    s.mods.seed("forge", "h_mc", {
      mods: [
        mod("lithium-fabric-mc1.20.1-0.11.2.jar", "lithium", "Lithium", "0.11.2"),
        mod("fabric-api-0.92.2+1.20.1.jar", "fabric-api", "Fabric API", "0.92.2"),
        mod(
          "simple-voice-chat-fabric-1.20.1-2.5.26.jar",
          "voicechat",
          "Simple Voice Chat",
          "2.5.26",
        ),
        mod("sodium-fabric-0.5.3+mc1.20.1.jar.disabled", "sodium", "Sodium", "0.5.3", {
          enabled: false,
        }),
        mod("create-fabric-0.5.1-f-build.1417+mc1.20.1.jar", "create", "Create", "0.5.1"),
        mod("spark-1.10.53-fabric.jar", "spark", "Spark", "1.10.53"),
        mod("ferritecore-6.0.1-fabric.jar", "ferritecore", "FerriteCore", "6.0.1"),
        mod("carpet-1.4.112.jar", "carpet", "Carpet", "1.4.112"),
      ],
    });
    s.backups.seed("forge", "h_mc", [
      {
        id: "01SIMBKAAAAAAA",
        createdAt: "2026-11-17T20:00:00Z",
        mods: { state: "recorded", active: 1, disabled: 0, retired: 1 },
      },
    ]);
    s.mods.seedBackupMods("forge", "h_mc", "01SIMBKAAAAAAA", {
      state: "recorded",
      recordedAt: "2026-11-17T20:00:00Z",
      game: { loader: "fabric", minecraft: "1.20.1" },
      counts: { active: 1, disabled: 0, retired: 1, notListed: 0 },
      mods: [
        {
          file: "lithium-fabric-mc1.20.1-0.11.1.jar",
          rank: 0,
          place: "active",
          origin: "server_pack",
          identity: identity("lithium", "Lithium", "0.11.1"),
        },
        {
          file: "iris-1.6.4.jar",
          rank: 1,
          place: "retired",
          origin: "hearth",
          identity: identity("iris", "Iris", "1.6.4"),
        },
      ],
      compare: {
        complete: true,
        unread: 0,
        same: false,
        changes: [
          {
            kind: "version_changed",
            id: "lithium",
            name: "Lithium",
            file: null,
            backupVersion: "0.11.1",
            currentVersion: "0.11.2",
          },
        ],
      },
    });
    window.location.hash = "#/servers/forge/services";
  });
  await page.locator('[data-row][data-key="h_mc"]').click();
}

for (const [width, height] of [
  [1024, 768],
  [1100, 680],
  [1280, 800],
] as const) {
  test(`${width} px : le tableau des mods remplit sa carte, noms et fichiers entiers`, async ({
    page,
  }) => {
    await open(page, width, height);
    await page.locator("[data-tab='mods']").click();
    const table = page.locator("[data-mods-table]");
    await expect(table).toBeVisible();
    const [card, tableBox] = await Promise.all([
      table.locator("xpath=..").boundingBox(),
      table.boundingBox(),
    ]);
    expect(Math.abs((tableBox?.width ?? 0) - (card?.width ?? 1))).toBeLessThan(2);
    // Aucun nom ni fichier n'est coupé par des points de suspension ni caché par un débordement.
    const clipped = await page.evaluate(
      () =>
        [...document.querySelectorAll("[data-mod-row] strong, [data-mod-row] .row__file")].filter(
          (el) => el.scrollWidth > el.clientWidth + 1,
        ).length,
    );
    expect(clipped).toBe(0);
    const name = await page.locator("[data-mod-row] strong").first().boundingBox();
    expect(name?.width ?? 0).toBeGreaterThan(60);
    const overflow = await page.evaluate(() => {
      const el = document.querySelector("[data-mods-table]")?.parentElement;
      return el ? el.scrollWidth - el.clientWidth : -1;
    });
    expect(overflow).toBeLessThanOrEqual(0);
  });

  test(`${width} px : Sauvegardes sans défilement horizontal, Vérifier et Supprimer visibles, comparaison contenue`, async ({
    page,
  }) => {
    await open(page, width, height);
    await page.locator("[data-tab='backups']").click();
    const row = page.locator("[data-backup='01SIMBKAAAAAAA']");
    await row.locator("[data-backup-toggle]").click();
    await page.locator("[data-backup-mods-compare]").click();
    await expect(page.locator("[data-backup-compare]")).toBeVisible();
    const box = await page.locator("[data-backup-table]").boundingBox();
    const right = (box?.x ?? 0) + (box?.width ?? 0);
    for (const selector of ["[data-backup-verify]", "[data-backup-remove]"]) {
      const b = await row.locator(selector).boundingBox();
      expect((b?.x ?? 0) + (b?.width ?? 0)).toBeLessThanOrEqual(right + 1);
    }
    const compare = await page.locator("[data-backup-compare]").boundingBox();
    expect((compare?.x ?? 0) + (compare?.width ?? 0)).toBeLessThanOrEqual(right + 1);
    const overflow = await page.evaluate(() => {
      const el = document.querySelector("[data-backup-table]");
      return el ? el.scrollWidth - el.clientWidth : -1;
    });
    expect(overflow).toBeLessThanOrEqual(0);
    await expect(page.locator("[data-backup-mods-retired]").first()).toContainText(
      "Mods retirés : 1",
    );
    await expect(page.locator("[data-backup-mods-hide]")).toHaveCount(0);
  });

  test(`${width} px : la barre du lot reste à portée en défilant`, async ({ page }) => {
    await open(page, width, height);
    await page.locator("[data-tab='mods']").click();
    await page.locator("[data-mod-row^='spark'] [data-mod-retire]").click();
    await expect(page.locator("[data-mods-pending-bar]")).toBeVisible();
    for (const place of ["top", "bottom"] as const) {
      await page.evaluate((where) => {
        const detail = document.querySelector<HTMLElement>("[data-service-detail]");
        if (detail) detail.scrollTop = where === "top" ? 0 : detail.scrollHeight;
      }, place);
      await page.waitForTimeout(150);
      const inside = await page.evaluate(() => {
        const bar = document.querySelector("[data-mods-pending-bar]")?.getBoundingClientRect();
        const pinned = document.querySelector("[data-detail-pinned]")?.getBoundingClientRect();
        return bar && pinned ? bar.top >= pinned.bottom - 1 && bar.bottom <= innerHeight : false;
      });
      expect(inside).toBe(true);
    }
  });
}
