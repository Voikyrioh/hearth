import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ServiceOperation, ServiceOperationEvent } from "@/link";
import { freshBridge } from "@/test/app";
import { SERVICE_OPS_RETRY_MS, useServiceOpsStore } from "./serviceOps";
import { useServicesStore } from "./services";
import { useToastsStore } from "./toasts";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function started() {
  const ctx = freshBridge();
  const store = useServiceOpsStore();
  await store.start();
  // La liste des services donne les noms (l'opération n'en porte pas).
  await useServicesStore().start();
  return { ...ctx, store, toasts: useToastsStore() };
}

function op(patch: Partial<ServiceOperation> = {}): ServiceOperation {
  return {
    id: "A",
    key: "h_1",
    kind: "deploy",
    status: "running",
    step: null,
    steps: [],
    percent: null,
    itemsDone: null,
    bytesDone: null,
    bytesTotal: null,
    etaS: null,
    cancellable: true,
    decision: null,
    cause: null,
    ...patch,
  };
}

function upsert(
  operation: ServiceOperation,
  seq: number,
  serverId = "forge",
): ServiceOperationEvent {
  return { kind: "upsert", serverId, seq, operation };
}

describe("le magasin des opérations de service", () => {
  it("tient les opérations de chaque serveur à part, la plus ancienne d'abord", async () => {
    const { store } = await started();
    store.apply(upsert(op({ id: "A" }), 1));
    store.apply(upsert(op({ id: "B" }), 2));
    store.apply(upsert(op({ id: "C" }), 3, "salon"));
    expect(store.of("forge").map((tracked) => tracked.operation.id)).toEqual(["A", "B"]);
    expect(store.of("salon").map((tracked) => tracked.operation.id)).toEqual(["C"]);
    store.reset();
  });

  it("écarte un rejeu : un numéro qui n'est pas plus grand ne remplace rien", async () => {
    const { store } = await started();
    store.apply(upsert(op({ percent: 50 }), 5));
    store.apply(upsert(op({ percent: 10 }), 5));
    store.apply(upsert(op({ percent: 20 }), 4));
    expect(store.of("forge")[0]?.operation.percent).toBe(50);
    store.apply(upsert(op({ percent: 60 }), 6));
    expect(store.of("forge")[0]?.operation.percent).toBe(60);
    store.reset();
  });

  it("garde l'ordre d'arrivée quand une opération change", async () => {
    const { store } = await started();
    store.apply(upsert(op({ id: "A" }), 1));
    store.apply(upsert(op({ id: "B" }), 2));
    store.apply(upsert(op({ id: "A", percent: 90 }), 3));
    expect(store.of("forge").map((tracked) => tracked.operation.id)).toEqual(["A", "B"]);
    store.reset();
  });

  it("annonce l'issue une seule fois, puis l'acquitte auprès de la coquille", async () => {
    const { store, toasts, bridge } = await started();
    bridge.services.seed("forge", [{ key: "h_1", name: "terraria", origin: "hearth" }]);
    store.apply(upsert(op(), 1));
    store.apply(upsert(op({ status: "succeeded" }), 2));
    // Le même état terminé répété (agent, relecture après coupure) : rien de plus.
    store.apply(upsert(op({ status: "succeeded" }), 2));
    store.apply(upsert(op({ status: "succeeded" }), 3));
    await flushPromises();
    expect(store.of("forge")).toEqual([]);
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "Le service terraria est en marche.",
    ]);
    expect(toasts.items[0]?.kind).toBe("success");
    // Le rejeu d'une issue déjà dite (numéro plus grand : la coquille la rejoue faute d'acquittement) est réacquitté.
    expect(bridge.serviceOps.acknowledged).toEqual([
      { serverId: "forge", ids: ["A"] },
      { serverId: "forge", ids: ["A"] },
    ]);
    store.reset();
  });

  it("dit le nom du service même quand l'opération l'a supprimé", async () => {
    const { store, toasts, bridge } = await started();
    bridge.services.seed("forge", [{ key: "h_1", name: "terraria", origin: "hearth" }]);
    store.apply(upsert(op({ kind: "delete" }), 1));
    bridge.services.remove("forge", "h_1");
    store.apply(upsert(op({ kind: "delete", status: "succeeded" }), 2));
    expect(toasts.items[0]?.message).toBe("La suppression de terraria a réussi.");
    store.reset();
  });

  it("une opération disparue de l'agent n'annonce aucune issue", async () => {
    const { store, toasts } = await started();
    store.apply(upsert(op(), 1));
    store.apply({ kind: "gone", serverId: "forge", seq: 2, id: "A" });
    expect(store.of("forge")).toEqual([]);
    expect(toasts.items).toEqual([]);
    store.reset();
  });

  it("une opération qui attend une décision reste suivie, et une issue ratée se dit en erreur", async () => {
    const { store, toasts } = await started();
    store.apply(
      upsert(
        op({ status: "awaiting_decision", decision: { choices: ["keep"], expiresAt: null } }),
        1,
      ),
    );
    expect(store.of("forge")).toHaveLength(1);
    store.apply(upsert(op({ id: "B", status: "failed" }), 2));
    expect(toasts.items[0]?.kind).toBe("error");
    store.reset();
  });

  it("reçoit ce que le pont rejoue à l'abonnement, même hors ligne", async () => {
    const ctx = freshBridge();
    ctx.bridge.serviceOps.accept("forge", { key: "h_1", kind: "update", step: "recreate" });
    const store = useServiceOpsStore();
    await store.start();
    expect(store.of("forge").map((tracked) => tracked.operation.kind)).toEqual(["update"]);
    store.reset();
  });

  it("oublie un serveur retiré", async () => {
    const { store } = await started();
    store.apply(upsert(op(), 1));
    store.forget("forge");
    expect(store.of("forge")).toEqual([]);
    store.reset();
  });

  it("réessaie l'abonnement après un échec, sans rien inventer entre-temps", async () => {
    const { store, bridge } = await started();
    store.reset();
    bridge.serviceOps.failSubscribe = true;
    await store.start();
    expect(store.failed).toBe(true);
    bridge.serviceOps.failSubscribe = false;
    await vi.advanceTimersByTimeAsync(SERVICE_OPS_RETRY_MS + 1);
    expect(store.failed).toBe(false);
    store.reset();
  });
});

