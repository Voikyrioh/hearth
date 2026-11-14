import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GameView, SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useGamePasswordStore } from "@/stores/gamePassword";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, OWN } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// Un serveur de jeu au quotidien (HRT-104, BR-JEU-004, 006, 007, 010, 012, 017 à 021, 025, 027), sur le pont simulé dont
// l'agent juge comme le vrai : les quatre cartes du détail, la confirmation de la fiche, la séquence « Sauvegarde du
// monde » puis « Arrêt » et la décision quand la confirmation manque.
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
  useGamePasswordStore().clear();
});

const PASSWORD = "Sentinelle-pw-91";

const SEQUENCE: GameView["stop"] = {
  graceSeconds: 30,
  worldSaved: "by_sequence",
  sequenceTested: true,
};

async function open(
  setup?: (bridge: SimulatedLinkBridge) => void,
  stop: GameView["stop"] = SEQUENCE,
  select = "h_game",
) {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    { key: "h_game", name: "mon-terraria", origin: "hearth", graceS: 30 },
    { key: "h_web", name: "web", origin: "hearth", graceS: 30 },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
  ]);
  ctx.bridge.games.register("forge", "h_game", { name: "Terraria", stop }, PASSWORD);
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get(`[data-row][data-key="${select}"]`).trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

/** La première opération que l'agent simulé tient pour ce serveur (il y en a une, sinon le test échoue ici). */
function firstOp(bridge: SimulatedLinkBridge) {
  const op = bridge.serviceOps.of("forge")[0];
  if (!op) throw new Error("aucune opération suivie");
  return op;
}

const toasts = () => useToastsStore().items.map((toast) => toast.message);
const dialogText = () => document.querySelector("dialog[open]")?.textContent ?? "";

async function stop(wrapper: ReturnType<typeof mount>, gesture: "stop" | "restart" = "stop") {
  await wrapper.get(`[data-lifecycle-action="${gesture}"]`).trigger("click");
  await flushPromises();
}

describe("les quatre cartes d'un serveur de jeu, au-dessus de « Général » (BR-JEU-001)", () => {
  it("sont là, dans l'ordre, pour un service créé depuis une fiche", async () => {
    const { wrapper } = await open();
    const order = [
      "[data-game-address]",
      "[data-game-ports]",
      "[data-game-password]",
      "[data-game-stop]",
      "[data-detail-general]",
    ].map((selector) => wrapper.get(selector).element);
    for (let index = 1; index < order.length; index += 1) {
      const before = order[index - 1] as Element;
      expect(
        before.compareDocumentPosition(order[index] as Element) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    wrapper.unmount();
  });

  it("n'existent pour aucun service qui ne vient pas d'une fiche, adopté ou déployé à la main", async () => {
    const plain = await open(undefined, SEQUENCE, "h_web");
    expect(plain.wrapper.find("[data-game-address]").exists()).toBe(false);
    expect(plain.wrapper.find("[data-game-password]").exists()).toBe(false);
    plain.wrapper.unmount();
    document.body.innerHTML = "";
    const adopted = await open(undefined, SEQUENCE, "e_old");
    expect(adopted.wrapper.find("[data-game-address]").exists()).toBe(false);
    expect(adopted.wrapper.find("[data-game-ports]").exists()).toBe(false);
    adopted.wrapper.unmount();
  });

  it("un service en déploiement n'a aucune carte (détail réduit)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", { key: "h_game", name: "mon-terraria", state: "deploying" }),
    );
    expect(wrapper.find("[data-game-address]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("une lecture qui échoue n'invente aucune carte", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.games.failRead = true;
    });
    expect(wrapper.find("[data-game-address]").exists()).toBe(false);
    expect(wrapper.find("[data-detail-general]").exists()).toBe(true);
    wrapper.unmount();
  });
});

