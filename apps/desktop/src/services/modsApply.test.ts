import { describe, expect, it } from "vitest";
import { type ChangeRow, placeSummary, stepText } from "./modsApply";

// Le suivi d'un lot n'a aucune liste de changements au-dessus de lui : la mise en place se dit en comptes (HRT-263).
const row = (key: string): ChangeRow => ({ key, sign: "+", title: "", detail: "", sentence: "" });

describe("placeSummary", () => {
  it("compte chaque genre de changement, au singulier ou au pluriel", () => {
    expect(placeSummary([row("add:a"), row("replace:b"), row("disable:c"), row("disable:d")])).toBe(
      "1 ajout, 1 remplacement, 2 désactivations.",
    );
  });

  it("dit que les fichiers retirés sont mis de côté", () => {
    expect(placeSummary([row("retire:a")])).toBe(
      "1 retrait. Les fichiers retirés sont mis de côté.",
    );
  });
});

describe("stepText, étape « Arrêt du serveur » (HRT-292)", () => {
  it("dit exactement si Hearth fait d'abord enregistrer le monde par la console, selon la fiche", () => {
    const withConsole = stepText("stop", 1, true, "image", true);
    expect(withConsole).toContain("fait d'abord enregistrer le monde par la console du jeu");
    const without = stepText("stop", 1, true, "image", false);
    expect(without).not.toContain("console");
    expect(without).not.toContain("Si la fiche");
    expect(stepText("stop", 1, true, "ask", true)).toContain("par la console du jeu");
    expect(stepText("stop", 1, true, "ask", false)).not.toContain("console");
  });

  it("fiche pas lue : aucune affirmation sur la console", () => {
    expect(stepText("stop", 1, true, null, null)).not.toContain("console");
  });
});
