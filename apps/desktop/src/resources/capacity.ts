/**
 * Ce que la machine permet de limiter (HRT-146, BR-RES-004). Le fil parle en octets et en millicœurs : les unités
 * lisibles (Mo, Go, cœurs) ne naissent qu'à l'affichage (`format.ts`).
 */
export interface EngineCapacity {
  /** Processeurs logiques de la machine : le plafond du curseur de processeur. */
  cpus: number;
  /** Mémoire totale de la machine, lue sur la machine et non dans un conteneur : le plafond du curseur de mémoire. */
  memoryBytes: number;
  /** Réserve du système et de Hearth (le plus grand de 1 Go et de 5 % de la mémoire). */
  reserveBytes: number;
  /** Somme des limites de mémoire des services en marche, gérés ou non : un total, jamais un nom. */
  committedMemoryBytes: number;
}

function count(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

/**
 * Lit la réponse de `GET /engine/capacity` (`host.{cpus, memory_bytes, reserve_bytes}` et
 * `committed.memory.{managed_running, unmanaged_running}`). Une réponse incomplète, négative ou sans processeur ni
 * mémoire donne `null` : sans bornes sûres, rien ne se règle et rien ne s'envoie (HRT-146).
 */
export function engineCapacityFromWire(wire: unknown): EngineCapacity | null {
  const root = record(wire);
  const host = record(root?.host);
  const memory = record(record(root?.committed)?.memory);
  const cpus = count(host?.cpus);
  const memoryBytes = count(host?.memory_bytes);
  const reserveBytes = count(host?.reserve_bytes);
  const managed = count(memory?.managed_running);
  const unmanaged = count(memory?.unmanaged_running);
  if (cpus === null || memoryBytes === null || reserveBytes === null) return null;
  if (managed === null || unmanaged === null) return null;
  if (cpus < 1 || memoryBytes < 1) return null;
  return { cpus, memoryBytes, reserveBytes, committedMemoryBytes: managed + unmanaged };
}
