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
}

/**
 * `running` : le magasin des opérations suit une opération sur ce service (une opération vue par le flux tient le
 * service même si la liste n'a pas encore son `busy`).
 */
export function pilotLock(service: Pick<ServiceInfo, "busy">, running = false): PilotLock {
  if (!service.busy && !running) return { locked: false, by: null };
  return { locked: true, by: launchedBy(service.busy?.by) };
}
