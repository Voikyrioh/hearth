import type {
  Level,
  MachineEvent,
  MachineInfo,
  MachineMetrics,
  MachineSample,
  MachineView,
  SampleLevels,
} from "./machine";
import type { Unsubscribe } from "./types";

const GIB = 1024 ** 3;
const HISTORY_CAP = 300;

/** Mesure qu'un test ou le panneau de développement peut pousser dans un niveau d'alerte. */
export type Pinnable = "cpu" | "mem" | "disk" | "gpuMem" | "gpuTemp" | "temp";

/**
 * Valeur représentative de chaque niveau, par mesure. Ce ne sont PAS les seuils (qui vivent dans
 * `hearth-proto::thresholds`, appliqués par la coquille Rust) : ce sont des valeurs choisies bien
 * à l'intérieur de chaque zone, et le niveau est annoncé tel quel, exactement comme la coquille
 * l'annonce à l'interface. Le pont simulé ne calcule aucun niveau.
 */
const PINNED_VALUE: Record<Pinnable, Record<Level, number>> = {
  cpu: { normal: 25, attention: 88, critical: 97 },
  mem: { normal: 30, attention: 88, critical: 97 },
  disk: { normal: 41, attention: 88, critical: 97 },
  gpuMem: { normal: 30, attention: 88, critical: 97 },
  gpuTemp: { normal: 52, attention: 83, critical: 93 },
  temp: { normal: 48, attention: 83, critical: 93 },
};

export interface SimulatedMachineOptions {
  /** Horloge injectable (millisecondes). */
  now?: () => number;
  /** Le serveur est-il « Connecté » ? Hors ligne, plus aucun échantillon n'est annoncé. */
  connected?: (serverId: string) => boolean;
  /** Machine de chaque serveur ; sans entrée, une machine complète (carte graphique, sondes). */
  machines?: Record<string, MachineInfo>;
}

/** Machine complète d'exemple : 16 cœurs, 64 Gio, deux disques, une carte NVIDIA, des sondes. */
export function sampleMachine(name: string): MachineInfo {
  return {
    name,
    os: { name: "NixOS", version: "25.05", kernel: "6.12.1", arch: "x86_64" },
    cpu: {
      model: "AMD Ryzen 9 7950X",
      physicalCores: 16,
      logicalCores: 16,
      frequencyMhz: 4500,
    },
    memoryTotalBytes: 64 * GIB,
    disks: [
      { name: "/dev/nvme0n1p2", mount: "/", fs: "ext4", totalBytes: 1000 * GIB, removable: false },
      {
        name: "/dev/sda1",
        mount: "/mnt/data",
        fs: "ext4",
        totalBytes: 4000 * GIB,
        removable: false,
      },
    ],
    gpus: [{ name: "NVIDIA GeForce RTX 4090", memoryTotalBytes: 24 * GIB }],
    capabilities: { gpu: true, temps: true },
  };
}

/** Machine sans carte graphique ni sonde (un boîtier de stockage, par exemple). */
export function bareMachine(name: string): MachineInfo {
  return {
    name,
    os: { name: "Debian", version: "12", kernel: null, arch: "x86_64" },
    cpu: { model: "Intel N100", physicalCores: null, logicalCores: 4, frequencyMhz: null },
    memoryTotalBytes: 16 * GIB,
    disks: [
      { name: "/dev/sda1", mount: "/", fs: "ext4", totalBytes: 2000 * GIB, removable: false },
    ],
    gpus: [],
    capabilities: { gpu: false, temps: false },
  };
}

interface Track {
  machine: MachineInfo;
  history: MachineSample[];
  levels: SampleLevels | null;
  pins: Partial<Record<Pinnable, Level>>;
  /** La carte graphique n'expose pas sa température (BR-DASH-007). */
  gpuTempMissing: boolean;
  step: number;
  listeners: Set<(event: MachineEvent) => void>;
}

