import type { AuditEntryDto, AuditFilterDto, AuditLiveEvent, AuditPageDto } from "@/bindings";

/**
 * Ce que la liaison dit du journal d'activité d'un serveur : types de l'interface et conversions
 * depuis les types générés (`bindings.ts`). Aucune règle ici : le filtrage, la recherche et la
 * pagination sont faits par l'agent, le regroupement des rafales est dans `audit/grouping.ts`.
 * Toute valeur d'une entrée est du TEXTE NON FIABLE (identifiants tentés, noms de poste : saisis
 * par des tiers) : elle ne se rend jamais en HTML (`audit/text.ts`).
 */

export type AuditOutcome = "ok" | "denied" | "failed";
export type AuditOriginKind = "client" | "cli" | "assistant";

/** Types d'action du filtre (la liste fermée de la spec). */
export const AUDIT_KINDS = ["login_ok", "login_denied", "accounts", "update", "denied"] as const;
export type AuditKind = (typeof AUDIT_KINDS)[number];

export const AUDIT_OUTCOMES: readonly AuditOutcome[] = ["ok", "denied", "failed"];

export interface AuditEntry {
  /** Croissant côté agent : ordre et curseur. */
  id: number;
  /** La source, RFC 3339 en UTC (conservée pour l'export, BR-AUDIT-012). */
  at: string;
  /** `at` en millisecondes depuis l'époque ; `NaN` si la date est illisible (l'entrée est gardée). */
  atMs: number;
  account: string | null;
  origin: { kind: AuditOriginKind; name: string | null; addr: string | null; text: string };
  action: string;
  actionLabel: string;
  target: string | null;
  outcome: AuditOutcome;
  reason: string | null;
  /** Entrée de synthèse de l'agent : combien d'AUTRES fois le même événement dans la minute. */
  repeatCount: number;
}

/** Le filtre demandé à la coquille (BR-AUDIT-014, 015, 016). Dates : secondes depuis l'époque. */
export interface AuditFilter {
  accounts: string[];
  kinds: AuditKind[];
  outcomes: AuditOutcome[];
  fromS: number | null;
  toS: number | null;
  text: string;
}

export const EMPTY_AUDIT_FILTER: AuditFilter = {
  accounts: [],
  kinds: [],
  outcomes: [],
  fromS: null,
  toS: null,
  text: "",
};

export interface AuditPage {
  /** De la plus récente à la plus ancienne. */
  events: AuditEntry[];
  /** Curseur de la page suivante ; `null` à la fin. */
  nextBefore: number | null;
}

export interface AuditExportResult {
  /** `false` : l'utilisateur a fermé la boîte de dialogue d'enregistrement. */
  saved: boolean;
  /** Seules les 10 000 entrées les plus récentes du résultat sont dans le fichier. */
  truncated: boolean;
}

export function toAuditEntry(dto: AuditEntryDto): AuditEntry | null {
  if (dto.id === null || !Number.isFinite(dto.id) || dto.id < 1) return null;
  return {
    id: dto.id,
    at: dto.at,
    atMs: Date.parse(dto.at),
    account: dto.account,
    origin: dto.origin,
    action: dto.action,
    actionLabel: dto.actionLabel,
    target: dto.target,
    outcome: dto.outcome,
    reason: dto.reason,
    repeatCount: dto.repeatCount,
  };
}

export function toAuditPage(dto: AuditPageDto): AuditPage {
  return {
    events: dto.events.flatMap((event) => toAuditEntry(event) ?? []),
    nextBefore: dto.nextBefore !== null && Number.isFinite(dto.nextBefore) ? dto.nextBefore : null,
  };
}

export function toAuditFilterDto(filter: AuditFilter): AuditFilterDto {
  return {
    accounts: filter.accounts,
    kinds: filter.kinds,
    outcomes: filter.outcomes,
    fromS: filter.fromS,
    toS: filter.toS,
    text: filter.text === "" ? null : filter.text,
  };
}

export function toAuditLive(dto: AuditLiveEvent): { serverId: string; entry: AuditEntry } | null {
  const entry = toAuditEntry(dto.event);
  return entry ? { serverId: dto.serverId, entry } : null;
}
