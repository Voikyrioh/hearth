import type { LinkStateEvent, OperationEvent, ServerInfo, Unsubscribe } from "./types";

/**
 * Pont entre l'interface et la liaison avec les serveurs. L'interface ne connaît que
 * cette interface : l'implémentation réelle (commandes et événements Tauri, branchée
 * dans `createLinkBridge`) et l'implémentation simulée (développement, tests) sont
 * interchangeables. Aucun accès réseau côté web (ADR-0002).
 *
 * Contrat des abonnements (calqué sur `listen` de Tauri, qui rend une promesse) :
 * - chaque `on...` rend une PROMESSE de désabonnement ;
 * - `onServersChanged` et `onLinkState` REJOUENT l'état courant à l'abonnement (le
 *   récepteur est appelé avec la liste ou avec l'événement de chaque serveur, au plus
 *   tard avant que la promesse ne se résolve) : il n'y a donc pas de « lecture initiale »
 *   séparée, et aucune fenêtre où un ajout ou une suppression serait perdu ;
 * - un implémenteur réel s'abonne d'abord aux événements, puis envoie l'instantané.
 */
export interface LinkBridge {
  /** Serveurs enregistrés : liste courante, puis à chaque ajout ou suppression. */
  onServersChanged(listener: (servers: ServerInfo[]) => void): Promise<Unsubscribe>;
  /** État du lien de chaque serveur (`link://state`) : état courant, puis chaque changement. */
  onLinkState(listener: (event: LinkStateEvent) => void): Promise<Unsubscribe>;
  /** « Réessayer maintenant » : relance une tentative immédiate (BR-RESIL-005). */
  retryNow(serverId: string): Promise<void>;
  /** Issues d'opérations incertaines (`link://operation`). Pas de rejeu. */
  onOperation(listener: (event: OperationEvent) => void): Promise<Unsubscribe>;
}
