import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OTHER_FINGERPRINT, SAMPLE_AGENT, setLinkBridge } from "@/link";
import { freshBridge } from "@/test/app";
import { RESUBSCRIBE_BASE_MS, useLinkStore } from "./link";
import { useToastsStore } from "./toasts";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

const SUBSCRIBE_LABEL =
  "Hearth n'arrive pas à suivre l'état de tes serveurs. Nouvelle tentative en cours.";

describe("subscription follow-ups (review HRT-09)", () => {
  it("never shows « Connecté » without an active listener, even after a replayed event", async () => {
    const { bridge } = freshBridge();
    // Le premier abonnement réussit et rejoue « Connecté » ; le suivant échoue.
    bridge.onOperation = async () => {
      throw new Error("abonnement refusé");
    };
    const link = useLinkStore();
    await link.start();
    expect(link.eventOf("forge")?.state).toBe("connected");
    expect(link.subscriptionFailed).toBe(true);
    expect(link.listening).toBe(false);
    expect(link.stateOf("forge")).toBe("offline");
    expect(link.stateOf("salon")).toBe("offline");
  });

  it("says in plain words that the internal subscription failed, not the generic UI error", async () => {
    const { bridge } = freshBridge();
    bridge.onNotice = async () => {
      throw new Error("canal fermé");
    };
    const link = useLinkStore();
    await link.start();
    const messages = useToastsStore().items.map((toast) => toast.message);
    expect(messages).toEqual([SUBSCRIBE_LABEL]);
    expect(messages.join("")).not.toContain("Un problème est survenu dans l'interface");
    link.stop();
  });

  it("resubscribes once on a double click on « Réessayer maintenant »", async () => {
    const { bridge } = freshBridge({ retryDelayMs: -1 });
    let failing = true;
    const original = bridge.onNotice.bind(bridge);
    bridge.onNotice = async (listener) => {
      if (failing) throw new Error("canal fermé");
      return original(listener);
    };
    let attempts = 0;
    const onLinkState = bridge.onLinkState.bind(bridge);
    bridge.onLinkState = async (listener) => {
      attempts += 1;
      return onLinkState(listener);
    };
    const link = useLinkStore();
    await link.start();
    expect(attempts).toBe(1);
    failing = false;
    // Deux clics dans le même instant : une seule nouvelle tentative d'abonnement.
    const first = link.retryNow("forge");
    const second = link.retryNow("forge");
    await Promise.all([first, second]);
    await flushPromises();
    expect(attempts).toBe(2);
    expect(link.subscriptionFailed).toBe(false);
    expect(link.listening).toBe(true);
    // Un seul jeu d'abonnements actifs : un état n'est livré qu'une fois à la boucle de rejeu.
    link.stop();
    await vi.advanceTimersByTimeAsync(RESUBSCRIBE_BASE_MS * 4);
    expect(attempts).toBe(2);
  });
});

describe("fingerprint alerts (BR-CONN-003)", () => {
  async function started() {
    const ctx = freshBridge({ agents: [{ ...SAMPLE_AGENT, host: "192.168.1.120" }] });
    const link = useLinkStore();
    await link.start();
    return { ...ctx, link };
  }

  it("keeps one blocking alert per server, hides it when refused and reopens it on request", async () => {
    const { bridge, link } = await started();
    bridge.reinstallAgent("forge", OTHER_FINGERPRINT);
    expect(link.eventOf("forge")).toMatchObject({
      state: "offline",
      blocked: "fingerprint_changed",
    });
    const alert = link.pendingAlert("forge");
    expect(alert?.presentedHex).toBe(OTHER_FINGERPRINT);
    expect(alert?.presented.split(" ")).toHaveLength(8);
    expect(link.pendingAlert("salon")).toBeUndefined();
    link.dismissAlert("forge");
    expect(link.pendingAlert("forge")).toBeUndefined();
    // Toujours suspendu : l'alerte masquée ne rend pas le lien.
    expect(link.eventOf("forge")?.blocked).toBe("fingerprint_changed");
    link.reopenAlert("forge");
    expect(link.pendingAlert("forge")?.presentedHex).toBe(OTHER_FINGERPRINT);
    // Un nouvel avis rouvre une alerte refusée.
    link.dismissAlert("forge");
    bridge.reinstallAgent("forge", "ab".repeat(32));
    expect(link.pendingAlert("forge")?.presentedHex).toBe("ab".repeat(32));
  });

  it("accepts the new fingerprint: the link comes back and the alert is gone", async () => {
    const { bridge, link } = await started();
    bridge.reinstallAgent("forge", OTHER_FINGERPRINT);
    await link.acceptAlert("forge");
    expect(bridge.calls).toContain("accept forge");
    expect(link.eventOf("forge")).toMatchObject({ state: "connected", blocked: null });
    expect(link.pendingAlert("forge")).toBeUndefined();
  });

  it("drops the alert when the block is lifted by anything else", async () => {
    const { bridge, link } = await started();
    bridge.reinstallAgent("forge", OTHER_FINGERPRINT);
    bridge.setState("forge", "connected");
    expect(link.pendingAlert("forge")).toBeUndefined();
  });
});

describe("notices of the library", () => {
  it("tells which server lost its pending-action files, without a blocking window", async () => {
    const { bridge } = freshBridge();
    const link = useLinkStore();
    await link.start();
    bridge.emitNotice({ kind: "operations_lost", serverId: "forge" });
    const toast = useToastsStore().items.at(-1);
    expect(toast).toMatchObject({
      kind: "warn",
      message:
        "forge : le suivi de certaines actions a été perdu. Vérifie l'état du serveur avant de relancer.",
    });
    // « Écoute en retard » n'est pas un message pour l'utilisateur.
    bridge.emitNotice({ kind: "lagged", serverId: null });
    expect(useToastsStore().items).toHaveLength(1);
  });
});
