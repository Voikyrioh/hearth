import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RING_CAP } from "@/dashboard/series";
import { type SampleLevels, setLinkBridge } from "@/link";
import { startedApp } from "@/test/app";
import { makeMachine, makeSample } from "@/test/machine";
import { MACHINE_RETRY_MS, useDashboardStore } from "./dashboard";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

describe("dashboard store", () => {
  it("starts on the 5 minute window", async () => {
    await startedApp();
    expect(useDashboardStore().windowKey).toBe("5m");
  });

  it("follows a server once, whatever the number of calls", async () => {
    const { bridge } = await startedApp();
    const spy = vi.spyOn(bridge, "onMachine");
    const store = useDashboardStore();
    await Promise.all([store.follow("forge"), store.follow("forge")]);
    await store.follow("forge");
    expect(spy).toHaveBeenCalledTimes(1);
    expect(store.of("forge")?.settled).toBe(true);
    expect(store.of("forge")?.machine).toBeNull();
  });

  it("replays the last known view on subscribing, then takes each sample", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 30);
    const store = useDashboardStore();
    await store.follow("forge");
    const entry = store.of("forge");
    expect(entry?.machine?.name).toBe("forge");
    expect(entry?.ring.length).toBe(30);
    const ticks = entry?.tick ?? 0;
    vi.advanceTimersByTime(1000);
    bridge.machine.tick("forge");
    expect(entry?.ring.length).toBe(31);
    expect(entry?.tick).toBe(ticks + 1);
    expect(entry?.latest?.sample.at).toBe(entry?.ring.last?.at);
  });

  it("ignores a sample that is not newer than the last one", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 5);
    let deliver: Parameters<typeof bridge.onMachine>[1] = () => {};
    vi.spyOn(bridge, "onMachine").mockImplementation(async (_id, listener) => {
      deliver = listener;
      return () => {};
    });
    const store = useDashboardStore();
    await store.follow("forge");
    const entry = store.of("forge");
    const last = makeSample(Date.now() + 1000);
    const levels: SampleLevels = { cpu: "normal", mem: "normal", disks: [], gpus: [], temps: [] };
    deliver({ kind: "metrics", metrics: { serverId: "forge", sample: last, levels } });
    const ticks = entry?.tick ?? 0;
    const length = entry?.ring.length ?? 0;
    // Même instant, ou plus ancien : le flux rejoué n'ajoute rien et ne redessine rien.
    deliver({ kind: "metrics", metrics: { serverId: "forge", sample: last, levels } });
    deliver({
      kind: "metrics",
      metrics: { serverId: "forge", sample: makeSample(last.at - 500), levels },
    });
    expect(entry?.ring.length).toBe(length);
    expect(entry?.tick).toBe(ticks);
  });

  it("a late view older than the live data does not put yesterday's identity back", async () => {
    const { bridge } = await startedApp();
    let deliver: Parameters<typeof bridge.onMachine>[1] = () => {};
    vi.spyOn(bridge, "onMachine").mockImplementation(async (_id, listener) => {
      deliver = listener;
      return () => {};
    });
    const store = useDashboardStore();
    await store.follow("forge");
    const now = Date.now();
    const todayHistory = [makeSample(now), makeSample(now + 1000)];
    const levels: SampleLevels = { cpu: "normal", mem: "normal", disks: [], gpus: [], temps: [] };
    const view = (name: string, history: typeof todayHistory) => ({
      kind: "view" as const,
      view: { serverId: "forge", machine: makeMachine({ name }), history, levels },
    });
    deliver(view("aujourd'hui", todayHistory));
    deliver(view("hier", [makeSample(now - 20 * 3600_000)]));
    expect(store.of("forge")?.machine?.name).toBe("aujourd'hui");
  });

  it("a view without any sample (identity only) does not overwrite the identity of a live snapshot", async () => {
    const { bridge } = await startedApp();
    let deliver: Parameters<typeof bridge.onMachine>[1] = () => {};
    vi.spyOn(bridge, "onMachine").mockImplementation(async (_id, listener) => {
      deliver = listener;
      return () => {};
    });
    const store = useDashboardStore();
    await store.follow("forge");
    const now = Date.now();
    const levels: SampleLevels = { cpu: "normal", mem: "normal", disks: [], gpus: [], temps: [] };
    const view = (name: string, history: ReturnType<typeof makeSample>[]) => ({
      kind: "view" as const,
      view: { serverId: "forge", machine: makeMachine({ name }), history, levels },
    });
    deliver(view("aujourd'hui", [makeSample(now), makeSample(now + 1000)]));
    deliver(view("hier", []));
    expect(store.of("forge")?.machine?.name).toBe("aujourd'hui");
    // Sans rien de connu encore, la même vue vide est prise (pas d'écran sans identité).
    store.forget("forge");
    await store.follow("forge");
    deliver(view("première", []));
    expect(store.of("forge")?.machine?.name).toBe("première");
  });

  it("the hour read at the opening fills the ring before the snapshot without touching identity or latest", async () => {
    const { bridge } = await startedApp();
    let deliver: Parameters<typeof bridge.onMachine>[1] = () => {};
    vi.spyOn(bridge, "onMachine").mockImplementation(async (_id, listener) => {
      deliver = listener;
      return () => {};
    });
    const store = useDashboardStore();
    await store.follow("forge");
    const now = Date.now();
    const levels: SampleLevels = { cpu: "normal", mem: "normal", disks: [], gpus: [], temps: [] };
    const snapshot = Array.from({ length: 300 }, (_, i) => makeSample(now - 300_000 + i * 1000));
    deliver({
      kind: "view",
      view: {
        serverId: "forge",
        machine: makeMachine({ name: "forge" }),
        history: snapshot,
        levels,
      },
    });
    const entry = store.of("forge");
    const latest = entry?.latest;
    const hour = Array.from({ length: 330 }, (_, i) => makeSample(now - 3_600_000 + i * 10_000));
    deliver({ kind: "history", history: hour });
    expect(entry?.ring.length).toBe(630);
    expect(entry?.ring.first?.at).toBe(now - 3_600_000);
    expect(entry?.ring.last?.at).toBe(snapshot.at(-1)?.at);
    expect(entry?.latest).toBe(latest);
    expect(entry?.machine?.name).toBe("forge");
    // Rejoué (nouvelle connexion) : rien n'est doublé.
    deliver({ kind: "history", history: hour });
    expect(entry?.ring.length).toBe(630);
  });

  it("is bounded to an hour of samples", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    const store = useDashboardStore();
    await store.follow("forge");
    const entry = store.of("forge");
    const start = Date.now() + 1000;
    for (let i = 0; i < RING_CAP + 200; i += 1) entry?.ring.push(makeSample(start + i * 1000));
    expect(entry?.ring.length).toBe(RING_CAP);
  });

  it("keeps each server apart", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    bridge.machine.prefill("salon", 20);
    const store = useDashboardStore();
    await store.follow("forge");
    await store.follow("salon");
    expect(store.of("forge")?.ring.length).toBe(10);
    expect(store.of("salon")?.ring.length).toBe(20);
    expect(store.of("salon")?.machine?.capabilities.gpu).toBe(false);
  });

  it("retries every few seconds when the subscription fails, and says so meanwhile", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    const real = bridge.onMachine.bind(bridge);
    let failures = 2;
    vi.spyOn(bridge, "onMachine").mockImplementation((id, listener) => {
      if (failures > 0) {
        failures -= 1;
        return Promise.reject(new Error("lecture impossible"));
      }
      return real(id, listener);
    });
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const store = useDashboardStore();
    await store.follow("forge");
    expect(store.of("forge")?.failed).toBe(true);
    expect(store.of("forge")?.settled).toBe(true);
    await vi.advanceTimersByTimeAsync(MACHINE_RETRY_MS);
    expect(store.of("forge")?.failed).toBe(true);
    await vi.advanceTimersByTimeAsync(MACHINE_RETRY_MS);
    await flushPromises();
    expect(store.of("forge")?.failed).toBe(false);
    expect(store.of("forge")?.machine?.name).toBe("forge");
  });

  it("forgets a server removed from the book and stops listening to it", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    const store = useDashboardStore();
    await store.follow("forge");
    expect(store.of("forge")).toBeDefined();
    bridge.dropServer("forge");
    await flushPromises();
    expect(store.of("forge")).toBeUndefined();
  });

  it("stops listening when reset", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    const store = useDashboardStore();
    await store.follow("forge");
    store.setWindow("1h");
    store.reset();
    expect(store.of("forge")).toBeUndefined();
    expect(store.windowKey).toBe("5m");
    // Plus aucun abonné : un échantillon de plus ne ressuscite rien.
    bridge.machine.tick("forge");
    expect(store.of("forge")).toBeUndefined();
  });

  it("applies a snapshot from the link: identity replaced, history pasted", async () => {
    const { bridge } = await startedApp();
    bridge.machine.prefill("forge", 10);
    const store = useDashboardStore();
    await store.follow("forge");
    bridge.machine.setMachine("forge", makeMachine({ name: "autre" }));
    expect(store.of("forge")?.machine?.name).toBe("autre");
  });
});
