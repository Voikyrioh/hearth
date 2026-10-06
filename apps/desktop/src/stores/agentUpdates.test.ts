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
    expect(store.noticeOf("forge")?.tone).toBe("crit");
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
