import { expect, type Page, test } from "@playwright/test";

// HRT-39 : la page Sécurité (revue UX du 2026-10-08, C34, C35, C36, C38, C39). Pont SIMULÉ.
type Sim = {
  security: {
    setAlert(id: string, alert: { own: boolean; others?: number | null }): void;
    setMode(id: string, state: string): void;
  };
};

const sim = (page: Page) => ({
  alert: (own: boolean, others: number) =>
    page.evaluate(
      ([o, n]) =>
        (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setAlert("forge", {
          own: o as boolean,
          others: n as number,
        }),
      [own, others],
    ),
  mode: (state: string) =>
    page.evaluate(
      (s) => (window as unknown as { __hearthSim: Sim }).__hearthSim.security.setMode("forge", s),
      state,
    ),
});

for (const width of [1100, 1366, 1920, 2560]) {
  test(`${width} : bandeaux à la largeur du contenu, action collée au message, un seul bouton d'activation`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto("/?nodev#/servers/forge/security");
    await sim(page).alert(true, 2);
    await expect(page.locator("[data-security-alert]")).toBeVisible();
    const banner = await page.locator("[data-security-alert]").boundingBox();
    const card = await page.locator("[data-attack-mode-panel]").boundingBox();
    expect(banner?.width ?? 0).toBeLessThanOrEqual(880 + 2);
    expect(banner?.x ?? 0).toBeCloseTo(card?.x ?? -1, 0);
    // Sur la page, « Activer le mode attaque » n'existe qu'une fois (dans la carte « Ce qui se passe »).
    await expect(page.getByRole("button", { name: "Activer le mode attaque" })).toHaveCount(1);
    await expect(page.locator("[data-attack-mode-toggle]")).toHaveCount(1);
    await sim(page).mode("active");
    const mode = await page
      .locator("[data-attack-mode-banner], [data-security-attack]")
      .first()
      .boundingBox()
      .catch(() => null);
    if (mode) expect(mode.width).toBeLessThanOrEqual(880 + 2);
  });
}

test("la page et le menu portent le même titre (C39)", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/security");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Sécurité");
  await expect(page.getByRole("link", { name: "Sécurité", exact: true })).toBeVisible();
});

test("la marque de sécurité ne recouvre pas les initiales du serveur (C38)", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/security");
  await sim(page).alert(true, 0);
  const mark = page.locator('[data-mark="alert"]').first();
  await expect(mark).toBeVisible();
  const overlap = await page.evaluate(() => {
    const markEl = document.querySelector('[data-mark="alert"]') as HTMLElement;
    const avatar = markEl.closest(".avatar") as HTMLElement;
    const textNode = avatar.querySelector(".avatar__initials")?.firstChild as Text;
    const range = document.createRange();
    range.selectNodeContents(textNode);
    const text = range.getBoundingClientRect();
    const box = markEl.getBoundingClientRect();
    return !(
      box.right <= text.left ||
      box.left >= text.right ||
      box.bottom <= text.top ||
      box.top >= text.bottom
    );
  });
  expect(overlap).toBe(false);
});

test("mode éteint : le futur, sans jargon ; actif : une phrase et le reste replié (C34)", async ({
  page,
}) => {
  await page.goto("/?nodev#/servers/forge/security");
  const panel = page.locator("[data-attack-mode-panel]");
  await expect(panel).toContainText("Le mode attaque permet de ne laisser se connecter");
  await expect(panel).not.toContainText("signe");
  await sim(page).mode("active");
  await expect(panel).toContainText("Le mode attaque est actif");
  const details = panel.locator("[data-attack-mode-details]");
  await expect(details).not.toHaveAttribute("open", "");
  await details.locator("summary").click();
  await expect(details).toContainText("30 minutes sans tentative refusée");
});

test("« Plus d'infos » mène à la carte qui dit ce qui se passe (C36)", async ({ page }) => {
  await page.goto("/?nodev#/servers/forge/dashboard");
  await sim(page).alert(true, 2);
  await page.getByRole("button", { name: "Plus d'infos" }).click();
  await expect(page).toHaveURL(/#\/servers\/forge\/security$/);
  const card = page.locator("[data-alert-card]");
  await expect(card).toContainText("Ton identifiant est visé");
  await expect(card).toContainText("2 autres comptes");
  await expect(card.getByRole("link", { name: "Voir le journal d'activité" })).toBeVisible();
});
