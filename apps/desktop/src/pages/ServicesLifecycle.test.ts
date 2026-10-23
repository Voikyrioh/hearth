import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, OWN, reauthField } from "@/test/confirm";
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

  it("l'action lancée par le compte connecté se dit « par vous », sans « déjà » (FIX:01M5A5VH5GR2M9PF9Y1SB04D65)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.serviceOps.accept("forge", { key: "h_web", kind: "backup" }, "marie"),
    );
    expect(wrapper.get("[data-lifecycle-reason]").text()).toBe(
      "Une action est en cours sur ce service. Lancée par vous.",
    );
    expect(wrapper.get("[data-detail-busy]").text()).toBe(
      "Une action est en cours sur ce service. Lancée par vous.",
    );
    wrapper.unmount();
  });

  it("un service créé hors de Hearth et non adopté n'a ni bouton ni menu, et dit pourquoi (BR-LIFE-024, FIX:01M5A5VG2H1ESNN212NNMT2PP0)", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.services.upsert("forge", { key: "e_base", name: "base-externe", origin: "external" });
      bridge.services.upsert("forge", {
        key: "e_old",
        name: "ancien",
        origin: "external",
        adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
      });
    });
    expect(wrapper.find('[data-row][data-key="e_base"] [data-row-menu-button]').exists()).toBe(
      false,
    );
    await wrapper.get('[data-row][data-key="e_base"]').trigger("click");
    await flushPromises();
    expect(actions(wrapper)).toEqual([]);
    expect(wrapper.get("[data-lifecycle-note]").text()).toContain(
      "Hearth ne pilote pas ce service : il a été créé hors de Hearth",
    );
    // Un service adopté reste pilotable.
    expect(wrapper.find('[data-row][data-key="e_old"] [data-row-menu-button]').exists()).toBe(true);
    await wrapper.get('[data-row][data-key="e_old"]').trigger("click");
    await flushPromises();
    expect(actions(wrapper)).toEqual(["stop", "restart"]);
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
    // En état normal, la fenêtre confirme l'effet sans demander le mot de passe (HRT-194).
    expect(reauthField()).toBeNull();
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

  it("la fenêtre s'ouvre sans avertissement de Vue (attribut sans destinataire, FIX:01M5A5VMDW33WJG3S1BWKE3N15)", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const { wrapper } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")).not.toBeNull();
    expect(warn.mock.calls.flat().join(" ")).not.toContain("Extraneous");
    warn.mockRestore();
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

describe("la grille du mot de passe (HRT-194)", () => {
  it("arrêter demande le mot de passe en état d'alerte et en mode attaque, jamais l'élévation en attaque", async () => {
    for (const trust of ["alert", "attack"] as const) {
      const { wrapper, bridge } = await open((sim) => sim.reauth.setTrust("forge", trust));
      await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
      await flushPromises();
      expect(reauthField(), trust).not.toBeNull();
      await confirmDialog("Arrêter le service", OWN);
      expect(bridge.gestures, trust).toEqual([
        { serverId: "forge", key: "h_web", gesture: "stop" },
      ]);
      expect(bridge.reauth.state("forge").elevatedForS > 0, trust).toBe(trust === "alert");
      wrapper.unmount();
      document.body.innerHTML = "";
    }
  });

  it("un agent d'avant, qui n'annonce pas l'état, garde la règle d'avant : le mot de passe, jamais « normal » par défaut", async () => {
    const { wrapper, bridge } = await open((sim) => sim.reauth.setTrust("forge", null));
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    await confirmDialog("Arrêter le service", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_web", gesture: "stop" }]);
    wrapper.unmount();
  });
});

describe("le mode attaque entre la lecture et l'envoi (HRT-194)", () => {
  it("démarrer rouvre la fenêtre avec le champ, sans toast d'erreur, et l'ordre part avec le mot de passe", async () => {
    const { wrapper, bridge } = await open();
    const read = bridge.getReauthState.bind(bridge);
    bridge.getReauthState = async (serverId) => {
      const state = await read(serverId);
      bridge.reauth.setTrust(serverId, "attack");
      return state;
    };
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-lifecycle-action="start"]').trigger("click");
    await flushPromises();
    expect(bridge.gestures).toEqual([]);
    expect(toasts()).toEqual([]);
    expect(document.querySelector("dialog")?.textContent).toContain("Démarrer base");
    expect(reauthField()).not.toBeNull();
    await confirmDialog("Démarrer", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_db", gesture: "start" }]);
    wrapper.unmount();
  });

  it("arrêter : la fenêtre ouverte en normal, puis l'attaque : la saisie est gardée et le champ apparaît", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-lifecycle-action="stop"]').trigger("click");
    await flushPromises();
    expect(reauthField()).toBeNull();
    bridge.reauth.setTrust("forge", "attack");
    dialogButton("Arrêter le service")?.click();
    await flushPromises();
    expect(bridge.gestures).toEqual([]);
    expect(toasts()).toEqual([]);
    expect(document.querySelector("dialog")).not.toBeNull();
    expect(reauthField()).not.toBeNull();
    await confirmDialog("Arrêter le service", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_web", gesture: "stop" }]);
    wrapper.unmount();
  });
});

describe("démarrer et redémarrer", () => {
  it("démarrer part sans fenêtre en état normal, la clé du poste suffit (HRT-194, BR-LIFE-016)", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-lifecycle-action="start"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_db", gesture: "start" }]);
    expect(bridge.reauth.state("forge").elevatedForS).toBe(0);
    wrapper.unmount();
  });

  it("démarrer ouvre la fenêtre avec le champ en état d'alerte, sans effet à confirmer", async () => {
    const { wrapper, bridge } = await open((sim) => sim.reauth.setTrust("forge", "alert"));
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-lifecycle-action="start"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain("Démarrer base");
    expect(document.querySelector("[data-lifecycle-effect]")).toBeNull();
    expect(reauthField()).not.toBeNull();
    await confirmDialog("Démarrer", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_db", gesture: "start" }]);
    wrapper.unmount();
  });

  it("l'état passe de normal à alerte entre la lecture et l'ordre : la fenêtre s'ouvre avec le champ, rien n'est perdu", async () => {
    const { wrapper, bridge } = await open();
    const read = bridge.getReauthState.bind(bridge);
    bridge.getReauthState = async (serverId) => {
      const state = await read(serverId);
      bridge.reauth.setTrust(serverId, "alert");
      return state;
    };
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    await wrapper.get('[data-lifecycle-action="start"]').trigger("click");
    await flushPromises();
    expect(bridge.gestures).toEqual([]);
    expect(document.querySelector("dialog")?.textContent).toContain("Démarrer base");
    expect(reauthField()).not.toBeNull();
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
