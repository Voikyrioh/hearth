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

/** Un point de courbe : `v` est `null` quand la mesure manque sur tout le pas (trou, jamais zéro). */
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

  /** Colle un historique (du plus ancien au plus récent) : il remplace ce qu'il recouvre. */
  merge(history: readonly MachineSample[]): void {
    const incoming: MachineSample[] = [];
    for (const sample of history) {
      const previous = incoming.at(-1);
      if (!previous || sample.at > previous.at) incoming.push(sample);
    }
    const head = incoming[0];
    if (!head) return;
    const older = this.items.filter((sample) => sample.at < head.at);
    this.items = [...older, ...incoming].slice(-RING_CAP);
  }

  clear(): void {
    this.items = [];
  }
}

/**
 * Courbe d'une mesure sur une fenêtre : `spanMs / stepMs` pas, chacun la MOYENNE des valeurs lues
 * dans le pas (`null` s'il n'y en a aucune). La fenêtre se termine au dernier échantillon.
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
  const sums = new Array<number>(count).fill(0);
  const counts = new Array<number>(count).fill(0);
  for (let index = samples.length - 1; index >= 0; index -= 1) {
    const sample = samples[index];
    if (!sample || sample.at <= start) break;
    const value = pick(sample);
    if (value === null) continue;
    const bucket = Math.min(count - 1, Math.floor((sample.at - start - 1) / stepMs));
    sums[bucket] = (sums[bucket] ?? 0) + value;
    counts[bucket] = (counts[bucket] ?? 0) + 1;
  }
  return sums.map((sum, bucket) => {
    const n = counts[bucket] ?? 0;
    return { t: start + (bucket + 1) * stepMs, v: n > 0 ? sum / n : null };
  });
}

/** Durée réellement couverte par l'historique (ms) : sert à dire qu'une fenêtre n'est pas pleine. */
export function coverageMs(samples: readonly MachineSample[]): number {
  const first = samples[0];
  const last = samples.at(-1);
  return first && last ? last.at - first.at : 0;
}

// ── Mesures tracées ──────────────────────────────────────────────────────────────────────────

export function percentOf(used: number, total: number): number | null {
  return total > 0 ? (used / total) * 100 : null;
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
