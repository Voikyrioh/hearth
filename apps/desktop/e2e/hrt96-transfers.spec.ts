import { expect, type Locator, type Page, test } from "@playwright/test";

// HRT-96 : envoyer un fichier dans la zone d'attente de l'agent depuis le client, sur le pont SIMULÉ (aucun réseau ; la
// chaîne réelle agent, liaison, coquille est prouvée côté Rust). L'agent simulé juge comme le vrai. Serveur « forge » :
// administrateur avec le droit de gérer les services. Captures : 1366 et 2560, dans `HEARTH_SHOTS_DIR` (par défaut
// `e2e/screenshots/`, non commité).
const SHOTS = process.env.HEARTH_SHOTS_DIR ?? "e2e/screenshots";
const WIDTHS = [1366, 2560] as const;
const HEIGHTS = { 1366: 800, 2560: 1440 } as const;

type Sim = {
  startDeposit(serverId: string, path: string, password: string | null): Promise<{ kind: string }>;
  transfers: {
    list(): Array<{ id: string; sent: number; status: string }>;
    advance(id: string, bytes: number): void;
    interrupt(id: string): void;
    complete(id: string): void;
    refuse(id: string, refusal: Record<string, unknown>): void;
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

async function open(page: Page) {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await expect(page.locator(".nav__item").first()).toBeVisible();
}

/** Un envoi lancé « par un autre écran » : le panneau le retrouve à sa relecture suivante. */
async function startPack(page: Page): Promise<string> {
  const outcome = await sim(page, (s) =>
    s.startDeposit(
      "forge",
      "C:\\Téléchargements\\Prominence_II_Server_Pack.zip",
      "Correct-Horse-9",
    ),
  );
  expect((outcome as { kind: string }).kind).toBe("started");
  return (await sim(page, (s) => s.transfers.list()[0]?.id)) as string;
}

test("un envoi apparaît, avance en octets, continue quand on quitte l'écran et finit vérifié", async ({
  page,
}) => {
  await open(page);
  await expect(page.locator("[data-transfers-strip]")).toHaveCount(0);
  const id = await startPack(page);
  const row = page.locator(`[data-transfer="${id}"]`);
  await expect(row).toBeVisible({ timeout: 10_000 });
  await expect(row.locator("[data-transfer-name]")).toHaveText("Prominence_II_Server_Pack.zip");
  await expect(row.locator("[data-transfer-status]")).toHaveText("Envoi en cours.");
  await expect(row.locator("[data-transfer-cancel]")).toBeVisible();
  await expect(row.locator("[data-transfer-resume]")).toHaveCount(0);

  await sim(page, (s, arg) => s.transfers.advance((arg as { id: string }).id, 250 * 1024 * 1024), {
    id,
  });
  await expect(row.locator("[data-transfer-progress]")).toHaveText("250 Mo sur 600 Mo");
  await shoot(page.locator("[data-transfers-strip]"), page, "hrt96-envoi");

  // Quitter l'écran : l'envoi continue et le panneau le retrouve ailleurs.
  await page.goto("/?nodev#/servers/forge/services");
  await expect(page.locator(`[data-transfer="${id}"]`)).toBeVisible();
  await sim(page, (s, arg) => s.transfers.advance((arg as { id: string }).id, 600 * 1024 * 1024), {
    id,
  });
  await sim(page, (s, arg) => s.transfers.complete((arg as { id: string }).id), { id });
  await expect(page.locator(`[data-transfer="${id}"] [data-transfer-status]`)).toHaveText(
    "Le fichier est arrivé sur le serveur, vérifié.",
    { timeout: 10_000 },
  );
  await expect(page.locator(`[data-transfer="${id}"] [data-transfer-progress]`)).toHaveText(
    "600 Mo sur 600 Mo",
  );
  await page.locator(`[data-transfer="${id}"] [data-transfer-dismiss]`).click();
  await expect(page.locator("[data-transfers-strip]")).toHaveCount(0);
});

test("une coupure laisse « Reprendre l'envoi » : rien ne repart seul, un bouton reprend", async ({
  page,
}) => {
  await open(page);
  const id = await startPack(page);
  const row = page.locator(`[data-transfer="${id}"]`);
  await expect(row).toBeVisible({ timeout: 10_000 });
  await sim(page, (s, arg) => s.transfers.advance((arg as { id: string }).id, 100 * 1024 * 1024), {
    id,
  });
  await sim(page, (s, arg) => s.transfers.interrupt((arg as { id: string }).id), { id });
  await expect(row.locator("[data-transfer-status]")).toContainText("Reprenez l'envoi", {
    timeout: 10_000,
  });
  const resume = row.locator("[data-transfer-resume]");
  await expect(resume).toHaveText("Reprendre l'envoi");
  // Rien ne repart seul : après plusieurs relectures, l'envoi est toujours interrompu et n'a pas avancé.
  await page.waitForTimeout(2500);
  await expect(row).toHaveAttribute("data-status", "interrupted");
  await expect(row.locator("[data-transfer-progress]")).toHaveText("100 Mo sur 600 Mo");
  await shoot(page.locator("[data-transfers-strip]"), page, "hrt96-coupure");
  await resume.click();
  await expect(row).toHaveAttribute("data-status", "sending");
  await expect(row.locator("[data-transfer-progress]")).toHaveText("100 Mo sur 600 Mo");
});

test("le manque de place dit ce qui manque, l'envoi reste reprenable, annuler retire la partie reçue", async ({
  page,
}) => {
  await open(page);
  const id = await startPack(page);
  const row = page.locator(`[data-transfer="${id}"]`);
  await expect(row).toBeVisible({ timeout: 10_000 });
  await sim(
    page,
    (s, arg) =>
      s.transfers.refuse((arg as { id: string }).id, {
        kind: "no_space",
        missing: 3 * 1024 * 1024 * 1024,
        remaining: 512 * 1024 * 1024,
      }),
    { id },
  );
  await expect(row.locator("[data-transfer-status]")).toHaveText(
    "Il manque 3 Go sur le disque pour écrire ce fichier. Il resterait 512 Mo de libre.",
    { timeout: 10_000 },
  );
  await expect(row.locator("[data-transfer-resume]")).toBeVisible();
  await row.locator("[data-transfer-cancel]").click();
  await expect(row.locator("[data-transfer-status]")).toContainText("partie reçue est supprimée");
  await expect(row.locator("[data-transfer-resume]")).toHaveCount(0);
  await expect(row.locator("[data-transfer-cancel]")).toHaveCount(0);
  await shoot(page.locator("[data-transfers-strip]"), page, "hrt96-annule");
  await row.locator("[data-transfer-dismiss]").click();
  await expect(page.locator("[data-transfers-strip]")).toHaveCount(0);
});
