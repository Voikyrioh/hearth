import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { type AgentUpdateView, LinkCommandError, SimulatedLinkBridge } from "@/link";
import { mountContext } from "@/test/mount";
import { useAgentUpdatesStore } from "./agentUpdates";
import { useToastsStore } from "./toasts";

afterEach(() => {
  document.body.innerHTML = "";
});

const progress = (step: "download" | "verify" | "done", percent: number | null = null) => ({
  version: "0.2.0",
  step,
  percent,
  outcome: null,
  reason: null,
});

/** Le pont simulé dont chaque lecture de l'état attend d'être relâchée par le test. */
class SlowBridge extends SimulatedLinkBridge {
  readonly reads: Array<(view: AgentUpdateView | Error) => void> = [];

  override getAgentUpdate(serverId: string): Promise<AgentUpdateView> {
    const read = super.getAgentUpdate(serverId);
    return new Promise((resolve, reject) => {
      this.reads.push((outcome) => {
        if (outcome instanceof Error) reject(outcome);
        else resolve(outcome);
      });
      void read;
    });
  }
}

async function started() {
  const ctx = await mountContext();
  const store = useAgentUpdatesStore();
  await store.start();
  await flushPromises();
  return { ...ctx, store };
}

describe("la lecture de l'état", () => {
  it("reads every connected server at the start, and again each time its link returns (BR-UPDATE-017)", async () => {
    const { bridge, store } = await started();
    expect(bridge.calls.filter((call) => call === "agent-update read")).toHaveLength(2);
    expect(store.entry("forge").view?.current).toBe("0.1.0");
    bridge.setState("forge", "offline");
    await flushPromises();
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(bridge.calls.filter((call) => call === "agent-update read")).toHaveLength(3);
  });

  it("keeps the last reading when a new one fails, without any message", async () => {
    const { bridge, store } = await started();
    bridge.setState("forge", "offline");
    await flushPromises();
    await store.refresh("forge");
    expect(store.entry("forge").status).toBe("error");
    expect(store.entry("forge").view?.current).toBe("0.1.0");
    expect(useToastsStore().items).toEqual([]);
  });

  it("never lets an older reading overwrite a newer one", async () => {
    const ctx = await mountContext();
    const slow = new SlowBridge({ retryDelayMs: -1 });
    const { setLinkBridge } = await import("@/link");
    setLinkBridge(slow);
    const store = useAgentUpdatesStore();
    const first = store.refresh("forge");
    const second = store.refresh("forge");
    const view = (current: string): AgentUpdateView => ({
      current,
      managed: false,
      inProgress: false,
      progress: null,
      last: null,
      available: null,
    });
    // La seconde répond d'abord, la première (plus ancienne) arrive après : elle est écartée.
    slow.reads[1]?.(view("0.2.0"));
    await second;
    slow.reads[0]?.(view("0.1.0"));
    await first;
    expect(store.entry("forge").view?.current).toBe("0.2.0");
    void ctx;
  });

  it("lets a progress event received during a reading win over that reading", async () => {
    const ctx = await mountContext();
    const slow = new SlowBridge({ retryDelayMs: -1 });
    const { setLinkBridge } = await import("@/link");
    setLinkBridge(slow);
    const store = useAgentUpdatesStore();
    await store.start();
    const reading = store.refresh("forge");
    // L'événement arrive pendant la lecture, qui rendra un état périmé (rien en cours).
    slow.agentUpdates.advance("forge", "verify");
    slow.reads.at(-1)?.({
      current: "0.1.0",
      managed: false,
      inProgress: false,
      progress: null,
      last: null,
      available: { version: "0.2.0" },
    });
    await reading;
    expect(store.progressOf("forge")?.step).toBe("verify");
    void ctx;
  });
});

describe("le flux de progression", () => {
  it("tells the end of an update by a discreet notification naming the server, whatever page is shown", async () => {
    const { bridge } = await started();
    bridge.agentUpdates.seed("forge", { progress: progress("verify") });
    bridge.agentUpdates.complete("forge", "rolled_back", "no_answer");
    await flushPromises();
    const toasts = useToastsStore();
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "forge : Mise à jour de l'agent annulée. Le nouvel agent n'a pas répondu. Retour à la version précédente.",
    ]);
    expect(toasts.items[0]?.kind).toBe("warn");
  });

  it("shows nothing as running once the end is announced, even before the reading comes back", async () => {
    const { bridge, store } = await started();
    bridge.agentUpdates.advance("forge", "download", 10);
    await flushPromises();
    expect(store.runningOf("forge")).toBe(true);
    bridge.agentUpdates.complete("forge", "failed", "unreachable");
    expect(store.runningOf("forge")).toBe(false);
    // Le résultat vient de la relecture (le signal `done` ne fait que la déclencher) : en attendant, rien
    // n'est montré, ni l'ancien résultat ni un résultat inventé par le signal.
    expect(store.noticeOf("forge")).toBeNull();
    await flushPromises();
    expect(store.noticeOf("forge")?.tone).toBe("crit");
  });
});

