import type {
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
  | { kind: "metrics"; metrics: MachineMetrics };

function num(value: number | null): number | null {
  return value !== null && Number.isFinite(value) ? value : null;
}

export function toMachine(dto: MachineDto): MachineInfo | null {
  const memory = num(dto.memoryTotalBytes);
  if (memory === null) return null;
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
    disks: dto.disks.map((disk) => ({
      name: disk.name,
      mount: disk.mount,
      fs: disk.fs,
      totalBytes: num(disk.totalBytes) ?? 0,
      removable: disk.removable,
    })),
    gpus: dto.gpus.map((gpu) => ({
      name: gpu.name,
      memoryTotalBytes: num(gpu.memoryTotalBytes),
    })),
    capabilities: dto.capabilities,
  };
}

/** `null` si ce qui est indispensable (instant, charge, mémoire) est illisible : l'échantillon est écarté. */
export function toSample(dto: SampleDto): MachineSample | null {
  const at = num(dto.at);
  const cpu = num(dto.cpu);
  const used = num(dto.mem.usedBytes);
  const total = num(dto.mem.totalBytes);
  if (at === null || cpu === null || used === null || total === null) return null;
  return {
    at,
    uptimeS: num(dto.uptimeS),
    cpu,
    cores: dto.cores.map((core) => num(core) ?? 0),
    mem: { usedBytes: used, totalBytes: total },
    disks: dto.disks.map((disk) => ({
      name: disk.name,
      mount: disk.mount,
      usedBytes: num(disk.usedBytes) ?? 0,
      totalBytes: num(disk.totalBytes) ?? 0,
    })),
    net: dto.net
      ? {
          upBytesPerS: num(dto.net.upBytesPerS) ?? 0,
          downBytesPerS: num(dto.net.downBytesPerS) ?? 0,
        }
      : null,
    gpus: dto.gpus.map((gpu) => ({
      name: gpu.name,
      loadPercent: num(gpu.loadPercent),
      memoryUsedBytes: num(gpu.memoryUsedBytes),
      memoryTotalBytes: num(gpu.memoryTotalBytes),
      tempC: num(gpu.tempC),
    })),
    temps: dto.temps.map((temp) => ({ label: temp.label, celsius: num(temp.celsius) ?? 0 })),
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

export function toMetrics(dto: MetricsDto): MachineMetrics | null {
  const sample = toSample(dto.sample);
  return sample ? { serverId: dto.serverId, sample, levels: toLevels(dto.levels) } : null;
}
