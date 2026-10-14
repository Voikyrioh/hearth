import type { Entry } from "./buffer";

/**
 * La recherche dans ce qui est chargé (HRT-67) : insensible à la casse, dans le texte des lignes seulement, bornée.
 * Rien n'est interprété : la requête est un texte, jamais un motif.
 */

/** Résultats gardés au plus (le compteur dit « 1 000 + »). */
export const MAX_MATCHES = 1_000;
/** Longueur maximale de la requête. */
export const MAX_QUERY = 200;

/** Les indices (dans le tampon) des lignes qui contiennent la requête, de la plus ancienne à la plus récente. */
export function findMatches(
  entries: readonly Entry[],
  query: string,
): { indices: number[]; more: boolean } {
  const needle = query.trim().slice(0, MAX_QUERY).toLowerCase();
  if (needle === "") return { indices: [], more: false };
  const indices: number[] = [];
  for (let i = 0; i < entries.length; i += 1) {
    const entry = entries[i];
    if (entry?.kind !== "line") continue;
    if (entry.text.toLowerCase().includes(needle)) {
      if (indices.length >= MAX_MATCHES) return { indices, more: true };
      indices.push(i);
    }
  }
  return { indices, more: false };
}

export interface Segment {
  text: string;
  hit: boolean;
}

/** Le texte découpé en morceaux, ceux qui contiennent la requête marqués (cinquante au plus par ligne). */
export function segmentsOf(text: string, query: string): Segment[] {
  const needle = query.trim().slice(0, MAX_QUERY).toLowerCase();
  if (needle === "") return [{ text, hit: false }];
  const lower = text.toLowerCase();
  // Une casse qui change la longueur (rare) décalerait les indices : on ne marque rien plutôt que de mal marquer.
  if (lower.length !== text.length) return [{ text, hit: false }];
  const out: Segment[] = [];
  let at = 0;
  let marked = 0;
  while (at < text.length && marked < 50) {
    const found = lower.indexOf(needle, at);
    if (found < 0) break;
    if (found > at) out.push({ text: text.slice(at, found), hit: false });
    out.push({ text: text.slice(found, found + needle.length), hit: true });
    at = found + needle.length;
    marked += 1;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}
