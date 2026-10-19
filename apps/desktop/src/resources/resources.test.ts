import { describe, expect, it } from "vitest";
import {
  cpuFromField,
  cpuInRange,
  cpuStops,
  type EngineCapacity,
  engineCapacityFromWire,
  formatCpu,
  formatMemory,
  GIB,
  MIB,
  memoryFieldOf,
  memoryFromField,
  memoryInRange,
  memoryStops,
  nearestStopIndex,
  proposedMemoryBytes,
  resourcesIssue,
  startingResources,
} from ".";

const machine = (over: Partial<EngineCapacity> = {}): EngineCapacity => ({
  cpus: 16,
  memoryBytes: 64 * GIB,
  reserveBytes: 3 * GIB,
  committedMemoryBytes: 0,
  ...over,
});

describe("memoryStops (BR-RES-004)", () => {
  const stops = memoryStops(64 * GIB);
  it("goes by 128 Mo up to 2 Go, 256 Mo up to 8 Go, 1 Go beyond", () => {
    expect(stops[0]).toBe(128 * MIB);
    expect(stops[1]).toBe(256 * MIB);
    expect(stops[15]).toBe(2 * GIB);
    expect(stops[16]).toBe(2 * GIB + 256 * MIB);
    expect(stops[39]).toBe(8 * GIB);
    expect(stops[40]).toBe(9 * GIB);
  });
  it("ends on the total memory of the machine", () => {
    expect(stops.at(-1)).toBe(64 * GIB);
  });
  it("adds the total as the last stop when it is not round", () => {
    expect(memoryStops(8 * GIB + 512 * MIB).at(-1)).toBe(8 * GIB + 512 * MIB);
  });
  it("keeps a single stop on a machine below the floor", () => {
    expect(memoryStops(64 * MIB)).toEqual([128 * MIB]);
  });
});

describe("cpuStops", () => {
  it("goes from 0.25 core to the number of processors by 0.25", () => {
    expect(cpuStops(2)).toEqual([250, 500, 750, 1000, 1250, 1500, 1750, 2000]);
  });
});

it("places the slider on the nearest allowed value", () => {
  const stops = memoryStops(64 * GIB);
  expect(stops[nearestStopIndex(stops, 300 * MIB)]).toBe(256 * MIB);
  expect(stops[nearestStopIndex(stops, 10 * GIB + 600 * MIB)]).toBe(11 * GIB);
  expect(stops[nearestStopIndex(stops, 999 * GIB)]).toBe(64 * GIB);
});

describe("units", () => {
  it("shows Mo under 1 024 Mo and Go above, two decimals at most", () => {
    expect(formatMemory(640 * MIB)).toBe("640 Mo");
    expect(formatMemory(1536 * MIB)).toBe("1,5 Go");
    expect(formatMemory(64 * GIB)).toBe("64 Go");
    expect(formatMemory(1152 * MIB)).toBe("1,13 Go");
  });
  it("shows cores, plural from 2", () => {
    expect(formatCpu(250)).toBe("0,25 cœur");
    expect(formatCpu(1500)).toBe("1,5 cœur");
    expect(formatCpu(2500)).toBe("2,5 cœurs");
  });
  it("reads a typed memory with a comma or a point, in whole Mo only", () => {
    expect(memoryFromField("1,5", "go")).toBe(1536 * MIB);
    expect(memoryFromField("1.5", "go")).toBe(1536 * MIB);
    expect(memoryFromField("640", "mo")).toBe(640 * MIB);
    expect(memoryFromField("640,5", "mo")).toBeNull();
    expect(memoryFromField("", "mo")).toBeNull();
    expect(memoryFromField("-2", "go")).toBeNull();
    expect(memoryFromField("1e3", "mo")).toBeNull();
    expect(memoryFromField("beaucoup", "go")).toBeNull();
  });
  it("reads a typed processor in millicores", () => {
    expect(cpuFromField("2,5")).toBe(2500);
    expect(cpuFromField("0.25")).toBe(250);
    expect(cpuFromField("0,3333")).toBeNull();
    expect(cpuFromField("")).toBeNull();
  });
  it("writes a value back the way it reads it", () => {
    expect(memoryFieldOf(640 * MIB)).toEqual({ text: "640", unit: "mo" });
    expect(memoryFieldOf(1536 * MIB)).toEqual({ text: "1,5", unit: "go" });
    expect(memoryFieldOf(1125 * MIB)).toEqual({ text: "1125", unit: "mo" });
    expect(memoryFieldOf(1152 * MIB)).toEqual({ text: "1,125", unit: "go" });
  });
});

