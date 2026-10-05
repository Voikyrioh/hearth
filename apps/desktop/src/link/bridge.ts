import type { LinkStateEvent, OperationEvent, ServerInfo, Unsubscribe } from "./types";

/**
 * Pont entre l'interface et la liaison avec les serveurs. L'interface ne connaît que
 * cette interface : l'implémentation réelle (commandes et événements Tauri, branchée
 * dans `createLinkBridge`) et l'implémentation simulée (développement, tests) sont
 * interchangeables. Aucun accès réseau côté web (ADR-0002).
 */
export interface LinkBridge {
  /** Serveurs enregistrés. */
  listServers(): Promise<ServerInfo[]>;
  /** Liste des serveurs modifiée (ajout, suppression). Rend la fonction de désabonnement. */
  onServersChanged(listener: (servers: ServerInfo[]) => void): Unsubscribe;
  /** État courant des liens, puis chaque changement (événement `link://state`). */
  onLinkState(listener: (event: LinkStateEvent) => void): Unsubscribe;
  /** « Réessayer maintenant » : relance une tentative immédiate (BR-RESIL-005). */
  retryNow(serverId: string): Promise<void>;
  /** Issues d'opérations incertaines (événement `link://operation`). */
  onOperation(listener: (event: OperationEvent) => void): Unsubscribe;
}
