import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { show } from "@/services/view";
import { freshBridge } from "@/test/app";
import { SERVICES_RETRY_MS, useServicesStore } from "./services";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function started() {
  const ctx = freshBridge();
  const store = useServicesStore();
  await store.start();
  return { ...ctx, store };
}

describe("services store", () => {
  it("tient chaque serveur à part (BR-ENGINE-018) et dit « chargement » tant que rien n'est arrivé", async () => {
    const { bridge, store } = await started();
    expect(show(store.of("forge"))).toEqual({ kind: "loading" });
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    bridge.services.seed("salon", [], { state: "stopped" });
    expect(show(store.of("forge"))).toMatchObject({ engine: { state: "ready" } });
    expect(show(store.of("salon"))).toMatchObject({ engine: { state: "stopped" } });
    store.reset();
  });

  it("rejoue le dernier état à l'abonnement : rien n'est perdu avant l'écoute", async () => {
    const ctx = freshBridge();
    ctx.bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    const store = useServicesStore();
    await store.start();
    expect(store.of("forge")?.services).toHaveLength(1);
    store.reset();
  });

  it("écarte un événement dont le numéro n'est pas plus grand que le dernier", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    const first = store.of("forge");
    store.apply({
      kind: "state",
      serverId: "forge",
      seq: first?.seq ?? 0,
      engine:
        first?.engine ??
        (() => {
          throw new Error("moteur attendu");
        })(),
      services: [],
    });
    expect(store.of("forge")?.services).toHaveLength(1);
    store.reset();
  });

  it("garde l'état d'avant et sa date quand le moteur change (« Dernier état connu »)", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", []);
    vi.setSystemTime(Date.now() + 40_000);
    bridge.services.setEngine("forge", { state: "unresponsive" });
    const entry = store.of("forge");
    expect(entry?.previous?.state).toBe("ready");
    expect(entry?.previous?.at).toBeLessThan(Date.now());
    // L'état d'avant reste tant que le moteur ne change pas d'état.
    bridge.services.setEngine("forge", { state: "unresponsive", version: "24.0.8" });
    expect(store.of("forge")?.previous?.state).toBe("ready");
    store.reset();
  });

  it("garde la dernière liste vue quand le moteur était prêt, pour la montrer grisée (BR-SVC-018)", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [
      { key: "c_1", name: "a" },
      { key: "c_2", name: "b" },
    ]);
    bridge.services.seed("forge", [], { state: "unsupported_version", version: "19.03" });
    // Même moteur (même nom) : la liste d'avant reste, datée de la dernière fois prêt.
    const shown = show(store.of("forge"));
    expect(shown).toMatchObject({ kind: "engine", list: { kind: "stale" } });
    // Le moteur détecté n'est plus le même : aucune liste ancienne.
    bridge.services.seed("forge", [], { state: "unsupported_engine", name: "Podman Engine" });
    expect(show(store.of("forge"))).toMatchObject({ list: { kind: "none" } });
    store.reset();
  });

  it("un agent sans la capacité remplace tout ; mis à jour, l'état complet revient", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    bridge.services.setUnsupported("forge", "0.1.0");
    expect(show(store.of("forge"))).toEqual({ kind: "old-agent", agentVersion: "0.1.0" });
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    expect(show(store.of("forge")).kind).toBe("engine");
    store.reset();
  });

  it("oublie un serveur retiré du carnet", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", []);
    store.forget("forge");
    expect(store.of("forge")).toBeUndefined();
    store.reset();
  });

  it("la lecture en échec se dit, puis reprend toute seule et sur demande", async () => {
    const ctx = freshBridge();
    const store = useServicesStore();
    ctx.bridge.services.failSubscribe = true;
    await store.start();
    expect(store.failed).toBe(true);
    expect(ctx.bridge.services.subscriptions).toBe(1);
    // La reprise automatique.
    ctx.bridge.services.failSubscribe = false;
    ctx.bridge.services.seed("forge", []);
    vi.advanceTimersByTime(SERVICES_RETRY_MS);
    await flushPromises();
    expect(store.failed).toBe(false);
    expect(store.of("forge")).toBeDefined();
    // Sur demande (« Vérifier à nouveau » sous l'erreur de lecture).
    await store.retry();
    expect(ctx.bridge.services.subscriptions).toBe(3);
    store.reset();
  });

  it("« Vérifier à nouveau » relance la détection, une demande à la fois, et dit son échec", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [], { state: "stopped" });
    const first = store.recheck("forge");
    expect(store.recheckOf("forge")).toBe("busy");
    await store.recheck("forge");
    await first;
    expect(bridge.services.rechecks).toEqual(["forge"]);
    expect(store.recheckOf("forge")).toBe("idle");
    bridge.services.failRecheck = true;
    await store.recheck("forge");
    expect(store.recheckOf("forge")).toBe("failed");
    store.reset();
  });
});
