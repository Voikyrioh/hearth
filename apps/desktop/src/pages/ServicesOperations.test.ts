import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, OWN, reauthField, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// La bande « Opérations en cours » dans la section Services (HRT-84, BR-SVC-029 à 031), pour les quatre profils :
// A administrateur avec le droit, B administrateur sans, C lecture seule sans, D lecture seule avec. Sur le pont
// simulé, dont les formes sont comparées à la coquille (`link/simulated-service-ops.test.ts`).
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

async function open(
  setup: ((bridge: SimulatedLinkBridge) => void) | undefined,
  serverId: "forge" | "salon",
) {
  const ctx = await mountContext();
  ctx.bridge.services.seed(serverId, [{ key: "c_web", name: "web" }]);
  setup?.(ctx.bridge);
  ctx.bridge.seedDemoOperations(serverId);
  useServersStore().setCurrent(serverId);
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

const rows = (wrapper: ReturnType<typeof mount>) => wrapper.findAll("[data-operations-strip] li");
const titles = (wrapper: ReturnType<typeof mount>) =>
  rows(wrapper).map((row) => row.get("[data-operation-title]").text());

describe("la bande « Opérations en cours »", () => {
  it("A : trois opérations, avec qui les a lancées et le bouton de la décision", async () => {
    const { wrapper } = await open(undefined, "forge");
    expect(wrapper.get("[data-operations-strip]").attributes("aria-label")).toBe(
      "Opérations en cours",
    );
    expect(titles(wrapper)).toEqual([
      "Déploiement de terraria",
      "Modification de minecraft",
      "Modification de robot-discord",
    ]);
    const deploy = rows(wrapper)[0];
    expect(deploy?.get("[data-operation-step]").text()).toBe("Récupération de l'image");
    expect(deploy?.get("[data-operation-progress]").text()).toBe("40 % · 381 Mo sur 954 Mo");
    expect(deploy?.get("[data-operation-by]").text()).toBe("Lancée par vous.");
    expect(rows(wrapper)[1]?.get("[data-operation-by]").text()).toBe("Lancée par claire.");
    // Seule l'opération qui attend une décision porte le bouton, actif pour le droit.
    expect(wrapper.findAll("[data-operation-decide]")).toHaveLength(1);
    const decide = wrapper.get("[data-operation-decide]");
    expect(decide.attributes("aria-disabled")).toBeUndefined();
    expect(decide.text()).toBe("Répondre");
    // Aucun bouton d'annulation dans la bande.
    expect(wrapper.get("[data-operations-strip]").text()).not.toContain("Annuler");
    wrapper.unmount();
  });

  it("A : « Modification en cours » est compté « en transition » (BR-SVC-011, BR-ENGINE-004)", async () => {
    const { wrapper } = await open(undefined, "forge");
    // Déploiement en cours et Modification en cours : deux services en transition.
    expect(wrapper.text()).toContain("2 en transition");
    wrapper.unmount();
  });

  it("B : sans le droit, le bouton est désactivé et sa raison est écrite dessous", async () => {
    const { wrapper } = await open(
      (bridge) => bridge.setServiceRightOnServer("forge", "marie", false),
      "forge",
    );
    expect(titles(wrapper)).toHaveLength(3);
    const decide = wrapper.get("[data-operation-decide]");
    expect(decide.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-operation-reason]").text()).toBe(
      "Il faut le droit de gérer les services pour répondre.",
    );
    wrapper.unmount();
  });

  it("C : lecture seule sans le droit, l'effacement d'un service se dit sans chemin ni progression", async () => {
    const { wrapper, bridge } = await open(
      (sim) => sim.setServiceRightOnServer("salon", "paul", false),
      "salon",
    );
    bridge.serviceOps.accept("salon", {
      key: "h_terraria",
      kind: "delete",
      step: "erase",
      percent: 30,
      itemsDone: 120,
    });
    await flushPromises();
    await wrapper.get("[data-operations-more]").trigger("click");
    const erase = rows(wrapper).find((row) => row.text().includes("Effacement"));
    expect(erase?.get("[data-operation-title]").text()).toBe(
      "Effacement des données de terraria en cours.",
    );
    expect(erase?.find("[data-operation-progress]").exists()).toBe(false);
    expect(erase?.find("[data-operation-step]").exists()).toBe(false);
    expect(erase?.find("svg").exists()).toBe(false);
    wrapper.unmount();
  });

  it("D : lecture seule avec le droit, la progression de l'effacement est visible", async () => {
    const { wrapper, bridge } = await open(undefined, "salon");
    bridge.serviceOps.accept("salon", {
      key: "h_terraria",
      kind: "delete",
      step: "erase",
      percent: 30,
      itemsDone: 120,
    });
    await flushPromises();
    await wrapper.get("[data-operations-more]").trigger("click");
    const erase = rows(wrapper).find((row) => row.text().includes("Effacement"));
    expect(erase?.get("[data-operation-title]").text()).toBe("Effacement des données de terraria");
    expect(erase?.get("[data-operation-progress]").text()).toBe("30 % · 120 éléments effacés");
    wrapper.unmount();
  });

  it("au-delà de trois lignes, « +N autres » déplie", async () => {
    const { wrapper, bridge } = await open(undefined, "forge");
    bridge.serviceOps.accept("forge", { key: "h_a", kind: "backup" });
    bridge.serviceOps.accept("forge", { key: "h_b", kind: "restore" });
    await flushPromises();
    expect(rows(wrapper)).toHaveLength(3);
    const more = wrapper.get("[data-operations-more]");
    expect(more.text()).toBe("+2 autres");
    await more.trigger("click");
    expect(rows(wrapper)).toHaveLength(5);
    wrapper.unmount();
  });

  it("la bande est absente quand il n'y a rien en cours", async () => {
    const ctx = await mountContext();
    ctx.bridge.services.seed("forge", [{ key: "c_web", name: "web" }]);
    useServersStore().setCurrent("forge");
    const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    expect(wrapper.find("[data-operations-strip]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("le retour du lien (BR-RESIL-010)", () => {
  it("une opération qui court encore reprend sans message, une opération finie annonce son issue une fois", async () => {
    const { wrapper, bridge } = await open(undefined, "forge");
    const toasts = useToastsStore();
    expect(titles(wrapper)).toHaveLength(3);
    bridge.publish("forge", "reconnecting");
    bridge.serviceOps.finish("forge", "01SIMDEMO000000000000DEPL", "succeeded");
    await flushPromises();
    bridge.publish("forge", "connected");
    await flushPromises();
    bridge.publish("forge", "reconnecting");
    bridge.publish("forge", "connected");
    await flushPromises();
    expect(titles(wrapper)).toEqual(["Modification de minecraft", "Modification de robot-discord"]);
    // Une seule notification, celle de l'issue ; rien pour les deux opérations qui courent encore.
    expect(toasts.items.map((toast) => toast.message)).toEqual([
      "Le service terraria est en marche.",
    ]);
    expect(bridge.serviceOps.acknowledged).toEqual([
      { serverId: "forge", ids: ["01SIMDEMO000000000000DEPL"] },
    ]);
    wrapper.unmount();
  });
});

describe("la fenêtre de décision", () => {
  it("demande le mot de passe, envoie le choix, et l'opération en tient compte", async () => {
    const { wrapper, bridge } = await open(undefined, "forge");
    await wrapper.get("[data-operation-decide]").trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain("Répondre pour robot-discord");
    // Aucune réponse n'est préchoisie : le bouton de validation est inactif.
    expect(dialogButton("Valider la réponse")?.getAttribute("aria-disabled")).toBe("true");
    const revert = document.querySelector<HTMLInputElement>('[data-decision-choice="revert"]');
    expect(revert).not.toBeNull();
    revert?.click();
    await flushPromises();
    await confirmDialog("Valider la réponse", OWN);
    expect(bridge.serviceOps.decisions).toEqual([
      { serverId: "forge", opId: "01SIMDEMO000000000000WAIT", choice: "revert" },
    ]);
    // « Revenir » démarre un retour arrière : l'opération n'attend plus de réponse.
    expect(wrapper.find("[data-operation-decide]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("un mot de passe faux se dit sous le champ, rien n'est décidé", async () => {
    const { wrapper, bridge } = await open((sim) => sim.reauth.setTrust("forge", "alert"), "forge");
    await wrapper.get("[data-operation-decide]").trigger("click");
    await flushPromises();
    document.querySelector<HTMLInputElement>('[data-decision-choice="keep"]')?.click();
    await flushPromises();
    typeInto(reauthField(), "faux");
    await flushPromises();
    dialogButton("Valider la réponse")?.click();
    await flushPromises();
    expect(bridge.serviceOps.decisions).toEqual([]);
    expect(document.querySelector("dialog")?.textContent).toContain("incorrect");
    wrapper.unmount();
  });
});

describe("le verrou de pilotage (BR-SVC-031)", () => {
  it("le détail d'un service tenu par une action dit qui l'a lancée", async () => {
    const { wrapper } = await open(undefined, "forge");
    await wrapper.get("[data-operation-see]").trigger("click");
    await flushPromises();
    const busy = wrapper.get("[data-detail-busy]");
    expect(busy.text()).toContain("Une action est en cours sur ce service.");
    expect(busy.text()).toContain("Lancée par vous.");
    // La carte de l'opération est dans le détail.
    expect(wrapper.get("[data-service-detail] [data-operation]").text()).toContain(
      "Déploiement de terraria",
    );
    wrapper.unmount();
  });
});
