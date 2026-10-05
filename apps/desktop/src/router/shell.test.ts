import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App.vue";
import { mountContext } from "@/test/mount";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function boot(path: string, options: Parameters<typeof mountContext>[0] = {}) {
  const ctx = await mountContext(options);
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

describe("redirections", () => {
  it("sends / to the first server's dashboard when servers exist", async () => {
    const { router, wrapper } = await boot("/");
    expect(router.currentRoute.value.fullPath).toBe("/servers/forge/dashboard");
    wrapper.unmount();
  });

  it("sends / and the server routes to the welcome page when there is no server", async () => {
    const { router, wrapper } = await boot("/", { servers: [] });
    expect(router.currentRoute.value.name).toBe("welcome");
    await router.push("/servers/forge/dashboard");
    expect(router.currentRoute.value.name).toBe("welcome");
    wrapper.unmount();
  });

  it("does not show the welcome page once a server exists", async () => {
    const { router, wrapper } = await boot("/welcome");
    expect(router.currentRoute.value.name).toBe("dashboard");
    wrapper.unmount();
  });

  it("sends an unknown server to the first server", async () => {
    const { router, wrapper } = await boot("/servers/inconnu/audit");
    expect(router.currentRoute.value.fullPath).toBe("/servers/forge/dashboard");
    wrapper.unmount();
  });

  it("opens /servers/:id on its dashboard and unknown paths on home", async () => {
    const { router, wrapper } = await boot("/servers/salon");
    expect(router.currentRoute.value.fullPath).toBe("/servers/salon/dashboard");
    await router.push("/n-importe-quoi");
    expect(router.currentRoute.value.fullPath).toBe("/servers/forge/dashboard");
    wrapper.unmount();
  });

  it("keeps the administrator-only views away from a read-only account", async () => {
    const { router, wrapper } = await boot("/servers/salon/accounts");
    expect(router.currentRoute.value.fullPath).toBe("/servers/salon/dashboard");
    await router.push("/servers/salon/audit");
    expect(router.currentRoute.value.fullPath).toBe("/servers/salon/dashboard");
    wrapper.unmount();
  });

  it("moves to the first remaining server when the displayed server is removed", async () => {
    const { router, bridge, wrapper } = await boot("/servers/salon/dashboard");
    bridge.removeServer("salon");
    await flushPromises();
    expect(router.currentRoute.value.fullPath).toBe("/servers/forge/dashboard");
    bridge.removeServer("forge");
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("welcome");
    wrapper.unmount();
  });

  it("serves settings without needing a server", async () => {
    const { router, wrapper } = await boot("/settings", { servers: [] });
    expect(router.currentRoute.value.name).toBe("settings");
    wrapper.unmount();
  });
});

describe("shell of a server", () => {
  it("shows rail, navigation, header with the link pill, and the page", async () => {
    const { wrapper } = await boot("/servers/forge/dashboard");
    expect(wrapper.find('nav[aria-label="Serveurs"]').exists()).toBe(true);
    expect(wrapper.find('nav[aria-label="Navigation du serveur"]').exists()).toBe(true);
    expect(wrapper.get("h1").text()).toBe("Tableau de bord");
    expect(wrapper.get(".head [role=status]").text()).toBe("Connecté");
    expect(wrapper.text()).toContain("Bientôt disponible");
    expect(wrapper.find(".banner").exists()).toBe(false);
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("goes connected -> reconnecting -> offline -> back: pill, banner, stale data", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.setState("forge", "reconnecting");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Reconnexion…");
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(true);
    expect(wrapper.find(".banner").exists()).toBe(false);

    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Hors ligne");
    expect(wrapper.get(".banner").text()).toContain("Serveur hors ligne. Dernier contact à");
    expect(wrapper.get(".surface__stamp").text()).toMatch(/^Vu il y a \d+ s$/);

    bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Connecté");
    expect(wrapper.find(".banner").exists()).toBe(false);
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("« Réessayer maintenant » asks the bridge to retry that server", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.setState("forge", "offline");
    await flushPromises();
    await wrapper.get(".banner button").trigger("click");
    expect(bridge.retries).toEqual(["forge"]);
    wrapper.unmount();
  });

  it("disables the header action of Comptes with its explanation while offline", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/accounts");
    const action = () => wrapper.get("#header-actions button");
    expect(action().text()).toBe("Ajouter un compte");
    expect(action().attributes("aria-disabled")).toBeUndefined();
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(action().attributes("aria-disabled")).toBe("true");
    expect(action().attributes("title")).toBe("Indisponible tant que le serveur est hors ligne.");
    await action().trigger("click");
    expect(wrapper.find(".toast").exists()).toBe(false);
    bridge.setState("forge", "connected");
    await flushPromises();
    await action().trigger("click");
    expect(wrapper.get(".toast").text()).toContain("Bientôt disponible");
    wrapper.unmount();
  });

  it("keeps each server's state independent when switching", async () => {
    const { wrapper, bridge, router } = await boot("/servers/forge/dashboard");
    bridge.setState("salon", "offline");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Connecté");
    await router.push("/servers/salon/dashboard");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Hors ligne");
    expect(wrapper.get(".side__name").text()).toBe("nas-salon");
    await router.push("/servers/forge/dashboard");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Connecté");
    wrapper.unmount();
  });
});