/**
 * Machines simulées : des mesures plausibles et lisses (sinusoïdes déterministes), un historique
 * de 5 minutes, et de quoi pousser une mesure dans un niveau d'alerte. Sert au navigateur de
 * développement, aux tests Vitest et à Playwright. Le code de simulation n'est pas dans le build
 * livré (`check:dist`).
 */
export class SimulatedMachine {
  private readonly now: () => number;
  private readonly connected: (serverId: string) => boolean;
  private readonly machines: Record<string, MachineInfo>;
  private readonly tracks = new Map<string, Track>();
  private timer: ReturnType<typeof setInterval> | undefined;

  constructor(options: SimulatedMachineOptions = {}) {
    this.now = options.now ?? Date.now;
    this.connected = options.connected ?? (() => true);
    this.machines = options.machines ?? {};
  }

  private track(serverId: string): Track {
    let track = this.tracks.get(serverId);
    if (!track) {
      track = {
        machine: this.machines[serverId] ?? sampleMachine(serverId),
        history: [],
        levels: null,
        pins: {},
        gpuTempMissing: false,
        step: 0,
        listeners: new Set(),
      };
      this.tracks.set(serverId, track);
    }
    return track;
  }

  private viewOf(serverId: string): MachineView | null {
    const track = this.track(serverId);
    if (track.history.length === 0) return null;
    return {
      serverId,
      machine: track.machine,
      history: track.history.map((sample) => ({ ...sample })),
      levels: track.levels,
    };
  }

  private announceView(serverId: string): void {
    const view = this.viewOf(serverId);
    if (!view) return;
    for (const listener of [...this.track(serverId).listeners]) listener({ kind: "view", view });
  }

  /** Abonnement : rejoue la dernière vue connue (s'il y en a une), puis chaque événement. */
  subscribe(serverId: string, listener: (event: MachineEvent) => void): Unsubscribe {
    const track = this.track(serverId);
    track.listeners.add(listener);
    const view = this.viewOf(serverId);
    if (view) listener({ kind: "view", view });
    return () => void track.listeners.delete(listener);
  }

  /** Remplit l'historique de `seconds` secondes passées (un échantillon par seconde), sans rien annoncer. */
  prefill(serverId: string, seconds: number): void {
    const track = this.track(serverId);
    const end = this.now();
    for (let back = seconds; back >= 1; back -= 1) {
      this.generate(track, end - back * 1000);
    }
  }

  /** Un échantillon de plus, à l'instant courant ; annoncé si le serveur est « Connecté ». */
  tick(serverId: string): MachineMetrics | null {
    const track = this.track(serverId);
    this.catchUp(track);
    const sample = this.generate(track, this.now());
    if (!this.connected(serverId) || !track.levels) return null;
    const metrics: MachineMetrics = { serverId, sample, levels: track.levels };
    for (const listener of [...track.listeners]) listener({ kind: "metrics", metrics });
    return metrics;
  }

  /** Le lien revient : l'agent renvoie son identité et son historique, trou compris (BR-DASH-011). */
  resync(serverId: string): void {
    this.catchUp(this.track(serverId));
    this.announceView(serverId);
  }

  /** La machine continue d'être mesurée pendant une coupure : rattrape les secondes manquées. */
  private catchUp(track: Track): void {
    const last = track.history.at(-1);
    if (!last) return;
    const now = this.now();
    for (let at = last.at + 1000; at <= now - 1000; at += 1000) this.generate(track, at);
  }

  /** Un échantillon par seconde pour tous les serveurs connus (navigateur de développement). */
  start(): void {
    this.timer ??= setInterval(() => {
      for (const serverId of [...this.tracks.keys()]) this.tick(serverId);
    }, 1000);
  }

  stop(): void {
    clearInterval(this.timer);
    this.timer = undefined;
  }

  /** Pousse une mesure dans un niveau d'alerte (`null` : la libère). */
  pin(serverId: string, measure: Pinnable, level: Level | null): void {
    const { pins } = this.track(serverId);
    if (level === null) delete pins[measure];
    else pins[measure] = level;
  }

