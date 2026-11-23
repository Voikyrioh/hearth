import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { simMod } from "@/link/simulated-mods";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

// L'onglet « Mods » dans le détail d'un service (HRT-261, BR-MOD-001, 020) : il existe pour un serveur dont la FICHE offre la
// voie `drop_jars`, jamais pour un autre ; quitter l'onglet avec un lot non appliqué demande confirmation (HRT-262).
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  useServicesStore().reset();
  useServiceOpsStore().reset();
  useServiceRightsStore().reset();
});

async function open(setup?: (bridge: SimulatedLinkBridge) => void, select = "h_mc") {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    { key: "h_mc", name: "Prominence", origin: "hearth" },
    { key: "h_terraria", name: "Terraria", origin: "hearth" },
  ]);
  ctx.bridge.games.register("forge", "h_mc", { name: "Minecraft", fiche: "essai" });
  ctx.bridge.games.register("forge", "h_terraria", { name: "Terraria", fiche: "terraria" });
  const fiche = ctx.bridge.catalog.addRich();
  fiche.mods?.ways.push("drop_jars");
  ctx.bridge.catalog.put(fiche);
  ctx.bridge.mods.seed("forge", "h_mc", {
    mods: [
      simMod({
        file: "lithium.jar",
        origin: "server_pack",
        identity: {
          id: "lithium",
          name: "Lithium",
          version: "0.11.2",
          loader: "fabric",
          minecraftDeclared: true,
          environment: "both",
        },
      }),
    ],
  });
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get(`[data-row][data-key="${select}"]`).trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

describe("l'onglet « Mods » : existence", () => {
  it("existe pour un serveur dont la fiche offre drop_jars, entre « Vue d'ensemble » et « Journaux »", async () => {
    const { wrapper } = await open();
    const tabs = wrapper.findAll("[role='tab']").map((tab) => tab.text());
    expect(tabs).toContain("Mods");
    expect(tabs.indexOf("Mods")).toBeLessThan(tabs.indexOf("Journaux"));
    await wrapper.get("[data-tab='mods']").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-mods-tab]").exists()).toBe(true);
    expect(wrapper.find("[data-mod-row='lithium.jar']").exists()).toBe(true);
    wrapper.unmount();
  });

  it("n'existe pas pour un serveur dont la fiche n'offre pas la voie", async () => {
    const { wrapper } = await open(undefined, "h_terraria");
    expect(wrapper.find("[data-tab='mods']").exists()).toBe(false);
    wrapper.unmount();
  });

  it("quitter l'onglet avec un lot non appliqué demande confirmation ; rester garde le lot", async () => {
    const { wrapper } = await open();
    await wrapper.get("[data-tab='mods']").trigger("click");
    await flushPromises();
    await wrapper.get("[data-mod-row='lithium.jar'] [data-mod-retire]").trigger("click");
    await wrapper.get("[data-tab='logs']").trigger("click");
    await flushPromises();
    expect(document.body.textContent).toContain("Quitter sans appliquer ?");
    expect(wrapper.find("[data-mods-tab]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("sans lot préparé, quitter l'onglet ne demande rien", async () => {
    const { wrapper } = await open();
    await wrapper.get("[data-tab='mods']").trigger("click");
    await flushPromises();
    await wrapper.get("[data-tab='logs']").trigger("click");
    await flushPromises();
    expect(document.body.textContent).not.toContain("Quitter sans appliquer ?");
    expect(wrapper.find("[data-mods-tab]").exists()).toBe(false);
    wrapper.unmount();
  });
});
