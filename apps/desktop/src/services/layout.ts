import type { ServiceInfo } from "@/link";

/**
 * La mise en page de la liste, calculée depuis les DONNÉES et jamais mesurée dans le DOM (conception
 * design, A2) : la hauteur d'une ligne se déduit de l'état du service (standard, ou plus haute avec sa bande
 * « Redémarre en boucle »). C'est ce qui rend la virtualisation exacte et empêche le défilement de sauter
 * quand une ligne bascule d'un état à l'autre (BR-SVC-015).
 */
export interface RowHeights {
  row: number;
  alert: number;
}

export function heightOf(service: Pick<ServiceInfo, "state">, heights: RowHeights): number {
  return service.state === "restart_loop" ? heights.alert : heights.row;
}

/** `offsets[i]` : le haut de la ligne `i` ; `offsets[n]` : la hauteur totale. */
export function offsetsOf(
  services: readonly Pick<ServiceInfo, "state">[],
  heights: RowHeights,
): number[] {
  const offsets = new Array<number>(services.length + 1);
  let top = 0;
  for (let index = 0; index < services.length; index += 1) {
    offsets[index] = top;
    const service = services[index];
    top += service ? heightOf(service, heights) : heights.row;
  }
  offsets[services.length] = top;
  return offsets;
}

/** L'indice de la ligne qui contient la position `y` (0 si la liste est vide ou `y` est avant elle). */
export function indexAt(offsets: readonly number[], y: number): number {
  const count = offsets.length - 1;
  if (count <= 0 || y <= 0) return 0;
  let low = 0;
  let high = count - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if ((offsets[middle] ?? 0) <= y) low = middle;
    else high = middle - 1;
  }
  return low;
}

/** Les lignes à dessiner : celles de la zone visible et `overscan` voisines de chaque côté (`end` exclu). */
export function windowOf(
  offsets: readonly number[],
  scrollTop: number,
  viewport: number,
  overscan: number,
): { start: number; end: number } {
  const count = offsets.length - 1;
  if (count <= 0) return { start: 0, end: 0 };
  const first = indexAt(offsets, scrollTop);
  const last = indexAt(offsets, scrollTop + Math.max(viewport, 1));
  return { start: Math.max(0, first - overscan), end: Math.min(count, last + 1 + overscan) };
}

/**
 * Le défilement qui garde la même ligne au même endroit quand la liste change (BR-SVC-015) : la ligne qui
 * était en haut de la zone visible y reste, qu'une ligne au-dessus se soit allongée, ajoutée ou retirée.
 * Tout en haut (`scrollTop` à 0), rien ne bouge : l'utilisateur lit le début de la liste. Si la ligne repère
 * a disparu, la position est gardée.
 */
export function anchoredScroll(
  before: { keys: readonly string[]; offsets: readonly number[] },
  after: { keys: readonly string[]; offsets: readonly number[] },
  scrollTop: number,
): number {
  if (scrollTop <= 0 || before.keys.length === 0) return scrollTop;
  const index = indexAt(before.offsets, scrollTop);
  const key = before.keys[index];
  const within = scrollTop - (before.offsets[index] ?? 0);
  const moved = key === undefined ? -1 : after.keys.indexOf(key);
  if (moved < 0) return scrollTop;
  return (after.offsets[moved] ?? 0) + within;
}
