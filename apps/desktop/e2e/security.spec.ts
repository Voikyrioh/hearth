import { expect, type Page, test } from "@playwright/test";

// HRT-23 : « Tes postes de confiance » dans la page Sécurité. Pont SIMULÉ (aucun réseau) ; la clé, le
// défi, la signature et la preuve du retrait (mot de passe ET clé de ce PC) sont prouvés côté Rust
// contre un vrai agent (`crates/hearth-link/tests/device_key.rs`,
// `apps/desktop/src-tauri/tests/devices_runtime.rs`). Ici : le parcours complet de l'écran.

type Sim = {
  setState(id: string, state: string): void;
  calls: string[];
  devices: { seed(server: { id: string }, devices: unknown[], options?: unknown): void };
};

const WIDTHS = [1366, 1920, 2560] as const;
const GOOD = "Correct-Horse-9";

async function shoot(page: Page, name: string) {
  for (const width of WIDTHS) {
    await page.setViewportSize({ width, height: 900 });
    await page.screenshot({ path: `e2e/screenshots/${name}-${width}.png` });
  }
  await page.setViewportSize({ width: 1366, height: 800 });
}

const row = (page: Page, name: string) => page.locator(`tr[data-device="${name}"]`);
const dialog = (page: Page) => page.locator("dialog[open]");

/** Remplace les postes du serveur `forge` et rouvre la page pour qu'elle les relise. */
async function seed(page: Page, devices: unknown[], options: unknown = {}) {
  await page.evaluate(
    ([list, opts]) => {
      const s = (window as unknown as { __hearthSim: Sim }).__hearthSim;
      s.devices.seed({ id: "forge" }, list as unknown[], opts);
    },
    [devices, options],
  );
  const nav = page.getByRole("navigation", { name: "Navigation du serveur" });
  await nav.getByRole("link", { name: "Tableau de bord" }).click();
  await nav.getByRole("link", { name: "Sécurité" }).click();
}

test("la page Sécurité : ce poste en premier, étiqueté, « Retirer » grisé sur lui, compteur", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  await expect(page.getByRole("heading", { level: 1, name: "Sécurité" })).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 2, name: "Tes postes de confiance" }),
  ).toBeVisible();
  await expect(page.locator("thead th")).toHaveText([
    "Nom du poste",
    "Dernière utilisation",
    "Actions",
  ]);
  await expect(page.locator("tbody tr")).toHaveCount(2);
  await expect(page.locator("tbody tr").first()).toContainText("Ce poste");
  await expect(page.locator(".card__counter")).toContainText("2 sur 8");
  const own = row(page, "salon/0.1.0").getByRole("button", { name: "Retirer salon/0.1.0" });
  await expect(own).toHaveAttribute("aria-disabled", "true");
  await own.focus();
  await expect(page.getByRole("tooltip")).toContainText(
    "Tu ne peux pas retirer le poste que tu utilises.",
  );
  await expect(
    row(page, "bureau/0.1.0").getByRole("button", { name: "Retirer bureau/0.1.0" }),
  ).not.toHaveAttribute("aria-disabled", "true");
  await shoot(page, "securite-liste");
});

test("retrait : confirmation avec le mot de passe, refus d'un mauvais, puis succès", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  await row(page, "bureau/0.1.0").getByRole("button", { name: "Retirer bureau/0.1.0" }).click();
  await expect(dialog(page).getByRole("heading", { name: "Retirer bureau/0.1.0 ?" })).toBeVisible();
  await expect(dialog(page)).toContainText("Es-tu sûr de vouloir retirer ce poste ?");
  await expect(dialog(page).getByRole("button", { name: "Retirer" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await shoot(page, "securite-retrait");

  await dialog(page).getByLabel("Ton mot de passe").fill("Faux-Mot-De-Passe-1");
  await dialog(page).getByRole("button", { name: "Retirer" }).click();
  await expect(dialog(page)).toContainText("Mot de passe incorrect.");
  await expect(dialog(page).getByLabel("Ton mot de passe")).toHaveValue("");
  await expect(row(page, "bureau/0.1.0")).toHaveCount(1);

  await dialog(page).getByLabel("Ton mot de passe").fill(GOOD);
  await dialog(page).getByRole("button", { name: "Retirer" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(
    page.getByText("Ce poste a été retiré de ta liste de postes de confiance."),
  ).toBeVisible();
  await expect(row(page, "bureau/0.1.0")).toHaveCount(0);
  await expect(page.locator(".card__counter")).toContainText("1 sur 8");
  // Le mot de passe ne figure dans aucun appel noté par le pont.
  const calls = await page.evaluate(
    () => (window as unknown as { __hearthSim: Sim }).__hearthSim.calls,
  );
  expect(JSON.stringify(calls)).not.toContain(GOOD);
});

test("liste pleine : 8 sur 8, et les voies de secours quand aucun poste n'est en main", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  const eight = Array.from({ length: 8 }, (_, i) => ({ name: `poste-${i + 1}/0.1.0` }));
  await seed(page, eight);
  await expect(page.locator(".card__counter")).toContainText("8 sur 8");
  await expect(page.locator(".card")).toContainText("Tu as atteint 8 postes sur 8.");
  await expect(page.locator(".card")).toContainText("Ce poste n'est pas encore enregistré.");
  await expect(page.locator(".card__help")).toContainText("hearth-agent account passwd");
  await shoot(page, "securite-limite");
});

test("liste vide, agent trop ancien", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/security");
  await seed(page, []);
  await expect(
    page.getByRole("heading", { level: 2, name: "Tu n'as pas encore de poste enregistré" }),
  ).toBeVisible();
  await seed(page, [], { supported: false });
  await expect(page.locator(".card")).toContainText(
    "Cette fonction n'existe pas encore sur ce serveur. Mets l'agent à jour.",
  );
  await shoot(page, "securite-agent-ancien");
});

test("hors ligne : la page est datée et désaturée, « Retirer » indisponible avec sa raison", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  await expect(row(page, "bureau/0.1.0")).toHaveCount(1);
  await page.evaluate(() =>
    (window as unknown as { __hearthSim: Sim }).__hearthSim.setState("forge", "offline"),
  );
  await expect(page.locator('[data-stale="true"]')).toHaveCount(1);
  const button = row(page, "bureau/0.1.0").getByRole("button", { name: "Retirer bureau/0.1.0" });
  await expect(button).toHaveAttribute("aria-disabled", "true");
  await button.focus();
  await expect(page.getByRole("tooltip")).toContainText(
    "Indisponible tant que le serveur est hors ligne.",
  );
  await shoot(page, "securite-hors-ligne");
});

test("un compte en lecture seule voit la page et ses postes", async ({ page }) => {
  await page.goto("/?nodev#/servers/salon/security");
  await expect(page.getByRole("heading", { level: 1, name: "Sécurité" })).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Navigation du serveur" }).getByRole("link"),
  ).toHaveText(["Tableau de bord", "Sécurité"]);
  await expect(page.locator("tbody tr")).toHaveCount(2);
});
