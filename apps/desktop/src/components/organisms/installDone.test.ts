import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { FicheDetail, ServiceOperation } from "@/link";
import { startedApp } from "@/test/app";
import InstallDone from "./InstallDone.vue";

// L'écran de fin d'une installation par fiche (HRT-247, BR-FICHE-066) : le titre dit « en marche » ou « démarre » selon ce
// que la fiche déclare, les adresses et les ports viennent de l'agent, la console à distance est dite non publiée, une
// lecture qui échoue se dit avec « Réessayer » sans cacher le reste.

afterEach(() => {
  document.body.innerHTML = "";
});

const operation = (): ServiceOperation => ({
  id: "01OP",
  key: "h_jeu",
  kind: "deploy",
  status: "succeeded",
  step: null,
  steps: [
    { step: "check", state: "done" },
    { step: "pull", state: "done" },
  ],
  percent: null,
  itemsDone: null,
  bytesDone: null,
  bytesTotal: null,
  etaS: null,
  cancellable: false,
  decision: null,
  cause: null,
});

async function open(
  patch: (detail: FicheDetail) => void = () => undefined,
  setup: (ctx: Awaited<ReturnType<typeof startedApp>>) => void = () => undefined,
  world: string | null = "Monde_2",
) {
  const ctx = await startedApp();
  const detail = structuredClone(ctx.bridge.catalog.get("terraria")) as FicheDetail;
  patch(detail);
  ctx.bridge.games.register("forge", "h_jeu", { name: "mon-terraria" });
  setup(ctx);
  const wrapper = mount(InstallDone, {
    props: {
      serverId: "forge",
      serviceKey: "h_jeu",
      name: "mon-terraria",
      world,
      detail,
      operation: operation(),
      withWorld: false,
    },
    global: { plugins: [ctx.pinia] },
    attachTo: document.body,
  });
  await flushPromises();
  return { ...ctx, wrapper };
}

describe("InstallDone", () => {
  it("dit « en marche » quand la fiche ne déclare aucun premier démarrage long", async () => {
    const { wrapper } = await open((detail) => {
      detail.firstStart = null;
    });
    expect(wrapper.get("[data-done-title]").text()).toBe(
      "Le serveur de jeu « mon-terraria » est en marche.",
    );
    expect(wrapper.find("[data-done-starting]").exists()).toBe(false);
    expect(wrapper.get("[data-done-next]").text()).toContain("Le monde « Monde_2 » est créé.");
    wrapper.unmount();
  });

  it("dit « démarre » avec la durée quand la fiche déclare un premier démarrage long", async () => {
    const { wrapper } = await open((detail) => {
      detail.firstStart = {
        readyTimeoutSeconds: 600,
        worldGeneration: true,
        downloads: [],
        stages: [],
      };
    });
    expect(wrapper.get("[data-done-title]").text()).toBe(
      "Le serveur de jeu « mon-terraria » démarre.",
    );
    expect(wrapper.get("[data-done-starting]").text()).toContain("jusqu'à 10 minutes");
    wrapper.unmount();
  });

  it("n'écrit pas le monde quand la fiche n'en nomme pas, et liste adresses et ports lus sur l'agent", async () => {
    const { wrapper } = await open(undefined, undefined, null);
    expect(wrapper.get("[data-done-next]").text()).toMatch(/^Il ne reste qu'à envoyer l'adresse/);
    expect(wrapper.find("[data-game-address-lan-value]").exists()).toBe(true);
    expect(wrapper.findAll("[data-game-port-row]").length).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it("dit que la console à distance n'est pas publiée quand la fiche en déclare une", async () => {
    const { wrapper } = await open((detail) => {
      const port = detail.ports[0];
      if (port)
        detail.ports.push({ ...port, id: "console", publish: "never", role: "remote_console" });
    });
    expect(wrapper.get("[data-done-console]").text()).toBe(
      "La console à distance du jeu n'est pas publiée : rien d'autre à ouvrir.",
    );
    wrapper.unmount();
  });

  it("dit une lecture d'adresses impossible, avec « Réessayer », sans cacher le reste de l'écran", async () => {
    const { wrapper, bridge } = await open(undefined, (ctx) => {
      ctx.bridge.games.failRead = true;
    });
    expect(wrapper.get("[data-done-read-failed]").text()).toContain("n'a pas pu lire les adresses");
    expect(wrapper.find("[data-done-title]").exists()).toBe(true);
    expect(wrapper.find("[data-done-close]").exists()).toBe(true);
    bridge.games.failRead = false;
    await wrapper.get("[data-done-retry]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-done-read-failed]").exists()).toBe(false);
    expect(wrapper.find("[data-game-address]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("déplie les étapes de l'installation et dit combien sont faites", async () => {
    const { wrapper } = await open();
    expect(wrapper.get("[data-done-steps] summary").text()).toMatch(
      /^Les étapes de l'installation : \d+ faites$/,
    );
    expect(wrapper.find("[data-progress-steps]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("émet « ouvrir » et « fermer »", async () => {
    const { wrapper } = await open();
    await wrapper.get("[data-done-open]").trigger("click");
    await wrapper.get("[data-done-close]").trigger("click");
    expect(wrapper.emitted("open")).toHaveLength(1);
    expect(wrapper.emitted("close")).toHaveLength(1);
    wrapper.unmount();
  });
});
