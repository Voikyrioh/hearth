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
      engineReceivedAt: null,
      engine:
        first?.engine ??
        (() => {
          throw new Error("moteur attendu");
        })(),
      services: [],
      unreadable: 0,
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

  describe("le repère de réception vient de la liaison, jamais de l'heure du traitement (I4)", () => {
    it("un état REJOUÉ à l'abonnement garde sa date de réception d'origine", async () => {
      const { bridge } = freshBridge();
      const received = Date.now() - 2 * 3_600_000;
      bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
        state: "stopped",
        listCurrentAt: new Date(received - 30_000).toISOString(),
        at: new Date(received).toISOString(),
      });
      bridge.services.setEngineReceivedAt("forge", received);
      // La fenêtre arrive après coup : l'état lui est rejoué (même chemin que `list_services_states`).
      const store = useServicesStore();
      await store.start();
      expect(store.of("forge")?.engineReceivedAt).toBe(received);
      // Moteur non prêt : la liste date de list_current_at (30 s avant la réception), pas du rejeu.
      expect(show(store.of("forge"))).toMatchObject({
        list: { kind: "stale", since: received - 30_000 },
      });
      store.reset();
    });

    it("un instantané relu du disque au démarrage garde sa date, ou n'en a aucune (jamais l'heure du chargement)", async () => {
      const { bridge } = freshBridge();
      bridge.services.seed("forge", [{ key: "c_1", name: "a" }], { state: "stopped" });
      bridge.services.setEngineReceivedAt("forge", null);
      const store = useServicesStore();
      await store.start();
      expect(store.of("forge")?.engineReceivedAt).toBeNull();
      expect(show(store.of("forge"))).toMatchObject({ list: { kind: "stale", since: null } });
      store.reset();
    });

    it("un lot qui ne republie pas la vue garde le repère de réception de la liaison", async () => {
      const { bridge, store } = await started();
      bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
      const first = store.of("forge")?.engineReceivedAt;
      vi.advanceTimersByTime(20_000);
      bridge.services.upsert("forge", { key: "c_2", name: "b" });
      expect(store.of("forge")?.engineReceivedAt).toBe(first);
      store.reset();
    });

    it("moteur arrêté depuis peu sur un serveur calme : l'âge part de list_current_at, pas du dernier événement", async () => {
      const { bridge, store } = await started();
      // Liste prête, puis trois heures sans aucun lot.
      bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
      vi.advanceTimersByTime(3 * 3_600_000);
      // Le moteur s'arrête : la vue est dite il y a un instant, la liste était à jour 10 s avant.
      bridge.services.setEngine("forge", {
        state: "stopped",
        listCurrentAt: new Date(Date.now() - 10_000).toISOString(),
      });
      const shown = show(store.of("forge"));
      expect(shown).toMatchObject({ list: { kind: "stale" } });
      if (shown.kind === "engine" && shown.list.kind === "stale" && shown.list.since !== null) {
        expect(Date.now() - shown.list.since).toBe(10_000);
      } else {
        throw new Error("liste périmée datée attendue");
      }
      store.reset();
    });
  });

  it("un lot qui passe le drapeau « liste non relisible » à vrai puis à faux arrive jusqu'à l'affichage", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }]);
    expect(show(store.of("forge"))).toMatchObject({ list: { kind: "live", degraded: null } });
    const currentAt = new Date(Date.now() - 60_000).toISOString();
    bridge.services.setEngine("forge", { listDegraded: true, listCurrentAt: currentAt });
    expect(show(store.of("forge"))).toMatchObject({
      list: { kind: "live", degraded: { currentAt: Date.parse(currentAt) } },
    });
    bridge.services.setEngine("forge", { listDegraded: false });
    expect(show(store.of("forge"))).toMatchObject({ list: { kind: "live", degraded: null } });
    store.reset();
  });

  it("l'âge de la liste grandit avec le temps écoulé chez ce poste, et un lot qui ne republie pas la vue ne le remet pas à zéro", async () => {
    const { bridge, store } = await started();
    const sixtySecondsAgo = new Date(Date.now() - 60_000).toISOString();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
      listDegraded: true,
      listCurrentAt: sixtySecondsAgo,
    });
    const age = () => {
      const shown = show(store.of("forge"));
      if (shown.kind !== "engine" || shown.list.kind !== "live" || !shown.list.degraded)
        return null;
      const { currentAt } = shown.list.degraded;
      return currentAt === null ? null : Date.now() - currentAt;
    };
    expect(age()).toBe(60_000);
    vi.advanceTimersByTime(10_000);
    expect(age()).toBe(70_000);
    // Un lot qui ne touche qu'un service : la vue du moteur est celle d'avant, le repère de réception reste.
    bridge.services.upsert("forge", { key: "c_2", name: "b" });
    expect(age()).toBe(70_000);
    vi.advanceTimersByTime(5_000);
    bridge.services.upsert("forge", { key: "c_3", name: "c" });
    expect(age()).toBe(75_000);
    // Une vue refabriquée (même dernier moment à jour, heure de l'agent plus tardive) : l'âge reste continu.
    bridge.services.setEngine("forge", {});
    expect(age()).toBe(75_000);
    store.reset();
  });

  it("à la reconnexion, un nouvel instantané du moteur donne son propre âge", async () => {
    const { bridge, store } = await started();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
      listDegraded: true,
      listCurrentAt: new Date(Date.now() - 30_000).toISOString(),
    });
    vi.advanceTimersByTime(120_000);
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
      listDegraded: true,
      listCurrentAt: new Date(Date.now() - 4_000).toISOString(),
    });
    const shown = show(store.of("forge"));
    expect(shown).toMatchObject({ list: { degraded: { currentAt: Date.now() - 4_000 } } });
    store.reset();
  });

  it("un agent plus ancien (sans heure) : la date de sa dernière lecture, comme avant ; jamais inventée", async () => {
    const { bridge, store } = await started();
    const readAt = new Date(Date.now() - 90_000).toISOString();
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
      listDegraded: true,
      listReadAt: readAt,
      at: null,
      listCurrentAt: null,
    });
    expect(show(store.of("forge"))).toMatchObject({
      list: { degraded: { currentAt: Date.parse(readAt) } },
    });
    bridge.services.seed("forge", [{ key: "c_1", name: "a" }], {
      listDegraded: true,
      listReadAt: null,
      at: null,
      listCurrentAt: null,
    });
    expect(show(store.of("forge"))).toMatchObject({ list: { degraded: { currentAt: null } } });
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
