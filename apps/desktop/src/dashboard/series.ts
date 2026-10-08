import type { MachineSample } from "@/link";

/**
 * Historique des mesures d'un serveur et sa mise à l'échelle pour les courbes (BR-DASH-010,
 * BR-DASH-011). Pur : sans Vue, sans horloge propre (le « maintenant » d'une courbe est son dernier
 * échantillon : coupée, la courbe ne glisse pas), sans seuil (les niveaux viennent de Rust).
 */

/** Échantillons gardés par serveur : une heure à 1 Hz. La mémoire d'une session de plusieurs jours est bornée. */
export const RING_CAP = 3600;

export type WindowKey = "1m" | "5m" | "1h";

/** Fenêtres de courbe : durée et pas de rééchantillonnage (1 s, 1 s, 10 s). */
export const WINDOWS: Record<WindowKey, { spanMs: number; stepMs: number }> = {
  "1m": { spanMs: 60_000, stepMs: 1000 },
  "5m": { spanMs: 300_000, stepMs: 1000 },
  "1h": { spanMs: 3_600_000, stepMs: 10_000 },
};

export const WINDOW_KEYS: readonly WindowKey[] = ["1m", "5m", "1h"];
export const DEFAULT_WINDOW: WindowKey = "5m";

/** Un point de courbe : `v` est le maximum du pas, `null` quand la mesure manque sur tout le pas (trou, jamais zéro). */
export interface Point {
  t: number;
  v: number | null;
}

/**
 * Anneau d'échantillons trié par instant, sans doublon, borné à [`RING_CAP`]. Un échantillon pas
 * plus récent que le dernier est ignoré (flux rejoué, horloge de l'agent qui recule). Un nouvel
 * instantané (après une reconnexion) REMPLACE ce qu'il recouvre et garde le plus ancien : la
 * courbe reprend sans trou ni déformation (BR-DASH-011).
 */
export class SampleRing {
  private items: MachineSample[] = [];

  get length(): number {
    return this.items.length;
  }

  get last(): MachineSample | undefined {
    return this.items.at(-1);
  }

  get first(): MachineSample | undefined {
    return this.items[0];
  }

  samples(): readonly MachineSample[] {
    return this.items;
  }

  /** Vrai si l'échantillon a été ajouté. */
  push(sample: MachineSample): boolean {
    const last = this.items.at(-1);
    if (last && sample.at <= last.at) return false;
    this.items.push(sample);
    if (this.items.length > RING_CAP) this.items.splice(0, this.items.length - RING_CAP);
    return true;
  }

  /**
   * Colle un historique (du plus ancien au plus récent) : il remplace ce qu'il recouvre, garde ce
   * qui est plus ancien ET ce qui est plus récent (un échantillon du flux arrivé pendant la
   * lecture n'est jamais jeté).
   */
  merge(history: readonly MachineSample[]): void {
    const incoming: MachineSample[] = [];
    for (const sample of history) {
      const previous = incoming.at(-1);
      if (!previous || sample.at > previous.at) incoming.push(sample);
    }
    const head = incoming[0];
    const tail = incoming.at(-1);
    if (!head || !tail) return;
    const older = this.items.filter((sample) => sample.at < head.at);
    const newer = this.items.filter((sample) => sample.at > tail.at);
    this.items = [...older, ...incoming, ...newer].slice(-RING_CAP);
  }

  /**
   * Comble ce que l'anneau n'a pas avec une heure lue à part (moyennes de l'agent, un échantillon par 10 s) :
   * ce qui est plus ancien que son premier échantillon et les trous. Un point n'est gardé que si aucun
   * échantillon de l'anneau n'est à moins de `BRIDGE_MS` de lui : elle ne remplace JAMAIS un point reçu en
   * direct (un pic à 100 % vu à la seconde ne retombe pas à la moyenne de son pas à une reconnexion).
   */
  fill(history: readonly MachineSample[]): void {
    const have = this.items;
    const kept: MachineSample[] = [];
    let cursor = 0;
    for (const sample of history) {
      while (cursor < have.length && (have[cursor]?.at ?? 0) < sample.at - BRIDGE_MS) cursor += 1;
      const near = have[cursor];
      if (near && near.at <= sample.at + BRIDGE_MS) continue;
      const previous = kept.at(-1);
      if (previous && sample.at <= previous.at) continue;
      kept.push(sample);
    }
    if (kept.length === 0) return;
    this.items = [...have, ...kept].sort((a, b) => a.at - b.at).slice(-RING_CAP);
  }

  clear(): void {
    this.items = [];
  }
}

/**
 * Deux échantillons voisins séparés d'au plus cette durée n'ont qu'un pas vide entre eux : un
 * échantillon à 1 Hz dont l'intervalle réel dérive (1,003 s) saute une case de temps de loin en
 * loin, et ses voisins restent à environ 1 s l'un de l'autre. Ce pas-là est comblé (moyenne de ses
 * voisins). Un échantillon RÉELLEMENT perdu laisse ses voisins à environ 2 s : au-delà de ce seuil
 * c'est un vrai trou, et il reste vide (BR-DASH-010, FIX:01M4CRD0HH1YX2RQHBK72GM0VQ).
 */
export const BRIDGE_MS = 1500;

