import { describe, expect, it } from "vitest";
import { GIB, makeSample, makeSeries } from "@/test/machine";
import {
  coverageMs,
  cpuLoad,
  fullestDiskPercent,
  gpuLoad,
  gpuMemoryPercent,
  hottest,
  memoryPercent,
  netUp,
  RING_CAP,
  resample,
  SampleRing,
  WINDOWS,
} from "./series";

const T0 = 1_000_000;
const at = (second: number) => T0 + second * 1000;

describe("SampleRing", () => {
  it("keeps samples in order and ignores one that is not newer than the last", () => {
    const ring = new SampleRing();
    expect(ring.push(makeSample(at(1)))).toBe(true);
    expect(ring.push(makeSample(at(2)))).toBe(true);
    expect(ring.push(makeSample(at(2)))).toBe(false);
    expect(ring.push(makeSample(at(1)))).toBe(false);
    expect(ring.samples().map((sample) => sample.at)).toEqual([at(1), at(2)]);
  });

  it("is bounded: a long session does not grow the memory", () => {
    const ring = new SampleRing();
    for (let second = 0; second < RING_CAP + 500; second += 1) ring.push(makeSample(at(second)));
    expect(ring.length).toBe(RING_CAP);
    expect(ring.first?.at).toBe(at(500));
    expect(ring.last?.at).toBe(at(RING_CAP + 499));
  });

  it("pastes a snapshot over what it covers and keeps what is older (no gap, no duplicate)", () => {
    const ring = new SampleRing();
    for (let second = 0; second <= 100; second += 1) ring.push(makeSample(at(second)));
    // Coupure de 20 s : l'agent renvoie 5 minutes d'historique qui couvrent le trou.
    const history = Array.from({ length: 121 }, (_, i) => makeSample(at(i), { cpu: 99 }));
    ring.merge(history);
    expect(ring.length).toBe(121);
    expect(ring.samples().every((sample) => sample.cpu === 99)).toBe(true);
    // Un snapshot plus court garde l'ancien début.
    ring.merge([makeSample(at(100), { cpu: 5 }), makeSample(at(101), { cpu: 5 })]);
    // Ce qui est plus récent que le snapshot reste : rien n'est jeté.
    expect(ring.length).toBe(121);
    expect(ring.samples()[100]?.cpu).toBe(5);
    expect(ring.last?.cpu).toBe(99);
    expect(ring.samples()[99]?.cpu).toBe(99);
  });

  it("keeps a live sample newer than the snapshot pasted after it", () => {
    const ring = new SampleRing();
    ring.push(makeSample(at(10)));
    ring.merge([makeSample(at(1)), makeSample(at(2))]);
    expect(ring.samples().map((sample) => sample.at)).toEqual([at(1), at(2), at(10)]);
  });

  it("ignores an empty snapshot and drops duplicates inside one", () => {
    const ring = new SampleRing();
    ring.push(makeSample(at(1)));
    ring.merge([]);
    expect(ring.length).toBe(1);
    ring.merge([makeSample(at(5)), makeSample(at(5)), makeSample(at(6))]);
    expect(ring.samples().map((sample) => sample.at)).toEqual([at(1), at(5), at(6)]);
  });
});

describe("resample", () => {
  const second = Array.from({ length: 400 }, (_, i) => makeSample(at(i), { cpu: i }));

  it("is empty without samples", () => {
    expect(resample([], "5m", cpuLoad)).toEqual([]);
  });

  it("gives 60 steps of 1 s for 1 min, 300 for 5 min, 360 steps of 10 s for 1 h", () => {
    expect(resample(second, "1m", cpuLoad)).toHaveLength(60);
    expect(resample(second, "5m", cpuLoad)).toHaveLength(300);
    const hour = resample(second, "1h", cpuLoad);
    expect(hour).toHaveLength(360);
    expect((hour[1]?.t ?? 0) - (hour[0]?.t ?? 0)).toBe(WINDOWS["1h"].stepMs);
  });

  it("ends at the last sample, so a link that is down does not slide the curve", () => {
    const points = resample(second, "1m", cpuLoad);
    expect(points.at(-1)?.t).toBe(at(399));
    expect(points.at(-1)?.v).toBe(399);
    expect(points[0]?.v).toBe(340);
  });

  it("averages the readings of each 10 s step on the hour window", () => {
    const hour = resample(second, "1h", cpuLoad);
    // Chaque échantillon va au pas le plus proche de son âge : le dernier pas reçoit les âges de
    // 0 à 5 s (échantillons 395 à 399), moyenne 397.
    expect(hour.at(-1)?.v).toBeCloseTo(397, 5);
  });

  it("leaves a hole (null) where nothing was measured, never a zero", () => {
    const sparse = [makeSample(at(0)), makeSample(at(50)), makeSample(at(59))];
    const points = resample(sparse, "1m", cpuLoad);
    expect(points.filter((point) => point.v === null).length).toBeGreaterThan(50);
    expect(points.filter((point) => point.v !== null).length).toBe(3);
    expect(points.every((point) => point.v !== 0)).toBe(true);
  });

  it("skips unreadable values of a measure but keeps the other steps", () => {
    const samples = [
      makeSample(at(0), { net: null }),
      makeSample(at(1), { net: { upBytesPerS: 10, downBytesPerS: 20 } }),
    ];
    const points = resample(samples, "1m", netUp);
    expect(points.filter((point) => point.v !== null)).toHaveLength(1);
  });
});

