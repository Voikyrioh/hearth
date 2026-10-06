import type { AuditEntry } from "@/link";

/**
 * Regroupement visuel des rafales de connexions refusées (BR-AUDIT-013, conception technique §7).
 * Pur : aucune E/S. La règle vit ICI et nulle part ailleurs côté interface.
 *
 * Ce que fait l'agent (établi, `hearth-agent` `domain/audit/repeat.rs`, BR-AUDIT-007) : les refus de
 * même CLÉ (compte, action, résultat, cible, raison ; JAMAIS l'adresse ni le poste) d'une même
 * minute sont condensés. Le premier est écrit seul (`repeat_count` 0) ; les suivants sont comptés ;
 * UNE entrée de synthèse est écrite à la fin de la fenêtre : elle EST la dernière occurrence (sa
 * date, son origine) et son `repeat_count` compte déjà cette occurrence. Une fenêtre de N+1 refus
 * donne donc la première entrée (1 tentative) et une synthèse à `repeat_count == N` : N tentatives,
 * pas N + 1. Les N occurrences d'une synthèse peuvent venir d'adresses DIFFÉRENTES : seule celle de
 * la dernière est connue. Enfin l'entrée de débordement (« activité trop variée ») mêle des comptes et
 * des cibles : elle n'entre jamais dans un groupe.
 *
 * Conséquences, pour qu'un groupe « X tentatives refusées en Y min » ne mente jamais :
 * - X est la somme EXACTE des occurrences représentées : `max(1, repeat_count)` par entrée ;
 * - le groupe ne NOMME une adresse (« depuis … ») que si toutes ses entrées sont des occurrences
 *   ordinaires, dont l'adresse est certaine ; dès qu'une synthèse y entre, l'adresse n'est plus
 *   affirmée (le décompte reste exact) ;
 * - une entrée d'un autre type, d'une autre adresse ou d'un autre résultat n'est jamais absorbée :
 *   elle interrompt la rafale ;
 * - une rafale qui touche le bas de la liste alors qu'il en reste à charger n'est PAS regroupée : son
 *   début n'est pas connu, le décompte serait partiel. Elle se regroupe quand la page suivante arrive.
 *
 * Seuils (choix du développeur, la spec les laisse « à définir », à confirmer par le détenteur du
 * produit) : au moins `BURST_MIN_ATTEMPTS` tentatives réparties sur `BURST_WINDOW_MS` au plus.
 */

export const BURST_MIN_ATTEMPTS = 5;
export const BURST_WINDOW_MS = 2 * 60 * 1000;
/** Début de la raison de l'entrée de débordement de l'agent (`Reason::TooVaried`). */
export const OVERFLOW_REASON_PREFIX = "activité trop variée";

export interface Burst {
  /** Clé de l'entrée la plus récente du groupe. */
  key: string;
  /** Adresse commune, `null` quand une synthèse de l'agent y entre (adresse non garantie). */
  addr: string | null;
  /** De la plus récente à la plus ancienne. */
  entries: AuditEntry[];
  /** Tentatives représentées, répétitions condensées par l'agent comprises (somme exacte). */
  attempts: number;
  /** Durée annoncée : de la première à la dernière tentative, arrondie au-dessus, au moins 1. */
  minutes: number;
}

export type Grouped = { kind: "entry"; entry: AuditEntry } | ({ kind: "burst" } & Burst);

function refusedLoginAddr(entry: AuditEntry): string | null {
  if (
    entry.action !== "login" ||
    entry.outcome !== "denied" ||
    entry.origin.kind !== "client" ||
    !entry.origin.addr ||
    (entry.reason ?? "").startsWith(OVERFLOW_REASON_PREFIX)
  ) {
    return null;
  }
  return entry.origin.addr;
}

/** Tentatives qu'une entrée représente : 1, ou le nombre compté par l'agent pour une synthèse. */
export function attemptsOf(entry: AuditEntry): number {
  return Math.max(1, entry.repeatCount);
}

/**
 * Les entrées, de la plus récente à la plus ancienne, regroupées ; l'ordre est conservé.
 * `complete` : plus rien à charger sous la dernière entrée (sinon la rafale du bas n'est pas regroupée).
 */
export function groupBursts(entries: readonly AuditEntry[], complete = true): Grouped[] {
  const out: Grouped[] = [];
  let index = 0;
  while (index < entries.length) {
    const first = entries[index] as AuditEntry;
    const addr = refusedLoginAddr(first);
    if (addr === null) {
      out.push({ kind: "entry", entry: first });
      index += 1;
      continue;
    }
    let end = index;
    while (end < entries.length && refusedLoginAddr(entries[end] as AuditEntry) === addr) end += 1;
    const run = entries.slice(index, end);
    if (end === entries.length && !complete) {
      for (const entry of run) out.push({ kind: "entry", entry });
    } else {
      out.push(...segments(run, addr));
    }
    index = end;
  }
  return out;
}

/** Découpe une suite (récente vers ancienne) en fenêtres ancrées sur la tentative la plus ancienne. */
function segments(run: readonly AuditEntry[], addr: string): Grouped[] {
  const windows: AuditEntry[][] = [];
  let current: AuditEntry[] = [];
  let low = Number.NaN;
  let high = Number.NaN;
  // De la plus ancienne à la plus récente (l'ordre de la liste est celui des identifiants : les
  // dates d'une synthèse, écrite après coup, ne sont pas forcément croissantes).
  for (let i = run.length - 1; i >= 0; i -= 1) {
    const entry = run[i] as AuditEntry;
    const nextLow = Math.min(low, entry.atMs);
    const nextHigh = Math.max(high, entry.atMs);
    const inside =
      current.length > 0 && Number.isFinite(entry.atMs) && nextHigh - nextLow <= BURST_WINDOW_MS;
    if (inside) {
      low = nextLow;
      high = nextHigh;
    } else {
      if (current.length > 0) windows.push(current);
      current = [];
      low = entry.atMs;
      high = entry.atMs;
    }
    current.push(entry);
  }
  if (current.length > 0) windows.push(current);
  const out: Grouped[] = [];
  for (const window of windows.reverse()) {
    const newestFirst = [...window].reverse();
    const attempts = newestFirst.reduce((sum, entry) => sum + attemptsOf(entry), 0);
    if (newestFirst.length >= 2 && attempts >= BURST_MIN_ATTEMPTS) {
      const times = newestFirst.map((entry) => entry.atMs).filter(Number.isFinite);
      const span = times.length > 0 ? Math.max(...times) - Math.min(...times) : 0;
      const certain = newestFirst.every((entry) => entry.repeatCount === 0);
      out.push({
        kind: "burst",
        key: `burst-${(newestFirst[0] as AuditEntry).id}`,
        addr: certain ? addr : null,
        entries: newestFirst,
        attempts,
        minutes: Math.max(1, Math.ceil(span / 60_000)),
      });
    } else {
      for (const entry of newestFirst) out.push({ kind: "entry", entry });
    }
  }
  return out;
}