/**
 * Courbe d'une mesure sur une fenêtre : `spanMs / stepMs` pas, chacun le MAXIMUM des valeurs lues
 * dans le pas (`null` s'il n'y en a aucune). La fenêtre se termine au dernier échantillon. Le
 * maximum, pas la moyenne : un pic à 100 % rangé avec un échantillon calme ne doit pas se tracer à
 * 55 % (FIX:01M4CRD1VMNQP4W619XVF1AWRE).
 *
 * L'agent date à la milliseconde réelle : un échantillon est rangé dans le pas le PLUS PROCHE de
 * son âge (arrondi, pas troncature), donc ±quelques ms de gigue ne changent pas de case. Un pas
 * isolé que la dérive de l'horloge a laissé vide entre deux échantillons voisins (`BRIDGE_MS`) est
 * comblé ; un vrai trou reste un trou.
 */
export function resample(
  samples: readonly MachineSample[],
  window: WindowKey,
  pick: (sample: MachineSample) => number | null,
): Point[] {
  const last = samples.at(-1);
  if (!last) return [];
  const { spanMs, stepMs } = WINDOWS[window];
  const count = spanMs / stepMs;
  const start = last.at - spanMs;
  const peaks = new Array<number | null>(count).fill(null);
  const firstAt = new Array<number>(count).fill(Number.POSITIVE_INFINITY);
  const lastAt = new Array<number>(count).fill(Number.NEGATIVE_INFINITY);
  for (let index = samples.length - 1; index >= 0; index -= 1) {
    const sample = samples[index];
    if (!sample || sample.at <= start) break;
    const value = pick(sample);
    if (value === null) continue;
    const bucket = count - 1 - Math.round((last.at - sample.at) / stepMs);
    if (bucket < 0) continue;
    const peak = peaks[bucket];
    peaks[bucket] = peak === null || peak === undefined ? value : Math.max(peak, value);
    firstAt[bucket] = Math.min(firstAt[bucket] ?? sample.at, sample.at);
    lastAt[bucket] = Math.max(lastAt[bucket] ?? sample.at, sample.at);
  }
  return peaks.map((peak, bucket) => {
    let v = peak;
    if (v === null && bucket > 0 && bucket < count - 1) {
      const before = peaks[bucket - 1] ?? null;
      const after = peaks[bucket + 1] ?? null;
      const gap = (firstAt[bucket + 1] ?? Number.POSITIVE_INFINITY) - (lastAt[bucket - 1] ?? 0);
      if (before !== null && after !== null && gap <= BRIDGE_MS) v = (before + after) / 2;
    }
    return { t: start + (bucket + 1) * stepMs, v };
  });
}

/** Écart au-delà duquel deux échantillons ne sont plus « contigus » (coupure, session précédente). */
export const MAX_GAP_MS = 5000;

/**
 * Durée couverte DANS la fenêtre affichée, sur des échantillons contigus : du plus ancien
 * échantillon de la fenêtre qu'aucun trou ne sépare du dernier, jusqu'au dernier. Un trou
 * (session précédente relue du disque, coupure) n'est jamais compté comme couvert.
 */
export function coverageMs(samples: readonly MachineSample[], window: WindowKey): number {
  const last = samples.at(-1);
  if (!last) return 0;
  const start = last.at - WINDOWS[window].spanMs;
  // Le trou toléré suit le pas de la fenêtre (10 s sur 1 h), jamais moins de MAX_GAP_MS.
  const gap = Math.max(MAX_GAP_MS, WINDOWS[window].stepMs * 1.5);
  let first = last.at;
  for (let index = samples.length - 2; index >= 0; index -= 1) {
    const sample = samples[index];
    if (!sample || sample.at <= start || first - sample.at > gap) break;
    first = sample.at;
  }
  return last.at - first;
}

// ── Mesures tracées ──────────────────────────────────────────────────────────────────────────

export function percentOf(used: number, total: number): number | null {
  return total > 0 ? (used * 100) / total : null;
}

export const cpuLoad = (sample: MachineSample): number | null => sample.cpu;

export const memoryPercent = (sample: MachineSample): number | null =>
  percentOf(sample.mem.usedBytes, sample.mem.totalBytes);

/** Occupation du disque le plus plein (c'est lui qui alerte). */
export function fullestDiskPercent(sample: MachineSample): number | null {
  let fullest: number | null = null;
  for (const disk of sample.disks) {
    const percent = percentOf(disk.usedBytes, disk.totalBytes);
    if (percent !== null && (fullest === null || percent > fullest)) fullest = percent;
  }
  return fullest;
}

export const netUp = (sample: MachineSample): number | null => sample.net?.upBytesPerS ?? null;
export const netDown = (sample: MachineSample): number | null => sample.net?.downBytesPerS ?? null;

export const gpuLoad =
  (index: number) =>
  (sample: MachineSample): number | null =>
    sample.gpus[index]?.loadPercent ?? null;

export const gpuMemoryPercent =
  (index: number) =>
  (sample: MachineSample): number | null => {
    const gpu = sample.gpus[index];
    return gpu?.memoryUsedBytes != null && gpu.memoryTotalBytes != null
      ? percentOf(gpu.memoryUsedBytes, gpu.memoryTotalBytes)
      : null;
  };

/** La température la plus haute lue (sondes et cartes graphiques). */
export function hottest(sample: MachineSample): number | null {
  let max: number | null = null;
  for (const temp of sample.temps) if (max === null || temp.celsius > max) max = temp.celsius;
  for (const gpu of sample.gpus) {
    if (gpu.tempC !== null && (max === null || gpu.tempC > max)) max = gpu.tempC;
  }
  return max;
}
