import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, OWN } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// Démarrer, arrêter, redémarrer depuis la section Services (HRT-65, BR-LIFE-003, 007, 008, 013, 016, 017, 020, 022),
// sur le pont simulé dont les formes sont celles de la coquille.
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

async function open(setup?: (bridge: SimulatedLinkBridge) => void) {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    { key: "h_web", name: "web", origin: "hearth", graceS: 120 },
    { key: "h_db", name: "base", origin: "hearth", state: "stopped" },
  ]);
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get('[data-row][data-key="h_web"]').trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

const actions = (wrapper: ReturnType<typeof mount>) =>
  wrapper
    .findAll("[data-lifecycle-action]")
    .map((button) => button.attributes("data-lifecycle-action"));
const toasts = () => useToastsStore().items.map((toast) => toast.message);

describe("les boutons de pilotage", () => {
  it("ne montrent que les gestes de l'état (BR-LIFE-003)", async () => {
    const { wrapper } = await open();
    expect(actions(wrapper)).toEqual(["stop", "restart"]);
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    expect(actions(wrapper)).toEqual(["start"]);
    wrapper.unmount();
  });

  it("un état de transition n'a aucun bouton et le dit", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", { key: "h_web", name: "web", state: "starting" }),
    );
    expect(actions(wrapper)).toEqual([]);
    expect(wrapper.get("[data-lifecycle-note]").text()).toContain(
      "Une action est déjà en cours sur ce service.",
    );
    wrapper.unmount();
  });

  it("une action en cours bloque les boutons, visibles, avec la raison et le compte écrits dessous", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.serviceOps.accept("forge", { key: "h_web", kind: "backup" }, "claire"),
    );
    const stop = wrapper.get('[data-lifecycle-action="stop"]');
    expect(stop.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-lifecycle-reason]").text()).toBe(
      "Une action est déjà en cours sur ce service. Lancée par claire.",
    );
    wrapper.unmount();
  });

  it("sans le droit, les boutons restent visibles et désactivés avec la raison (BR-GRANT-014)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.setServiceRightOnServer("forge", "marie", false),
    );
    expect(wrapper.get('[data-lifecycle-action="stop"]').attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-lifecycle-reason]").text()).toBe(
      "Votre compte ne peut pas gérer les services de ce serveur.",
    );
    wrapper.unmount();
  });
});

describe("arrêter", () => {
  it("demande une confirmation qui dit l'effet et le délai réel, annuler ne change rien (BR-LIFE-007, 017)", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    const dialog = document.querySelector("dialog");
    expect(dialog?.textContent).toContain("Arrêter web ?");
    expect(dialog?.textContent).toContain("au bout de 120 secondes");
    expect(dialogButton("Arrêter le service")).toBeDefined();
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(bridge.gestures).toEqual([]);
    expect(bridge.calls.some((call) => call.startsWith("service "))).toBe(false);
    wrapper.unmount();
  });

  it("un service créé hors de Hearth ajoute son avertissement (BR-LIFE-008)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", {
        key: "h_web",
        name: "web",
        adopted: { by: "marie", at: new Date().toISOString() },
      }),
    );
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("[data-lifecycle-warning='adopted']")?.textContent).toContain(
      "créé hors de Hearth",
    );
    wrapper.unmount();
  });

  it("envoie l'acte, bloque le service, puis annonce l'issue une seule fois", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await confirmDialog("Arrêter le service", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_web", gesture: "stop" }]);
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(actions(wrapper)).toEqual([]);
    bridge.finishGesture("forge", "h_web");
    await flushPromises();
    expect(toasts()).toEqual(["Le service web est arrêté."]);
    expect(actions(wrapper)).toEqual(["start"]);
    wrapper.unmount();
  });

  it("l'état change sous la fenêtre : elle le dit et la confirmation est inactive (BR-LIFE-022)", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    bridge.services.upsert("forge", { key: "h_web", name: "web", state: "stopped" });
    await flushPromises();
    expect(document.querySelector("[data-lifecycle-stale]")?.textContent).toBe(
      "Ce service vient de passer à l'état « Arrêté ».",
    );
    expect(dialogButton("Arrêter le service")?.getAttribute("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("le lien coupé après l'envoi : résultat inconnu, jamais rejoué (BR-LIFE-020)", async () => {
    const { wrapper, bridge } = await open();
    bridge.actionMode = "cut";
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await confirmDialog("Arrêter le service", OWN);
    expect(toasts()).toContain("Le résultat de cette action n'est pas connu.");
    expect(bridge.gestures).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("démarrer et redémarrer", () => {
  it("démarrer ouvre la fenêtre du socle quand le mot de passe est demandé, sans effet à confirmer", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-lifecycle-action="start"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain("Démarrer base");
    expect(document.querySelector("[data-lifecycle-effect]")).toBeNull();
    await confirmDialog("Démarrer", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_db", gesture: "start" }]);
    wrapper.unmount();
  });

  it("redémarrer annonce l'issue", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="restart"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain("Redémarrer web ?");
    await confirmDialog("Redémarrer le service", OWN);
    bridge.finishGesture("forge", "h_web");
    await flushPromises();
    expect(toasts()).toEqual(["Le service web a redémarré."]);
    wrapper.unmount();
  });

  it("une cause de l'agent se dit en phrase, jamais en code (BR-LIFE-009)", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await confirmDialog("Arrêter le service", OWN);
    bridge.finishGesture("forge", "h_web", "succeeded", "forced_stop");
    await flushPromises();
    expect(toasts()).toEqual([
      "Le service web ne s'est pas arrêté à temps : il a été coupé de force.",
    ]);
    wrapper.unmount();
  });
});

describe("le menu d'une ligne", () => {
  it("propose les gestes de l'état et ouvre le même parcours", async () => {
    const { wrapper, bridge } = await open();
    const button = wrapper.get('[data-row][data-key="h_web"] [data-row-menu-button]');
    expect(button.attributes("aria-label")).toBe("Actions de web");
    await button.trigger("click");
    await flushPromises();
    const items = [...document.querySelectorAll("[data-row-menu-item]")].map((item) =>
      item.getAttribute("data-row-menu-item"),
    );
    expect(items).toEqual(["stop", "restart"]);
    document.querySelector<HTMLButtonElement>('[data-row-menu-item="stop"]')?.click();
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain("Arrêter web ?");
    expect(bridge.gestures).toEqual([]);
    wrapper.unmount();
  });
});
