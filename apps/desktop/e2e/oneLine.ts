import type { Page } from "@playwright/test";

// HRT-249 : la mesure « aucun bouton sur deux lignes », partagée par les scénarios qui l'appliquent à leurs écrans
// (`boutons-une-ligne`, `mods`, `console`). Pour chaque bouton visible : `scrollWidth <= clientWidth` (aucune
// troncature) et une seule ligne de texte.
export async function oneLineButtons(page: Page): Promise<{ failures: string[]; seen: number }> {
  return page.evaluate(() => {
    const failures: string[] = [];
    let seen = 0;
    for (const button of Array.from(document.querySelectorAll("button"))) {
      const box = button.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      const style = getComputedStyle(button);
      if (style.visibility === "hidden") continue;
      seen += 1;
      const label = (button.textContent ?? "").trim().slice(0, 40);
      if (button.scrollWidth > button.clientWidth + 1) failures.push(`${label} : tronqué`);
      const range = document.createRange();
      range.selectNodeContents(button);
      const lines = new Set(
        Array.from(range.getClientRects()).map((rect) => Math.round(rect.top / 4)),
      );
      if (lines.size > 1) failures.push(`${label} : plusieurs lignes`);
    }
    return { failures, seen };
  });
}
