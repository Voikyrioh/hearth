import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import App from "@/App.vue";
import { reportUiError } from "@/errors/report";
import { mountContext } from "@/test/mount";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function boot(
  path: string,
  options: Parameters<typeof mountContext>[0] = {},
  prepare?: (router: Awaited<ReturnType<typeof mountContext>>["router"]) => void,
) {
  const ctx = await mountContext(options);
  prepare?.(ctx.router);
  await ctx.router.push(path);
  await ctx.router.isReady();
  // Comme `main.ts` : le gestionnaire global rapporte ce qu'aucune frontière n'a pris.
  const global = {
    ...ctx.global,
    config: { errorHandler: (error: unknown) => reportUiError(error, "test") },
  };
  const wrapper = mount(App, { global, attachTo: document.body });
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
    bridge.dropServer("salon");
    await flushPromises();
    expect(router.currentRoute.value.fullPath).toBe("/servers/forge/dashboard");
    bridge.dropServer("forge");
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
    // Aucune mesure n'est encore arrivée : le chargement du tableau de bord (HRT-11).
    expect(wrapper.text()).toContain("Chargement des mesures");
    expect(wrapper.find(".banner").exists()).toBe(false);
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("goes connected -> reconnecting -> offline -> back: pill, banner, stale data", async () => {
    // « Comptes » : une page qui passe par `StaleSurface` comme les autres ; le tableau de bord a ses
    // propres tests (`dashboard.test.ts`).
    const { wrapper, bridge } = await boot("/servers/forge/accounts");
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
    expect(wrapper.get("#header-actions [role=tooltip]").text()).toBe(
      "Indisponible tant que le serveur est hors ligne.",
    );
    await action().trigger("click");
    expect(wrapper.find(".toast").exists()).toBe(false);
    bridge.setState("forge", "connected");
    await flushPromises();
    await action().trigger("click");
    await flushPromises();
    // Connecté : le bouton ouvre la fenêtre de création (HRT-13).
    expect(document.body.textContent).toContain("Créer un compte");
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

describe("error containment: the shell is never replaced", () => {
  const BrokenRender = defineComponent({
    setup() {
      throw new Error("rendu cassé");
    },
    render: () => h("p"),
  });
  const BrokenClick = defineComponent({
    setup: () => ({
      boom() {
        throw new Error("clic cassé");
      },
    }),
    template: '<button type="button" class="page-action" @click="boom">Agir</button>',
  });
  const addBroken = (router: Parameters<NonNullable<Parameters<typeof boot>[2]>>[0]) => {
    router.addRoute("server", {
      path: "render",
      name: "render",
      component: BrokenRender,
      meta: { title: "pages.dashboard" },
    });
    router.addRoute("server", {
      path: "click",
      name: "click",
      component: BrokenClick,
      meta: { title: "pages.dashboard" },
    });
  };

  beforeEach(() => vi.spyOn(console, "warn").mockImplementation(() => {}));

  it("a rejecting action (retryNow) leaves the shell and the page intact, with a discreet notification", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/accounts");
    vi.spyOn(bridge, "retryNow").mockRejectedValue(new Error("liaison en panne"));
    bridge.setState("forge", "offline");
    await flushPromises();
    await wrapper.get(".banner button").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.find('nav[aria-label="Serveurs"]').exists()).toBe(true);
    expect(wrapper.find('nav[aria-label="Navigation du serveur"]').exists()).toBe(true);
    expect(wrapper.get(".head [role=status]").text()).toBe("Hors ligne");
    expect(wrapper.find(".banner").exists()).toBe(true);
    expect(wrapper.text()).toContain("Identifiant");
    expect(wrapper.get(".toast").text()).toContain("problème est survenu");
    wrapper.unmount();
  });

  it("a page whose render crashes shows the fallback but keeps the shell usable and the pill visible", async () => {
    const { wrapper, router } = await boot("/servers/forge/dashboard", {}, addBroken);
    await router.push("/servers/forge/render");
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain("Cette page a rencontré un problème");
    expect(wrapper.get(".head [role=status]").text()).toBe("Connecté");
    expect(wrapper.find('nav[aria-label="Serveurs"]').exists()).toBe(true);
    // La navigation fonctionne encore : on quitte la page cassée.
    await wrapper.findAll(".nav__item")[0]?.trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("dashboard");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("an event handler that throws inside a page does not replace the page", async () => {
    const { wrapper, router } = await boot("/servers/forge/dashboard", {}, addBroken);
    await router.push("/servers/forge/click");
    await flushPromises();
    await wrapper.get(".page-action").trigger("click");
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.find(".page-action").exists()).toBe(true);
    expect(wrapper.get(".toast").text()).toContain("problème est survenu");
    wrapper.unmount();
  });
});

describe("hors ligne (HRT-12)", () => {
  it("tells the shell which server is displayed, and none on the settings page", async () => {
    const { bridge, router, wrapper } = await boot("/servers/forge/dashboard");
    expect(bridge.displayedServer).toBe("forge");
    await router.push("/servers/salon/dashboard");
    await flushPromises();
    expect(bridge.displayedServer).toBe("salon");
    await router.push("/settings");
    await flushPromises();
    expect(bridge.displayedServer).toBeNull();
    wrapper.unmount();
  });

  it("keeps the page, dims it, dates it and explains the disabled action while offline, without any dialog", async () => {
    const { bridge, wrapper } = await boot("/servers/forge/accounts");
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get(".head [role=status]").text()).toBe("Hors ligne");
    expect(wrapper.text()).toContain("Dernier contact à");
    expect(wrapper.text()).toContain("Réessayer maintenant");
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(true);
    expect(wrapper.text()).toMatch(/Vu il y a \d+ s/);
    const action = wrapper.findAll("button").find((b) => b.text() === "Ajouter un compte");
    expect(action?.attributes("aria-disabled")).toBe("true");
    expect(wrapper.text()).toContain("Indisponible tant que le serveur est hors ligne.");
    expect(wrapper.find('[role="dialog"], [role="alertdialog"], dialog').exists()).toBe(false);
    // Le lien revient : tout se remet en place.
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Dernier contact à");
    wrapper.unmount();
  });

  it("dims and dates EVERY page of a server from the layout, none can forget it (BR-RESIL-007)", async () => {
    const { bridge, router, wrapper } = await boot("/servers/forge/dashboard");
    for (const name of ["dashboard", "accounts", "audit", "security"]) {
      await router.push(`/servers/forge/${name}`);
      await flushPromises();
      expect(wrapper.findAll('[data-stale="true"]'), `${name} connecté`).toHaveLength(0);
      bridge.setState("forge", "offline");
      await flushPromises();
      expect(wrapper.findAll('[data-stale="true"]'), `${name} hors ligne`).toHaveLength(1);
      // Le tableau de bord de ce test n'a reçu aucune mesure : il n'y a rien à dater (HRT-41).
      if (name === "dashboard") expect(wrapper.text(), name).not.toMatch(/Vu il y a/);
      else expect(wrapper.text(), name).toMatch(/Vu il y a \d+ s/);
      bridge.setState("forge", "connected");
      await flushPromises();
    }
    // Toute page déclarée sous le gabarit du serveur est couverte : la liste ci-dessus suit le routeur.
    const pages = router
      .getRoutes()
      .filter((route) => route.path.startsWith("/servers/:id/") && route.name)
      .map((route) => String(route.name));
    expect(pages.sort()).toEqual(["accounts", "audit", "dashboard", "security"]);
    wrapper.unmount();
  });
});
