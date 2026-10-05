import type { Level, MachineInfo, MachineSample, SampleLevels } from "@/link";

export const GIB = 1024 ** 3;

/** Un échantillon plausible à l'instant `at` (ms) ; `over` remplace des champs. */
export function makeSample(at: number, over: Partial<MachineSample> = {}): MachineSample {
  return {
    at,
    uptimeS: 266_400,
    cpu: 12.5,
    cores: [10, 15, 9.5, 14.8],
    mem: { usedBytes: 8 * GIB, totalBytes: 16 * GIB },
    disks: [{ name: "/dev/nvme0n1p2", mount: "/", usedBytes: 400 * GIB, totalBytes: 1000 * GIB }],
    net: { upBytesPerS: 1200, downBytesPerS: 45_000 },
    gpus: [
      {
        name: "RTX 4090",
        loadPercent: 37,
        memoryUsedBytes: 4 * GIB,
        memoryTotalBytes: 24 * GIB,
        tempC: 52,
      },
    ],
    temps: [{ label: "coretemp Package id 0", celsius: 48 }],
    ...over,
  };
}

export function makeMachine(over: Partial<MachineInfo> = {}): MachineInfo {
  return {
    name: "forge",
    os: { name: "NixOS", version: "25.05", kernel: "6.12.1", arch: "x86_64" },
    cpu: { model: "AMD Ryzen 9 7950X", physicalCores: 16, logicalCores: 32, frequencyMhz: 4500 },
    memoryTotalBytes: 16 * GIB,
    disks: [
      {
        name: "/dev/nvme0n1p2",
        mount: "/",
        fs: "ext4",
        totalBytes: 1000 * GIB,
        removable: false,
      },
    ],
    gpus: [{ name: "NVIDIA GeForce RTX 4090", memoryTotalBytes: 24 * GIB }],
    capabilities: { gpu: true, temps: true },
    ...over,
  };
}

/** Tous les niveaux à `normal`, alignés sur les listes de l'échantillon. */
export function normalLevels(sample: MachineSample): SampleLevels {
  const normal: Level = "normal";
  return {
    cpu: normal,
    mem: normal,
    disks: sample.disks.map(() => normal),
    gpus: sample.gpus.map(() => ({ memory: normal, temp: normal })),
    temps: sample.temps.map(() => normal),
  };
}
