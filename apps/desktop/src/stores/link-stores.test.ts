import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkStateEvent } from "@/link";
import { LINK_STATES, SimulatedLinkBridge, setLinkBridge } from "@/link";
import { freshBridge, startedApp } from "@/test/app";
import { useLinkStore } from "./link";
import { useServersStore } from "./servers";
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
    bridge.removeServer("forge");
    expect(servers.servers.map((s) => s.id)).toEqual(["salon"]);
    bridge.addServer({ id: "x", name: "x", address: "x", color: 2, role: "admin" });
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
    bridge.addServer({ id: "x", name: "x", address: "x", color: 2, role: "admin" });
    bridge.setState("x", "offline");
    const lists: string[][] = [];
    const states: string[] = [];
    const stopServers = await bridge.onServersChanged((l) => lists.push(l.map((s) => s.id)));
    const stopStates = await bridge.onLinkState((e) => states.push(`${e.serverId}:${e.state}`));
    expect(lists).toEqual([["forge", "salon", "x"]]);
    expect(states).toContain("x:offline");
    bridge.removeServer("salon");
    expect(lists.at(-1)).toEqual(["forge", "x"]);
    stopServers();
    stopStates();
    bridge.removeServer("x");
    expect(lists).toHaveLength(2);
  });
});

describe("link store ordering and operations", () => {
  it("never lets an older event overwrite a newer one", async () => {
    const { bridge } = freshBridge();
    let push: (event: LinkStateEvent) => void = () => {};
    vi.spyOn(bridge, "onLinkState").mockImplementation(async (listener) => {
      push = listener;
      return () => {};
    });
    const link = useLinkStore();
    await link.start();
    const event = (state: LinkStateEvent["state"], since: number): LinkStateEvent => ({
      serverId: "forge",
      state,
      since,
      lastContactAt: null,
      nextRetryAt: null,
    });
    push(event("offline", 2000));
    // Un instantané en retard (since plus ancien) arrive ensuite : ignoré.
    push(event("connected", 1000));
    expect(link.stateOf("forge")).toBe("offline");
    push(event("connected", 3000));
    expect(link.stateOf("forge")).toBe("connected");
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
    bridge.removeServer("forge");
    bridge.setState("salon", "offline");
    bridge.emitOperation({ opId: "z", serverId: "salon", outcome: "done" });
    expect(servers.servers).toEqual([]);
    expect(link.eventOf("salon")).toBeUndefined();
    expect(link.outcomeOf("z")).toBeUndefined();
  });
});
