import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";
import { formatClock } from "@/composables/format";
import { mountContext } from "@/test/mount";
import AppHeader from "./AppHeader.vue";
import OfflineBanner from "./OfflineBanner.vue";
import ServerNav from "./ServerNav.vue";
import ServerRail from "./ServerRail.vue";

const stub = { template: "<div />" };

async function mountRail(path = "/") {
  const ctx = await mountContext();
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", name: "home", component: stub },
      { path: "/settings", name: "settings", component: stub },
      { path: "/servers/new", name: "add-server", component: stub },
      { path: "/servers", name: "servers", component: stub },
      { path: "/servers/:id/dashboard", name: "dashboard", component: stub },
    ],
  });
  await router.push(path);
  const wrapper = mount(ServerRail, { global: { plugins: [ctx.pinia, router] } });
  return { ...ctx, wrapper, router };
}

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

describe("ServerRail", () => {
  it("is a labelled navigation with logo, one avatar per server, add button and settings", async () => {
    const { wrapper } = await mountRail();
    expect(wrapper.get("nav").attributes("aria-label")).toBe("Serveurs");
    expect(wrapper.get('a[aria-label="Accueil"]').attributes("href")).toBe("/");
    expect(wrapper.get('a[aria-label="Réglages"]').attributes("href")).toBe("/settings");
    const servers = wrapper.findAll("a[data-server]");
    expect(servers.map((a) => a.attributes("data-server"))).toEqual(["forge", "salon"]);
    expect(servers[0]?.attributes("href")).toBe("/servers/forge/dashboard");
    expect(servers[0]?.text()).toBe("FO");
  });

  it("marks the selected server (ring + aria-current) and only that one", async () => {
    const { wrapper } = await mountRail("/servers/salon/dashboard");
    const [forge, salon] = wrapper.findAll("a[data-server]");
    expect(salon?.attributes("aria-current")).toBe("true");
    expect(salon?.find(".avatar--active").exists()).toBe(true);
    expect(forge?.attributes("aria-current")).toBeUndefined();
    expect(forge?.find(".avatar--active").exists()).toBe(false);
  });

  it("shows each server's own link state in its avatar name (independent states)", async () => {
    const { wrapper, bridge } = await mountRail();
    bridge.setState("salon", "offline");
    await flushPromises();
    const labels = wrapper
      .findAll("a[data-server] [role=img]")
      .map((e) => e.attributes("aria-label"));
    expect(labels).toEqual(["forge, Connecté", "nas-salon, Hors ligne"]);
  });

  it("opens the add-server wizard and the server book from the rail", async () => {
    const { wrapper } = await mountRail();
    expect(wrapper.get('a[aria-label="Ajouter un serveur"]').attributes("href")).toBe(
      "/servers/new",
    );
    expect(wrapper.get('a[aria-label="Mes serveurs"]').attributes("href")).toBe("/servers");
  });

  it("hides the server book link while no server is registered", async () => {
    const ctx = await mountContext({ servers: [] });
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/", name: "home", component: stub },
        { path: "/servers/new", name: "add-server", component: stub },
        { path: "/servers", name: "servers", component: stub },
        { path: "/settings", name: "settings", component: stub },
      ],
    });
    await router.push("/");
    const wrapper = mount(ServerRail, { global: { plugins: [ctx.pinia, router] } });
    expect(wrapper.find('a[aria-label="Mes serveurs"]').exists()).toBe(false);
    expect(wrapper.find('a[aria-label="Ajouter un serveur"]').exists()).toBe(true);
  });

  it("moves the focus with the arrow keys, wrapping around", async () => {
    const { wrapper } = await mountRail();
    const root = wrapper.element as HTMLElement;
    document.body.appendChild(root);
    const items = [...root.querySelectorAll<HTMLElement>("[data-rail-item]")];
    items[0]?.focus();
    await wrapper.get("nav").trigger("keydown", { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[1]);
    await wrapper.get("nav").trigger("keydown", { key: "ArrowUp" });
    await wrapper.get("nav").trigger("keydown", { key: "ArrowUp" });
    expect(document.activeElement).toBe(items[items.length - 1]);
    await wrapper.get("nav").trigger("keydown", { key: "Home" });
    expect(document.activeElement).toBe(items[0]);
    root.remove();
  });
});