describe("la carte « Adresse pour vos joueurs » (BR-JEU-020, BR-JEU-021, BR-JEU-018)", () => {
  it("dit {adresse}:{port}, que l'adresse ne marche que depuis le réseau et que Hearth ne vérifie pas", async () => {
    const { wrapper } = await open();
    const host = useServersStore().byId("forge")?.host ?? "";
    expect(wrapper.get("[data-game-address-value]").text()).toBe(`${host}:7777`);
    expect(wrapper.get("[data-game-address-local]").text()).toBe(
      "Cette adresse ne marche que depuis votre réseau.",
    );
    expect(wrapper.get("[data-game-address-unverified]").text()).toBe(
      "Hearth ne peut pas vérifier que cette adresse est joignable.",
    );
    expect(wrapper.find("[data-game-address-stopped]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("un jeu à plusieurs ports dit que d'autres ports servent aussi", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.games.patch("forge", "h_game", {
        ports: [
          { hostPort: 7777, protocol: "tcp", role: "game", changed: false },
          { hostPort: 7778, protocol: "udp", role: "query", changed: false },
        ],
      }),
    );
    expect(wrapper.get("[data-game-address-others]").text()).toBe(
      "Ce jeu utilise d'autres ports : ils sont listés dans la carte « Ports à ouvrir ».",
    );
    wrapper.unmount();
  });

  it("copie l'adresse : « Copiée » sur le bouton, notification annoncée", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText: write } });
    const { wrapper } = await open();
    await wrapper.get("[data-game-address-copy]").trigger("click");
    await flushPromises();
    const host = useServersStore().byId("forge")?.host ?? "";
    expect(write).toHaveBeenCalledWith(`${host}:7777`);
    expect(wrapper.get("[data-game-address-copy]").text()).toContain("Copiée");
    expect(toasts()).toContain("Adresse copiée.");
    wrapper.unmount();
  });

  it("une copie qui échoue dit de copier à la main, sans rien bloquer", async () => {
    vi.stubGlobal("navigator", { ...navigator, clipboard: undefined });
    const { wrapper } = await open();
    await wrapper.get("[data-game-address-copy]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-game-address-failed]").text()).toBe(
      "Impossible de copier. Sélectionnez le texte et copiez-le à la main.",
    );
    wrapper.unmount();
  });

  it("un service arrêté grise la carte et dit que personne ne peut se connecter", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", { key: "h_game", name: "mon-terraria", state: "stopped" }),
    );
    expect(wrapper.get("[data-game-address-stopped]").text()).toBe(
      "Le service est arrêté : personne ne peut se connecter.",
    );
    expect(wrapper.get("[data-game-address-body]").classes()).toContain("addr--off");
    // La phrase n'est pas dans la partie grisée : elle reste lisible.
    expect(
      wrapper.get("[data-game-address-body]").find("[data-game-address-stopped]").exists(),
    ).toBe(false);
    wrapper.unmount();
  });
});

describe("la carte « Ports à ouvrir » (BR-JEU-017 à BR-JEU-019)", () => {
  it("liste les ports avec protocole et rôle, sans rien promettre", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.games.patch("forge", "h_game", {
        ports: [
          { hostPort: 7778, protocol: "udp", role: "query", changed: false },
          { hostPort: 7777, protocol: "tcp", role: "game", changed: false },
        ],
      }),
    );
    const rows = wrapper
      .findAll("[data-game-port-row]")
      .map((row) => row.findAll("td").map((cell) => cell.text()));
    expect(rows).toEqual([
      ["7777", "TCP", "Port du jeu"],
      ["7778", "UDP", "Port de requêtes"],
    ]);
    expect(wrapper.get("[data-game-ports-open]").text()).toContain(
      "Hearth ne les ouvre pas lui-même.",
    );
    expect(wrapper.get("[data-game-ports-unverified]").text()).toBe(
      "Hearth ne peut pas vérifier que ces ports sont joignables depuis Internet.",
    );
    expect(wrapper.find("[data-game-ports-changed]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("coexiste avec la carte « Ports » du lot 1", async () => {
    const { wrapper } = await open();
    expect(wrapper.find("[data-detail-ports]").exists()).toBe(true);
    expect(wrapper.find("[data-game-ports]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("un port qui a changé laisse un rappel permanent", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.games.patch("forge", "h_game", {
        ports: [{ hostPort: 7790, protocol: "tcp", role: "game", changed: true }],
      }),
    );
    expect(wrapper.get("[data-game-ports-changed]").text()).toBe(
      "Les ports ont changé : mettez à jour votre box et votre pare-feu.",
    );
    wrapper.unmount();
  });

  it("copie la liste « port/protocole »", async () => {
    const write = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText: write } });
    const { wrapper } = await open();
    await wrapper.get("[data-game-ports-copy]").trigger("click");
    await flushPromises();
    expect(write).toHaveBeenCalledWith("7777/tcp");
    expect(toasts()).toContain("Liste copiée.");
    wrapper.unmount();
  });
});

