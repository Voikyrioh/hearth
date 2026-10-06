import type { AgentUpdateOutcome as BoundOutcome } from "@/bindings";

/**
 * Ce que la liaison dit de la mise à jour de l'AGENT d'un serveur (HRT-17) : types de l'interface et
 * conversions depuis les types générés (`bindings.ts`). Aucune règle ici : la version « disponible »
 * est décidée par la coquille (jamais de rétrogradation), la cible (adresse, signature, somme) ne
 * sort jamais de la coquille, le rôle est jugé par l'agent. L'interface ne connaît que des CODES
 * (étapes, issues, raisons) et choisit ses textes.
 */

/** Les étapes visibles d'une mise à jour (BR-UPDATE-013), dans l'ordre ; `done` la clôt. */
export type UpdateStep = "download" | "verify" | "install" | "restart" | "check" | "done";

/** Les étapes affichées, dans l'ordre (hors `done`). */
export const UPDATE_STEPS: readonly Exclude<UpdateStep, "done">[] = [
  "download",
  "verify",
  "install",
  "restart",
  "check",
];

export type UpdateOutcome = "succeeded" | "rolled_back" | "failed";

/** Pourquoi une mise à jour n'a pas abouti ; `unknown` : une raison que ce client ne connaît pas. */
export type UpdateReason =
  | "unreachable"
  | "download_failed"
  | "bad_checksum"
  | "bad_signature"
  | "bad_binary"
  | "staging"
  | "swap"
  | "supervisor_launch"
  | "no_answer"
  | "identity_changed"
  | "interrupted"
  | "rollback_failed"
  | "unknown";

/** Où en est la mise à jour en cours. */
export interface UpdateProgress {
  version: string;
  step: UpdateStep;
  /** 0 à 100, pour l'étape `download` seulement. */
  percent: number | null;
  outcome: UpdateOutcome | null;
  reason: UpdateReason | null;
}

/** Le dernier résultat connu (il survit au redémarrage de l'agent). */
export interface UpdateResult {
  /** `null` : la version visée ne se sait pas (le texte n'a alors pas de numéro). */
  version: string | null;
  previous: string;
  outcome: UpdateOutcome;
  reason: UpdateReason | null;
  /** RFC 3339, UTC. */
  at: string;
  /** Moins de 24 h (décidé par la coquille) : annoncé comme un message ; sinon, une ligne d'historique. */
  recent: boolean;
  /** Déjà annoncé à l'utilisateur (la coquille note la date, même après un redémarrage du client). */
  announced: boolean;
}

/** L'état de la mise à jour de l'agent d'un serveur. */
export interface AgentUpdateView {
  /** La version de l'agent qui répond (BR-UPDATE-022). */
  current: string;
  /** Installation gérée par le système : pas de mise à jour à distance. */
  managed: boolean;
  inProgress: boolean;
  progress: UpdateProgress | null;
  last: UpdateResult | null;
  /** La version disponible, strictement plus récente que `current` ; `null` sinon. */
  available: { version: string } | null;
}

/** Une progression reçue du flux d'un serveur (`agent-update://progress`). */
export interface AgentUpdateEvent {
  serverId: string;
  progress: UpdateProgress;
}

/** Pourquoi la demande n'a pas été acceptée (le refus de rôle de l'agent est l'échec `forbidden`). */
export type AgentUpdateRefusal =
  | { kind: "managed_install" }
  | { kind: "in_progress" }
  | { kind: "bad_signature" }
  | { kind: "invalid_target" }
  | { kind: "no_target" }
  | { kind: "target_changed" }
  | { kind: "not_newer" }
  | { kind: "other" };

/**
 * Issue de « Mettre à jour l'agent » : acceptée (elle s'exécute chez l'agent, l'avancement arrive par
 * le flux), refusée, ou lien coupé avant la réponse (`unknown`, BR-RESIL-009) : jamais rejouée,
 * l'issue arrive par `link://operation` et l'état se relit au retour du lien.
 */
export type AgentUpdateOutcome =
  | { kind: "accepted"; version: string }
  | { kind: "refused"; refusal: AgentUpdateRefusal }
  | { kind: "unknown"; opId: string };

/** L'issue d'une demande, avec les noms de champs de l'interface. */
export function toAgentUpdateOutcome(dto: BoundOutcome): AgentUpdateOutcome {
  switch (dto.kind) {
    case "accepted":
    case "refused":
      return dto;
    case "unknown":
      return { kind: "unknown", opId: dto.op_id };
  }
}
