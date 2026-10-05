import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory } from "vue-router";
import App from "@/App.vue";
import { OTHER_FINGERPRINT, SAMPLE_AGENT, setLinkBridge } from "@/link";
import { createAppRouter } from "@/router";
import { useToastsStore } from "@/stores/toasts";
import { freshBridge } from "@/test/app";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
  document.body.innerHTML = "";
});

async function mountApp(options: Parameters<typeof freshBridge>[0] = {}) {
  const ctx = freshBridge({ agents: [{ ...SAMPLE_AGENT, host: "192.168.1.120" }], ...options });
  const router = createAppRouter(createMemoryHistory());
  const wrapper = mount(App, { global: { plugins: [ctx.pinia, router] }, attachTo: document.body });
  return { ...ctx, router, wrapper };
}

describe("router errors (review HRT-09)", () => {
  it("reports a failed navigation as a discreet notification instead of ignoring it", async () => {
    const ctx = freshBridge();
    const router = createAppRouter(createMemoryHistory());
    router.beforeEach((to) => {
      if (to.path === "/settings") throw new Error("garde en panne");
    });
    await router.push("/welcome").catch(() => {});
    await router.push("/settings").catch(() => {});
    await flushPromises();
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Un problème est survenu dans l'interface. Il a été noté dans le journal.",
    );
    expect(ctx.pinia).toBeDefined();
  });
});

describe("server list failure is visible (review HRT-09)", () => {
  it("shows a message with a retry button when the list of servers cannot be read, then recovers", async () => {
    const { bridge, router, wrapper } = await mountApp();
    let failing = true;
    const original = bridge.onServersChanged.bind(bridge);
    bridge.onServersChanged = async (listener) => {
      if (failing) throw new Error("pont en panne");
      return original(listener);
    };
    await router.push("/welcome");
    await flushPromises();
    const banner = wrapper.get("[data-bridge-down]");
    expect(banner.text()).toContain("Hearth n'arrive pas à lire la liste de tes serveurs.");
    // L'écran d'accueil reste utilisable (jamais d'écran blanc).
    expect(wrapper.text()).toContain("Bienvenue dans Hearth");
    failing = false;
    await banner.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-bridge-down]").exists()).toBe(false);
  });
});

describe("changed identity at the application level", () => {
  it("blocks with the alert, keeps the link suspended when refused, and reconnects when accepted", async () => {
    const { bridge, router, wrapper } = await mountApp();
    await router.push("/servers/forge/dashboard");
    await flushPromises();
    expect(document.body.querySelector("dialog[data-fingerprint-alert]")).toBeNull();

    bridge.reinstallAgent("forge", OTHER_FINGERPRINT);
    await flushPromises();
    const dialog = document.body.querySelector("dialog[data-fingerprint-alert]");
    expect(dialog).not.toBeNull();
    expect(dialog?.textContent).toContain("0F1E2D3C4B5A6978");
    // Le serveur est suspendu : bandeau explicite, pastille « Hors ligne ».
    expect(wrapper.text()).toContain("L'identité de ce serveur a changé.");

    // « Ne pas se connecter » : l'alerte se ferme, le lien reste suspendu, on peut la rouvrir.
    const refuse = [...(dialog?.querySelectorAll("button") ?? [])][0];
    refuse?.click();
    await flushPromises();
    expect(document.body.querySelector("dialog[data-fingerprint-alert]")).toBeNull();
    expect(wrapper.get("[role=status][data-state]").attributes("data-state")).toBe("offline");
    await wrapper
      .findAll("button")
      .find((b) => b.text() === "Voir l'alerte")
      ?.trigger("click");
    await flushPromises();
    const reopened = document.body.querySelector("dialog[data-fingerprint-alert]");
    expect(reopened).not.toBeNull();

    // « Accepter la nouvelle empreinte » : le lien repart.
    const accept = [...(reopened?.querySelectorAll("button") ?? [])][1];
    accept?.click();
    await flushPromises();
    expect(document.body.querySelector("dialog[data-fingerprint-alert]")).toBeNull();
    expect(wrapper.get("[role=status][data-state]").attributes("data-state")).toBe("connected");
  });

  it("shows the sign-in form above the last view when a session has ended", async () => {
    const { bridge, router, wrapper } = await mountApp();
    await router.push("/servers/forge/dashboard");
    await flushPromises();
    bridge.publish("forge", "session_expired", { reason: "expired" });
    await flushPromises();
    expect(wrapper.text()).toContain("Session expirée. Reconnecte-toi.");
    expect(
      (wrapper.get('section form input[autocomplete="username"]').element as HTMLInputElement)
        .value,
    ).toBe("marie");
    // Le serveur se reconnecte avec le mot de passe saisi.
    await wrapper.get('input[autocomplete="current-password"]').setValue("Correct-Horse-9");
    await wrapper.get("section form").trigger("submit");
    await flushPromises();
    expect(wrapper.find("section form").exists()).toBe(false);
  });
});