describe("la carte « Mot de passe du serveur » (BR-JEU-025, 027)", () => {
  const toggle = (wrapper: ReturnType<typeof mount>) => wrapper.get("[data-game-password-toggle]");

  it("masque par défaut, sans la valeur dans la page, et la révèle à la demande", async () => {
    const { wrapper, bridge } = await open();
    expect(wrapper.get("[data-game-password-mask]").text()).toContain("••••••••");
    expect(wrapper.text()).not.toContain(PASSWORD);
    expect(bridge.games.reveals).toEqual([]);
    await toggle(wrapper).trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-game-password-value]").text()).toBe(PASSWORD);
    expect(wrapper.get("[data-game-password-status]").text()).toContain("Mot de passe visible");
    expect(toggle(wrapper).text()).toContain("Masquer le mot de passe");
    expect(bridge.games.reveals).toHaveLength(1);
    wrapper.unmount();
  });

  it("remasque à la demande, et la valeur n'est plus dans la mémoire", async () => {
    const { wrapper } = await open();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-game-password-value]").exists()).toBe(false);
    expect(useGamePasswordStore().password).toBeNull();
    wrapper.unmount();
  });

  it("remasque quand le détail se ferme ou que le service change", async () => {
    const { wrapper } = await open();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    await wrapper.get('[data-row][data-key="h_web"]').trigger("click");
    await flushPromises();
    expect(useGamePasswordStore().password).toBeNull();
    await wrapper.get('[data-row][data-key="h_game"]').trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-game-password-value]").exists()).toBe(false);
    wrapper.unmount();
    expect(useGamePasswordStore().password).toBeNull();
  });

  it("un compte sans le droit lit « Réservé aux comptes qui peuvent gérer les services. », sans bouton", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.setServiceRightOnServer("forge", "marie", false),
    );
    expect(wrapper.get("[data-game-password-reserved]").text()).toBe(
      "Réservé aux comptes qui peuvent gérer les services.",
    );
    expect(wrapper.find("[data-game-password-toggle]").exists()).toBe(false);
    expect(bridge.games.reveals).toEqual([]);
    wrapper.unmount();
  });

  it("un serveur sans mot de passe le dit, sans bouton", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.games.register("forge", "h_game", { name: "Terraria", stop: SEQUENCE }, null),
    );
    expect(wrapper.get("[data-game-password-none]").text()).toBe(
      "Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie.",
    );
    expect(wrapper.find("[data-game-password-toggle]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit que certains jeux écrivent leurs réglages dans leurs journaux", async () => {
    const { wrapper } = await open();
    expect(wrapper.get("[data-game-password-logs]").text()).toBe(
      "Certains jeux écrivent leurs réglages, mot de passe compris, dans leurs journaux : Hearth ne peut pas le filtrer.",
    );
    wrapper.unmount();
  });

  it("une lecture qui échoue le dit et ne montre aucune valeur", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.games.failReveal = true;
    });
    await toggle(wrapper).trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-game-password-failed]").text()).toBe(
      "Impossible de lire les valeurs. Réessayez.",
    );
    expect(wrapper.find("[data-game-password-value]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("le lien coupé remasque", async () => {
    const { wrapper, bridge } = await open();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    bridge.publish("forge", "reconnecting");
    await flushPromises();
    expect(useGamePasswordStore().password).toBeNull();
    wrapper.unmount();
  });
});

