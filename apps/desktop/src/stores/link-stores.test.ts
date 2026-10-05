import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkStateEvent } from "@/link";
import { LINK_STATES, SimulatedLinkBridge, setLinkBridge } from "@/link";
import { freshBridge, startedApp } from "@/test/app";
import { RESUBSCRIBE_BASE_MS, RESUBSCRIBE_MAX_MS, useLinkStore } from "./link";
import { LOAD_TIMEOUT_MS, useServersStore } from "./servers";
import { MAX_VISIBLE_TOASTS, TOAST_LIFETIME_MS, useToastsStore } from "./toasts";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

describe("simulated link bridge", () => {
  it("lists the sample servers, all connected, and emits every state it is driven to", async () => {
    const { bridge } = freshBridge();
    let listed: string[] = [];
    await bridge.onServersChanged((list) => {
      listed = list.map((s) => s.id);
    });
    expect(listed).toEqual(["forge", "salon"]);
    const seen: string[] = [];
    const stop = await bridge.onLinkState((e) => seen.push(`${e.serverId}:${e.state}`));
    expect(seen).toEqual(["forge:connected", "salon:connected"]);
    for (const state of LINK_STATES) bridge.setState("forge", state);
    expect(seen.slice(2)).toEqual(LINK_STATES.map((s) => `forge:${s}`));
    stop();
    bridge.setState("forge", "offline");
    expect(seen).toHaveLength(2 + LINK_STATES.length);
  });

  it("keeps the last contact while the link is down and refreshes it on return", async () => {
    let now = 1000;
    const bridge = new SimulatedLinkBridge({ now: () => now });
    const events: { state: string; lastContactAt: number | null; nextRetryAt: number | null }[] =
      [];
    await bridge.onLinkState((e) => e.serverId === "forge" && events.push(e));
    now = 5000;
    bridge.setState("forge", "offline");
    now = 9000;
    bridge.setState("forge", "connected");
    expect(events[1]).toMatchObject({ state: "offline", lastContactAt: 1000 });
    expect(events[1]?.nextRetryAt).not.toBeNull();
    expect(events[2]).toMatchObject({ state: "connected", lastContactAt: 9000, nextRetryAt: null });
  });

  it("goes reconnecting on « Réessayer maintenant », then back to connected", async () => {
    const bridge = new SimulatedLinkBridge({ retryDelayMs: 1500 });
    const states: string[] = [];
    await bridge.onLinkState((e) => e.serverId === "forge" && states.push(e.state));
    bridge.setState("forge", "offline");
    await bridge.retryNow("forge");
    expect(states.at(-1)).toBe("reconnecting");
    expect(bridge.retries).toEqual(["forge"]);
    await vi.advanceTimersByTimeAsync(1500);
    expect(states.at(-1)).toBe("connected");
  });
});

describe("servers store", () => {
  it("loads the servers from the bridge once, and follows additions and removals", async () => {
    const { bridge, servers } = await startedApp();
    expect(servers.loaded).toBe(true);
    expect(servers.servers.map((s) => s.id)).toEqual(["forge", "salon"]);
    expect(servers.first?.id).toBe("forge");
    bridge.dropServer("forge");
    expect(servers.servers.map((s) => s.id)).toEqual(["salon"]);
    bridge.seedServer({
      id: "x",
      name: "x",
      address: "x",
      host: "x",
      port: 7341,
      color: 2,
      role: "admin",
      username: "",
      remember: false,
    });
    expect(servers.byId("x")?.name).toBe("x");
    await servers.load();
    expect(servers.servers).toHaveLength(2);
  });

  it("exposes the current server by id, null when it does not exist", async () => {
    const { servers } = await startedApp();
    servers.setCurrent("salon");
    expect(servers.current?.role).toBe("readonly");
    servers.setCurrent("nope");
    expect(servers.current).toBeNull();
  });
});

