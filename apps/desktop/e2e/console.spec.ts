import { expect, type Page, test } from "@playwright/test";

// HRT-269, HRT-291 : la console d'un serveur Minecraft, sur le pont SIMULÉ (aucun réseau ; la chaîne réelle agent, liaison,
// coquille est prouvée côté Rust). L'agent simulé juge comme le vrai. Serveur « forge » : administrateur avec le droit de gérer
// les services.
const PASSWORD = "Correct-Horse-9";

type Sim = {
  services: { seed(serverId: string, services: Array<Record<string, unknown>>): void };
  games: { register(serverId: string, key: string, view: Record<string, unknown>): void };
  console: {
    seed(serverId: string, key: string, view: Record<string, unknown>): void;
    seedPlayers(serverId: string, key: string, view: Record<string, unknown>): void;
    setState(serverId: string, key: string, state: string): void;
    reply(verb: string, reply: Record<string, unknown>): void;
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

async function open(page: Page, state = "ready") {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
  await sim(
    page,
    (s, arg) => {
      s.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth" }]);
      s.games.register("forge", "h_mc", { name: "Minecraft" });
      s.console.seed("forge", "h_mc", {
        available: true,
        unavailableReason: null,
        state: arg,
        canWrite: true,
        policy: {
          refused: [{ verb: "stop", reason: "use_hearth_stop" }],
          sensitive: [
            {
              verb: "op",
              title: "Donner les droits d'opérateur à {joueur} ?",
              sentence: "Un opérateur peut taper toutes les commandes du jeu.",
              target: "first_arg",
            },
          ],
        },
        quick: [{ id: "save", label: "Enregistrer le monde", text: "save-all", mode: "send" }],
        limits: { maxChars: 256, perMinute: 20, replyTimeoutS: 10 },
      });
      s.console.seedPlayers("forge", "h_mc", {
        state: "ok",
        online: 2,
        max: 20,
        names: ["Alice_42", "Bram"],
        hiddenNames: 0,
        moreNames: 0,
        readAt: new Date().toISOString(),
      });
      s.console.reply("list", {
        outcome: "answered",
        reply: "There are 2 of a max of 20 players online: Alice_42, Bram",
        truncated: false,
      });
    },
    state,
  );
  await page.evaluate(() => {
    window.location.hash = "#/servers/forge/services";
  });
  await page.locator('[data-row][data-key="h_mc"]').click();
  await expect(page.locator("[data-players-count]")).toHaveText("2 sur 20 places");
  await page.locator("[data-tab='console']").click();
  await expect(page.locator("[data-console-tab]")).toBeVisible();
}

test("envoyer, refus d'une commande, sensible avec mot de passe", async ({ page }) => {
  await open(page);
  await page.locator("[data-console-input]").fill("list");
  await page.locator("[data-console-send]").click();
  await expect(page.locator("[data-console-reply]")).toContainText("There are 2 of a max of 20");

  await page.locator("[data-console-input]").fill("stop");
  await page.locator("[data-console-send]").click();
  await expect(page.locator("[data-console-entry][data-outcome='refused']")).toContainText(
    "utilisez le bouton « Arrêter »",
  );

  await page.locator("[data-console-input]").fill("op Bram");
  await page.locator("[data-console-send]").click();
  const dialog = page.locator("dialog[open]");
  await expect(dialog).toContainText("Donner les droits d'opérateur à Bram ?");
  await dialog.locator("[data-reauth-field] input").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Envoyer la commande", exact: true }).click();
  await expect(page.locator("[data-console-entry]")).toHaveCount(3);
});

test("serveur arrêté : champ inactif avec sa phrase", async ({ page }) => {
  await open(page, "stopped");
  await expect(page.locator("[data-console-input]")).toBeDisabled();
  await expect(page.locator("[data-console-help]")).toContainText("Démarrez le serveur");
});
