import type { FicheDetail } from "@/link";
import {
  CPU_STEP_MILLIS,
  type EngineCapacity,
  MIN_CPU_MILLIS,
  type Resources,
  startingResources,
} from "@/resources";
import { isMeasured, type ProofKind, proofOf, proven } from "./proof";

/**
 * Ce que la machine peut faire d'une fiche (HRT-102, BR-FICHE-020 à 023), des fonctions pures sans E/S : la machine est-
 * elle assez grande, y a-t-il de la place, où placer les curseurs. Seul un minimum MESURÉ bloque (BR-FICHE-021) ; le
 * chiffre de l'éditeur avertit sans bloquer ; une valeur non vérifiée n'a aucun nombre et le dit. Le serveur refait
 * chaque jugement au lancement (`HOST_TOO_SMALL`, `NO_SPACE`) : ceci dit d'avance, honnêtement, ce qu'il refusera.
 * Chaque fonction rend des CODES et des nombres ; les phrases sont dans `fiche/messages.ts`.
 */

export type Lack = "cpu" | "memory";

export type MinimumVerdict =
  /** Le minimum MESURÉ dépasse la machine : « Installer » est désactivé, aucune case ne passe outre. */
  | {
      kind: "too_small";
      lack: Lack;
      needed: number;
      has: number;
      proof: "measured" | "measured_idle";
    }
  /** Le chiffre de l'éditeur dépasse la machine : rien n'est bloqué, la case « Je comprends » est demandée. */
  | { kind: "publisher_above"; lack: Lack; needed: number; has: number }
  /** Aucune preuve : Hearth ne peut pas dire si la machine est assez grande. */
  | { kind: "unproven"; lack: Lack };

function minimumOf(detail: FicheDetail, lack: Lack, has: number | null): MinimumVerdict | null {
  const resource = lack === "memory" ? detail.resources.memory : detail.resources.cpu;
  const needed =
    lack === "memory" ? detail.resources.memory.minBytes : detail.resources.cpu.minMillis;
  const proof = proofOf(resource.evidence);
  if (proof === "unmeasured") return { kind: "unproven", lack };
  if (needed === null || has === null || needed <= has) return null;
  if (proof === "publisher") return { kind: "publisher_above", lack, needed, has };
  return { kind: "too_small", lack, needed, has, proof };
}

/** Les constats sur les minimums de la fiche, la mémoire d'abord. */
export function minimumVerdicts(
  detail: FicheDetail,
  capacity: EngineCapacity | null,
): MinimumVerdict[] {
  const found: MinimumVerdict[] = [];
  const memory = minimumOf(detail, "memory", capacity ? capacity.memoryBytes : null);
  if (memory) found.push(memory);
  const cpu = minimumOf(detail, "cpu", capacity ? capacity.cpus * 1000 : null);
  if (cpu) found.push(cpu);
  return found;
}

/** La machine est trop petite pour un minimum MESURÉ : l'installation est bloquée (BR-FICHE-021). */
export function tooSmall(verdicts: readonly MinimumVerdict[]): MinimumVerdict | null {
  return verdicts.find((verdict) => verdict.kind === "too_small") ?? null;
}

/** Le minimum annoncé par l'éditeur dépasse la machine : la case « Je comprends » est demandée. */
export function publisherAbove(verdicts: readonly MinimumVerdict[]): MinimumVerdict[] {
  return verdicts.filter((verdict) => verdict.kind === "publisher_above");
}

export type Coverage = { lack: Lack; state: "covers" | "below_recommended" };

/**
 * La machine couvre-t-elle la valeur RECOMMANDÉE ? Seulement quand cette valeur est prouvée (écart 17.13) : sinon
 * aucun verdict, jamais une phrase sur un chiffre qu'on n'a pas.
 */
export function coverage(detail: FicheDetail, capacity: EngineCapacity | null): Coverage[] {
  if (capacity === null) return [];
  const out: Coverage[] = [];
  const memory = proven(detail.resources.memory.recommendedBytes, detail.resources.memory.evidence);
  if (memory !== null) {
    out.push({
      lack: "memory",
      state: capacity.memoryBytes >= memory ? "covers" : "below_recommended",
    });
  }
  const cpu = proven(detail.resources.cpu.recommendedMillis, detail.resources.cpu.evidence);
  if (cpu !== null) {
    out.push({ lack: "cpu", state: capacity.cpus * 1000 >= cpu ? "covers" : "below_recommended" });
  }
  return out;
}

export type DiskVerdict =
  | { kind: "unknown" }
  | { kind: "ok" }
  /** Le besoin MESURÉ dépasse la place libre : refus, aucune case (BR-FICHE-023). */
  | { kind: "short_measured"; needed: number; free: number; missing: number }
  /** Le besoin annoncé par l'éditeur dépasse la place libre : avertissement et case « Continuer malgré le manque de place ». */
  | { kind: "short_publisher"; needed: number; free: number };

