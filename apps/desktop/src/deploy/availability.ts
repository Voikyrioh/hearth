import { type MessageKey, t } from "@/i18n";
import type { EngineState } from "@/link";
import { canAct, type RightsGate } from "@/services/rights";

/**
 * Le bouton « Déployer un service » et la route du formulaire (HRT-69, BR-DEPLOY-001, 027, BR-ENGINE-014, BR-GRANT-014),
 * des fonctions pures sans E/S. Le serveur reste l'arbitre à chaque demande : ceci dit d'avance, honnêtement, ce que
 * l'interface propose.
 *
 * - absent : le moteur n'est pas utilisable du tout (absent, non pris en charge, version non prise en charge, pilotage
 *   désactivé) ou l'agent ne gère pas les services ;
 * - visible et inactif : le droit manque (`byRights`, la raison est dite par `ServiceRightReason`, une seule fois) ou le
 *   moteur n'est pas prêt (la raison est écrite sous le bouton) ;
 * - actif : le droit est accordé et le moteur prêt.
 */
export type DeployDoor =
  | { kind: "hidden" }
  | { kind: "disabled"; byRights: boolean; reason: string | null }
  | { kind: "open" };

const ENGINE_REASONS: Partial<Record<EngineState, MessageKey>> = {
  stopped: "deploy.engineStopped",
  access_denied: "deploy.engineDenied",
  unresponsive: "deploy.engineUnresponsive",
};

/** Les états du moteur où le formulaire n'a aucun sens : le bouton n'est pas proposé. */
const UNUSABLE: readonly EngineState[] = [
  "absent",
  "unsupported_engine",
  "unsupported_version",
  "disabled",
  "unknown",
];

export function deployDoor(input: {
  /** `null` : l'agent ne gère pas les services, ou rien n'est encore arrivé. */
  engine: EngineState | null;
  gate: RightsGate;
}): DeployDoor {
  const { engine, gate } = input;
  if (engine === null || UNUSABLE.includes(engine) || gate.kind === "engine-disabled") {
    return { kind: "hidden" };
  }
  if (!canAct(gate)) return { kind: "disabled", byRights: true, reason: null };
  if (engine === "ready") return { kind: "open" };
  const key = ENGINE_REASONS[engine] ?? "deploy.engineNotReady";
  return { kind: "disabled", byRights: false, reason: t(key) };
}

/**
 * La route du formulaire est-elle ouverte ? Oui quand le bouton est actif, et quand seul le moteur manque : le
 * formulaire s'ouvre et explique l'état (BR-DEPLOY-027). Non (retour à la liste) pour un compte sans le droit ou un
 * moteur inutilisable (BR-DEPLOY-001).
 */
export function routeOpen(door: DeployDoor): boolean {
  return door.kind === "open" || (door.kind === "disabled" && !door.byRights);
}

/** Pourquoi « Déployer » est inactif, écrit à gauche du bouton ; `null` : il est actif (BR-DEPLOY-027, design écran F). */
export interface SubmitReason {
  text: string;
  /** Des champs sont à corriger : le pied propose d'aller au premier. */
  fixable: boolean;
}

/**
 * Le motif du blocage, par priorité : le lien, le moteur (ou le droit), les bornes de la machine, un formulaire vide,
 * un champ à corriger. Hors lien et moteur, le bouton s'active dès que tout est valide : les constats du serveur en
 * attente ne bloquent pas (il refait tout au lancement, BR-DEPLOY-032).
 */
export function submitReason(input: {
  /** La raison du lien (`useNeedsLink`), `null` : connecté. */
  link: string | null;
  door: DeployDoor;
  /** Le serveur a dit que ce compte n'a plus (ou pas) le droit de gérer les services (BR-DEPLOY-030). */
  rightsLost: boolean;
  capacityReady: boolean;
  /** Le formulaire est vide : rien n'est encore signalé. */
  pristine: boolean;
  /** Un défaut de forme, ou un constat du serveur qui bloque (nom pris, port tenu). */
  invalid: boolean;
}): SubmitReason | null {
  if (input.link) return { text: input.link, fixable: false };
  if (input.door.kind === "disabled" && input.door.reason) {
    return { text: input.door.reason, fixable: false };
  }
  if (input.rightsLost) return { text: t("rights.lost"), fixable: false };
  if (!input.capacityReady) return { text: t("deploy.capacityPending"), fixable: false };
  if (input.pristine) return { text: t("deploy.fillRequired"), fixable: false };
  if (input.invalid) return { text: t("deploy.fixFields"), fixable: true };
  return null;
}