describe("link store", () => {
  it("tracks the 5 states per server, independently", async () => {
    const { bridge, link } = await startedApp();
    expect(link.stateOf("forge")).toBe("connected");
    for (const state of LINK_STATES) {
      bridge.setState("forge", state);
      expect(link.stateOf("forge")).toBe(state);
      expect(link.stateOf("salon")).toBe("connected");
    }
  });

  it("treats a server with no event yet as connecting, never as connected", async () => {
    const { link } = await startedApp();
    expect(link.stateOf("inconnu")).toBe("reconnecting");
  });

  it("switches connected -> offline -> connected with the right dates", async () => {
    vi.setSystemTime(10_000);
    const { bridge, link } = await startedApp();
    vi.setSystemTime(20_000);
    bridge.setState("forge", "offline");
    expect(link.eventOf("forge")?.lastContactAt).toBe(10_000);
    expect(link.eventOf("forge")?.since).toBe(20_000);
    vi.setSystemTime(30_000);
    bridge.setState("forge", "connected");
    expect(link.eventOf("forge")?.lastContactAt).toBe(30_000);
  });

  it("asks the bridge to retry now", async () => {
    const { bridge, link } = await startedApp();
    await link.retryNow("forge");
    expect(bridge.retries).toEqual(["forge"]);
  });

  it("turns operation outcomes into discreet notifications with the exact texts", async () => {
    const { bridge } = await startedApp();
    bridge.emitOperation({ opId: "1", serverId: "forge", outcome: "done" });
    bridge.emitOperation({ opId: "2", serverId: "salon", outcome: "not_executed" });
    bridge.emitOperation({ opId: "3", serverId: "forge", outcome: "unknown" });
    const toasts = useToastsStore();
    expect(toasts.items.map((t) => [t.kind, t.message])).toEqual([
      ["success", "forge : Fait pendant la coupure."],
      ["info", "nas-salon : Non exécuté. Tu peux relancer."],
      ["warn", "forge : Résultat inconnu. Vérifie l'état du serveur."],
    ]);
  });

  it("stops listening when reset", async () => {
    const { bridge, link } = await startedApp();
    link.reset();
    bridge.setState("forge", "offline");
    expect(link.eventOf("forge")).toBeUndefined();
  });
});

describe("toasts store", () => {
  it("stacks distinct notifications, shows 3 at most and keeps the newest visible", () => {
    freshBridge();
    const toasts = useToastsStore();
    for (const message of ["a", "b", "c", "d", "e"]) toasts.push({ kind: "info", message });
    expect(toasts.items).toHaveLength(5);
    expect(toasts.visible.map((t) => t.message)).toEqual(["c", "d", "e"]);
    expect(MAX_VISIBLE_TOASTS).toBe(3);
  });

  it("counts a repeated notification instead of stacking it (10 cuts in a minute = 1 line)", () => {
    freshBridge();
    const toasts = useToastsStore();
    for (let i = 0; i < 10; i++) toasts.push({ kind: "warn", message: "Reconnexion échouée." });
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.count).toBe(10);
  });

  it("does not merge the same text of another kind", () => {
    freshBridge();
    const toasts = useToastsStore();
    toasts.push({ kind: "info", message: "x" });
    toasts.push({ kind: "error", message: "x" });
    expect(toasts.items).toHaveLength(2);
  });

  it("dismisses on its own after the lifetime, and a repeat renews it", async () => {
    freshBridge();
    const toasts = useToastsStore();
    toasts.push({ kind: "info", message: "x" });
    await vi.advanceTimersByTimeAsync(TOAST_LIFETIME_MS - 1000);
    toasts.push({ kind: "info", message: "x" });
    await vi.advanceTimersByTimeAsync(TOAST_LIFETIME_MS - 1000);
    expect(toasts.items).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1500);
    expect(toasts.items).toHaveLength(0);
  });

  it("can be dismissed by hand and never grows without bound", () => {
    freshBridge();
    const toasts = useToastsStore();
    const id = toasts.push({ kind: "info", message: "x" });
    toasts.dismiss(id);
    expect(toasts.items).toHaveLength(0);
    for (let i = 0; i < 500; i++) toasts.push({ kind: "info", message: `m${i}` });
    expect(toasts.items.length).toBeLessThanOrEqual(50);
    toasts.clear();
    expect(toasts.items).toHaveLength(0);
  });
});

describe("servers store failure", () => {
  it("lets a failing bridge be retried", async () => {
    freshBridge();
    const store = useServersStore();
    const bridge = new SimulatedLinkBridge();
    vi.spyOn(bridge, "onServersChanged").mockRejectedValueOnce(new Error("pont en panne"));
    setLinkBridge(bridge);
    await expect(store.load()).rejects.toThrow("pont en panne");
    await store.load();
    await flushPromises();
    expect(store.loaded).toBe(true);
  });
});

