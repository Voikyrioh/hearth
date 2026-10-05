/** États du lien avec un serveur (spec lien résilient, ADR-0007). */
export type LinkState =
  | "connected"
  | "reconnecting"
  | "offline"
  | "session_expired"
  | "access_revoked";

export const LINK_STATES: readonly LinkState[] = [
  "connected",
  "reconnecting",
  "offline",
  "session_expired",
  "access_revoked",
];

export type Role = "admin" | "readonly";

/** Numéro de couleur dans la palette de 8 jetons `--server-1` à `--server-8`. */
export type ServerColor = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

/** Un serveur enregistré dans le client. */
export interface ServerInfo {
  id: string;
  name: string;
  address: string;
  color: ServerColor;
  role: Role;
}

/**
 * Événement `link://state`. `seq` est un numéro de séquence strictement croissant PAR SERVEUR,
 * fourni par la liaison : c'est lui (et non l'horloge) qui sert à écarter un événement en retard.
 * `since` et les dates (millisecondes depuis l'époque, `null` si inconnues) ne sont que de
 * l'affichage.
 */
export interface LinkStateEvent {
  serverId: string;
  seq: number;
  state: LinkState;
  since: number;
  lastContactAt: number | null;
  nextRetryAt: number | null;
}

/** Issue d'une opération incertaine, retrouvée au retour du lien (BR-RESIL-010). */
export type OperationOutcome = "done" | "not_executed" | "unknown";

/** Événement `link://operation`. */
export interface OperationEvent {
  opId: string;
  serverId: string;
  outcome: OperationOutcome;
}

export type Unsubscribe = () => void;
