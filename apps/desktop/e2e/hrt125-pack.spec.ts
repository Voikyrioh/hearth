import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-125 : installer un serveur avec un pack de mods, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison,
// coquille est prouvée côté Rust). La carte « Mods » : choisir le fichier, l'envoyer par un transfert reprenable (fenêtre
// de confirmation), voir où en est l'envoi, puis ce que l'agent a LU dans le pack. Serveur « forge » : administrateur avec
// le droit de gérer les services. Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut `e2e/screenshots/`, non
// commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;
const PASSWORD = "Correct-Horse-9";

type Sim = {
  catalog: { addRich(): unknown };
  packs: { willRead(check: Record<string, unknown>): void };
  transfers: {
    list(): Array<{ id: string; sent: number; status: string }>;
    advance(id: string, bytes: number): void;
    complete(id: string): void;
  };
};

const sim = <T>(page: Page, run: (sim: Sim, arg: unknown) => T, arg?: unknown) =>
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

test("un pack de mods : choisir, envoyer avec confirmation, suivre l'envoi, lire ce que l'agent a vu", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(page, (s) => {
    s.catalog.addRich();
    s.packs.willRead({
      kind: "read",
      pack: {
        kind: "server_pack",
        sizeBytes: 794_497_024,
        files: 2314,
        mods: 652,
        unpackedBytes: 1_650_000_000,
        topDirs: ["config", "mods"],
        modrinth: null,
      },
    });
  });
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/catalogue/essai/installer";
  });

  const card = page.locator("[data-install-pack]");
  await expect(card).toBeVisible();
  // La case n'est jamais cochée d'avance, et rien n'est choisi tant qu'elle ne l'est pas.
  await expect(card.locator("input[type='checkbox']")).not.toBeChecked();
  await expect(page.locator("[data-pack-choose]")).toHaveCount(0);
  await card.locator("input[type='checkbox']").check();
  await shoot(page, page, "pack-carte");

  // Choisir : la boîte « Ouvrir » simulée rend un chemin ; rien n'est encore parti, la fenêtre demande la confirmation.
  await page.locator("[data-pack-choose]").click();
  await expect(page.locator("[data-pack-file]")).toContainText("Prominence_II_Server_Pack.zip");
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Envoyer le pack au serveur");
  await expect(page.locator("[data-pack-read]")).toHaveCount(0);
  await shoot(dialog, page, "pack-envoi-confirmation");

  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Envoyer", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  // L'envoi avance en octets durables, puis la carte le dit.
  const id = (await sim(page, (s) => s.transfers.list()[0]?.id)) as string;
  await sim(page, (s, arg) => s.transfers.advance(arg as string, 250 * 1024 * 1024), id);
  await expect(page.locator("[data-pack-progress]")).toContainText("Envoi du pack en cours", {
    timeout: 10_000,
  });
  await shoot(card, page, "pack-envoi-en-cours");

  // Le fichier est entièrement arrivé : l'agent le lit, la carte montre ce qu'il a vu (le genre n'est jamais saisi).
  await sim(page, (s, arg) => s.transfers.complete(arg as string), id);
  await expect(page.locator("[data-pack-genre]")).toHaveText("Pack serveur", { timeout: 10_000 });
  await expect(page.locator("[data-pack-read]")).toContainText("652 mods");
  await expect(page.locator("[data-pack-progress]")).toHaveCount(0);
  await shoot(card, page, "pack-lu");
});
