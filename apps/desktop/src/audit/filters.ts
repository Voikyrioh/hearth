import {
  AUDIT_KINDS,
  AUDIT_OUTCOMES,
  type AuditFilter,
  type AuditKind,
  type AuditOutcome,
} from "@/link";

/**
 * Les filtres tels que l'utilisateur les édite (BR-AUDIT-014, 015, 016) et leur résolution en
 * filtre demandé à la coquille. Pur : l'heure est un paramètre, aucune lecture d'horloge. Le
 * filtrage lui-même est fait par l'agent (jamais recopié ici) : cela garantit qu'un filtre ne
 * ment pas, pour la page chargée comme pour les entrées reçues en direct.
 */

export const PERIODS = ["all", "today", "week", "month", "custom"] as const;
export type Period = (typeof PERIODS)[number];

/** Jours d'historique conservés (BR-AUDIT-008). */
export const RETENTION_DAYS = 90;

export interface FilterDraft {
  accounts: string[];
  kinds: AuditKind[];
  outcomes: AuditOutcome[];
  period: Period;
  /** `AAAA-MM-JJ` (champ de date du navigateur) : jour local. */
  fromDate: string;
  toDate: string;
  text: string;
}

export function emptyDraft(): FilterDraft {
  return {
    accounts: [],
    kinds: [],
    outcomes: [],
    period: "all",
    fromDate: "",
    toDate: "",
    text: "",
  };
}

export type PeriodError = "reversed" | "tooOld" | "incomplete" | null;

/**
 * Les jours se comptent au CALENDRIER du fuseau du PC, jamais par addition de 86 400 000 ms : un jour
 * de changement d'heure dure 23 h ou 25 h.
 */

/** Début du jour local qui contient `ms`. */
export function startOfDay(ms: number): number {
  const date = new Date(ms);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** Début du jour local `days` jours après (ou avant, si négatif) celui qui contient `ms`. */
export function startOfDayPlus(ms: number, days: number): number {
  const date = new Date(ms);
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() + days);
  return date.getTime();
}

/** Millisecondes du début du jour local `AAAA-MM-JJ`, `null` si ce n'est pas une date. */
export function parseLocalDay(day: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return null;
  const [year, month, date] = [Number(match[1]), Number(match[2]) - 1, Number(match[3])];
  const value = new Date(year, month, date, 0, 0, 0, 0);
  if (value.getFullYear() !== year || value.getMonth() !== month || value.getDate() !== date) {
    return null;
  }
  return value.getTime();
}

/** Ce qui ne va pas dans la période choisie (BR-AUDIT-015 : messages exacts de la spec). */
export function periodError(draft: FilterDraft, now: number): PeriodError {
  if (draft.period !== "custom") return null;
  const from = draft.fromDate === "" ? null : parseLocalDay(draft.fromDate);
  const to = draft.toDate === "" ? null : parseLocalDay(draft.toDate);
  if ((draft.fromDate !== "" && from === null) || (draft.toDate !== "" && to === null)) {
    return "incomplete";
  }
  const oldest = startOfDayPlus(now, -RETENTION_DAYS);
  if ((from !== null && from < oldest) || (to !== null && to < oldest)) return "tooOld";
  if (from !== null && to !== null && to < from) return "reversed";
  return null;
}

/** Le filtre à demander, ou `null` si la période choisie est invalide. */
export function resolveFilter(draft: FilterDraft, now: number): AuditFilter | null {
  if (periodError(draft, now) !== null) return null;
  let fromS: number | null = null;
  let toS: number | null = null;
  if (draft.period === "today") {
    fromS = Math.floor(startOfDay(now) / 1000);
  } else if (draft.period === "week") {
    // Au jour près : le début du jour d'il y a 7 jours (le filtre ne bouge pas avant minuit).
    fromS = Math.floor(startOfDayPlus(now, -7) / 1000);
  } else if (draft.period === "month") {
    fromS = Math.floor(startOfDayPlus(now, -30) / 1000);
  } else if (draft.period === "custom") {
    const from = draft.fromDate === "" ? null : parseLocalDay(draft.fromDate);
    const to = draft.toDate === "" ? null : parseLocalDay(draft.toDate);
    if (from !== null) fromS = Math.floor(from / 1000);
    // La fin est INCLUSE : jusqu'à la dernière seconde du jour choisi.
    if (to !== null) toS = Math.floor(startOfDayPlus(to, 1) / 1000) - 1;
  }
  return {
    accounts: [...draft.accounts],
    kinds: AUDIT_KINDS.filter((kind) => draft.kinds.includes(kind)),
    outcomes: AUDIT_OUTCOMES.filter((outcome) => draft.outcomes.includes(outcome)),
    fromS,
    toS,
    text: boundedText(draft.text),
  };
}

/** Longueur maximale du texte de recherche envoyé (celle de la coquille). */
export const SEARCH_MAX_CHARS = 200;

/** Le texte de recherche sans espaces de bord, tronqué (par caractères) à la borne de la coquille. */
function boundedText(text: string): string {
  return Array.from(text.trim()).slice(0, SEARCH_MAX_CHARS).join("").trim();
}

/** Vrai quand le filtre ne retient rien de particulier : toute entrée correspond. */
export function isUnfiltered(filter: AuditFilter): boolean {
  return (
    filter.accounts.length === 0 &&
    filter.kinds.length === 0 &&
    filter.outcomes.length === 0 &&
    filter.fromS === null &&
    filter.toS === null &&
    filter.text === ""
  );
}

/** Vrai quand le brouillon contient au moins un filtre ou une recherche. */
export function hasAnyFilter(draft: FilterDraft): boolean {
  return (
    draft.accounts.length > 0 ||
    draft.kinds.length > 0 ||
    draft.outcomes.length > 0 ||
    draft.period !== "all" ||
    draft.text.trim() !== ""
  );
}

/** Deux brouillons disent-ils la même chose ? (le bouton « Appliquer » attend une différence). */
export function sameDraft(a: FilterDraft, b: FilterDraft): boolean {
  const sameSet = (x: readonly string[], y: readonly string[]) =>
    x.length === y.length && x.every((value) => y.includes(value));
  return (
    sameSet(a.accounts, b.accounts) &&
    sameSet(a.kinds, b.kinds) &&
    sameSet(a.outcomes, b.outcomes) &&
    a.period === b.period &&
    (a.period !== "custom" || (a.fromDate === b.fromDate && a.toDate === b.toDate)) &&
    a.text.trim() === b.text.trim()
  );
}

export function cloneDraft(draft: FilterDraft): FilterDraft {
  return {
    ...draft,
    accounts: [...draft.accounts],
    kinds: [...draft.kinds],
    outcomes: [...draft.outcomes],
  };
}
