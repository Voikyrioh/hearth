import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateStateDto } from "@/bindings";
import { SimulatedUpdateBridge, setUpdateBridge } from "@/updates";
import { useUpdatesStore } from "./updates";

const NEWER = { version: "1.1.0", notes: "Corrections." };

function setup(options: ConstructorParameters<typeof SimulatedUpdateBridge>[0] = {}) {
  setActivePinia(createPinia());
  const bridge = new SimulatedUpdateBridge(options);
  setUpdateBridge(bridge);
  return { bridge, store: useUpdatesStore() };
}

function state(overrides: Partial<UpdateStateDto> = {}): UpdateStateDto {
  return {
    seq: 1,
    currentVersion: "0.1.0",
    phase: "idle",
    progress: null,
    available: null,
    bannerVisible: false,
    postponedUntil: null,
    lastCheckedAt: null,
    upToDate: false,
    failure: null,
    ...overrides,
  };
}

afterEach(() => setUpdateBridge(null));

describe("updates store", () => {
  it("has no banner until the shell announces a newer version", async () => {
    const { store } = setup();
    await store.start();
    expect(store.state?.currentVersion).toBe("0.1.0");
    expect(store.banner).toBeNull();
  });

  it("shows the available banner with the notes of the release", async () => {
    const { store } = setup({ available: NEWER });
    await store.start();
    expect(store.banner).toBe("available");
    expect(store.available).toEqual(NEWER);
  });

  it("follows the states the shell publishes", async () => {
    const { bridge, store } = setup();
    await store.start();
    bridge.setFeed(NEWER);
    await bridge.check();
    expect(store.banner).toBe("available");
  });

  it("drops a state older than the last one seen but replays the same one", async () => {
    setActivePinia(createPinia());
    let push: (state: UpdateStateDto) => void = () => {};
    setUpdateBridge({
      getState: async () => state({ seq: 5, available: NEWER, bannerVisible: true }),
      check: async () => state(),
      postpone: async () => state(),
      install: async () => state(),
      onState: async (listener) => {
        push = listener;
        return () => {};
      },
    });
    const store = useUpdatesStore();
    await store.start();
    expect(store.state?.seq).toBe(5);
    push(state({ seq: 4, available: null, bannerVisible: false }));
    expect(store.banner).toBe("available");
    push(state({ seq: 6, available: null, bannerVisible: false }));
    expect(store.banner).toBeNull();
    push(state({ seq: 6, available: NEWER, bannerVisible: true }));
    expect(store.banner).toBe("available");
  });

  it("listens before reading, so nothing published in between is lost", async () => {
    setActivePinia(createPinia());
    const order: string[] = [];
    setUpdateBridge({
      getState: async () => {
        order.push("read");
        return state();
      },
      check: async () => state(),
      postpone: async () => state(),
      install: async () => state(),
      onState: async () => {
        order.push("listen");
        return () => {};
      },
    });
    await useUpdatesStore().start();
    expect(order).toEqual(["listen", "read"]);
  });

  it("'Plus tard' hides the banner and keeps the release known", async () => {
    const { bridge, store } = setup({ available: NEWER });
    await store.start();
    await store.postpone();
    expect(store.banner).toBeNull();
    expect(store.available).toEqual(NEWER);
    expect(bridge.calls.postpone).toBe(1);
  });

  it("the banner comes back after a day", async () => {
    const { bridge, store } = setup({ available: NEWER });
    await store.start();
    await store.postpone();
    bridge.advance(23 * 60 * 60 * 1000);
    expect(store.banner).toBeNull();
    bridge.advance(60 * 60 * 1000);
    expect(store.banner).toBe("available");
  });

  it("'Vérifier maintenant' without Internet changes nothing and raises no error", async () => {
    const { bridge, store } = setup({ checkedAgoMs: 3 * 24 * 60 * 60 * 1000 });
    await store.start();
    const before = store.state?.lastCheckedAt;
    bridge.setOnline(false);
    await store.checkNow();
    expect(store.state?.lastCheckedAt).toBe(before);
    expect(store.state?.failure).toBeNull();
    expect(store.banner).toBeNull();
    expect(store.checking).toBe(false);
  });

  it("'Vérifier maintenant' finds a release, or says the client is up to date", async () => {
    const { bridge, store } = setup({ checkedAgoMs: null });
    await store.start();
    expect(store.state?.lastCheckedAt).toBeNull();
    await store.checkNow();
    expect(store.state?.upToDate).toBe(true);
    bridge.setFeed(NEWER);
    await store.checkNow();
    expect(store.banner).toBe("available");
    expect(store.state?.upToDate).toBe(false);
  });

  it("one manual check at a time", async () => {
    const { bridge, store } = setup();
    await store.start();
    await Promise.all([store.checkNow(), store.checkNow()]);
    expect(bridge.calls.check).toBe(1);
  });

  it("the click downloads, then installs; nothing is installed before the click", async () => {
    const { bridge, store } = setup({ available: NEWER });
    await store.start();
    expect(bridge.installed).toBe(0);
    await store.install();
    expect(store.banner).toBe("downloading");
    bridge.setProgress(35);
    expect(store.state?.progress).toBe(35);
    bridge.finishInstall();
    expect(store.banner).toBe("installing");
    expect(bridge.installed).toBe(1);
  });

  it("ignores a second click while an installation is running", async () => {
    const { bridge, store } = setup({ available: NEWER });
    await store.start();
    await store.install();
    await store.install();
    expect(bridge.calls.install).toBe(1);
  });

  it.each([
    ["interrupted", "failed"],
    ["corrupted", "failed"],
  ] as const)("a %s update shows a failed banner and can be retried", async (outcome, banner) => {
    const { bridge, store } = setup({ available: NEWER });
    await store.start();
    bridge.setInstallOutcome(outcome);
    await store.install();
    bridge.finishInstall();
    expect(store.banner).toBe(banner);
    expect(store.state?.failure).toBe(outcome);
    bridge.setInstallOutcome("ok");
    await store.install();
    bridge.finishInstall();
    expect(store.banner).toBe("installing");
    expect(store.state?.failure).toBeNull();
  });

  it("does not use the page's storage for anything", async () => {
    const spy = vi.spyOn(Storage.prototype, "setItem");
    const { store } = setup({ available: NEWER });
    await store.start();
    await store.postpone();
    await store.checkNow();
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it("stays silent when the bridge is down", async () => {
    setActivePinia(createPinia());
    setUpdateBridge({
      getState: () => Promise.reject(new Error("pas de pont")),
      check: () => Promise.reject(new Error("pas de pont")),
      postpone: () => Promise.reject(new Error("pas de pont")),
      install: () => Promise.reject(new Error("pas de pont")),
      onState: () => Promise.reject(new Error("pas de pont")),
    });
    const store = useUpdatesStore();
    await store.start();
    await store.checkNow();
    expect(store.banner).toBeNull();
    expect(store.state).toBeNull();
  });
});

beforeEach(() => vi.restoreAllMocks());
