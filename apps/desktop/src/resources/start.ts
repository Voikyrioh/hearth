import type { EngineCapacity } from "./capacity";
import { GIB, MIB, MIN_MEMORY_BYTES, memoryStops } from "./scale";

/** BR-RES-039 : 2 Go, ou la moitié de la mémoire disponible si elle est plus petite, jamais moins de 256 Mo. */
const PROPOSED_MEMORY_BYTES = 2 * GIB;
const PROPOSED_MEMORY_FLOOR_BYTES = 256 * MIB;

/** La mémoire de la machine encore disponible : totale, moins la réserve, moins ce qui est déjà engagé. */
function availableBytes(capacity: EngineCapacity): number {
  return Math.max(0, capacity.memoryBytes - capacity.reserveBytes - capacity.committedMemoryBytes);
}

function clampToMachine(capacity: EngineCapacity, bytes: number): number {
  const max = memoryStops(capacity.memoryBytes).at(-1) ?? bytes;
  return Math.max(MIN_MEMORY_BYTES, Math.min(Math.floor(bytes / MIB) * MIB, max));
}

/**
 * La mémoire que Hearth propose pour une application qu'il ne connaît pas (BR-RES-039) : jamais plus de 2 Go, jamais
 * plus de la moitié de ce qui reste, jamais moins de 256 Mo, jamais plus que la machine.
 */
export function proposedMemoryBytes(capacity: EngineCapacity): number {
  const half = Math.floor(availableBytes(capacity) / 2);
  const proposed = Math.max(Math.min(PROPOSED_MEMORY_BYTES, half), PROPOSED_MEMORY_FLOOR_BYTES);
  return clampToMachine(capacity, proposed);
}

/** Le processeur proposé : tous les processeurs de la machine (BR-RES-039). */
export function proposedCpuMillis(capacity: EngineCapacity): number {
  return capacity.cpus * 1000;
}

export interface Resources {
  /** Octets ; `null` = la saisie est vide ou refusée, le formulaire ne part pas. */
  memoryBytes: number | null;
  /** Millicœurs ; `null` comme ci-dessus. */
  cpuMillis: number | null;
}

/**
 * Les valeurs de départ des deux curseurs (BR-RES-037 : jamais « sans limite »). Une fiche donne sa mémoire
 * recommandée, ramenée au maximum de la machine si elle le dépasse ; une application sans fiche reçoit
 * la proposition de BR-RES-039.
 */
export function startingResources(
  capacity: EngineCapacity,
  recommendedBytes: number | null,
): Resources {
  return {
    memoryBytes:
      recommendedBytes === null
        ? proposedMemoryBytes(capacity)
        : clampToMachine(capacity, Math.max(recommendedBytes, 0)),
    cpuMillis: proposedCpuMillis(capacity),
  };
}
