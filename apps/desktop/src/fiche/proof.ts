import type { Evidence } from "@/link";

/**
 * La preuve d'un chiffre de ressource (HRT-102, BR-FICHE-008, BR-FICHE-057), des fonctions pures sans E/S. Une fiche ne
 * donne un chiffre qu'avec sa preuve : « mesuré par Hearth », « mesuré au repos », « chiffre de l'éditeur » ou
 * « non vérifié » (aucun nombre). Une preuve que ce client ne connaît pas se lit « non vérifiée » : jamais un chiffre
 * sans preuve.
 */

export type ProofKind = "measured" | "measured_idle" | "publisher" | "unmeasured";

export function proofOf(evidence: Evidence): ProofKind {
  switch (evidence.kind) {
    case "measured":
    case "measured_idle":
    case "publisher":
      return evidence.kind;
    default:
      return "unmeasured";
  }
}

/** Un minimum mesuré par Hearth (au repos ou non) : le seul qui bloque (BR-FICHE-021). */
export function isMeasured(evidence: Evidence): boolean {
  return evidence.kind === "measured" || evidence.kind === "measured_idle";
}

/** L'adresse où l'éditeur annonce son chiffre ; `null` quand la preuve n'en a pas. */
export function proofUrl(evidence: Evidence): string | null {
  return evidence.kind === "publisher" ? evidence.url : null;
}

/** Le chiffre, seulement s'il a une preuve (BR-FICHE-008) : sans preuve, aucun nombre n'est montré ni utilisé. */
export function proven(value: number | null, evidence: Evidence): number | null {
  return value !== null && proofOf(evidence) !== "unmeasured" ? value : null;
}
