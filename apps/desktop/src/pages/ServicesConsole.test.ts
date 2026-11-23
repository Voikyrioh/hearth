import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { simConsoleView } from "@/link/simulated-console";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { confirmDialog, OWN, reauthField, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// L'onglet « Console » d'un serveur de jeu (HRT-269, BR-CONSOLE-002 à 010), sur le pont simulé dont l'agent juge comme le vrai.
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
  useServicesStore().reset();
  useServiceOpsStore().reset();
  useServiceRightsStore().reset();
});

async function open(setup?: (bridge: SimulatedLinkBridge) => void, view = simConsoleView()) {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    { key: "h_mc", name: "Prominence", origin: "hearth" },
    { key: "h_plain", name: "Terraria", origin: "hearth" },
  ]);
  ctx.bridge.games.register("forge", "h_mc", { name: "Minecraft" });
  ctx.bridge.games.register("forge", "h_plain", { name: "Terraria" });
  ctx.bridge.console.seed("forge", "h_mc", view);
  ctx.bridge.console.seedPlayers("forge", "h_mc", {
    state: "ok",
    online: 2,
    max: 20,
    names: ["Alice_42", "Bram"],
    hiddenNames: 0,
    moreNames: 0,
    readAt: new Date().toISOString(),
  });
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get('[data-row][data-key="h_mc"]').trigger("click");
  await flushPromises();
  await wrapper.get("[data-tab='console']").trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

async function type(wrapper: ReturnType<typeof mount>, text: string) {
  await wrapper.get("[data-console-input]").setValue(text);
  await flushPromises();
}

async function send(wrapper: ReturnType<typeof mount>) {
  await wrapper.get("[data-console-field]").trigger("submit");
  await flushPromises();
}