describe("la carte « Arrêt du jeu » (BR-JEU-010)", () => {
  it("dit ce que le jeu fait de son monde et que le délai d'arrêt est celui de la fiche", async () => {
    const { wrapper } = await open();
    expect(wrapper.get("[data-game-stop-main]").text()).toContain(
      "Hearth lui demande de l'enregistrer",
    );
    expect(wrapper.find("[data-game-stop-outside]").exists()).toBe(true);
    expect(wrapper.get("[data-game-stop-grace]").text()).toBe("Délai d'arrêt : 30 secondes");
    wrapper.unmount();
  });
});

describe("la confirmation d'un arrêt reprend la phrase de la fiche (BR-JEU-004, BR-JEU-007)", () => {
  it("jeu qui n'enregistre pas, suite essayée : l'enregistrement d'abord", async () => {
    const { wrapper } = await open();
    await stop(wrapper);
    expect(dialogText()).toContain("Hearth va d'abord demander à Terraria d'enregistrer son monde");
    expect(dialogText()).not.toContain("ne peut pas encore envoyer d'ordre de sauvegarde");
    wrapper.unmount();
  });

  it("jeu qui enregistre en s'arrêtant : le délai de la fiche", async () => {
    const { wrapper } = await open(undefined, {
      graceSeconds: 120,
      worldSaved: "by_image_on_signal",
      sequenceTested: false,
    });
    await stop(wrapper);
    expect(dialogText()).toContain(
      "Terraria enregistre son monde en s'arrêtant. Il lui faut jusqu'à 120 secondes",
    );
    wrapper.unmount();
  });

  it("suite non essayée : Hearth ne peut pas demander à enregistrer", async () => {
    const { wrapper } = await open(undefined, { ...SEQUENCE, sequenceTested: false });
    await stop(wrapper);
    expect(dialogText()).toContain(
      "Hearth ne peut pas demander à Terraria d'enregistrer son monde.",
    );
    wrapper.unmount();
  });

  it("un service qui n'est pas un jeu garde la phrase générale", async () => {
    const { wrapper } = await open(undefined, SEQUENCE, "h_web");
    await stop(wrapper);
    expect(dialogText()).toContain(
      "Hearth ne peut pas encore envoyer d'ordre de sauvegarde au service.",
    );
    wrapper.unmount();
  });

  it("un redémarrage reprend la phrase du jeu aussi", async () => {
    const { wrapper } = await open();
    await stop(wrapper, "restart");
    expect(dialogText()).toContain("et le relancer");
    wrapper.unmount();
  });

  it("annuler n'envoie rien", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(bridge.gestures).toEqual([]);
    wrapper.unmount();
  });
});

