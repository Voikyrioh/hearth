import type {
  AttackModeDto,
  AttackModeOutcome as BoundOutcome,
  SecurityRead as BoundRead,
  AttackModeRefusal as BoundRefusal,
  SecurityEvent,
} from "@/bindings";

/**
 * Sécurité d'un serveur (HRT-26, ADR-0025) : l'alerte « attaque probable », le mode attaque, et ce que
 * l'agent dit de la session de ce poste. L'interface ne reçoit que des états, des dates, des
 * compteurs et des booléens : JAMAIS une clé (privée ou publique), une empreinte de clé, un défi, une
 * signature, un jeton, ni le NOM d'un autre compte visé (l'agent n'en donne que le nombre). La clé de
 * ce PC reste dans la coquille Rust, la WebView n'a même pas de commande qui la nomme.
 */

/** L'alerte « attaque probable » (BR-TRUST-008). */
export interface SecurityAlert {
  /** L'identifiant du compte connecté est visé en ce moment. */
  own: boolean;
  /** Début de l'épisode (RFC 3339, UTC), seulement quand `own`. */
  since: string | null;
  /** Administrateur seulement : combien d'AUTRES comptes sont visés. Jamais leurs noms. */
  others: number | null;
}

export type AttackModeState = "off" | "active" | "suspended";

/** Le mode attaque du serveur. */
export interface AttackMode {
  state: AttackModeState;
  since: string | null;
  /** Seulement « suspendu » : secondes avant la reprise. */
  resumesInS: number | null;
  /** Comment le dernier mode s'est terminé : à la main, tout seul, ou par la commande du serveur. */
  lastEnd: "manual" | "auto" | "cli" | null;
}

/**
 * Ce que l'agent dit de la session de ce poste : prouvée par la clé d'un poste inscrit (`proven`),
 * ou non (`none`). `unknown` : pas encore lu (le message du flux ne le porte pas).
 */
export type SecurityDevice = "proven" | "none" | "unknown";

/** L'état de sécurité d'un serveur. `seq` croît strictement par serveur. */
export interface SecurityState {
  serverId: string;
  seq: number;
  alert: SecurityAlert;
  attackMode: AttackMode;
  device: SecurityDevice;
  /** Ce PC garde une clé d'appareil pour ce serveur (un booléen : la clé elle-même ne sort pas). */
  keyAtHand: boolean;
}

/** La lecture, ou « l'agent ne connaît pas cette fonction » (agent d'avant l'alerte et le mode attaque). */
export type SecurityRead = { kind: "known"; state: SecurityState } | { kind: "unsupported" };

/** Pourquoi une activation ou une désactivation est refusée : le message est choisi d'après `kind`. */
export type AttackModeRefusal = BoundRefusal;

/**
 * Issue d'une activation ou d'une désactivation : fait, refusé, ou lien coupé avant la réponse
 * (`unknown`, BR-RESIL-009) : jamais rejoué, l'issue arrive par `link://operation` et l'état se relit
 * au retour du lien. Le rôle insuffisant (`forbidden`) et le poste non reconnu (`not_recognized`) sont
 * des échecs typés du pont, pas des refus.
 */
export type AttackModeOutcome =
  | { kind: "done"; attackMode: AttackMode }
  | { kind: "refused"; refusal: AttackModeRefusal }
  | { kind: "unknown"; opId: string };

function toAttackMode(dto: AttackModeDto): AttackMode {
  return { ...dto };
}

export function toSecurityState(dto: SecurityEvent): SecurityState {
  return {
    serverId: dto.serverId,
    seq: dto.seq,
    alert: { ...dto.alert },
    attackMode: toAttackMode(dto.attackMode),
    device: dto.device,
    keyAtHand: dto.keyAtHand,
  };
}

export function toSecurityRead(dto: BoundRead): SecurityRead {
  if (dto.kind === "unsupported") return dto;
  return { kind: "known", state: toSecurityState(dto.snapshot) };
}

export function toAttackModeOutcome(dto: BoundOutcome): AttackModeOutcome {
  switch (dto.kind) {
    case "done":
      return { kind: "done", attackMode: toAttackMode(dto.attack_mode) };
    case "refused":
      return dto;
    case "unknown":
      return { kind: "unknown", opId: dto.op_id };
  }
}

/** Y a-t-il une alerte à montrer à ce compte : son identifiant est visé, ou (administrateur) d'autres. */
export function alertVisible(alert: SecurityAlert): boolean {
  return alert.own || (alert.others ?? 0) > 0;
}

/** Le mode attaque est-il en vigueur ou suspendu (le bandeau permanent s'affiche dans les deux cas) ? */
export function modeOn(mode: AttackMode): boolean {
  return mode.state !== "off";
}
