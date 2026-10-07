import type { MessageKey } from "@/i18n";
import type { Role } from "@/link";
import type { ServerSecurity } from "@/stores/security";

/**
 * Pourquoi le mode attaque ne se change pas d'ici (conception design, écran A : trois raisons, UNE
 * seule affichée, dans cet ordre) :
 * - `readonly` : un compte Lecture seule voit l'alerte mais l'agent lui refuse le geste ;
 * - `agent_old` : l'agent de ce serveur est d'avant l'alerte et le mode attaque ;
 * - `not_enrolled` : ce PC n'a pas de clé inscrite ou prouvée (Q14 point 3) : la coquille ne part pas ;
 * - `unreadable` : la lecture de l'état a échoué et ce poste n'est pas connu : on le dit et on propose de réessayer ;
 * - `pending` : l'état n'est pas encore lu (rien à dire, le bouton attend).
 * `null` : le geste est possible. L'interface n'arbitre rien : l'agent reste le juge, ceci n'est que
 * ce qu'on peut dire SANS lui envoyer ce qui serait refusé.
 */
export type AttackModeBlock = "readonly" | "agent_old" | "not_enrolled" | "unreadable" | "pending";

export function attackModeBlock(
  role: Role | undefined,
  entry: ServerSecurity | undefined,
): AttackModeBlock | null {
  if (role !== "admin") return "readonly";
  if (!entry || entry.status === "loading") return "pending";
  if (entry.status === "unsupported") return "agent_old";
  const state = entry.state;
  // La lecture a échoué et ce poste n'est pas connu : l'état est inconnu, pas « non enregistré ». On le
  // dit, avec « Réessayer » (jamais un bouton muet).
  if (entry.status === "error" && (!state || state.device === "unknown")) return "unreadable";
  if (!state || state.device === "unknown") return "pending";
  if (state.device !== "proven" || !state.keyAtHand) return "not_enrolled";
  return null;
}

/** Le texte de la raison, ou `undefined` quand il n'y a rien à dire (geste possible, ou état pas lu). */
export function blockMessageKey(block: AttackModeBlock | null): MessageKey | undefined {
  switch (block) {
    case "readonly":
      return "security.noPermission";
    case "agent_old":
      return "security.agentTooOld";
    case "not_enrolled":
      return "security.notEnrolled";
    case "unreadable":
      return "security.loadFailed";
    default:
      return undefined;
  }
}
