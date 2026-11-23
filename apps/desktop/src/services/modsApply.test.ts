import { describe, expect, it } from "vitest";
import { type ChangeRow, placeSummary } from "./modsApply";

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
