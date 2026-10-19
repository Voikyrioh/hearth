import type { LifecycleOutcome as BoundLifecycleOutcome } from "@/bindings";
import type { AdminActKind } from "./reauth";
import type { OperationKind } from "./serviceOps";
import type { EngineState, ServiceState } from "./services";

/**
 * Démarrer, arrêter, redémarrer un service (HRT-65) : types de l'interface et conversions depuis les types
 * générés (`bindings.ts`). Aucune règle ici : la liaison construit l'acte, l'agent arbitre. Une réponse `202` ne dit
 * JAMAIS l'issue : elle arrive par le suivi des opérations (HRT-84).
 */

export type LifecycleGesture = "start" | "stop" | "restart";

/** L'acte d'administration de chaque geste : ce que la confirmation lit et ce que l'agent vérifie. */
export const LIFECYCLE_ACT_KINDS: Record<LifecycleGesture, AdminActKind> = {
  start: "service_start",
  stop: "service_stop",
  restart: "service_restart",
};

/** Pourquoi l'agent a refusé le geste (liste fermée de codes ; l'interface les met en phrases). */
export type LifecycleRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "service_not_found" }
  | { kind: "service_busy"; by: string | null; action: OperationKind | null }
  | { kind: "already_in_state"; state: ServiceState | null }
  | { kind: "not_drivable"; state: ServiceState | null }
  | { kind: "other" };

/**
 * L'issue d'un geste : accepté (l'opération court à l'arrière-plan), refusé, ou résultat inconnu (le lien est
 * tombé avant la réponse : jamais rejoué).
 */
export type LifecycleOutcome =
  | { kind: "accepted"; actionId: string; graceS: number }
  | { kind: "refused"; refusal: LifecycleRefusal }
  | { kind: "unknown"; opId: string };

export function toLifecycleOutcome(dto: BoundLifecycleOutcome): LifecycleOutcome {
  switch (dto.kind) {
    case "accepted":
      return { kind: "accepted", actionId: dto.actionId, graceS: dto.graceS };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as LifecycleRefusal };
  }
}