describe("la liste des serveurs (BR-UPDATE-023)", () => {
  it("shows « Mise à jour disponible » on the server that has one, and only on it", async () => {
    const ctx = await mountContext();
    ctx.bridge.agentUpdates.seed("salon", { target: null });
    await ctx.router.push("/servers");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    const rows = wrapper.findAll("[data-server-row]");
    expect(rows.map((row) => row.attributes("data-server-row"))).toEqual(["forge", "salon"]);
    expect(rows[0]?.find("[data-update-available]").text()).toBe("Mise à jour disponible");
    expect(rows[1]?.find("[data-update-available]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("puts « État du serveur » cards in the settings, one per registered server", async () => {
    const ctx = await mountContext();
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    expect(wrapper.findAll("[data-agent-card] h3").map((h) => h.text())).toEqual([
      "État du serveur : forge",
      "État du serveur : nas-salon",
    ]);
    wrapper.unmount();
  });
});

describe("un échec typé", () => {
  it("is an error of the bridge, never swallowed as a result", async () => {
    const { bridge } = await started();
    bridge.setState("forge", "offline");
    await expect(bridge.getAgentUpdate("forge")).rejects.toBeInstanceOf(LinkCommandError);
  });
});

describe("l'annonce du résultat vient de la relecture, une seule fois", () => {
  function clocked() {
    let at = Date.parse("2026-10-06T10:00:00Z");
    return () => {
      at += 60_000;
      return at;
    };
  }

  async function startedAt(now: () => number) {
    const ctx = await mountContext({ now });
    const store = useAgentUpdatesStore();
    await store.start();
    await flushPromises();
    return { ...ctx, store, toasts: useToastsStore() };
  }

  async function backOnline(bridge: SimulatedLinkBridge) {
    bridge.setState("forge", "offline");
    await flushPromises();
    bridge.setState("forge", "connected");
    await flushPromises();
  }

  it("announces a result whose `done` was never seen on the stream, when the link returns", async () => {
    const { bridge, toasts } = await startedAt(clocked());
    bridge.agentUpdates.seed("forge", { progress: progress("verify") });
    // Le nouvel agent a annoncé `done` avant le réabonnement : aucun signal ne parvient au client.
    bridge.agentUpdates.complete("forge", "rolled_back", "no_answer", false);
    await flushPromises();
    expect(toasts.items).toEqual([]);
    await backOnline(bridge);
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "forge : Mise à jour de l'agent annulée. Le nouvel agent n'a pas répondu. Retour à la version précédente.",
    ]);
    expect(bridge.calls.filter((call) => call.startsWith("agent-update ack "))).toHaveLength(1);
    // Une nouvelle lecture (retour du lien suivant) ne l'annonce pas de nouveau.
    await backOnline(bridge);
    expect(toasts.items).toHaveLength(1);
  });

  it("announces two successive results of the same outcome twice, one each", async () => {
    const { bridge, toasts } = await startedAt(clocked());
    for (let round = 1; round <= 2; round += 1) {
      bridge.agentUpdates.seed("forge", { progress: progress("verify") });
      bridge.agentUpdates.complete("forge", "failed", "unreachable", false);
      await backOnline(bridge);
      toasts.clear();
      // Le même résultat relu ne repart pas.
      await backOnline(bridge);
      expect(toasts.items).toEqual([]);
      expect(bridge.calls.filter((call) => call.startsWith("agent-update ack "))).toHaveLength(
        round,
      );
    }
  });

  it("does not announce again a result already seen before the client was relaunched", async () => {
    const { bridge, store, toasts } = await startedAt(clocked());
    bridge.agentUpdates.complete("forge", "succeeded", null, false);
    await backOnline(bridge);
    expect(toasts.items).toHaveLength(1);
    // Relance du client : le store repart de zéro, la coquille (ici le pont) se souvient.
    store.reset();
    toasts.clear();
    await store.start();
    await flushPromises();
    expect(toasts.items).toEqual([]);
    // Un résultat plus récent, lui, est annoncé.
    bridge.agentUpdates.complete("forge", "succeeded", null, false);
    await backOnline(bridge);
    expect(toasts.items).toHaveLength(1);
  });

  it("announces a result of an update launched by another administrator, never followed here", async () => {
    const { bridge, toasts } = await startedAt(clocked());
    bridge.agentUpdates.complete("forge", "failed", "bad_checksum", false);
    await backOnline(bridge);
    expect(toasts.items.map((toast) => toast.kind)).toEqual(["error"]);
  });

  it("does not announce a result while an update is still running", async () => {
    const { bridge, toasts, store } = await startedAt(clocked());
    bridge.agentUpdates.complete("forge", "succeeded", null, false);
    bridge.agentUpdates.seed("forge", { progress: progress("download", 10) });
    await store.refresh("forge");
    expect(toasts.items).toEqual([]);
  });

  it("announces a result even when the comparison of the two clocks says it is not recent", async () => {
    const { bridge, toasts } = await startedAt(clocked());
    // Serveur très en avance (ou en retard) sur le PC : `recent` est faux, le résultat n'est pas perdu.
    bridge.agentUpdates.seed("forge", {
      last: {
        version: "0.2.0",
        previous: "0.1.0",
        outcome: "succeeded",
        reason: null,
        at: "2026-10-09T10:00:00Z",
        recent: false,
        announced: false,
      },
    });
    await backOnline(bridge);
    expect(toasts.items.map((toast) => toast.kind)).toEqual(["success"]);
    await backOnline(bridge);
    expect(toasts.items).toHaveLength(1);
  });
});