describe("resample with a real agent clock (jitter, drift)", () => {
  const holes = (points: { v: number | null }[]) =>
    points.filter((point) => point.v === null).length;

  it.each([1, 3, 20])("±%i ms of jitter: no empty step, no step with two samples", (jitterMs) => {
    const samples = makeSeries(400, T0, { jitterMs });
    for (const window of ["1m", "5m"] as const) {
      const points = resample(samples, window, cpuLoad);
      expect(holes(points), `${window} ±${jitterMs} ms`).toBe(0);
    }
    // Deux échantillons légitimes ne se rangent jamais dans la même case : la valeur d'un pas est
    // celle d'UN échantillon (cpu = 10 + (i % 7), jamais une moyenne de deux).
    const values = resample(samples, "1m", cpuLoad).map((point) => point.v);
    expect(values.every((value) => Number.isInteger(value))).toBe(true);
  });

  it("a slow drift (1.003 s between samples over 5 minutes) leaves no hole", () => {
    const samples = makeSeries(300, T0, { intervalMs: 1003, jitterMs: 5 });
    expect(holes(resample(samples, "5m", cpuLoad).slice(-299))).toBe(0);
    expect(coverageMs(samples, "5m")).toBeGreaterThan(298_000);
  });

  it("a real hole of several seconds stays a hole, with jitter", () => {
    const samples = makeSeries(200, T0, { jitterMs: 20 });
    const withHole = [...samples.slice(0, 100), ...samples.slice(110)];
    const points = resample(withHole, "5m", cpuLoad);
    expect(holes(points.slice(-200))).toBeGreaterThanOrEqual(9);
    // Un trou de 2 échantillons manquants (3 s) reste un trou aussi.
    const short = [...samples.slice(0, 160), ...samples.slice(162)];
    expect(holes(resample(short, "1m", cpuLoad))).toBeGreaterThanOrEqual(1);
  });

  it("the hour window (10 s steps) is not hurt by jitter either", () => {
    const samples = makeSeries(3600, T0, { jitterMs: 20 });
    const points = resample(samples, "1h", cpuLoad);
    expect(holes(points)).toBe(0);
  });

  it("the tolerated gap follows the step of the window", () => {
    const samples = makeSeries(600, T0, { jitterMs: 3 });
    const gappy = [...samples.slice(0, 300), ...samples.slice(306)];
    expect(coverageMs(gappy, "5m")).toBeLessThan(300_000);
    expect(coverageMs(gappy, "1h")).toBeGreaterThan(590_000);
  });
});

describe("coverage and measures", () => {
  it("measures the time covered inside the window, on contiguous samples", () => {
    expect(coverageMs([], "5m")).toBe(0);
    const run = Array.from({ length: 91 }, (_, i) => makeSample(at(i)));
    expect(coverageMs(run, "5m")).toBe(90_000);
    // Fenêtre d'une minute : seule la dernière minute compte.
    expect(coverageMs(run, "1m")).toBe(59_000);
  });

  it("never counts a hole as covered (previous session, long outage)", () => {
    const yesterday = Array.from({ length: 300 }, (_, i) => makeSample(at(i) - 20 * 3600_000));
    const today = Array.from({ length: 301 }, (_, i) => makeSample(at(i)));
    const ring = new SampleRing();
    ring.merge(yesterday);
    ring.merge(today);
    expect(ring.length).toBe(601);
    expect(coverageMs(ring.samples(), "1h")).toBe(300_000);
    // Le trou n'est pas relié : les pas de la veille sont hors fenêtre, ceux de la coupure sont vides.
    const outage = [...today.slice(0, 100), ...today.slice(200)];
    expect(coverageMs(outage, "5m")).toBe(100_000);
    const points = resample(outage, "5m", cpuLoad);
    expect(points.filter((point) => point.v === null).length).toBeGreaterThanOrEqual(100);
  });

  it("takes the fullest disk, the hottest probe, and memory as a percentage", () => {
    const sample = makeSample(at(0), {
      mem: { usedBytes: 4 * GIB, totalBytes: 16 * GIB },
      disks: [
        { name: "a", mount: "/", usedBytes: 10, totalBytes: 100 },
        { name: "b", mount: "/b", usedBytes: 90, totalBytes: 100 },
        { name: "c", mount: "/c", usedBytes: 0, totalBytes: 0 },
      ],
      temps: [{ label: "p", celsius: 61 }],
    });
    expect(memoryPercent(sample)).toBe(25);
    expect(fullestDiskPercent(sample)).toBe(90);
    // La carte graphique à 70 °C l'emporte sur la sonde à 61 °C.
    expect(hottest(makeSample(at(0), { temps: sample.temps }))).toBe(61);
    expect(hottest(sample)).toBe(61);
    expect(hottest(makeSample(at(0), { temps: [], gpus: [] }))).toBeNull();
  });

  it("reports a missing GPU reading as null, not as zero", () => {
    const sample = makeSample(at(0), {
      gpus: [
        {
          name: "g",
          loadPercent: null,
          memoryUsedBytes: null,
          memoryTotalBytes: 24 * GIB,
          tempC: null,
        },
      ],
    });
    expect(gpuLoad(0)(sample)).toBeNull();
    expect(gpuMemoryPercent(0)(sample)).toBeNull();
    expect(gpuLoad(3)(sample)).toBeNull();
    expect(fullestDiskPercent(makeSample(at(0), { disks: [] }))).toBeNull();
  });
});