describe("la séquence « Sauvegarde du monde » puis « Arrêt » (BR-JEU-003, 005, 008)", () => {
  it("s'ouvre à l'acceptation, montre les deux étapes, puis dit que le monde est enregistré", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    await confirmDialog("Arrêter le service", OWN);
    expect(bridge.gestures).toEqual([{ serverId: "forge", key: "h_game", gesture: "stop" }]);
    const sequence = document.querySelector("[data-game-sequence]");
    expect(sequence).not.toBeNull();
    expect(dialogText()).toContain("Arrêt de mon-terraria");
    const steps = [...document.querySelectorAll("[data-progress-steps] [data-step]")];
    expect(steps.map((step) => step.getAttribute("data-step"))).toEqual(["save_world", "stop"]);
    expect(steps[0]?.getAttribute("data-state")).toBe("now");
    expect(dialogText()).toContain(
      "Hearth attend que Terraria confirme l'enregistrement de son monde",
    );
    // L'agent a vu le signe, l'ordre de sortie part, le moteur arrête.
    const op = firstOp(bridge);
    bridge.serviceOps.update("forge", op.id, {
      step: "stop",
      steps: [
        { step: "save_world", state: "done" },
        { step: "stop", state: "intent" },
      ],
    });
    await flushPromises();
    expect(document.querySelector('[data-step="save_world"]')?.getAttribute("data-state")).toBe(
      "done",
    );
    expect(document.querySelector('[data-step="stop"]')?.getAttribute("data-state")).toBe("now");
    expect(document.querySelector("[data-game-sequence-saved]")?.textContent).toContain(
      "Le monde est enregistré.",
    );
    expect(dialogText()).toContain("Il lui reste 30 secondes avant d'être coupé de force.");
    bridge.finishGesture("forge", "h_game");
    await flushPromises();
    expect(document.querySelector("[data-game-sequence-end]")?.textContent).toContain(
      "Le service mon-terraria est arrêté. Le monde est enregistré.",
    );
    // L'issue est dite par la fenêtre, pas par une notification.
    expect(toasts()).toEqual([]);
    wrapper.unmount();
  });

  it("« Fermer » n'arrête rien : la séquence continue et son issue est annoncée comme les autres", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    await confirmDialog("Arrêter le service", OWN);
    document.querySelector<HTMLButtonElement>("[data-game-sequence-close]")?.click();
    await flushPromises();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(
      wrapper.find("[data-ops-strip]").exists() || useServiceOpsStore().of("forge").length > 0,
    ).toBe(true);
    const op = firstOp(bridge);
    bridge.serviceOps.update("forge", op.id, {
      steps: [{ step: "save_world", state: "done" }],
    });
    bridge.finishGesture("forge", "h_game");
    await flushPromises();
    expect(toasts()).toEqual(["Le service mon-terraria est arrêté. Le monde est enregistré."]);
    wrapper.unmount();
  });

  it("« Voir » rouvre la séquence fermée en cours de route, avec son étape", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    await confirmDialog("Arrêter le service", OWN);
    document.querySelector<HTMLButtonElement>("[data-game-sequence-close]")?.click();
    await flushPromises();
    expect(document.querySelector("dialog[open]")).toBeNull();
    const op = firstOp(bridge);
    bridge.serviceOps.update("forge", op.id, {
      step: "stop",
      steps: [
        { step: "save_world", state: "done" },
        { step: "stop", state: "intent" },
      ],
    });
    await flushPromises();
    await wrapper.get("[data-operation-see]").trigger("click");
    await flushPromises();
    expect(document.querySelector("[data-game-sequence]")).not.toBeNull();
    expect(document.querySelector('[data-step="stop"]')?.getAttribute("data-state")).toBe("now");
    wrapper.unmount();
  });

  it("un jeu qui enregistre en s'arrêtant n'a que l'étape « Arrêt »", async () => {
    const { wrapper } = await open(undefined, {
      graceSeconds: 120,
      worldSaved: "by_image_on_signal",
      sequenceTested: false,
    });
    await stop(wrapper);
    await confirmDialog("Arrêter le service", OWN);
    const steps = [...document.querySelectorAll("[data-progress-steps] [data-step]")];
    expect(steps.map((step) => step.getAttribute("data-step"))).toEqual(["stop"]);
    wrapper.unmount();
  });

  it("un redémarrage ajoute « Démarrage »", async () => {
    const { wrapper } = await open();
    await stop(wrapper, "restart");
    await confirmDialog("Redémarrer le service", OWN);
    expect(dialogText()).toContain("Redémarrage de mon-terraria");
    const steps = [...document.querySelectorAll("[data-progress-steps] [data-step]")];
    expect(steps.map((step) => step.getAttribute("data-step"))).toEqual([
      "save_world",
      "stop",
      "start",
    ]);
    wrapper.unmount();
  });

  it("le lien coupé avant la réponse dit que le résultat n'est pas connu et ne rejoue rien (BR-JEU-012)", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    bridge.actionMode = "cut";
    await confirmDialog("Arrêter le service", OWN);
    expect(document.querySelector("[data-game-sequence-unknown]")?.textContent).toContain(
      "Le résultat de cette action n'est pas connu.",
    );
    // Le geste n'est parti qu'une fois : rien n'est rejoué.
    expect(bridge.gestures).toHaveLength(1);
    wrapper.unmount();
  });

  it("l'arrêt interrompu par l'agent dit que le service tourne toujours (BR-JEU-011)", async () => {
    const { wrapper, bridge } = await open();
    await stop(wrapper);
    await confirmDialog("Arrêter le service", OWN);
    const op = firstOp(bridge);
    bridge.serviceOps.update("forge", op.id, {
      step: "save_world",
      steps: [{ step: "save_world", state: "intent" }],
    });
    bridge.finishGesture("forge", "h_game", "interrupted", "interrupted");
    await flushPromises();
    expect(document.querySelector("[data-game-sequence-end]")?.textContent).toContain(
      "L'arrêt de mon-terraria a été interrompu. Le service tourne toujours.",
    );
    wrapper.unmount();
  });
});

