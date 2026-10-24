import { describe, expect, it } from "vitest";
import type { ServiceOperation } from "@/link";
import {
  formatSize,
  launchedBy,
  operationTitle,
  outcomeText,
  progressOf,
  refusalText,
  statusText,
  stepText,
} from "./messages";

function op(patch: Partial<ServiceOperation> = {}): ServiceOperation {
  return {
    id: "01J",
    key: "h_01J",
    kind: "deploy",
    status: "running",
    step: null,
    steps: [],
    percent: null,
    itemsDone: null,
    bytesDone: null,
    bytesTotal: null,
    etaS: null,
    cancellable: true,
    decision: null,
    cause: null,
    ...patch,
  };
}

describe("les textes d'une opération", () => {
  it("le titre dit l'opération et le service", () => {
    expect(operationTitle(op({ kind: "deploy" }), "terraria", true)).toBe(
      "Déploiement de terraria",
    );
    expect(operationTitle(op({ kind: "update" }), "terraria", false)).toBe(
      "Modification de terraria",
    );
    expect(operationTitle(op({ kind: "unknown" }), "terraria", true)).toBe(
      "Opération sur terraria",
    );
  });

  it("l'effacement des données : sans le droit, une phrase sans chemin ni progression (BR-SVC-029)", () => {
    const erase = op({ kind: "delete", itemsDone: 120, percent: 30, step: "erase" });
    expect(operationTitle(erase, "terraria", false)).toBe(
      "Effacement des données de terraria en cours.",
    );
    expect(progressOf(erase, false)).toEqual({ ratio: null, text: null });
    // Avec le droit : le titre et la progression.
    expect(operationTitle(erase, "terraria", true)).toBe("Effacement des données de terraria");
    expect(progressOf(erase, true)).toEqual({ ratio: 0.3, text: "30 % · 120 éléments effacés" });
  });

  it("une étape inconnue n'est jamais affichée telle quelle", () => {
    expect(stepText(op({ step: "pull" }))).toBe("Récupération de l'image");
    expect(stepText(op({ step: "etape_du_futur" }))).toBeNull();
    expect(stepText(op({ step: null }))).toBeNull();
  });

  it("la progression dit les octets quand l'agent les donne, jamais un pourcentage inventé", () => {
    expect(
      progressOf(op({ percent: 40, bytesDone: 400 * 1024 * 1024, bytesTotal: 1024 ** 3 }), true),
    ).toEqual({ ratio: 0.4, text: "40 % · 400 Mo sur 1 Go" });
    expect(progressOf(op(), true)).toEqual({ ratio: null, text: null });
  });

  it("l'issue se dit par le statut, en phrase complète", () => {
    expect(outcomeText(op({ status: "succeeded" }), "terraria")).toBe(
      "Le service terraria est en marche.",
    );
    expect(outcomeText(op({ status: "failed", kind: "update" }), "terraria")).toBe(
      "La modification de terraria a échoué.",
    );
    expect(outcomeText(op({ status: "rolled_back", kind: "update" }), "terraria")).toBe(
      "La modification de terraria : le service a été remis comme avant.",
    );
    expect(outcomeText(op({ status: "running" }), "terraria")).toBeNull();
    expect(outcomeText(op({ status: "needs_attention" }), "terraria")).toBeNull();
  });

  it("une réponse attendue et une vérification à faire se disent", () => {
    expect(statusText(op({ status: "awaiting_decision" }))).toBe("Votre réponse est attendue.");
    expect(statusText(op({ status: "needs_attention" }))).toBe(
      "L'état de ce service est à vérifier.",
    );
    expect(statusText(op({ status: "unknown" }))).toBe("L'état de cette opération est inconnu.");
  });

  it("un refus inconnu a une phrase neutre", () => {
    expect(refusalText({ kind: "expired" })).toBe(
      "Le délai de réponse est dépassé : l'opération a été annulée.",
    );
    expect(refusalText({ kind: "busy" })).toBe("Votre réponse n'a pas pu être prise en compte.");
  });

  it("« Lancée par {compte}. » seulement pour un identifiant valide", () => {
    expect(launchedBy("marie")).toBe("Lancée par marie.");
    expect(launchedBy("<b>x</b>")).toBeNull();
    expect(launchedBy(null)).toBeNull();
    expect(launchedBy(undefined)).toBeNull();
  });

  it("une taille est arrondie en unités lisibles", () => {
    expect(formatSize(0)).toBe("0 o");
    expect(formatSize(1536)).toBe("1,5 Ko");
    expect(formatSize(5 * 1024 ** 3)).toBe("5 Go");
  });

  it("aucun texte ne contient de tiret cadratin", () => {
    const sentences = [
      operationTitle(op({ kind: "world_import" }), "x", true),
      outcomeText(op({ status: "interrupted" }), "x") ?? "",
      statusText(op({ status: "rolling_back" })),
    ];
    for (const sentence of sentences) expect(sentence).not.toContain("—");
  });
});
