import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { LinkCommandError } from "@/link";
import { mountContext } from "@/test/mount";

// HRT-38 (C30, C46) : serveur hors ligne, page jamais chargée : UN seul « Réessayer » (celui du bandeau), le texte
// « pas encore chargé », pas d'estampille « Vu il y a… » puisque rien n'a été vu.

afterEach(() => {
  document.body.innerHTML = "";
});

const NOT_LOADED = "Pas encore chargé, sera disponible quand le serveur reviendra.";

async function bootBroken(path: string) {
  const ctx = await mountContext();
  const down = () => {
    throw new LinkCommandError({ kind: "unreachable" });
  };
  ctx.bridge.listAccounts = async () => down();
  ctx.bridge.getSecurity = async () => down();
  ctx.bridge.listTrustedDevices = async () => down();
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

const retries = (text: string[]) => text.filter((label) => /^Réessayer/.test(label));

describe("une page jamais chargée, serveur hors ligne", () => {
  it("Comptes : « pas encore chargé », un seul « Réessayer », aucune estampille", async () => {
    const { wrapper, bridge } = await bootBroken("/servers/forge/accounts");
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get("[data-not-loaded-yet]").text()).toBe(NOT_LOADED);
    expect(wrapper.text()).not.toContain("Impossible de lire la liste des comptes.");
    expect(retries(wrapper.findAll("button").map((b) => b.text()))).toEqual([
      "Réessayer maintenant",
    ]);
    expect(wrapper.find('[data-stale="true"]').exists()).toBe(true);
    expect(wrapper.find(".surface__stamp").exists()).toBe(false);
    wrapper.unmount();
  });

  it("Sécurité : « pas encore chargé » pour l'état et pour les postes, un seul « Réessayer », aucune estampille", async () => {
    const { wrapper, bridge } = await bootBroken("/servers/forge/security");
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get("[data-attack-mode-reason]").text()).toBe(NOT_LOADED);
    expect(wrapper.get("[data-not-loaded-yet]").text()).toBe(NOT_LOADED);
    expect(wrapper.text()).not.toContain("Impossible de lire l'état de sécurité");
    expect(wrapper.text()).not.toContain("Impossible de charger la liste de tes postes");
    expect(wrapper.find("[data-security-retry]").exists()).toBe(false);
    expect(retries(wrapper.findAll("button").map((b) => b.text()))).toEqual([
      "Réessayer maintenant",
    ]);
    expect(wrapper.find(".surface__stamp").exists()).toBe(false);
    wrapper.unmount();
  });

  // La cause est dite avec justesse : le serveur « reviendra » seulement quand il ne répond pas ; en session
  // expirée ou accès révoqué il répond, c'est la session qui manque ; connecté, c'est un vrai échec avec « Réessayer ».
  it.each([
    ["offline", NOT_LOADED],
    ["reconnecting", NOT_LOADED],
    ["session_expired", "Pas encore chargé, sera disponible une fois ta session rétablie."],
    ["access_revoked", "Pas encore chargé, sera disponible une fois ta session rétablie."],
  ] as const)(
    "Comptes jamais lu, état « %s » : la phrase qui dit la bonne cause",
    async (state, text) => {
      const { wrapper, bridge } = await bootBroken("/servers/forge/accounts");
      bridge.setState("forge", state);
      await flushPromises();
      expect(wrapper.get("[data-not-loaded-yet]").text()).toBe(text);
      wrapper.unmount();
    },
  );

  it("Comptes jamais lu, serveur connecté : l'échec est dit avec son « Réessayer »", async () => {
    const { wrapper } = await bootBroken("/servers/forge/accounts");
    expect(wrapper.find("[data-not-loaded-yet]").exists()).toBe(false);
    expect(wrapper.text()).toContain("Impossible de lire la liste des comptes.");
    expect(retries(wrapper.findAll("button").map((b) => b.text()))).toEqual(["Réessayer"]);
    wrapper.unmount();
  });

  it("une page déjà lue garde son estampille « Vu il y a… » hors ligne", async () => {
    const ctx = await mountContext();
    await ctx.router.push("/servers/forge/accounts");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get(".surface__stamp").text()).toMatch(/^Vu il y a \d+ s$/);
    wrapper.unmount();
  });
});
