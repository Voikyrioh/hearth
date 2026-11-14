import { describe, expect, it } from "vitest";
import type { ServiceOperation } from "@/link";
import { operationTitle, outcomeOf, progressOf, stepText } from "./messages";

// L'issue d'une suppression annoncée par une notification quand aucune fenêtre ne la suit (HRT-86, BR-DEL-013 à 016) :
// dite par sa CAUSE, jamais par un texte de l'agent.
function op(patch: Partial<ServiceOperation>): ServiceOperation {
  return {
    id: "01OP",
    key: "h_web",
    kind: "delete",
    status: "succeeded",
    step: null,
    steps: [],
    percent: null,
    itemsDone: null,
    bytesDone: null,
    bytesTotal: null,
    etaS: null,
    cancellable: false,
    decision: null,
    cause: null,
    ...patch,
  };
}

describe("l'issue d'une suppression", () => {
  it("réussie : le service est supprimé", () => {
    expect(outcomeOf(op({}), "web")).toEqual({
      message: "Le service web est supprimé.",
      tone: "success",
    });
  });

  it("des données pas effacées ou un arrêt forcé : un avertissement", () => {
    expect(outcomeOf(op({ cause: "data_not_erased" }), "web")).toEqual({
      message: "Le service web est supprimé, mais certaines données n'ont pas pu être effacées.",
      tone: "warn",
    });
    expect(outcomeOf(op({ cause: "forced_stop" }), "web")?.tone).toBe("warn");
  });

  it("échouée : le service n'a pas été supprimé ; annulée : dit sans alarme", () => {
    expect(outcomeOf(op({ status: "failed", cause: "engine_refused" }), "web")).toEqual({
      message: "Le service web n'a pas été supprimé.",
      tone: "error",
    });
    expect(outcomeOf(op({ status: "cancelled", cause: "cancelled_by_account" }), "web")).toEqual({
      message: "La suppression de web a été annulée.",
      tone: "info",
    });
  });

  it("interrompue : l'issue générale, le résultat n'est pas connu", () => {
    expect(outcomeOf(op({ status: "interrupted" }), "web")?.message).toBe(
      "La suppression de web : l'opération a été interrompue.",
    );
  });

  it("les étapes du plan ont un nom, l'effacement dit les éléments à qui a le droit seulement", () => {
    expect(stepText(op({ step: "remove_container" }))).toBe("Suppression du service");
    expect(stepText(op({ step: "forget" }))).toBe("Effacement de la configuration");
    const erasing = op({ status: "running", step: "erase", itemsDone: 12 });
    expect(operationTitle(erasing, "web", false)).toBe("Effacement des données de web en cours.");
    expect(progressOf(erasing, false)).toEqual({ ratio: null, text: null });
    expect(progressOf(erasing, true).text).toBe("12 éléments effacés");
    // Avant l'effacement (arrêt, suppression du conteneur), l'opération ne « efface » rien : elle se dit « Suppression ».
    const removing = op({ status: "running", step: "remove_container" });
    expect(operationTitle(removing, "web", true)).toBe("Suppression de web");
    expect(operationTitle(removing, "web", false)).toBe("Suppression de web");
    expect(operationTitle(op({ status: "running", step: null }), "web", true)).toBe(
      "Suppression de web",
    );
    // Le nombre d'éléments s'écrit comme dans la fenêtre : espace entre les milliers.
    expect(
      progressOf(op({ status: "running", step: "erase", itemsDone: 48210 }), true).text,
    ).toMatch(/48\s210 éléments effacés/);
  });
});
