import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-86 : supprimer un service depuis le client, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison,
// coquille est prouvée côté Rust). L'agent simulé juge comme le vrai. Serveur « forge » : administrateur avec le droit
// de gérer les services. Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  deletes: {
    running(serverId: string): Array<{ operationId: string; key: string; name: string }>;
    toStep(serverId: string, opId: string, step: string): void;
    erasing(serverId: string, opId: string, items: number): void;
    succeed(serverId: string, opId: string, cause?: string | null): void;
    fail(serverId: string, opId: string, cause: string): void;
    setPreview(serverId: string, key: string, preview: Record<string, unknown>): void;
    received: Array<{ request: Record<string, unknown> }>;
  };
};

const sim = <T>(page: Page, run: (sim: Sim) => T, arg?: unknown) =>
  page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      return new Function("sim", "arg", `return (${source as string})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );

async function shoot(target: Page | Locator, page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: HEIGHTS[width] });
    await target.screenshot({ path: `${SHOTS}/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

async function open(page: Page) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) =>
    s.services.seed("forge", [
      {
        key: "h_terraria",
        name: "terraria",
        origin: "hearth",
        graceS: 120,
        mounts: [
          { kind: "folder", source: "/srv/hearth/data/terraria", target: "/data", readOnly: false },
        ],
      },
      {
        key: "h_robot",
        name: "robot-discord",
        origin: "hearth",
        state: "stopped",
        mounts: [
          {
            kind: "folder",
            source: "/srv/hearth/data/robot-discord",
            target: "/data",
            readOnly: false,
          },
        ],
      },
      {
        key: "c_vieux",
        name: "ancien-service",
        origin: "external",
        adopted: { by: "marie", at: Date.parse("2026-10-13T08:00:00Z") },
      },
    ]),
  );
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await expect(page.locator("[data-row]").first()).toBeVisible();
}

const row = (page: Page, key: string) => page.locator(`[data-row][data-key="${key}"]`);

async function openDialog(page: Page, key = "h_terraria") {
  await row(page, key).click();
  await page.locator("[data-delete-open]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Supprimer");
  await expect(dialog.locator("[data-delete-folders]")).toBeVisible();
  return dialog;
}

test("la carte « Suppression » du détail, hors du menu de la ligne ; un service adopté n'a qu'une phrase", async ({
  page,
}) => {
  await open(page);
  await row(page, "h_terraria").locator("[data-row-menu-button]").click();
  await expect(page.locator("[data-row-menu-item]")).toHaveText(["Arrêter", "Redémarrer"]);
  await page.keyboard.press("Escape");
  await row(page, "h_terraria").click();
  await expect(page.locator("[data-detail-delete]")).toContainText("Supprimer ce service");
  await shoot(page.locator("[data-service-detail]"), page, "delete-carte-detail");
  await row(page, "c_vieux").click();
  await expect(page.locator("[data-delete-open]")).toHaveCount(0);
  await expect(page.locator("[data-delete-adopted]")).toContainText(
    "Hearth ne supprime pas les services créés hors de Hearth.",
  );
  await shoot(page.locator("[data-service-detail]"), page, "delete-carte-adopte");
});