describe("bridge contract", () => {
  it("replays the current servers and states at subscription, and a server added before is not lost", async () => {
    const { bridge } = freshBridge();
    bridge.seedServer({
      id: "x",
      name: "x",
      address: "x",
      host: "x",
      port: 7341,
      color: 2,
      role: "admin",
      username: "",
      remember: false,
    });
    bridge.setState("x", "offline");
    const lists: string[][] = [];
    const states: string[] = [];
    const stopServers = await bridge.onServersChanged((l) => lists.push(l.map((s) => s.id)));
    const stopStates = await bridge.onLinkState((e) => states.push(`${e.serverId}:${e.state}`));
    expect(lists).toEqual([["forge", "salon", "x"]]);
    expect(states).toContain("x:offline");
    bridge.dropServer("salon");
    expect(lists.at(-1)).toEqual(["forge", "x"]);
    stopServers();
    stopStates();
    bridge.dropServer("x");
    expect(lists).toHaveLength(2);
  });
});

describe("link store ordering and operations", () => {
  it("orders events by seq, never by the wall clock", async () => {
    const { bridge } = freshBridge();
    let push: (event: LinkStateEvent) => void = () => {};
    vi.spyOn(bridge, "onLinkState").mockImplementation(async (listener) => {
      push = listener;
      return () => {};
    });
    const link = useLinkStore();
    await link.start();
    const event = (state: LinkStateEvent["state"], seq: number, since: number): LinkStateEvent => ({
      serverId: "forge",
      seq,
      state,
      since,
      lastContactAt: null,
      nextRetryAt: null,
      blocked: null,
      reason: null,
      failedAttempts: 0,
    });
    push(event("offline", 5, 2000));
    // Un instantané en retard (seq plus petit) arrive ensuite : ignoré, même avec un `since` plus récent.
    push(event("connected", 4, 9999));
    expect(link.stateOf("forge")).toBe("offline");
    // Un rejeu du même seq ne change rien ; un seq supérieur gagne même avec une horloge reculée.
    push(event("connected", 5, 1));
    expect(link.stateOf("forge")).toBe("offline");
    push(event("connected", 6, 1));
    expect(link.stateOf("forge")).toBe("connected");
  });

  it("the simulated bridge numbers events strictly per server", async () => {
    const { bridge } = freshBridge();
    const seen: Record<string, number[]> = {};
    await bridge.onLinkState((e) => {
      const list = seen[e.serverId] ?? [];
      list.push(e.seq);
      seen[e.serverId] = list;
    });
    bridge.setState("forge", "offline");
    bridge.setState("forge", "connected");
    bridge.setState("salon", "offline");
    expect(seen.forge).toEqual([1, 2, 3]);
    expect(seen.salon).toEqual([1, 2]);
  });

  it("keeps the outcome by opId so a page finds the one of ITS action, bounded in count and age", async () => {
    const { bridge, link } = await startedApp();
    bridge.emitOperation({ opId: "mine", serverId: "forge", outcome: "done" });
    expect(link.outcomeOf("mine")).toBe("done");
    expect(link.outcomeOf("other")).toBeUndefined();
    for (let i = 0; i < 300; i++) {
      bridge.emitOperation({ opId: `op${i}`, serverId: "forge", outcome: "unknown" });
    }
    expect(Object.keys(link.operations).length).toBeLessThanOrEqual(100);
    expect(link.outcomeOf("mine")).toBeUndefined();
    expect(link.outcomeOf("op299")).toBe("unknown");
    vi.setSystemTime(Date.now() + 11 * 60_000);
    bridge.emitOperation({ opId: "late", serverId: "forge", outcome: "done" });
    expect(Object.keys(link.operations)).toEqual(["late"]);
  });
});

describe("real teardown", () => {
  it("stops following the bridge for real once reset (servers and link)", async () => {
    const { bridge, servers, link } = await startedApp();
    servers.reset();
    link.reset();
    bridge.dropServer("forge");
    bridge.setState("salon", "offline");
    bridge.emitOperation({ opId: "z", serverId: "salon", outcome: "done" });
    expect(servers.servers).toEqual([]);
    expect(link.eventOf("salon")).toBeUndefined();
    expect(link.outcomeOf("z")).toBeUndefined();
  });
});