  /** La carte graphique n'expose plus sa température (BR-DASH-007). */
  setGpuTempMissing(serverId: string, missing: boolean): void {
    this.track(serverId).gpuTempMissing = missing;
  }

  /** Remplace l'identité de la machine (sans carte, sans sonde, deux disques…) et l'annonce. */
  setMachine(serverId: string, machine: MachineInfo): void {
    this.track(serverId).machine = machine;
    this.announceView(serverId);
  }

  private level(track: Track, measure: Pinnable): Level {
    return track.pins[measure] ?? "normal";
  }

  private value(track: Track, measure: Pinnable, natural: number): number {
    const pinned = track.pins[measure];
    return pinned ? PINNED_VALUE[measure][pinned] : natural;
  }

  private generate(track: Track, at: number): MachineSample {
    track.step += 1;
    const n = track.step;
    const wave = (period: number, phase = 0) => (Math.sin(n / period + phase) + 1) / 2;
    const one = (value: number) => Math.round(value * 10) / 10;
    const machine = track.machine;
    const cpu = this.value(track, "cpu", 12 + 28 * wave(11) + 5 * wave(3, 1));
    const memPercent = this.value(track, "mem", 22 + 12 * wave(37));
    const first = machine.disks[0];
    const gpus = machine.capabilities.gpu ? machine.gpus : [];
    const sample: MachineSample = {
      at,
      uptimeS: 266_400 + n,
      cpu: one(cpu),
      cores: Array.from({ length: machine.cpu.logicalCores }, (_, core) =>
        one(Math.min(100, Math.max(0, cpu + 18 * (wave(5 + core, core) - 0.5)))),
      ),
      mem: {
        usedBytes: Math.round((machine.memoryTotalBytes * memPercent) / 100),
        totalBytes: machine.memoryTotalBytes,
      },
      disks: machine.disks.map((disk) => ({
        name: disk.name,
        mount: disk.mount,
        totalBytes: disk.totalBytes,
        usedBytes: Math.round(
          (disk.totalBytes * (disk === first ? this.value(track, "disk", 41) : 18)) / 100,
        ),
      })),
      net: {
        upBytesPerS: Math.round(40_000 + 900_000 * wave(13) * wave(2)),
        downBytesPerS: Math.round(150_000 + 3_200_000 * wave(7, 2)),
      },
      gpus: gpus.map((gpu, index) => {
        const total = gpu.memoryTotalBytes;
        const memoryPercent = this.value(track, "gpuMem", 28 + 10 * wave(19, index));
        return {
          name: gpu.name,
          loadPercent: one(20 + 55 * wave(9, index)),
          memoryUsedBytes: total === null ? null : Math.round((total * memoryPercent) / 100),
          memoryTotalBytes: total,
          tempC: track.gpuTempMissing
            ? null
            : one(this.value(track, "gpuTemp", 48 + 8 * wave(23, index))),
        };
      }),
      temps: machine.capabilities.temps
        ? [
            {
              label: "coretemp Package id 0",
              celsius: one(this.value(track, "temp", 44 + 8 * wave(17))),
            },
            { label: "nvme Composite", celsius: one(36 + 4 * wave(29)) },
          ]
        : [],
    };
    track.history.push(sample);
    while (track.history.length > HISTORY_CAP) track.history.shift();
    track.levels = {
      cpu: this.level(track, "cpu"),
      mem: this.level(track, "mem"),
      disks: machine.disks.map((disk) => (disk === first ? this.level(track, "disk") : "normal")),
      gpus: gpus.map(() => ({
        memory: this.level(track, "gpuMem"),
        temp: track.gpuTempMissing ? "normal" : this.level(track, "gpuTemp"),
      })),
      temps: sample.temps.map((_, index) => (index === 0 ? this.level(track, "temp") : "normal")),
    };
    return sample;
  }
}