describe("la décision quand la confirmation manque (BR-JEU-006, BR-JEU-007)", () => {
  const UNTESTED: GameView["stop"] = { ...SEQUENCE, sequenceTested: false };

  async function ask() {
    const opened = await open(undefined, UNTESTED);
    await stop(opened.wrapper);
    await confirmDialog("Arrêter le service", OWN);
    return opened;
  }

  it("s'ouvre d'elle-même, sans rien avoir arrêté, avec « Annuler » présélectionné et en premier", async () => {
    const { wrapper } = await ask();
    expect(dialogText()).toContain(
      "Hearth ne peut pas demander à Terraria d'enregistrer son monde.",
    );
    expect(dialogText()).toContain("Rien n'a été arrêté : le service tourne toujours.");
    const radios = [...document.querySelectorAll<HTMLInputElement>("[data-decision-choice]")];
    expect(radios.map((radio) => radio.getAttribute("data-decision-choice"))).toEqual([
      "cancel",
      "continue",
    ]);
    expect(radios[0]?.checked).toBe(true);
    expect(dialogText()).toContain("Annuler : le service reste en marche");
    expect(dialogText()).toContain("Arrêter quand même");
    expect(dialogButton("Annuler l'arrêt")).toBeDefined();
    // Le service n'a pas bougé.
    expect(
      useServicesStore()
        .of("forge")
        ?.services.find((s) => s.key === "h_game")?.state,
    ).toBe("running");
    wrapper.unmount();
  });

  it("« Annuler » laisse le service en marche et le dit", async () => {
    const { wrapper, bridge } = await ask();
    await confirmDialog("Annuler l'arrêt", OWN);
    expect(bridge.serviceOps.decisions.map((decision) => decision.choice)).toEqual(["cancel"]);
    await flushPromises();
    expect(document.querySelector("[data-game-sequence-end]")?.textContent).toContain(
      "L'arrêt de mon-terraria a été annulé. Le service tourne toujours.",
    );
    expect(
      useServicesStore()
        .of("forge")
        ?.services.find((s) => s.key === "h_game")?.state,
    ).toBe("running");
    wrapper.unmount();
  });

  it("« Arrêter quand même » demande l'arrêt, consigné comme un choix du compte", async () => {
    const { wrapper, bridge } = await ask();
    const radio = document.querySelector<HTMLInputElement>('[data-decision-choice="continue"]');
    radio?.click();
    radio?.dispatchEvent(new Event("change", { bubbles: true }));
    await flushPromises();
    expect(dialogButton("Arrêter quand même")).toBeDefined();
    await confirmDialog("Arrêter quand même", OWN);
    expect(bridge.serviceOps.decisions.map((decision) => decision.choice)).toEqual(["continue"]);
    await flushPromises();
    // La séquence reprend la main : l'arrêt est en cours.
    expect(document.querySelector("[data-game-sequence]")).not.toBeNull();
    expect(document.querySelector('[data-step="stop"]')?.getAttribute("data-state")).toBe("now");
    wrapper.unmount();
  });

  it("« Décider plus tard » ne répond pas, ferme tout, et « Voir » rouvre la séquence", async () => {
    const { wrapper, bridge } = await ask();
    dialogButton("Décider plus tard")?.click();
    await flushPromises();
    expect(bridge.serviceOps.decisions).toEqual([]);
    // Aucune fenêtre ne reste : la séquence montrerait un arrêt qui n'a pas commencé.
    expect(document.querySelector("dialog[open]")).toBeNull();
    // La bande garde « Répondre ».
    expect(wrapper.find("[data-operation-decide]").exists()).toBe(true);
    await wrapper.get("[data-operation-see]").trigger("click");
    await flushPromises();
    expect(document.querySelector("[data-game-sequence-line]")?.textContent).toContain(
      "Hearth attend votre choix : le service tourne toujours.",
    );
    document.querySelector<HTMLButtonElement>("[data-game-sequence-decide]")?.click();
    await flushPromises();
    expect(dialogText()).toContain("Rien n'a été arrêté");
    wrapper.unmount();
  });

  it("sans réponse en 15 minutes l'arrêt est annulé et le service tourne toujours", async () => {
    const { wrapper, bridge } = await ask();
    const op = firstOp(bridge);
    bridge.serviceOps.finish("forge", op.id, "cancelled", "decision_expired");
    await flushPromises();
    expect(dialogText()).toContain(
      "La demande est restée sans réponse : l'arrêt a été annulé. Le service tourne toujours.",
    );
    wrapper.unmount();
  });

  it("après l'ordre sans confirmation, dit que Hearth n'a pas pu s'assurer que le monde est enregistré", async () => {
    const opened = await open();
    await stop(opened.wrapper);
    await confirmDialog("Arrêter le service", OWN);
    const op = firstOp(opened.bridge);
    opened.bridge.serviceOps.update("forge", op.id, {
      step: "save_world",
      steps: [{ step: "save_world", state: "intent" }],
    });
    opened.bridge.serviceOps.askDecision("forge", op.id, ["continue", "cancel"]);
    await flushPromises();
    expect(dialogText()).toContain(
      "Hearth n'a pas pu s'assurer que le monde est enregistré. Si vous arrêtez maintenant, ce qui est arrivé depuis le dernier enregistrement automatique du jeu peut être perdu.",
    );
    opened.wrapper.unmount();
  });
});

describe("les décisions des autres opérations ne changent pas", () => {
  it("une mise à jour en attente garde ses choix dans l'ordre de l'agent, sans présélection", async () => {
    const { wrapper, bridge } = await open();
    const op = bridge.serviceOps.accept(
      "forge",
      { key: "h_web", kind: "update", step: "verify" },
      "marie",
    );
    bridge.serviceOps.askDecision("forge", op.id, ["revert", "keep"]);
    await flushPromises();
    await wrapper.get("[data-operation-decide]").trigger("click");
    await flushPromises();
    const radios = [...document.querySelectorAll<HTMLInputElement>("[data-decision-choice]")];
    expect(radios.map((radio) => radio.getAttribute("data-decision-choice"))).toEqual([
      "revert",
      "keep",
    ]);
    expect(radios.some((radio) => radio.checked)).toBe(false);
    wrapper.unmount();
  });
});
