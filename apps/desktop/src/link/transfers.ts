import type {
  DepositOutcome as BoundDepositOutcome,
  DepositRefusal as BoundDepositRefusal,
  TransferDto,
  TransferRefusalDto,
} from "@/bindings";
import type { AdminActKind } from "./reauth";

/**
 * Envoyer un fichier dans la zone d'attente de l'agent (HRT-96, HRT-95, `BR-FILES-019`, `BR-MOD-015`) : types de
 * l'interface et conversions depuis les types générés (`bindings.ts`). Aucune règle ici : la coquille lit le fichier
 * local et parle à l'agent, l'agent arbitre. La page ne reçoit JAMAIS le contenu du fichier : un chemin au départ,
 * des nombres ensuite. Le nom du fichier est celui que la page a choisi : il n'est ni journalisé ni écrit.
 */

/** L'acte d'administration du dépôt : couvert par l'élévation de cinq minutes (BR-FILES-023). */
export const DEPOSIT_ACT_KIND: AdminActKind = "file_write";

/** Où en est un transfert. */
export type TransferStatus = "sending" | "interrupted" | "done" | "cancelled" | "failed";

/** Pourquoi un transfert est coupé ou refusé. */
export type TransferRefusal =
  | { kind: "no_space"; missing: number; remaining: number }
  | { kind: "too_many" }
  | { kind: "missing_right" }
  | { kind: "rejected" }
  | { kind: "source_unreadable" }
  | { kind: "source_changed" }
  | { kind: "gone" };

/** Un transfert, tel que le panneau « Transferts » le montre. */
export interface Transfer {
  /** L'identifiant du dépôt sur le serveur : celui que le consommateur du dépôt reçoit. */
  id: string;
  serverId: string;
  name: string;
  size: number;
  /** Les octets reçus ET écrits sur le serveur (jamais en arrière). */
  sent: number;
  status: TransferStatus;
  refusal: TransferRefusal | null;
}

/** Pourquoi l'ouverture du dépôt est refusée. */
export type DepositRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; waitS: number }
  | { kind: "busy" }
  | { kind: "no_space"; missing: number; remaining: number }
  | { kind: "too_many" }
  | { kind: "name_invalid" }
  | { kind: "source_unreadable" }
  | { kind: "other" };

export type DepositOutcome =
  | { kind: "started"; transfer: Transfer }
  | { kind: "refused"; refusal: DepositRefusal }
  /** Le lien est tombé avant la réponse : on ne sait pas si le dépôt est ouvert. Jamais rejoué. */
  | { kind: "unknown" };

/** Un nombre de la coquille : `null` (valeur non finie) se lit zéro, jamais une panne. */
function num(value: number | null): number {
  return value ?? 0;
}

function toRefusal(dto: TransferRefusalDto): TransferRefusal {
  if (dto.kind === "no_space") {
    return { kind: "no_space", missing: num(dto.missing), remaining: num(dto.remaining) };
  }
  return dto;
}

export function toTransfer(dto: TransferDto): Transfer {
  return {
    id: dto.id,
    serverId: dto.serverId,
    name: dto.name,
    size: num(dto.size),
    sent: num(dto.sent),
    status: dto.status,
    refusal: dto.refusal === null ? null : toRefusal(dto.refusal),
  };
}

function toDepositRefusal(dto: BoundDepositRefusal): DepositRefusal {
  if (dto.kind === "too_many_attempts") {
    return { kind: "too_many_attempts", waitS: dto.wait ?? 60 };
  }
  if (dto.kind === "no_space") {
    return { kind: "no_space", missing: num(dto.missing), remaining: num(dto.remaining) };
  }
  return dto;
}

export function toDepositOutcome(dto: BoundDepositOutcome): DepositOutcome {
  switch (dto.kind) {
    case "started":
      return { kind: "started", transfer: toTransfer(dto.transfer) };
    case "refused":
      return { kind: "refused", refusal: toDepositRefusal(dto.refusal) };
    case "unknown":
      return { kind: "unknown" };
  }
}