describe("l'onglet « Console » : existence et ordre", () => {
  it("existe pour un serveur dont l'agent parle à la console, entre « Mods » et « Sauvegardes »", async () => {
    const { wrapper } = await open();
    const tabs = wrapper.findAll("[role='tab']").map((tab) => tab.text());
    expect(tabs).toContain("Console");
    expect(tabs.indexOf("Console")).toBeLessThan(tabs.indexOf("Sauvegardes"));
    expect(wrapper.find("[data-console-tab]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("n'existe pas pour un serveur sans console ouverte à Hearth", async () => {
    const ctx = await mountContext();
    ctx.bridge.services.seed("forge", [{ key: "h_plain", name: "Terraria", origin: "hearth" }]);
    ctx.bridge.games.register("forge", "h_plain", { name: "Terraria" });
    useServersStore().setCurrent("forge");
    const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await wrapper.get('[data-row][data-key="h_plain"]').trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-tab='console']").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("envoyer une commande", () => {
  it("« time set day » : la réponse du jeu apparaît sous la commande, en texte brut, sans code de couleur", async () => {
    const { wrapper, bridge } = await open((b) =>
      b.console.reply("time", {
        outcome: "answered",
        reply: "§aSet the time to <b>1000</b>",
        truncated: false,
      }),
    );
    await type(wrapper, "time set day");
    await send(wrapper);
    expect(bridge.console.received).toHaveLength(1);
    expect(bridge.console.received[0]).toMatchObject({ command: "time set day", commandClass: 1 });
    const entry = wrapper.get("[data-console-entry]");
    expect(entry.get("[data-console-command]").text()).toBe("time set day");
    expect(entry.get("[data-console-reply]").text()).toBe("Set the time to <b>1000</b>");
    expect(entry.find("b").exists()).toBe(false);
    expect((wrapper.get("[data-console-input]").element as HTMLTextAreaElement).value).toBe("");
    wrapper.unmount();
  });

  it("une réponse vide et une absence de réponse sont dites", async () => {
    const { wrapper } = await open((b) =>
      b.console.reply("list", { outcome: "no_answer", reply: null, truncated: false }),
    );
    await type(wrapper, "say bonjour");
    await send(wrapper);
    expect(wrapper.get("[data-console-sentence]").text()).toBe(
      "Commande envoyée. Le jeu n'a rien répondu.",
    );
    await type(wrapper, "list");
    await send(wrapper);
    expect(wrapper.findAll("[data-console-sentence]")[1]?.text()).toContain(
      "Le jeu n'a pas répondu en 10 secondes",
    );
    wrapper.unmount();
  });

  it("« stop » n'est jamais envoyé : la ligne est barrée avec la phrase de la fiche", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "stop");
    await send(wrapper);
    expect(bridge.console.received).toHaveLength(0);
    const entry = wrapper.get("[data-console-entry]");
    expect(entry.attributes("data-outcome")).toBe("refused");
    expect(entry.get("[data-console-sentence]").text()).toContain("utilisez le bouton « Arrêter »");
    wrapper.unmount();
  });

  it("« op Bram » : confirmation avec le titre et la phrase de la fiche, mot de passe, envoi seulement après", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "op Bram");
    expect(wrapper.get("[data-console-help]").text()).toContain("demande votre mot de passe");
    await send(wrapper);
    expect(bridge.console.received).toHaveLength(0);
    expect(document.body.textContent).toContain("Donner les droits d'opérateur à Bram ?");
    expect(document.body.textContent).toContain("Un opérateur peut taper toutes les commandes");
    expect(document.querySelector("[data-console-confirm-command]")?.textContent).toContain(
      "op Bram",
    );
    await confirmDialog("Envoyer la commande");
    await flushPromises();
    expect(bridge.console.received).toHaveLength(1);
    expect(bridge.console.received[0]).toMatchObject({ command: "op Bram", commandClass: 2 });
    wrapper.unmount();
  });

  it("annuler la confirmation ne laisse rien partir, et un mauvais mot de passe est refusé", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "ban Bram");
    await send(wrapper);
    await flushPromises();
    const field = reauthField();
    if (field) typeInto(field, "pas-le-bon");
    await flushPromises();
    [...document.querySelectorAll<HTMLButtonElement>("dialog button")]
      .find((button) => button.textContent?.trim() === "Envoyer la commande")
      ?.click();
    await flushPromises();
    expect(bridge.console.received).toHaveLength(0);
    expect(document.body.textContent).toContain("mot de passe");
    [...document.querySelectorAll<HTMLButtonElement>("dialog button")]
      .find((button) => button.textContent?.trim() === "Annuler")
      ?.click();
    await flushPromises();
    expect(bridge.console.received).toHaveLength(0);
    void OWN;
    wrapper.unmount();
  });
});

