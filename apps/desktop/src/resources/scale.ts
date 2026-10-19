import type { EngineCapacity } from "./capacity";

/** Puissances de 1 024, comme le système (BR-RES-004). */
export const MIB = 1024 * 1024;
export const GIB = 1024 * MIB;

/** Plancher des deux limites (BR-RES-004) : 128 Mo, 0,25 cœur. */
export const MIN_MEMORY_BYTES = 128 * MIB;
export const MIN_CPU_MILLIS = 250;
/** Pas du processeur (BR-RES-004) : 0,25 cœur, pour le curseur comme pour la saisie. */
export const CPU_STEP_MILLIS = 250;

/** La mémoire la plus haute permise : la mémoire totale arrondie au Mo inférieur, jamais sous le plancher. */
export function maxMemoryBytes(totalBytes: number): number {
  return Math.max(Math.floor(totalBytes / MIB) * MIB, MIN_MEMORY_BYTES);
}

/**
 * Les valeurs que le curseur de mémoire parcourt (BR-RES-004) : de 128 Mo à 2 Go par 128 Mo, jusqu'à 8 Go par
 * 256 Mo, au-delà par 1 Go jusqu'à la mémoire totale (qui est toujours le dernier cran, même si elle n'est pas
 * ronde). Le curseur est discret : une liste de valeurs permises, pas un pas uniforme.
 */
export function memoryStops(totalBytes: number): number[] {
  const maxMib = maxMemoryBytes(totalBytes) / MIB;
  const stops: number[] = [];
  for (let mib = MIN_MEMORY_BYTES / MIB; mib <= maxMib; ) {
    stops.push(mib * MIB);
    mib += mib < 2048 ? 128 : mib < 8192 ? 256 : 1024;
  }
  if ((stops.at(-1) ?? 0) < maxMib * MIB) stops.push(maxMib * MIB);
  return stops;
}

/** Le curseur de processeur : de 0,25 cœur au nombre de processeurs de la machine, par pas de 0,25 (en millicœurs). */
export function cpuStops(cpus: number): number[] {
  const stops: number[] = [];
  for (let millis = MIN_CPU_MILLIS; millis <= cpus * 1000; millis += CPU_STEP_MILLIS) {
    stops.push(millis);
  }
  return stops;
}

/** Le cran le plus proche d'une valeur (le curseur se place sur la valeur permise la plus proche). */
export function nearestStopIndex(stops: readonly number[], value: number): number {
  let best = 0;
  for (let i = 1; i < stops.length; i++) {
    if (Math.abs((stops[i] ?? 0) - value) < Math.abs((stops[best] ?? 0) - value)) best = i;
  }
  return best;
}

/** Une mémoire est valable si c'est un nombre entier de Mo entre le plancher et la mémoire totale. */
export function memoryInRange(capacity: EngineCapacity, bytes: number | null): boolean {
  if (bytes === null || !Number.isInteger(bytes) || bytes % MIB !== 0) return false;
  return bytes >= MIN_MEMORY_BYTES && bytes <= maxMemoryBytes(capacity.memoryBytes);
}

/** Un processeur est valable s'il est un multiple de 0,25 cœur entre le plancher et le nombre de processeurs. */
export function cpuInRange(capacity: EngineCapacity, millis: number | null): boolean {
  if (millis === null || !Number.isInteger(millis) || millis % CPU_STEP_MILLIS !== 0) return false;
  return millis >= MIN_CPU_MILLIS && millis <= capacity.cpus * 1000;
}