describe("ranges", () => {
  const cap = machine();
  it("accepts any whole Mo between 128 Mo and the total", () => {
    expect(memoryInRange(cap, 1000 * MIB)).toBe(true);
    expect(memoryInRange(cap, 127 * MIB)).toBe(false);
    expect(memoryInRange(cap, 64 * GIB + MIB)).toBe(false);
    expect(memoryInRange(cap, null)).toBe(false);
  });
  it("accepts multiples of 0.25 core up to the processors", () => {
    expect(cpuInRange(cap, 3250)).toBe(true);
    expect(cpuInRange(cap, 3300)).toBe(false);
    expect(cpuInRange(cap, 16250)).toBe(false);
    expect(cpuInRange(cap, 0)).toBe(false);
  });
});

describe("starting values (BR-RES-037, BR-RES-039)", () => {
  it("proposes 2 Go on a large free machine and every processor", () => {
    expect(startingResources(machine(), null)).toEqual({ memoryBytes: 2 * GIB, cpuMillis: 16000 });
  });
  it("proposes half of the free memory when that is smaller", () => {
    const roomy = machine({
      memoryBytes: 8 * GIB,
      reserveBytes: 1 * GIB,
      committedMemoryBytes: 3 * GIB,
    });
    expect(proposedMemoryBytes(roomy)).toBe(2 * GIB);
    const tight = machine({
      memoryBytes: 8 * GIB,
      reserveBytes: 1 * GIB,
      committedMemoryBytes: 5 * GIB,
    });
    expect(proposedMemoryBytes(tight)).toBe(1 * GIB);
  });
  it("never goes under 256 Mo, even with nothing left", () => {
    const full = machine({
      memoryBytes: 4 * GIB,
      reserveBytes: 1 * GIB,
      committedMemoryBytes: 4 * GIB,
    });
    expect(proposedMemoryBytes(full)).toBe(256 * MIB);
  });
  it("never goes over the machine", () => {
    expect(proposedMemoryBytes(machine({ memoryBytes: 128 * MIB }))).toBe(128 * MIB);
  });
  it("starts on the recommended memory of a card, brought down to the machine if above", () => {
    expect(startingResources(machine(), 6 * GIB).memoryBytes).toBe(6 * GIB);
    expect(startingResources(machine({ memoryBytes: 4 * GIB }), 6 * GIB).memoryBytes).toBe(4 * GIB);
  });
});

describe("resourcesIssue (BR-RES-037)", () => {
  it("refuses everything without bounds read from the machine", () => {
    expect(resourcesIssue(null, { memoryBytes: 2 * GIB, cpuMillis: 1000 })).toBe("capacity");
  });
  it("refuses an empty value, never reads it as no limit", () => {
    expect(resourcesIssue(machine(), { memoryBytes: null, cpuMillis: 1000 })).toBe("memory");
    expect(resourcesIssue(machine(), { memoryBytes: 2 * GIB, cpuMillis: null })).toBe("cpu");
    expect(resourcesIssue(machine(), { memoryBytes: 2 * GIB, cpuMillis: 1000 })).toBeNull();
  });
});

describe("engineCapacityFromWire", () => {
  const wire = {
    host: { cpus: 16, memory_bytes: 68719476736, reserve_bytes: 3435973837 },
    committed: { memory: { managed_running: 1000, unmanaged_running: 24 } },
  };
  it("reads the bounds and sums what is already committed", () => {
    expect(engineCapacityFromWire(wire)).toEqual({
      cpus: 16,
      memoryBytes: 68719476736,
      reserveBytes: 3435973837,
      committedMemoryBytes: 1024,
    });
  });
  it.each([
    ["nothing", undefined],
    ["no host", { committed: wire.committed }],
    ["a negative memory", { ...wire, host: { ...wire.host, memory_bytes: -1 } }],
    ["no processor", { ...wire, host: { ...wire.host, cpus: 0 } }],
    ["a text memory", { ...wire, host: { ...wire.host, memory_bytes: "64" } }],
    ["no committed", { host: wire.host }],
  ])("refuses %s", (_name, bad) => {
    expect(engineCapacityFromWire(bad)).toBeNull();
  });
});
