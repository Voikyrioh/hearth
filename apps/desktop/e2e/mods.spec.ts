import { expect, type Page, test } from "@playwright/test";

// HRT-263, HRT-294 : les mods d'un serveur Minecraft, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison,
// coquille est prouvée côté Rust). L'agent simulé juge comme le vrai ; la suite de l'opération est pilotée par le test.
// Serveur « forge » : administrateur avec le droit de gérer les services.
const PASSWORD = "Correct-Horse-9";

type Identity = {
  id: string;
  name: string;
  version: string;
  loader: string;
  minecraftDeclared: boolean;
  environment: string;
};
type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  games: { register(serverId: string, key: string, view: Record<string, unknown>): void };
  catalog: {
    addRich(): { mods?: { ways: string[] } };
    put(detail: unknown): void;
  };
  mods: {
    seed(serverId: string, key: string, view: Record<string, unknown>): void;
    willRead(file: string, deposit: { identity: Identity }): void;
    lastOperationId(serverId: string): string | undefined;
    toStep(serverId: string, id: string, step: string): void;
    succeed(serverId: string, id: string): void;
    rollBack(serverId: string, id: string, tail: Record<string, unknown>): void;
  };
  transfers: { pickedMany: string[]; list(): Array<{ id: string }>; complete(id: string): void };
};

const sim = <T>(page: Page, run: (sim: Sim, arg: unknown) => T, arg?: unknown) =>
  page.evaluate(
    ([source, value]) => {
      const bridge = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      return new Function("sim", "arg", `return (${source as string})(sim, arg)`)(bridge, value);
    },
    [run.toString(), arg] as const,
  );

async function open(page: Page) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) => {
    s.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth" }]);
    s.games.register("forge", "h_mc", { name: "Minecraft", fiche: "essai" });
    const fiche = s.catalog.addRich();
    fiche.mods?.ways.push("drop_jars");
    s.catalog.put(fiche);
    const lithium = {
      id: "lithium",
      name: "Lithium",
      version: "0.11.2",
      loader: "fabric",
      minecraftDeclared: true,
      environment: "both",
    };
    s.mods.seed("forge", "h_mc", {
      mods: [
        {
          file: "lithium-0.11.2.jar",
          rank: 0,
          place: "active",
          enabled: true,
          origin: "server_pack",
          readableName: true,
          identity: lithium,
          verdict: { code: "compatible", missing: [], neededBy: [] },
        },
      ],
    });
    s.mods.willRead("chunky-fabric-1.3.146.jar", {
      identity: { ...lithium, id: "chunky", name: "Chunky", version: "1.3.146" },
    });
  });
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await page.locator('[data-row][data-key="h_mc"]').click();
  await page.locator("[data-tab='mods']").click();
  await expect(page.locator("[data-mods-table]")).toBeVisible();
}

async function confirm(page: Page, label: string) {
  const dialog = page.locator("dialog[open]");
  const field = dialog.locator("[data-reauth-field] input");
  if (await field.isVisible().catch(() => false)) await field.fill(PASSWORD);
  await dialog.getByRole("button", { name: label, exact: true }).click();
}

test("liste, retrait préparé, vérification, application réussie, fin et liste mise à jour", async ({
  page,
}) => {
  await open(page);
  await expect(page.locator("[data-mods-heading]")).toHaveText("1 mod installé, 1 activé");
  const row = page.locator("[data-mod-row='lithium-0.11.2.jar']");
  await row.locator("[data-mod-retire]").click();
  await expect(row.locator("[data-mod-pending-text]")).toHaveText(
    "Sera mis de côté au prochain redémarrage",
  );
  await page.locator("[data-mods-review]").click();
  await expect(page.locator("[data-mods-apply] h3")).toHaveText("Vérifiez avant d'appliquer");
  await page.locator("[data-reauth-field] input").fill(PASSWORD);
  await page.locator("[data-mods-apply-submit]").click();
  await expect(page.locator("[data-mods-progress]")).toBeVisible();
  const id = (await sim(page, (s) => s.mods.lastOperationId("forge"))) as string;
  await sim(page, (s, arg) => s.mods.toStep("forge", arg as string, "copy"), id);
  await expect(page.locator("[data-mods-progress-cancel]")).toBeVisible();
  await sim(page, (s, arg) => s.mods.toStep("forge", arg as string, "place"), id);
  await expect(page.locator("[data-mods-progress-cancel]")).toHaveCount(0);
  await sim(page, (s, arg) => s.mods.succeed("forge", arg as string), id);
  await expect(page.locator("[data-mods-end='succeeded']")).toBeVisible();
  await page.locator("[data-mods-end-close]").click();
  await expect(page.locator("[data-mod-row='lithium-0.11.2.jar'] [data-mod-state]")).toHaveText(
    "Retiré",
  );
});

test("l'échec revient à l'état d'avant : aucune décision à prendre", async ({ page }) => {
  await open(page);
  await page.locator("[data-mod-row='lithium-0.11.2.jar'] [data-mod-switch]").click();
  await page.locator("[data-mods-review]").click();
  await page.locator("[data-reauth-field] input").fill(PASSWORD);
  await page.locator("[data-mods-apply-submit]").click();
  await expect(page.locator("[data-mods-progress]")).toBeVisible();
  const id = (await sim(page, (s) => s.mods.lastOperationId("forge"))) as string;
  await sim(
    page,
    (s, arg) =>
      s.mods.rollBack("forge", arg as string, {
        lines: ["Mod waystones requires balm 7.x or later, which is missing"],
        reason: "missing_dependency",
        sentence: "Le mod Waystones a besoin du mod Balm, qui n'est pas installé.",
      }),
    id,
  );
  await expect(page.locator("[data-mods-fail-lead]")).toContainText(
    "Hearth a remis les mods d'avant",
  );
  await expect(page.locator("[data-mods-fail-tail]")).toContainText("requires balm");
  await expect(page.getByRole("button", { name: "Laisser tel quel" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Revenir à l'état d'avant" })).toHaveCount(0);
});

test("un lot de dépôt : choisir, envoyer, contrôler, continuer", async ({ page }) => {
  await open(page);
  await sim(page, (s) => {
    s.transfers.pickedMany = ["C:\\dl\\chunky-fabric-1.3.146.jar"];
  });
  await page.locator("[data-mods-add]").click();
  await page.locator("[data-mods-deposit-choose]").click();
  await confirm(page, "Envoyer");
  await expect(page.locator("[data-mods-deposit-status]")).toBeVisible();
  await sim(page, (s) => {
    for (const transfer of s.transfers.list()) s.transfers.complete(transfer.id);
  });
  await expect(page.locator("[data-mods-deposit-check]")).toBeEnabled({ timeout: 10_000 });
  await page.locator("[data-mods-deposit-check]").click();
  await expect(page.locator("[data-mods-check-title]")).toHaveText(
    "Contrôle terminé : 1 fichier prêt",
  );
  await page.locator("[data-mods-check-continue]").click();
  await expect(page.locator("[data-mods-pending-bar]")).toContainText("1 changement prêt");
});