describe("une opération suivie par une fenêtre (HRT-69)", () => {
  it("son issue n'est pas annoncée par une notification : la fenêtre la reçoit et la règle", async () => {
    const { store, toasts, bridge } = await started();
    store.watch("forge", "A");
    store.apply(upsert(op({ step: "pull", percent: 20 }), 1));
    expect(store.get("forge", "A")?.operation.percent).toBe(20);
    store.apply(upsert(op({ status: "failed", cause: "tag_not_found" }), 2));
    await flushPromises();
    expect(toasts.items).toEqual([]);
    expect(store.of("forge")).toEqual([]);
    expect(store.finishedOf("forge", "A")?.operation).toMatchObject({
      status: "failed",
      cause: "tag_not_found",
    });
    // Rien n'est acquitté tant que la fenêtre n'a pas dit l'issue : la coquille la rejouerait au besoin.
    expect(bridge.serviceOps.acknowledged).toEqual([]);
    store.settle("forge", "A");
    await flushPromises();
    expect(store.finishedOf("forge", "A")).toBeUndefined();
    expect(toasts.items).toEqual([]);
    expect(bridge.serviceOps.acknowledged).toEqual([{ serverId: "forge", ids: ["A"] }]);
  });

  it("une fenêtre qui se ferme avant l'issue la laisse à la notification, dite une seule fois", async () => {
    const { store, toasts } = await started();
    store.watch("forge", "A");
    store.apply(upsert(op(), 1));
    store.release("forge", "A");
    store.apply(upsert(op({ status: "succeeded" }), 2));
    await flushPromises();
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "Le service ce service est en marche.",
    ]);
    store.apply(upsert(op({ status: "succeeded" }), 3));
    expect(toasts.items).toHaveLength(1);
  });

  it("une fenêtre qui se ferme APRÈS l'issue la fait annoncer tout de suite", async () => {
    const { store, toasts } = await started();
    store.watch("forge", "A");
    store.apply(upsert(op({ status: "failed", cause: "disk_full" }), 1));
    expect(toasts.items).toEqual([]);
    store.release("forge", "A");
    await flushPromises();
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "Le déploiement de ce service a échoué : disque plein.",
    ]);
  });

  it("une issue arrivée avant la réponse qui désigne l'opération est rendue à la fenêtre qui la suit ensuite", async () => {
    const { store } = await started();
    store.apply(upsert(op({ status: "succeeded" }), 1));
    await flushPromises();
    store.watch("forge", "A");
    expect(store.finishedOf("forge", "A")?.operation.status).toBe("succeeded");
  });
});
