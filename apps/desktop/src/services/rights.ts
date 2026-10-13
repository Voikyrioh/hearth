import { isHearthUsername } from "@/accounts/name";
import type { EngineState, ServiceRights } from "@/link";

/**
 * Le droit « gérer les services » vu par l'interface (HRT-78, ADR-0041, BR-GRANT-011 à 016) : des fonctions pures,
 * sans E/S. Le droit est LU, jamais écrit : rien ici ne l'accorde ni ne le retire, il se donne depuis le serveur
 * seulement (BR-GRANT-005). Le serveur reste l'arbitre à chaque demande (BR-GRANT-004) : ce qui est calculé ici ne
 * fait que dire d'avance, honnêtement, pourquoi un bouton sera désactivé.
 */

/** Une lecture du droit du compte connecté, telle que le magasin la tient. */
export type RightsPhase = "reading" | "known" | "failed";

export interface RightsState {
  phase: RightsPhase;
  /** La dernière lecture réussie (conservée lien coupé : les derniers droits connus, BR-RESIL-008). */
  rights: ServiceRights | null;
}

/**
 * Ce que le droit permet, pour un bouton d'action de service :
 * - `reading` : lecture en cours (5 s au plus), boutons désactivés SANS raison affichée ;
 * - `failed` : lecture en échec, boutons désactivés avec « Impossible de lire ton droit… » ;
 * - `old-agent` : l'agent n'annonce pas `service-rights` (BR-GRANT-016) ;
 * - `engine-disabled` : le pilotage est désactivé sur le serveur, l'état du moteur l'emporte (BR-GRANT-015) ;
 * - `denied` : le compte n'a pas le droit, avec la commande à copier pour un administrateur (BR-GRANT-014) ;
 * - `allowed` : le droit est accordé.
 */
export type RightsGate =
  | { kind: "reading" }
  | { kind: "failed" }
  | { kind: "old-agent" }
  | { kind: "engine-disabled" }
  | { kind: "denied"; isAdmin: boolean; command: string | null }
  | { kind: "allowed" };

/**
 * La commande à lancer sur le serveur pour accorder (`--grant`) ou retirer (`--revoke`) le droit d'un compte
 * (BR-GRANT-008, BR-GRANT-013). Construite SEULEMENT pour un identifiant qui passe `isHearthUsername` : la même
 * validation que l'aide de l'état « Accès refusé à l'agent » (HRT-74 S7). Sinon `null` : aucune commande n'est
 * proposée pour ce compte. `granted` : le compte a déjà le droit, la commande le retire.
 */
export function grantCommand(account: string, granted: boolean): string | null {
  if (!isHearthUsername(account)) return null;
  return `hearth-agent account services ${account} ${granted ? "--revoke" : "--grant"}`;
}

export function rightsGate(input: {
  state: RightsState | undefined;
  isAdmin: boolean;
  engine: EngineState | null;
}): RightsGate {
  const { state, isAdmin, engine } = input;
  // L'état du moteur l'emporte sur le droit (BR-GRANT-015) : même un compte qui a le droit n'a aucune action.
  if (engine === "disabled") return { kind: "engine-disabled" };
  if (!state || state.phase === "reading") return { kind: "reading" };
  if (state.phase === "failed" || !state.rights) return { kind: "failed" };
  if (!state.rights.known) return { kind: "old-agent" };
  if (state.rights.granted) return { kind: "allowed" };
  // La commande vise le compte connecté, tel que l'agent le connaît ; un administrateur la copie, un compte
  // en lecture seule ne la voit pas (BR-GRANT-014).
  return {
    kind: "denied",
    isAdmin,
    command: isAdmin ? grantCommand(state.rights.account, false) : null,
  };
}

/** Les boutons d'action de service sont-ils actifs ? Seul le droit accordé les ouvre. */
export function canAct(gate: RightsGate): boolean {
  return gate.kind === "allowed";
}
