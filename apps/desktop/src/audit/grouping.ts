import type { AuditEntry } from "@/link";

/**
 * Regroupement visuel des rafales de connexions refusées (BR-AUDIT-013, conception technique §7).
 * Pur : aucune E/S. La règle vit ICI et nulle part ailleurs côté interface ; l'agent, lui, ne
 * fait que condenser les répétitions IDENTIQUES d'une même minute (`repeatCount`), ce qui est
 * autre chose et se lit dans la ligne (« N autres fois »).
 *
 * Rafale : au moins `BURST_MIN_ATTEMPTS` tentatives (une entrée de synthèse compte pour
 * `1 + repeatCount`) de connexion REFUSÉE depuis la même adresse, étalées sur `BURST_WINDOW_MS` au
 * plus, qui se SUIVENT dans la liste : une entrée d'un autre type, d'une autre adresse ou d'un autre
 * résultat interrompt la rafale, donc n'est jamais masquée ni déplacée. Les fenêtres sont ancrées
 * sur la tentative la plus ANCIENNE : une nouvelle tentative en haut de la liste prolonge ou
 * ouvre une rafale sans changer la composition des précédentes.
 */

export const BURST_MIN_ATTEMPTS = 5;
export const BURST_WINDOW_MS = 2 * 60 * 1000;

export interface Burst {
  /** Identifiant stable : celui de la tentative la plus ancienne. */
  key: string;
  /** Adresse d'origine commune. */
  addr: string;
  /** De la plus récente à la plus ancienne. */
  entries: AuditEntry[];
  /** Tentatives, répétitions condensées par l'agent comprises. */
  attempts: number;
  /** Durée annoncée : de la première à la dernière tentative, arrondie au-dessus, au moins 1. */
  minutes: number;
}

export type Grouped = { kind: "entry"; entry: AuditEntry } | ({ kind: "burst" } & Burst);

function refusedLoginAddr(entry: AuditEntry): string | null {
  return entry.action === "login" &&
    entry.outcome === "denied" &&
    entry.origin.kind === "client" &&
    entry.origin.addr
    ? entry.origin.addr
    : null;
}

function attemptsOf(entries: readonly AuditEntry[]): number {
  return entries.reduce((sum, entry) => sum + 1 + entry.repeatCount, 0);
}

/** Les entrées, de la plus récente à la plus ancienne, regroupées. L'ordre est conservé. */
export function groupBursts(entries: readonly AuditEntry[]): Grouped[] {
  const out: Grouped[] = [];
  let index = 0;
  while (index < entries.length) {
    const first = entries[index];
    const addr = first ? refusedLoginAddr(first) : null;
    if (!first || addr === null) {
      if (first) out.push({ kind: "entry", entry: first });
      index += 1;
      continue;
    }
    // La suite ininterrompue de refus de connexion depuis cette adresse.
    let end = index;
    while (end < entries.length && refusedLoginAddr(entries[end] as AuditEntry) === addr) end += 1;
    const run = entries.slice(index, end);
    out.push(...segments(run, addr));
    index = end;
  }
  return out;
}

/** Découpe une suite (récente vers ancienne) en fenêtres ancrées sur la plus ancienne. */
function segments(run: readonly AuditEntry[], addr: string): Grouped[] {
  const windows: AuditEntry[][] = [];
  let current: AuditEntry[] = [];
  let anchor = Number.NaN;
  // De la plus ancienne à la plus récente.
  for (let i = run.length - 1; i >= 0; i -= 1) {
    const entry = run[i] as AuditEntry;
    const inside = current.length > 0 && entry.atMs - anchor <= BURST_WINDOW_MS;
    if (!inside) {
      if (current.length > 0) windows.push(current);
      current = [];
      anchor = entry.atMs;
    }
    current.push(entry);
  }
  if (current.length > 0) windows.push(current);
  const out: Grouped[] = [];
  // Les fenêtres vont de l'ancienne à la récente ; la liste va de la récente à l'ancienne.
  for (const window of windows.reverse()) {
    const newestFirst = [...window].reverse();
    const attempts = attemptsOf(newestFirst);
    if (newestFirst.length >= 2 && attempts >= BURST_MIN_ATTEMPTS) {
      const oldest = newestFirst[newestFirst.length - 1] as AuditEntry;
      const newest = newestFirst[0] as AuditEntry;
      const span = Number.isFinite(newest.atMs - oldest.atMs) ? newest.atMs - oldest.atMs : 0;
      out.push({
        kind: "burst",
        key: `burst-${oldest.id}`,
        addr,
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
