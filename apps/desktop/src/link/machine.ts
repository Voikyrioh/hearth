import type {
  HistoryEvent as HistoryDto,
  LevelsDto,
  MachineDto,
  MetricsEvent as MetricsDto,
  SampleDto,
  SnapshotEvent as SnapshotDto,
} from "@/bindings";

/**
 * Ce que la liaison dit de la machine d'un serveur (tableau de bord) : types de l'interface et
 * conversions depuis les types générés (`bindings.ts`, où tout nombre peut être `null`). Aucune
 * règle ici, et aucun seuil : les niveaux d'alerte arrivent déjà décidés par `hearth-proto`
 * (BR-DASH-003, 004).
 */

/** Niveau d'alerte d'une mesure (BR-DASH-003) : décidé côté Rust, jamais recalculé ici. */
export type Level = "normal" | "attention" | "critical";

export interface DiskInfo {
  name: string;
  mount: string;
  fs: string | null;
  totalBytes: number;
  removable: boolean;
}

export interface GpuInfo {
  name: string;
  memoryTotalBytes: number | null;
}

/** Identité de la machine : change rarement (BR-DASH-001). */
export interface MachineInfo {
  name: string;
  os: { name: string; version: string | null; kernel: string | null; arch: string };
  cpu: {
    model: string;
    physicalCores: number | null;
    logicalCores: number;
    frequencyMhz: number | null;
  };
  memoryTotalBytes: number;
  disks: DiskInfo[];
  /** Vide sans carte graphique mesurable (BR-DASH-005). */
  gpus: GpuInfo[];
  /** `gpu` : au moins une carte ; `temps` : au moins une sonde (BR-DASH-005, 006). */
  capabilities: { gpu: boolean; temps: boolean };
}

export interface GpuSample {
  name: string;
  /** Une mesure illisible est `null`, jamais un zéro inventé (BR-DASH-007, 008). */
  loadPercent: number | null;
  memoryUsedBytes: number | null;
  memoryTotalBytes: number | null;
  tempC: number | null;
}

/** Une seconde de la vie de la machine. `at` : millisecondes depuis l'époque. */
export interface MachineSample {
  at: number;
  uptimeS: number | null;
  cpu: number;
  cores: number[];
  mem: { usedBytes: number; totalBytes: number };
  disks: { name: string; mount: string; usedBytes: number; totalBytes: number }[];
  net: { upBytesPerS: number; downBytesPerS: number } | null;
  gpus: GpuSample[];
  temps: { label: string; celsius: number }[];
}

/** Niveau de chaque mesure d'un échantillon, aux mêmes positions que ses listes. */
export interface SampleLevels {
  cpu: Level;
  mem: Level;
  disks: Level[];
  gpus: { memory: Level; temp: Level }[];
  temps: Level[];
}

/** Identité, historique (du plus ancien au plus récent) et niveaux du dernier échantillon. */
export interface MachineView {
  serverId: string;
  machine: MachineInfo;
  history: MachineSample[];
  levels: SampleLevels | null;
}

/** Un échantillon en direct avec ses niveaux. */
export interface MachineMetrics {
  serverId: string;
  sample: MachineSample;
  levels: SampleLevels;
}

/** Ce que reçoit un abonné au tableau de bord d'un serveur. */
export type MachineEvent =
  | { kind: "view"; view: MachineView }
  | { kind: "metrics"; metrics: MachineMetrics }
  /** L'heure écoulée avant l'instantané (du plus ancien au plus récent), lue à la connexion. */
  | { kind: "history"; history: MachineSample[] };

function num(value: number | null): number | null {
  return value !== null && Number.isFinite(value) ? value : null;
}

/** Les nombres d'une liste, ou `null` si l'un d'eux est illisible (jamais remplacé par zéro). */
function all(values: readonly (number | null)[]): number[] | null {
  const out: number[] = [];
  for (const value of values) {
    const read = num(value);
    if (read === null) return null;
    out.push(read);
  }
  return out;
}

/**
 * `null` si l'identité est incomplète. La coquille n'envoie jamais de nombre illisible pour ces
 * champs : un `null` ici est un message abîmé, et la vue est écartée plutôt que complétée de zéros.
 */
