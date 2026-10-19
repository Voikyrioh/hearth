import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import { launchedBy } from "@/serviceOps/messages";

/**
 * Le verrou d'un service tenu par une action (BR-SVC-031, BR-LIFE-013) : pendant qu'une action le tient (y compris
 * une sauvegarde ou une restauration), les boutons de pilotage sont désactivés, quel que soit l'état réel
 * affiché. Des fonctions pures : les boutons de pilotage (démarrer, arrêter, redémarrer, supprimer…) des
 * tickets suivants lisent ce verdict sans rien recalculer. L'agent reste l'arbitre à chaque demande ; ceci dit
 * d'avance, honnêtement, pourquoi un bouton est inactif.
 */
export interface PilotLock {
  /** Une action tient le service : tout bouton de pilotage est désactivé. */
  locked: boolean;
  /**
   * « Lancée par {compte}. », seulement quand l'agent a dit qui (un compte qui ne peut pas lire les opérations
   * du service ne reçoit pas cette indication) et que l'identifiant est valide.
   */
  by: string | null;
  /** FIX:01M5A5VH5GR2M9PF9Y1SB04D65 : l'action tenant le service a été lancée par le compte connecté. */
  own: boolean;
}

/**
 * `running` : le magasin des opérations suit une opération sur ce service (une opération vue par le flux tient le
 * service même si la liste n'a pas encore son `busy`).
 */
export function pilotLock(
  service: Pick<ServiceInfo, "busy">,
  running = false,
  self: string | null = null,
): PilotLock {
  if (!service.busy && !running) return { locked: false, by: null, own: false };
  const account = service.busy?.by;
  return {
    locked: true,
    by: launchedBy(account, self),
    own: !!self && !!account && account === self,
  };
}

/** La phrase du verrou : « Une action est déjà en cours » pour une action d'un autre, sans « déjà » pour la sienne. */
export function busyText(lock: Pick<PilotLock, "own">): string {
  return t(lock.own ? "serviceOps.busyOwn" : "serviceOps.busy");
}
