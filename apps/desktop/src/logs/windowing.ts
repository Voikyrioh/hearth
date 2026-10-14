import type { Entry } from "./buffer";

/**
 * Le rendu par FENÊTRE d'une longue liste de hauteurs variables (HRT-67, BR-SLOG-007) : seules les lignes visibles,
 * plus une marge, sont dans le DOM (une soixantaine), quel que soit le nombre de lignes gardées. Les hauteurs sont
 * ESTIMÉES depuis la largeur d'un caractère (police à chasse fixe, coupure à n'importe quel caractère), puis CORRIGÉES
 * par la mesure des lignes réellement dessinées. Pur : aucune E/S.
 */

/** Marge dessinée de chaque côté de la zone visible, en lignes de texte. */
export const OVERSCAN_LINES = 20;
/** Distance du bas en dessous de laquelle la vue est « en bas » (BR-SLOG-005). */
export const BOTTOM_PX = 24;

export interface Metrics {
  /** Hauteur d'une ligne de texte. */
  lineHeight: number;
  /** Nombre de caractères par ligne dessinée. */
  columns: number;
  /** Marge verticale d'une entrée (haut et bas). */
  padding: number;
  /** Hauteur d'un repère (redémarrage, trou). */
  marker: number;
}

/** Nombre de lignes dessinées pour un texte de cette longueur. */
export function linesFor(length: number, columns: number): number {
  return Math.max(1, Math.ceil(length / Math.max(1, columns)));
}

/**
 * Hauteur estimée d'une entrée. `shown` : nombre de caractères réellement dessinés (ligne repliée) ; `extra` : lignes
 * de texte en plus (suffixe « ligne tronquée », bouton de repli).
 */
export function estimateHeight(entry: Entry, metrics: Metrics, shown: number, extra = 0): number {
  if (entry.kind !== "line") return metrics.marker;
  return (linesFor(shown, metrics.columns) + extra) * metrics.lineHeight + metrics.padding * 2;
}

/** Positions du haut de chaque entrée et hauteur totale. */
export function layout(heights: readonly number[]): { tops: number[]; total: number } {
  const tops = new Array<number>(heights.length);
  let at = 0;
  for (let i = 0; i < heights.length; i += 1) {
    tops[i] = at;
    at += heights[i] ?? 0;
  }
  return { tops, total: at };
}

/** Première entrée dont le bas dépasse `y`. */
export function indexAt(tops: readonly number[], heights: readonly number[], y: number): number {
  let low = 0;
  let high = tops.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if ((tops[mid] ?? 0) + (heights[mid] ?? 0) <= y) low = mid + 1;
    else high = mid;
  }
  return low;
}

/** Les entrées à dessiner : [premier, dernier exclu[. */
export function visibleRange(
  tops: readonly number[],
  heights: readonly number[],
  scrollTop: number,
  viewport: number,
  metrics: Metrics,
): [number, number] {
  if (tops.length === 0) return [0, 0];
  const margin = OVERSCAN_LINES * metrics.lineHeight;
  const first = indexAt(tops, heights, Math.max(0, scrollTop - margin));
  const last = indexAt(tops, heights, scrollTop + viewport + margin);
  return [Math.min(first, tops.length), Math.min(tops.length, last + 1)];
}

/** La vue est-elle « en bas » ? */
export function atBottom(scrollTop: number, viewport: number, total: number): boolean {
  return total - scrollTop - viewport <= BOTTOM_PX;
}
