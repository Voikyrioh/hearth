import { describe, expect, it } from "vitest";
import { UPDATE_STEPS, type UpdateReason } from "@/link";
import {
  historyMessage,
  progressSentence,
  refusalMessage,
  resultMessage,
  resultTone,
  stepLabel,
} from "./messages";

// Les textes de la mise à jour de l'agent sont indexés par les CODES de l'agent : chaque code a son
// texte (le compilateur l'exige), et les textes de la spécification sont repris à la lettre.

describe("les étapes", () => {
  it("labels the five steps, with the percentage during the download only", () => {
    expect(UPDATE_STEPS.map((step) => stepLabel(step, null))).toEqual([
      "Téléchargement…",
      "Vérification…",
      "Installation…",
      "Redémarrage…",
      "Contrôle…",
    ]);
    expect(stepLabel("download", 35)).toBe("Téléchargement : 35 %");
    expect(stepLabel("verify", 35)).toBe("Vérification…");
  });

  it("says the current step in the sentence of the specification", () => {
    expect(progressSentence("download", 35)).toBe(
      "Mise à jour de l'agent en cours. Étape : téléchargement (35 %)…",
    );
    expect(progressSentence("download", null)).toBe(
      "Mise à jour de l'agent en cours. Étape : téléchargement…",
    );
    expect(progressSentence("restart", null)).toBe(
      "Mise à jour de l'agent en cours. Étape : redémarrage…",
    );
  });
});

describe("les résultats", () => {
  const reasons: UpdateReason[] = [
    "unreachable",
    "download_failed",
    "bad_checksum",
    "bad_signature",
    "bad_binary",
    "staging",
    "swap",
    "supervisor_launch",
    "no_answer",
    "identity_changed",
    "interrupted",
    "rollback_failed",
    "unknown",
  ];

  it("has a distinct, non empty text for every outcome and reason, and never an em dash", () => {
    for (const reason of reasons) {
      for (const outcome of ["failed", "rolled_back"] as const) {
        const text = resultMessage({ outcome, reason, version: "0.2.0" });
        expect(text.length, `${outcome}/${reason}`).toBeGreaterThan(10);
        expect(text).not.toContain("—");
      }
    }
    expect(resultMessage({ outcome: "failed", reason: null, version: null })).toContain(
      "n'a pas abouti",
    );
  });

  it("uses the texts of the specification to the letter", () => {
    expect(resultMessage({ outcome: "rolled_back", reason: "no_answer", version: "0.2.0" })).toBe(
      "Mise à jour de l'agent annulée. Le nouvel agent n'a pas répondu. Retour à la version précédente.",
    );
    expect(resultMessage({ outcome: "failed", reason: "unreachable", version: "0.2.0" })).toBe(
      "Le serveur n'a pas accès à Internet pour télécharger la mise à jour de l'agent.",
    );
  });

  it("puts the version in the success text, and says nothing of it when it is unknown", () => {
    expect(resultMessage({ outcome: "succeeded", reason: null, version: "0.2.0" })).toBe(
      "Mise à jour de l'agent réussie. L'agent est en version 0.2.0.",
    );
    expect(resultMessage({ outcome: "succeeded", reason: null, version: null })).toBe(
      "Mise à jour de l'agent réussie.",
    );
    // Un retour arrière sans raison connue se dit quand même, sans inventer la raison.
    expect(resultMessage({ outcome: "rolled_back", reason: "unknown", version: null })).toBe(
      "Mise à jour de l'agent annulée. Retour à la version précédente.",
    );
  });

  it("gives a tone to each outcome and a history line to an old one", () => {
    expect(resultTone({ outcome: "succeeded", reason: null, version: "1" })).toBe("ok");
    expect(resultTone({ outcome: "rolled_back", reason: "no_answer", version: "1" })).toBe("warn");
    expect(resultTone({ outcome: "failed", reason: "swap", version: "1" })).toBe("crit");
    expect(
      historyMessage({ outcome: "succeeded", reason: null, version: "1" }, "il y a 3 jours"),
    ).toBe("Dernière mise à jour de l'agent : réussie (il y a 3 jours).");
  });
});

describe("les refus de la demande", () => {
  it("says the same sentences as the agent and the specification", () => {
    expect(refusalMessage({ kind: "in_progress" })).toBe(
      "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard.",
    );
    expect(refusalMessage({ kind: "managed_install" })).toBe(
      "Cette installation est gérée par le système : l'agent ne se met pas à jour à distance. Mets-le à jour par la configuration du système.",
    );
    expect(refusalMessage({ kind: "bad_signature" })).toBe(
      "La signature de la mise à jour est refusée. Rien n'a été téléchargé ni modifié.",
    );
    for (const kind of [
      "invalid_target",
      "no_target",
      "target_changed",
      "not_newer",
      "other",
    ] as const) {
      expect(refusalMessage({ kind }).length).toBeGreaterThan(10);
    }
  });
});
