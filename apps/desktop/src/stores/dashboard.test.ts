import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RING_CAP } from "@/dashboard/series";
import { setLinkBridge } from "@/link";
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
    const store = useDashboardStore();
    await store.follow("forge");
    const entry = store.of("forge");
    vi.advanceTimersByTime(1000);
    bridge.machine.tick("forge");
    const ticks = entry?.tick ?? 0;
    const length = entry?.ring.length ?? 0;
    // Même instant : le flux rejoué n'ajoute rien et ne redessine rien.
    bridge.machine.tick("forge");
    expect(entry?.ring.length).toBe(length);
    expect(entry?.tick).toBe(ticks);
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