describe("les états du champ", () => {
  it("arrêté : champ inactif avec sa phrase et « Démarrer »", async () => {
    const { wrapper } = await open(undefined, simConsoleView({ state: "stopped" }));
    expect((wrapper.get("[data-console-input]").element as HTMLTextAreaElement).disabled).toBe(
      true,
    );
    expect(wrapper.get("[data-console-help]").text()).toContain("Démarrez le serveur pour écrire");
    expect(wrapper.get("[data-console-input]").attributes("placeholder")).toBe(
      "Le serveur est arrêté.",
    );
    expect(wrapper.find("[data-console-start]").exists()).toBe(true);
    expect(wrapper.get("[data-console-send]").attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("qui démarre, et opération en cours (« Voir le suivi »)", async () => {
    const starting = await open(undefined, simConsoleView({ state: "starting" }));
    expect(starting.wrapper.get("[data-console-help]").text()).toContain(
      "La console répond quand le serveur est prêt",
    );
    starting.wrapper.unmount();
    document.body.innerHTML = "";
    const busy = await open(undefined, simConsoleView({ state: "busy" }));
    expect(busy.wrapper.get("[data-console-help]").text()).toContain(
      "La console revient à la fin de l'opération",
    );
    expect(busy.wrapper.find("[data-console-follow]").exists()).toBe(true);
    busy.wrapper.unmount();
  });

  it("sans le droit de gérer les services : lecture seule", async () => {
    const { wrapper } = await open(undefined, simConsoleView({ canWrite: false }));
    expect(wrapper.get("[data-console-input]").attributes("placeholder")).toBe(
      "La console est en lecture seule.",
    );
    expect(wrapper.get("[data-console-help]").text()).toContain(
      "demande le droit de gérer les services",
    );
    wrapper.unmount();
  });

  it("257 caractères et retour à la ligne refusés avant l'envoi, avec leur raison", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "a".repeat(257));
    expect(wrapper.get("[data-console-help]").text()).toBe("256 caractères au plus.");
    expect(wrapper.get("[data-console-send]").attributes("aria-disabled")).toBe("true");
    await type(wrapper, "say a\nstop");
    expect(wrapper.get("[data-console-help]").text()).toBe("Une seule ligne à la fois.");
    await type(wrapper, "a".repeat(256));
    expect(wrapper.get("[data-console-send]").attributes("aria-disabled")).not.toBe("true");
    expect(bridge.console.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("trop de commandes d'un coup : la phrase dit le délai, rien n'est envoyé de plus", async () => {
    const { wrapper, bridge } = await open();
    for (let i = 0; i < 20; i += 1) await bridge.sendConsole("forge", "h_mc", "list", 1, null);
    await type(wrapper, "time set day");
    await send(wrapper);
    expect(wrapper.get("[data-console-help]").text()).toContain("Vingt commandes en une minute");
    expect((wrapper.get("[data-console-input]").element as HTMLTextAreaElement).disabled).toBe(
      true,
    );
    // La vingt et unième est jugée par l'agent, qui la refuse sans l'envoyer : aucune ligne dans l'échange.
    expect(wrapper.findAll("[data-console-entry]")).toHaveLength(0);
    expect(bridge.console.received).toHaveLength(21);
    wrapper.unmount();
  });
});

describe("les commandes rapides", () => {
  it("« Enregistrer le monde » envoie save-all ; « Écrire à tous les joueurs » remplit « say » sans envoyer", async () => {
    const { wrapper, bridge } = await open();
    await wrapper.get("[data-console-quick-id='save']").trigger("click");
    await flushPromises();
    expect(bridge.console.received.map((item) => item.command)).toEqual(["save-all"]);
    await wrapper.get("[data-console-quick-id='say']").trigger("click");
    await flushPromises();
    expect((wrapper.get("[data-console-input]").element as HTMLTextAreaElement).value).toBe("say ");
    expect(bridge.console.received).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("l'encart et la carte des joueurs", () => {
  it("dit ce que Hearth surveille à partir de la fiche, et montre les joueurs", async () => {
    const { wrapper } = await open();
    expect(wrapper.get("[data-console-watch]").text()).toContain("« op »");
    expect(wrapper.get("[data-console-watch]").text()).toContain("jamais par la console");
    expect(wrapper.get("[data-players-count]").text()).toBe("2 sur 20 places");
    wrapper.unmount();
  });

  it("la carte est relue toutes les 30 secondes dans l'onglet Console, sans quitter l'onglet", async () => {
    const { wrapper, bridge } = await open();
    expect(wrapper.get("[data-players-count]").text()).toBe("2 sur 20 places");
    bridge.console.seedPlayers("forge", "h_mc", {
      state: "ok",
      online: 3,
      max: 20,
      names: ["Alice_42", "Bram", "Chloé_m"],
      hiddenNames: 0,
      moreNames: 0,
      readAt: new Date().toISOString(),
    });
    await vi.advanceTimersByTimeAsync(30_000);
    await flushPromises();
    expect(wrapper.get("[data-players-count]").text()).toBe("3 sur 20 places");
    wrapper.unmount();
  });
});