describe("ServerNav", () => {
  async function nav(role: "admin" | "readonly") {
    const ctx = await mountContext();
    await ctx.router.push("/servers/forge/dashboard");
    const server = {
      id: "forge",
      name: "forge",
      address: "192.168.1.120",
      host: "192.168.1.120",
      port: 7341,
      color: 1,
      role,
      username: "marie",
      remember: true,
    } as const;
    return mount(ServerNav, { props: { server }, global: ctx.global });
  }

  it("shows the name, the address in a monospace face and the four entries for an administrator", async () => {
    const wrapper = await nav("admin");
    expect(wrapper.get(".side__name").text()).toBe("forge");
    expect(wrapper.get(".side__address").text()).toBe("192.168.1.120");
    expect(wrapper.findAll(".nav__item").map((a) => a.text())).toEqual([
      "Tableau de bord",
      "Comptes",
      "Journal d'activité",
      "Sécurité",
    ]);
  });

  it("hides Comptes and Journal d'activité, but keeps Sécurité, for a read-only role", async () => {
    const wrapper = await nav("readonly");
    expect(wrapper.findAll(".nav__item").map((a) => a.text())).toEqual([
      "Tableau de bord",
      "Sécurité",
    ]);
  });

  it("marks the active entry", async () => {
    const wrapper = await nav("admin");
    expect(wrapper.find(".nav__item.router-link-active").text()).toBe("Tableau de bord");
  });

  it("is a labelled navigation", async () => {
    expect((await nav("admin")).get("nav").attributes("aria-label")).toBe("Navigation du serveur");
  });
});

describe("AppHeader", () => {
  it("shows the view title and the link pill of its server, always", async () => {
    const { pinia, bridge } = await mountContext();
    const wrapper = mount(AppHeader, {
      props: { title: "Comptes", serverId: "forge" },
      global: { plugins: [pinia] },
    });
    expect(wrapper.get("h1").text()).toBe("Comptes");
    expect(wrapper.get("[role=status]").text()).toBe("Connecté");
    bridge.setState("forge", "reconnecting");
    await flushPromises();
    expect(wrapper.get("[role=status]").text()).toBe("Reconnexion…");
  });

  it("has a slot for actions and no pill without a server", async () => {
    const { pinia } = await mountContext();
    const wrapper = mount(AppHeader, {
      props: { title: "Réglages" },
      slots: { actions: "<button>Action</button>" },
      global: { plugins: [pinia] },
    });
    expect(wrapper.get(".head__actions button").text()).toBe("Action");
    expect(wrapper.find("[role=status]").exists()).toBe(false);
  });
});

describe("OfflineBanner", () => {
  it("shows the exact text with the time of the last contact and a retry button", async () => {
    const last = new Date(2026, 9, 5, 14, 23).getTime();
    const wrapper = mount(OfflineBanner, { props: { lastContactAt: last } });
    expect(formatClock(last)).toBe("14h23");
    expect(wrapper.text()).toContain(
      "Serveur hors ligne. Dernier contact à 14h23. Nouvelle tentative automatique en cours.",
    );
    const button = wrapper.get("button");
    expect(button.text()).toBe("Réessayer maintenant");
    await button.trigger("click");
    expect(wrapper.emitted("retry")).toHaveLength(1);
  });

  it("says so honestly when there was never any contact", () => {
    const wrapper = mount(OfflineBanner, { props: { lastContactAt: null } });
    expect(wrapper.text()).toContain(
      "Serveur hors ligne. Nouvelle tentative automatique en cours.",
    );
  });

  it("is a non-blocking status, not a dialog", () => {
    const wrapper = mount(OfflineBanner, { props: { lastContactAt: null } });
    expect(wrapper.attributes("role")).toBe("status");
  });
});
