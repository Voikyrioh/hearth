import { describe, expect, it } from "vitest";
import { newFicheDraft } from "@/fiche/draft";
import { stopTexts } from "@/fiche/messages";
import { launchReason, plannedAddress, whatWillHappen, whatWillHappenNote } from "@/fiche/review";
import type { FicheDetail } from "@/link";
import { freshBridge } from "@/test/app";

// La page « Vérifiez avant de lancer » dit d'avance ce que Hearth va faire (HRT-246, BR-FICHE-025) : des fonctions pures
// lues dans la fiche embarquée (Terraria) et la saisie, jamais un chiffre sans preuve ni une valeur secrète.

function terraria(): FicheDetail {
  const detail = freshBridge().bridge.catalog.get("terraria");
  if (!detail) throw new Error("Terraria est la fiche de départ");
  return detail;
}

describe("whatWillHappen", () => {
  it("dit les quatre étapes dans l'ordre, avec le nom du serveur de jeu et la mémoire choisie", () => {
    const detail = terraria();
    const draft = newFicheDraft(detail, "terraria");
    draft.name = "mon-terraria";
    draft.resources = { memoryBytes: 2 * 1024 ** 3, cpuMillis: 2000 };
    const lines = whatWillHappen(detail, draft, "Maison");
    expect(lines).toHaveLength(4);
    expect(lines[0]).toMatch(/^Hearth récupère le programme serveur de Terraria/);
    expect(lines[1]).toContain("« mon-terraria » sur Maison");
    expect(lines[1]).toContain("2 Go de mémoire");
    expect(lines[2]).toMatch(/^Le jeu démarre/);
    expect(lines[3]).toBe(
      "Hearth s'assure que le jeu répond, puis vous donne l'adresse à transmettre à vos joueurs.",
    );
  });

  it("reprend la phrase de l'arrêt de la fiche, la même que la ligne « Arrêt du jeu », puis dit l'annulation", () => {
    const detail = terraria();
    const note = whatWillHappenNote(detail);
    expect(note).toContain(
      "Vous pourrez annuler tant que le serveur n'est pas en cours de création.",
    );
    expect(
      note.startsWith(stopTexts(detail.stop as NonNullable<typeof detail.stop>)[0] ?? ""),
    ).toBe(true);
    // Une fiche dont la suite d'arrêt n'est pas essayée ne promet jamais « Hearth demandera au jeu d'enregistrer ».
    const untested = structuredClone(detail);
    untested.stop = { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: false };
    expect(whatWillHappenNote(untested)).not.toContain("demandera");
    expect(whatWillHappenNote(untested)).toContain("ne peut pas encore");
  });
});

describe("plannedAddress", () => {
  it("donne l'adresse du réseau maison avec le port choisi et un port à ouvrir par port publié", () => {
    const detail = terraria();
    const draft = newFicheDraft(detail, "terraria");
    const address = plannedAddress(detail, draft, "192.168.1.20");
    expect(address.lan).toMatch(/^192\.168\.1\.20:\d+$/);
    expect(address.internet.length).toBeGreaterThan(0);
    expect(address.internet[0]?.protocol).toMatch(/^(TCP|UDP)$/);
  });

  it("met une adresse IPv6 entre crochets", () => {
    const detail = terraria();
    expect(plannedAddress(detail, newFicheDraft(detail, "terraria"), "fe80::1").lan).toMatch(
      /^\[fe80::1\]:\d+$/,
    );
  });
});

describe("launchReason", () => {
  it("dit ce qui retient le bouton, dans les trois cas, et rien quand il est actif", () => {
    expect(launchReason(true, true)).toBe(
      "Cochez les points d'attention et saisissez votre mot de passe pour lancer.",
    );
    expect(launchReason(true, false)).toBe("Cochez les points d'attention pour lancer.");
    expect(launchReason(false, true)).toBe("Saisissez votre mot de passe pour lancer.");
    expect(launchReason(false, false)).toBeNull();
  });
});