export function toMachine(dto: MachineDto): MachineInfo | null {
  const memory = num(dto.memoryTotalBytes);
  const disks = all(dto.disks.map((disk) => disk.totalBytes));
  if (memory === null || disks === null) return null;
  return {
    name: dto.name,
    os: dto.os,
    cpu: {
      model: dto.cpu.model,
      physicalCores: dto.cpu.physicalCores,
      logicalCores: dto.cpu.logicalCores,
      frequencyMhz: num(dto.cpu.frequencyMhz),
    },
    memoryTotalBytes: memory,
    disks: dto.disks.map((disk, index) => ({
      name: disk.name,
      mount: disk.mount,
      fs: disk.fs,
      totalBytes: disks[index] ?? 0,
      removable: disk.removable,
    })),
    gpus: dto.gpus.map((gpu) => ({
      name: gpu.name,
      memoryTotalBytes: num(gpu.memoryTotalBytes),
    })),
    capabilities: dto.capabilities,
  };
}

/**
 * `null` si un champ qui n'a pas le droit d'être absent (instant, charge, mémoire, cœurs, disques,
 * sondes) est illisible : l'échantillon est écarté, car ses listes doivent garder les positions de
 * ses niveaux. Un débit illisible est un `net` absent ; une mesure de carte graphique illisible reste
 * `null` (BR-DASH-007, 008).
 */
export function toSample(dto: SampleDto): MachineSample | null {
  const at = num(dto.at);
  const cpu = num(dto.cpu);
  const used = num(dto.mem.usedBytes);
  const total = num(dto.mem.totalBytes);
  const cores = all(dto.cores);
  const diskUsed = all(dto.disks.map((disk) => disk.usedBytes));
  const diskTotal = all(dto.disks.map((disk) => disk.totalBytes));
  const celsius = all(dto.temps.map((temp) => temp.celsius));
  if (
    at === null ||
    cpu === null ||
    used === null ||
    total === null ||
    !cores ||
    !diskUsed ||
    !diskTotal ||
    !celsius
  ) {
    return null;
  }
  const up = dto.net ? num(dto.net.upBytesPerS) : null;
  const down = dto.net ? num(dto.net.downBytesPerS) : null;
  return {
    at,
    uptimeS: num(dto.uptimeS),
    cpu,
    cores,
    mem: { usedBytes: used, totalBytes: total },
    disks: dto.disks.map((disk, index) => ({
      name: disk.name,
      mount: disk.mount,
      usedBytes: diskUsed[index] ?? 0,
      totalBytes: diskTotal[index] ?? 0,
    })),
    net: up !== null && down !== null ? { upBytesPerS: up, downBytesPerS: down } : null,
    gpus: dto.gpus.map((gpu) => ({
      name: gpu.name,
      loadPercent: num(gpu.loadPercent),
      memoryUsedBytes: num(gpu.memoryUsedBytes),
      memoryTotalBytes: num(gpu.memoryTotalBytes),
      tempC: num(gpu.tempC),
    })),
    temps: dto.temps.map((temp, index) => ({ label: temp.label, celsius: celsius[index] ?? 0 })),
  };
}

export function toLevels(dto: LevelsDto): SampleLevels {
  return { cpu: dto.cpu, mem: dto.mem, disks: dto.disks, gpus: dto.gpus, temps: dto.temps };
}

export function toView(dto: SnapshotDto): MachineView | null {
  const machine = toMachine(dto.machine);
  if (!machine) return null;
  return {
    serverId: dto.serverId,
    machine,
    history: dto.history.flatMap((sample) => toSample(sample) ?? []),
    levels: dto.levels ? toLevels(dto.levels) : null,
  };
}

export function toHistory(dto: HistoryDto): MachineSample[] {
  return dto.history.flatMap((sample) => toSample(sample) ?? []);
}

export function toMetrics(dto: MetricsDto): MachineMetrics | null {
  const sample = toSample(dto.sample);
  return sample ? { serverId: dto.serverId, sample, levels: toLevels(dto.levels) } : null;
}
