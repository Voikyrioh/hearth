import type {
  DecisionOutcome as BoundDecisionOutcome,
  ServiceOperationEvent as BoundServiceOperationEvent,
  OperationDto,
} from "@/bindings";

/**
 * Les opérations de service d'un serveur (HRT-84, ADR-0045) : types de l'interface et conversions depuis
 * les types générés (`bindings.ts`). Aucune règle ici : la liaison dédoublonne, annonce une issue une seule
 * fois et relit ce que l'agent ne redit pas après une coupure. Une opération ne porte JAMAIS de valeur, de
 * commande, de nom de service ni de chemin du serveur : le nom d'un service vient de la liste des services.
 */

export type OperationKind = OperationDto["kind"];
export type OperationStatus = OperationDto["status"];
export type OperationChoice = Exclude<
  NonNullable<OperationDto["decision"]>["choices"][number],
  "unknown"
>;
export type OperationCause = NonNullable<OperationDto["cause"]>;

/** Une opération telle que l'interface la suit. */
export interface ServiceOperation {
  id: string;
  /** La clé du service touché (stable, opaque). */
  key: string;
  kind: OperationKind;
  status: OperationStatus;
  /** L'étape en cours (code stable), `null` avant la première. */
  step: string | null;
  steps: { step: string; state: "intent" | "done" | "failed" | "undone" | "unknown" }[];
  /** Pourcentage connu ; `null` : inconnu (jamais borné ni inventé). */
  percent: number | null;
  itemsDone: number | null;
  bytesDone: number | null;
  bytesTotal: number | null;
  etaS: number | null;
  /** L'annulation est encore possible (aucun bouton d'annulation dans cette tranche). */
  cancellable: boolean;
  /** La décision attendue : les choix que ce client connaît (un choix inconnu est écarté) et l'échéance (ms). */
  decision: { choices: OperationChoice[]; expiresAt: number | null } | null;
  cause: OperationCause | null;
}

/**
 * Un changement d'opération. `seq` croît strictement par exécution de la coquille : le récepteur écarte tout
 * changement dont `seq` n'est pas supérieur au dernier connu POUR CETTE OPÉRATION (rejeu compris).
 */
export type ServiceOperationEvent =
  | { kind: "upsert"; serverId: string; seq: number; operation: ServiceOperation }
  | { kind: "gone"; serverId: string; seq: number; id: string };

/** Les statuts définitifs : l'opération ne court plus, son issue est annoncée une fois. */
export const FINISHED_STATUSES: readonly OperationStatus[] = [
  "cancelled",
  "rolled_back",
  "succeeded",
  "failed",
  "interrupted",
];

export function isFinished(status: OperationStatus): boolean {
  return FINISHED_STATUSES.includes(status);
}

export function toOperation(dto: OperationDto): ServiceOperation {
  return {
    id: dto.id,
    key: dto.key,
    kind: dto.kind,
    status: dto.status,
    step: dto.step,
    steps: dto.steps.map((step) => ({ step: step.step, state: step.state })),
    percent: dto.percent,
    itemsDone: dto.itemsDone,
    bytesDone: dto.bytesDone,
    bytesTotal: dto.bytesTotal,
    etaS: dto.etaS,
    cancellable: dto.cancellable,
    decision: dto.decision
      ? {
          choices: dto.decision.choices.filter(
            (choice): choice is OperationChoice => choice !== "unknown",
          ),
          expiresAt: dto.decision.expiresAt,
        }
      : null,
    cause: dto.cause,
  };
}

export function toServiceOperationEvent(dto: BoundServiceOperationEvent): ServiceOperationEvent {
  if (dto.change.kind === "gone") {
    return { kind: "gone", serverId: dto.serverId, seq: dto.seq, id: dto.change.id };
  }
  return {
    kind: "upsert",
    serverId: dto.serverId,
    seq: dto.seq,
    operation: toOperation(dto.change.operation),
  };
}

/** Pourquoi l'agent a refusé une décision (liste fermée de codes ; l'interface les met en phrases). */
export type DecisionRefusal =
  | { kind: "not_found" }
  | { kind: "not_waiting" }
  | { kind: "not_offered" }
  | { kind: "not_applicable" }
  | { kind: "expired" }
  | { kind: "too_late" }
  | { kind: "changed" }
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "other" };

/** L'issue d'une décision : prise, refusée, ou résultat inconnu (le lien est tombé avant la réponse, jamais rejouée). */
export type DecisionOutcome =
  | { kind: "done"; operation: ServiceOperation }
  | { kind: "refused"; refusal: DecisionRefusal }
  | { kind: "unknown"; opId: string };

export function toDecisionOutcome(dto: BoundDecisionOutcome): DecisionOutcome {
  switch (dto.kind) {
    case "done":
      return { kind: "done", operation: toOperation(dto.operation) };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused": {
      const refusal = dto.refusal;
      if (refusal.kind === "too_many_attempts") {
        return {
          kind: "refused",
          refusal: { kind: "too_many_attempts", retry_after_s: refusal.retry_after_s },
        };
      }
      return { kind: "refused", refusal: { kind: refusal.kind } };
    }
  }
}