/** La place pour installer, d'après l'espace libre que le serveur a dit (`null` : il ne l'a pas dit). */
export function diskVerdict(detail: FicheDetail, freeBytes: number | null): DiskVerdict {
  const disk = detail.resources.disk;
  const needed = proven(disk.installBytes, disk.evidence);
  if (needed === null || freeBytes === null) return { kind: "unknown" };
  if (needed <= freeBytes) return { kind: "ok" };
  return isMeasured(disk.evidence)
    ? { kind: "short_measured", needed, free: freeBytes, missing: needed - freeBytes }
    : { kind: "short_publisher", needed, free: freeBytes };
}

export interface Placement {
  /** Les valeurs de départ des deux curseurs. */
  resources: Resources;
  memory: {
    /** La mémoire recommandée PROUVÉE, `null` sans preuve. */
    recommended: number | null;
    proof: ProofKind;
    /** La recommandation dépasse la machine : le curseur est ramené à son maximum. */
    clamped: boolean;
  };
  cpu: { recommended: number | null; proof: ProofKind };
}

/**
 * Où placer les curseurs (BR-FICHE-020) : la mémoire sur la valeur recommandée de la fiche (ramenée au maximum de la
 * machine si elle le dépasse) ; le processeur sur la valeur recommandée quand elle est prouvée, sinon sur tous les
 * processeurs du serveur. Une valeur sans preuve n'est jamais utilisée.
 */
export function placement(detail: FicheDetail, capacity: EngineCapacity): Placement {
  const memoryRecommended = proven(
    detail.resources.memory.recommendedBytes,
    detail.resources.memory.evidence,
  );
  const cpuRecommended = proven(
    detail.resources.cpu.recommendedMillis,
    detail.resources.cpu.evidence,
  );
  const start = startingResources(capacity, memoryRecommended);
  const memoryBytes =
    memoryRecommended === null ? start.memoryBytes : (start.memoryBytes ?? memoryRecommended);
  const cpuMillis =
    cpuRecommended === null
      ? start.cpuMillis
      : Math.min(
          capacity.cpus * 1000,
          Math.max(MIN_CPU_MILLIS, Math.ceil(cpuRecommended / CPU_STEP_MILLIS) * CPU_STEP_MILLIS),
        );
  return {
    resources: { memoryBytes, cpuMillis },
    memory: {
      recommended: memoryRecommended,
      proof: proofOf(detail.resources.memory.evidence),
      clamped:
        memoryRecommended !== null &&
        memoryBytes !== null &&
        memoryBytes < Math.floor(memoryRecommended / (1024 * 1024)) * 1024 * 1024,
    },
    cpu: { recommended: cpuRecommended, proof: proofOf(detail.resources.cpu.evidence) },
  };
}

/** La mémoire encore disponible : totale, moins la réserve, moins les limites des services en marche (BR-FICHE-022). */
export function availableMemory(capacity: EngineCapacity): number {
  return Math.max(0, capacity.memoryBytes - capacity.reserveBytes - capacity.committedMemoryBytes);
}

export type MemoryNotice =
  /** Ce qui reste est sous la recommandation (ou, sans recommandation prouvée, la phrase sans chiffre de fiche). */
  | { kind: "remaining"; available: number; recommended: number | null }
  /** La limite choisie dépasse ce qui reste : la case « Je comprends » est demandée dans la fenêtre. */
  | { kind: "over_committed"; chosen: number; available: number }
  /** La limite choisie est sous le minimum de la fiche. */
  | { kind: "below_minimum"; chosen: number; minimum: number };

/** Les avertissements chiffrés de l'étape « Ressources », en direct (BR-FICHE-022). */
export function memoryNotices(
  detail: FicheDetail,
  capacity: EngineCapacity,
  chosen: number | null,
): MemoryNotice[] {
  const notices: MemoryNotice[] = [];
  const available = availableMemory(capacity);
  const recommended = proven(
    detail.resources.memory.recommendedBytes,
    detail.resources.memory.evidence,
  );
  // La machine est assez grande, mais ce qui reste l'est moins que la recommandation.
  if (recommended === null || (capacity.memoryBytes >= recommended && available < recommended)) {
    notices.push({ kind: "remaining", available, recommended });
  }
  if (chosen !== null) {
    if (chosen > available) notices.push({ kind: "over_committed", chosen, available });
    const minimum = proven(detail.resources.memory.minBytes, detail.resources.memory.evidence);
    if (minimum !== null && chosen < minimum) {
      notices.push({ kind: "below_minimum", chosen, minimum });
    }
  }
  return notices;
}