describe("link store subscriptions", () => {
  function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
      resolve = r;
    });
    return { promise, resolve };
  }

  it("a stop() during the pending subscription leaks no listener", async () => {
    const { bridge } = freshBridge();
    const slow = deferred<() => void>();
    const unsubscribe = vi.fn();
    vi.spyOn(bridge, "onLinkState").mockReturnValue(slow.promise);
    const operations = vi.spyOn(bridge, "onOperation");
    const link = useLinkStore();
    const started = link.start();
    link.stop();
    slow.resolve(unsubscribe);
    await started;
    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(operations).not.toHaveBeenCalled();
  });

  it("keeps each unsubscribe as soon as it is obtained: a failure of the second releases the first", async () => {
    const { bridge } = freshBridge();
    const first = vi.fn();
    vi.spyOn(bridge, "onLinkState").mockResolvedValue(first);
    vi.spyOn(bridge, "onOperation").mockRejectedValue(new Error("pont en panne"));
    const link = useLinkStore();
    await link.start();
    expect(first).toHaveBeenCalledTimes(1);
    link.stop();
  });

  it("shows « Hors ligne » (not a frozen « Reconnexion… ») when the subscription fails, then retries with growing delays and recovers", async () => {
    const { bridge } = freshBridge();
    const link = useLinkStore();
    const attempts = vi
      .spyOn(bridge, "onLinkState")
      .mockRejectedValueOnce(new Error("1"))
      .mockRejectedValueOnce(new Error("2"));
    await link.start();
    expect(link.subscriptionFailed).toBe(true);
    expect(link.stateOf("forge")).toBe("offline");
    await vi.advanceTimersByTimeAsync(RESUBSCRIBE_BASE_MS - 10);
    expect(attempts).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(20);
    expect(attempts).toHaveBeenCalledTimes(2);
    // Deuxième échec : l'espacement double.
    await vi.advanceTimersByTimeAsync(RESUBSCRIBE_BASE_MS * 2 - 60);
    expect(attempts).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(100);
    expect(attempts).toHaveBeenCalledTimes(3);
    // Troisième tentative : les vrais abonnements réussissent.
    expect(link.subscriptionFailed).toBe(false);
    expect(link.stateOf("forge")).toBe("connected");
    link.stop();
  });

  it("« Réessayer maintenant » relaunches a failed subscription at once", async () => {
    const { bridge } = freshBridge();
    const link = useLinkStore();
    vi.spyOn(bridge, "onLinkState").mockRejectedValueOnce(new Error("x"));
    await link.start();
    expect(link.subscriptionFailed).toBe(true);
    await link.retryNow("forge");
    await vi.advanceTimersByTimeAsync(0);
    expect(link.subscriptionFailed).toBe(false);
    link.stop();
  });

  it("stop() cancels the pending retry", async () => {
    const { bridge } = freshBridge();
    const link = useLinkStore();
    const attempts = vi.spyOn(bridge, "onLinkState").mockRejectedValue(new Error("x"));
    await link.start();
    link.stop();
    await vi.advanceTimersByTimeAsync(RESUBSCRIBE_MAX_MS * 2);
    expect(attempts).toHaveBeenCalledTimes(1);
  });
});

describe("servers store with a bridge that never answers", () => {
  it("gives up after the delay with an error state instead of blocking the router guard forever", async () => {
    const { bridge } = freshBridge();
    const store = useServersStore();
    const unsubscribe = vi.fn();
    let release!: (value: () => void) => void;
    vi.spyOn(bridge, "onServersChanged").mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );
    const outcome = store.load().then(
      () => "ok",
      () => "failed",
    );
    await vi.advanceTimersByTimeAsync(LOAD_TIMEOUT_MS + 10);
    expect(await outcome).toBe("failed");
    expect(store.loadFailed).toBe(true);
    expect(store.loaded).toBe(false);
    // L'abonnement arrivé trop tard est libéré, pas laissé actif.
    release(unsubscribe);
    await vi.advanceTimersByTimeAsync(0);
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("lets the router start without servers (welcome page) when the bridge is silent", async () => {
    const { bridge } = freshBridge();
    vi.spyOn(bridge, "onServersChanged").mockReturnValue(new Promise(() => {}));
    const { createAppRouter } = await import("@/router");
    const { createMemoryHistory } = await import("vue-router");
    const router = createAppRouter(createMemoryHistory());
    const navigation = router.push("/");
    for (let i = 0; i < 4; i++) await vi.advanceTimersByTimeAsync(LOAD_TIMEOUT_MS);
    await navigation;
    expect(router.currentRoute.value.name).toBe("welcome");
  });
});
