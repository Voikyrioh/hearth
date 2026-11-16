import { describe, expect, it } from "vitest";
import type { FicheCard } from "@/link";
import { counts, hasSoon, normalize, sectionsOf } from "./sections";

function card(id: string, name: string, soon = false): FicheCard {
  return {
    id,
    name,
    summary: "Une phrase.",
    version: 1,
    status: soon ? "preview" : "published",
    installable: !soon,
    previewReason: soon ? "Une raison écrite pour le joueur." : null,
  };
}

const FICHES = [
  card("valheim", "Valheim"),
  card("terraria", "Terraria"),
  card("zomboid", "Project Zomboid", true),
  card("elephant", "Éléphant", true),
  card("minecraft", "Minecraft Java"),
];

describe("le rangement du Catalogue (BR-FICHE-063)", () => {
  it("range les fiches prêtes puis celles qui attendent, chacune triée par nom", () => {
    const sections = sectionsOf(FICHES, "", "all");
    expect(sections.ready.map((c) => c.name)).toEqual(["Minecraft Java", "Terraria", "Valheim"]);
    expect(sections.soon.map((c) => c.name)).toEqual(["Éléphant", "Project Zomboid"]);
  });

  it("cherche sans tenir compte des majuscules ni des accents", () => {
    expect(normalize("Éléphant")).toBe("elephant");
    expect(sectionsOf(FICHES, "ELEPHANT", "all").soon.map((c) => c.id)).toEqual(["elephant"]);
    expect(sectionsOf(FICHES, "  zomb ", "all").soon.map((c) => c.id)).toEqual(["zomboid"]);
    expect(sectionsOf(FICHES, "craft", "all").ready.map((c) => c.id)).toEqual(["minecraft"]);
  });

  it("un filtre garde une seule section", () => {
    expect(sectionsOf(FICHES, "", "ready").soon).toEqual([]);
    expect(sectionsOf(FICHES, "", "soon").ready).toEqual([]);
    expect(sectionsOf(FICHES, "", "soon").soon).toHaveLength(2);
  });

  it("les comptes suivent la recherche en cours, jusqu'à zéro", () => {
    expect(counts(FICHES, "")).toEqual({ all: 5, ready: 3, soon: 2 });
    expect(counts(FICHES, "m")).toEqual({ all: 3, ready: 2, soon: 1 });
    expect(counts(FICHES, "introuvable")).toEqual({ all: 0, ready: 0, soon: 0 });
  });

  it("le filtre « Bientôt » n'existe que s'il y a une fiche qui attend", () => {
    expect(hasSoon(FICHES)).toBe(true);
    expect(hasSoon(FICHES.filter((c) => c.installable))).toBe(false);
    expect(hasSoon([])).toBe(false);
  });
});
