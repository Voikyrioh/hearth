import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { LevelsDto, MachineDto, MetricsEvent, SampleDto, SnapshotEvent } from "@/bindings";
import type { MachineEvent } from "./machine";
import { toMetrics, toSample, toView } from "./machine";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

const GIB = 1024 ** 3;

const machine: MachineDto = {
  name: "forge",
  os: { name: "NixOS", version: "25.05", kernel: null, arch: "x86_64" },
  cpu: { model: "Ryzen", physicalCores: 16, logicalCores: 32, frequencyMhz: 4500 },
  memoryTotalBytes: 64 * GIB,
  disks: [],
  gpus: [],
  capabilities: { gpu: false, temps: false },
};

const levels: LevelsDto = { cpu: "normal", mem: "attention", disks: [], gpus: [], temps: [] };

function sample(at: number, over: Partial<SampleDto> = {}): SampleDto {
  return {
    at,
    uptimeS: 100,
    cpu: 12.5,
    cores: [10, 15],
    mem: { usedBytes: 8 * GIB, totalBytes: 64 * GIB },
    disks: [],
    net: null,
    gpus: [],
    temps: [],
    ...over,
  };
}

const view = (server: string, history: SampleDto[]): SnapshotEvent => ({
  serverId: server,
  machine,
  history,
  levels,
});

function ipc(handler: (cmd: string) => unknown) {
  const calls: string[] = [];
  mockIPC(
    (cmd) => {
      calls.push(cmd);
      return handler(cmd);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

afterEach(() => clearMocks());

describe("machine conversions", () => {
  it("drops a sample whose essential fields are unreadable, keeps an unreadable optional one", () => {
    expect(toSample(sample(1000))).toMatchObject({ at: 1000, cpu: 12.5, net: null });
    expect(toSample(sample(null as unknown as number))).toBeNull();
    expect(toSample(sample(1000, { cpu: null }))).toBeNull();
    expect(toSample(sample(1000, { mem: { usedBytes: null, totalBytes: 5 } }))).toBeNull();
    const gpu = {
      name: "g",
      loadPercent: null,
      memoryUsedBytes: null,
      memoryTotalBytes: 5,
      tempC: null,
    };
    expect(toSample(sample(1000, { gpus: [gpu] }))?.gpus[0]).toEqual(gpu);
  });

  it("drops a sample whose lists hold an unreadable number instead of inventing a zero", () => {
    expect(toSample(sample(1000, { cores: [1, null] }))).toBeNull();
    expect(toSample(sample(1000, { temps: [{ label: "t", celsius: null }] }))).toBeNull();
    expect(
      toSample(
        sample(1000, { disks: [{ name: "d", mount: "/", usedBytes: null, totalBytes: 1 }] }),
      ),
    ).toBeNull();
    expect(
      toSample(sample(1000, { net: { upBytesPerS: 1, downBytesPerS: null } }))?.net,
    ).toBeNull();
  });

  it("turns a snapshot and a live message into the interface types, levels untouched", () => {
    const converted = toView(view("a", [sample(1000), sample(null as unknown as number)]));
    expect(converted?.history).toHaveLength(1);
    expect(converted?.levels?.mem).toBe("attention");
    expect(
      toView({ ...view("a", []), machine: { ...machine, memoryTotalBytes: null } }),
    ).toBeNull();
    const metrics: MetricsEvent = { serverId: "a", sample: sample(2000), levels };
    expect(toMetrics(metrics)?.levels.mem).toBe("attention");
    expect(toMetrics({ ...metrics, sample: sample(2000, { cpu: null }) })).toBeNull();
  });
});

describe("TauriLinkBridge.onMachine", () => {
  it("listens first, replays the last known view, then delivers snapshots and samples of that server only", async () => {
    const calls = ipc((cmd) => (cmd === "get_dashboard" ? view("a", [sample(1000)]) : undefined));
    const got: string[] = [];
    const off = await new TauriLinkBridge().onMachine("a", (event: MachineEvent) =>
      got.push(
        event.kind === "view"
          ? `view:${event.view.history.length}`
          : event.kind === "history"
            ? `history:${event.history.length}`
            : `metrics:${event.metrics.sample.at}`,
      ),
    );
    expect(got).toEqual(["view:1"]);
    expect(calls.indexOf("plugin:event|listen")).toBeLessThan(calls.indexOf("get_dashboard"));
    await emit(LINK_EVENTS.metrics, { serverId: "a", sample: sample(2000), levels });
    await emit(LINK_EVENTS.metrics, { serverId: "b", sample: sample(3000), levels });
    await emit(LINK_EVENTS.snapshot, view("a", [sample(1000), sample(2000)]));
    await emit(LINK_EVENTS.snapshot, view("b", []));
    await emit(LINK_EVENTS.history, { serverId: "a", history: [sample(500)] });
    await emit(LINK_EVENTS.history, { serverId: "b", history: [sample(600)] });
    expect(got).toEqual(["view:1", "metrics:2000", "view:2", "history:1"]);
    off();
    await emit(LINK_EVENTS.metrics, { serverId: "a", sample: sample(4000), levels });
    expect(got).toHaveLength(4);
  });

  it("replays nothing when the shell has no view yet", async () => {
    ipc(() => null);
    const got: string[] = [];
    await new TauriLinkBridge().onMachine("a", () => got.push("event"));
    expect(got).toEqual([]);
  });

  it("merges the view read with a sample that arrived meanwhile: nothing is dropped, the identity always comes", async () => {
    ipc((cmd) => {
      if (cmd !== "get_dashboard") return undefined;
      void emit(LINK_EVENTS.metrics, { serverId: "a", sample: sample(5000), levels });
      return view("a", [sample(1000)]);
    });
    const got: string[] = [];
    await new TauriLinkBridge().onMachine("a", (event) =>
      got.push(event.kind === "view" ? "view" : "metrics"),
    );
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(got.sort()).toEqual(["metrics", "view"]);
  });

  it("rejects with the typed failure and leaves no listener behind", async () => {
    ipc((cmd) => {
      if (cmd === "get_dashboard") throw { kind: "unknown_server" };
      return undefined;
    });
    const got: string[] = [];
    await expect(
      new TauriLinkBridge().onMachine("a", () => got.push("event")),
    ).rejects.toBeInstanceOf(LinkCommandError);
    await emit(LINK_EVENTS.metrics, { serverId: "a", sample: sample(2000), levels });
    expect(got).toEqual([]);
  });
});