test("la fenêtre : dossiers et sort, cases décochées, nom retapé, mot de passe toujours demandé", async ({
  page,
}) => {
  await open(page);
  const dialog = await openDialog(page);
  await expect(dialog).toContainText("Supprimer terraria ?");
  await expect(dialog).toContainText(
    "Sa configuration (variables, ports, commande) sera effacée avec lui.",
  );
  await expect(dialog).toContainText("Le service est en marche : il sera arrêté d'abord.");
  await expect(dialog.locator("tbody tr").first()).toContainText("Conservé");
  await shoot(dialog, page, "delete-fenetre");
  await dialog.getByLabel("Supprimer aussi les données").check();
  await expect(dialog.locator("tbody tr").first()).toContainText("Effacé");
  await expect(dialog).toContainText("il n'y a pas de corbeille");
  const name = dialog.getByLabel("Retapez le nom du service pour confirmer");
  await name.fill("Terraria");
  await expect(dialog).toContainText("Le nom saisi ne correspond pas.");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await expect(dialog.getByRole("button", { name: "Supprimer le service" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await shoot(dialog, page, "delete-fenetre-donnees");
  await name.fill("terraria");
  await expect(dialog.getByRole("button", { name: "Supprimer le service" })).not.toHaveAttribute(
    "aria-disabled",
    "true",
  );
});

test("la mesure interrompue, un dossier partagé et un dossier du serveur", async ({ page }) => {
  await open(page);
  await sim(page, (s) =>
    s.deletes.setPreview("forge", "h_terraria", {
      measuredInTime: false,
      serverFolders: ["/mnt/jeux/terraria"],
      volumes: 1,
      folders: [
        {
          path: "/srv/hearth/services/terraria/mondes",
          bytes: 6 * 1024 ** 3,
          items: 1200000,
          partial: true,
          unmeasured: false,
          fate: "erasable",
          sharedWith: [],
          sharedWithUnmanaged: false,
        },
        {
          path: "/srv/hearth/services/terraria/partage",
          bytes: 4096,
          items: 12,
          partial: false,
          unmeasured: false,
          fate: "kept_shared",
          sharedWith: ["robot-discord"],
          sharedWithUnmanaged: true,
        },
      ],
    }),
  );
  const dialog = await openDialog(page);
  await expect(dialog).toContainText("plus de 6 Go (mesure interrompue)");
  await expect(dialog).toContainText(
    "Taille non mesurée pour /srv/hearth/services/terraria/mondes : plus grande que 6 Go.",
  );
  await expect(dialog).toContainText("Conservé (partagé)");
  await expect(dialog).toContainText("Conservé (appartient au serveur)");
  await shoot(dialog, page, "delete-fenetre-mesure");
});

test("sans case cochée : suivi, point de non-retour, issue, le service quitte la liste", async ({
  page,
}) => {
  await open(page);
  const dialog = await openDialog(page);
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Supprimer le service" }).click();
  const progress = page.locator("[data-delete-progress]");
  await expect(progress).toContainText("Supprimer terraria");
  await expect(progress.locator("[data-step='stop'][data-state='now']")).toBeVisible();
  const [op] = await sim(page, (s) => s.deletes.running("forge"));
  await shoot(progress, page, "delete-suivi-arret");
  await sim(page, (s) =>
    s.deletes.toStep("forge", s.deletes.running("forge")[0]?.operationId ?? "", "remove_container"),
  );
  await expect(progress.locator("[data-delete-too-late]")).toBeVisible();
  await expect(progress.locator("[data-delete-cancel]")).toHaveAttribute("aria-disabled", "true");
  await sim(page, (s) =>
    s.deletes.succeed("forge", s.deletes.running("forge")[0]?.operationId ?? ""),
  );
  await expect(progress.locator("[data-delete-success]")).toContainText(
    "Le service terraria est supprimé. Ses données sont conservées dans",
  );
  await shoot(progress, page, "delete-issue-donnees-gardees");
  await progress.locator("[data-delete-close]").click();
  await expect(row(page, "h_terraria")).toHaveCount(0);
  expect(op?.name).toBe("terraria");
});

test("avec les données : l'effacement, ses éléments, l'issue et un effacement incomplet", async ({
  page,
}) => {
  await open(page);
  const dialog = await openDialog(page, "h_robot");
  await dialog.getByLabel("Supprimer aussi les données").check();
  await dialog.getByLabel("Retapez le nom du service pour confirmer").fill("robot-discord");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Supprimer le service" }).click();
  const progress = page.locator("[data-delete-progress]");
  await sim(page, (s) => {
    const id = s.deletes.running("forge")[0]?.operationId ?? "";
    s.deletes.erasing("forge", id, 48210);
  });
  await expect(progress).toContainText("éléments effacés");
  await shoot(progress, page, "delete-suivi-effacement");
  await sim(page, (s) =>
    s.deletes.succeed("forge", s.deletes.running("forge")[0]?.operationId ?? "", "data_not_erased"),
  );
  await expect(progress.locator("[data-delete-partial]")).toContainText(
    "certaines données n'ont pas pu être effacées",
  );
  await shoot(progress, page, "delete-issue-incomplete");
});

test("l'échec avant le point de non-retour laisse le service entier", async ({ page }) => {
  await open(page);
  const dialog = await openDialog(page, "h_robot");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Supprimer le service" }).click();
  const progress = page.locator("[data-delete-progress]");
  await sim(page, (s) =>
    s.deletes.fail("forge", s.deletes.running("forge")[0]?.operationId ?? "", "engine_refused"),
  );
  await expect(progress.locator("[data-delete-not-deleted]")).toContainText(
    "Le service robot-discord n'a pas été supprimé.",
  );
  await shoot(progress, page, "delete-issue-echec");
  await progress.locator("[data-delete-close]").click();
  await expect(row(page, "h_robot")).toHaveCount(1);
});
