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

export const SERVER_COLORS: readonly ServerColor[] = [1, 2, 3, 4, 5, 6, 7, 8];

/** Port d'écoute par défaut d'un agent. */
export const DEFAULT_PORT = 7341;

/** Un serveur enregistré dans le client (jamais de secret). */
export interface ServerInfo {
  id: string;
  name: string;
  /** Adresse telle qu'on l'affiche : le port n'apparaît que s'il n'est pas celui par défaut. */
  address: string;
  host: string;
  port: number;
  color: ServerColor;
  /** Rôle à la dernière connexion ; lecture seule tant qu'on ne s'est jamais connecté. */
  role: Role;
  /** Identifiant du dernier compte connecté (vide avant la première connexion). */
  username: string;
  /** Le mot de passe est-il mémorisé dans le coffre de Windows ? */
  remember: boolean;
}

/** Pourquoi les tentatives sont arrêtées alors que l'état affiché est « Hors ligne » (BR-CONN-003, 014). */
export type Blocked = "fingerprint_changed" | "incompatible_agent" | "incompatible_client";

/** Pourquoi un état d'arrêt est affiché (session expirée, accès révoqué). */
export type Reason =
  | "no_session"
  | "expired"
  | "stored_password_refused"
  | "user_disconnected"
  | "revoked";

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
  blocked: Blocked | null;
  reason: Reason | null;
  failedAttempts: number;
}

/** Issue d'une opération incertaine, retrouvée au retour du lien (BR-RESIL-010). */
export type OperationOutcome = "done" | "not_executed" | "unknown";

/** Événement `link://operation`. */
export interface OperationEvent {
  opId: string;
  serverId: string;
  outcome: OperationOutcome;
}

/** Événement `link://fingerprint` : le certificat présenté n'est plus celui qui a été confirmé (BR-CONN-003). */
export interface FingerprintChange {
  serverId: string;
  /** Empreinte mémorisée, en 8 groupes de 4. */
  expected: string;
  /** Empreinte reçue, en 8 groupes de 4. */
  presented: string;
  /** Empreinte reçue, forme complète, à renvoyer pour l'accepter. */
  presentedHex: string;
}

/** Événement `link://notice` : quelque chose que la liaison veut dire sans qu'on le lui demande. */
export interface LinkNotice {
  /** Numéro de l'avis retenu par la coquille (acquittement, dédoublonnage) ; 0 : non retenu. */
  id: number;
  /** Les suivis d'actions étaient illisibles / l'écoute a pris du retard. */
  kind: "operations_lost" | "lagged";
  serverId: string | null;
}

/** Première prise de contact : l'empreinte à faire confirmer. */
export interface ProbeResult {
  /** Forme complète (64 caractères hexadécimaux). */
  fingerprint: string;
  /** 8 groupes de 4 caractères. */
  display: string;
  machineName: string;
  agentVersion: string;
  macAddresses: string[];
}

/**
 * Ce que l'assistant envoie à sa dernière étape : le serveur dont l'empreinte vient d'être
 * confirmée et les identifiants. Le serveur n'est enregistré que si la connexion réussit.
 */
export interface NewServerInput {
  name: string;
  color: ServerColor;
  host: string;
  /** `null` : port par défaut. */
  port: number | null;
  /** Empreinte confirmée par l'utilisateur, forme complète (celle de la dernière sonde). */
  fingerprint: string;
  macAddresses: string[];
  username: string;
  password: string;
  remember: boolean;
}

export interface ServerEdit {
  name: string;
  color: ServerColor;
  host: string;
  port: number | null;
  /** Obligatoire si l'adresse change : l'empreinte relue et confirmée de nouveau (BR-CONN-009). */
  fingerprint: string | null;
}

export type InvalidField = "name" | "address" | "port" | "credentials" | "fingerprint" | "other";

/** Échec d'une commande de liaison : `kind` seulement, l'interface choisit son texte. */
export type LinkFailure =
  | { kind: "unreachable" }
  | { kind: "not_agent" }
  | { kind: "incompatible_agent" }
  | { kind: "incompatible_client" }
  | { kind: "invalid_credentials" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "fingerprint_changed" }
  | { kind: "name_taken" }
  | { kind: "already_exists" }
  | { kind: "invalid_input"; field: InvalidField }
  | { kind: "verification_required" }
  | { kind: "unknown_server" }
  | { kind: "storage" }
  | { kind: "vault" }
  | { kind: "not_connected" }
  | { kind: "tracking_unavailable" }
  | { kind: "tracking_slow" }
  | { kind: "internal" };

/** Erreur d'une commande de liaison, porteuse de l'échec typé. */
export class LinkCommandError extends Error {
  readonly failure: LinkFailure;
  constructor(failure: LinkFailure) {
    super(`liaison : ${failure.kind}`);
    this.name = "LinkCommandError";
    this.failure = failure;
  }
}

export type Unsubscribe = () => void;
